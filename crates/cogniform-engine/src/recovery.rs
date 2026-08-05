use core::fmt;

use cogniform_protocol::FrameId;
use cogniform_replay::{ReplayConfig, ReplayConfigError};
use sha2::{Digest, Sha256};

const RECOVERY_MAGIC: &[u8; 6] = b"CNFRCP";
const RECOVERY_FORMAT_VERSION: u16 = 1;
const RECOVERY_HASH_DOMAIN: &[u8] = b"cogniform.engine-recovery-point.v1\0";
const VERSION_OFFSET: usize = RECOVERY_MAGIC.len();
const FRAME_OFFSET: usize = VERSION_OFFSET + size_of::<u16>();
const REPLAY_LENGTH_OFFSET: usize = FRAME_OFFSET + size_of::<u64>();
const REPLAY_OFFSET: usize = REPLAY_LENGTH_OFFSET + size_of::<u32>();
const DIGEST_BYTES: usize = 32;
const ENVELOPE_OVERHEAD_BYTES: usize = REPLAY_OFFSET + DIGEST_BYTES;

/// Complete in-memory state required to restore engine causality.
///
/// Replay bytes preserve accepted authoritative state. The next frame identity
/// prevents reuse of frames already reserved by the source renderer but not
/// represented in the replay stream.
#[derive(Clone, PartialEq, Eq)]
pub struct EngineRecoveryPoint {
    replay_bytes: Vec<u8>,
    next_frame_id: FrameId,
}

impl EngineRecoveryPoint {
    /// Returns the maximum encoded envelope bytes for one replay configuration.
    pub fn envelope_byte_limit(
        replay_config: ReplayConfig,
    ) -> Result<u64, RecoveryPointCodecError> {
        validate_replay_config(replay_config)?;
        Ok(u64::from(replay_config.max_log_bytes.get()) + ENVELOPE_OVERHEAD_BYTES as u64)
    }

    /// Creates a caller-owned recovery point for later bounded validation.
    #[must_use]
    pub const fn from_parts(replay_bytes: Vec<u8>, next_frame_id: FrameId) -> Self {
        Self {
            replay_bytes,
            next_frame_id,
        }
    }

    /// Returns the complete encoded replay stream.
    #[must_use]
    pub fn replay_bytes(&self) -> &[u8] {
        &self.replay_bytes
    }

    /// Returns the first frame identity available to the restored renderer.
    #[must_use]
    pub const fn next_frame_id(&self) -> FrameId {
        self.next_frame_id
    }

    /// Splits this point into its complete replay stream and next frame identity.
    #[must_use]
    pub fn into_parts(self) -> (Vec<u8>, FrameId) {
        (self.replay_bytes, self.next_frame_id)
    }

    /// Encodes this point as one deterministic, integrity-protected v1 envelope.
    ///
    /// The SHA-256 digest detects accidental corruption; it does not authenticate
    /// or encrypt the caller-owned replay bytes.
    pub fn to_envelope_bytes(
        &self,
        replay_config: ReplayConfig,
    ) -> Result<Vec<u8>, RecoveryPointCodecError> {
        validate_replay_config(replay_config)?;
        let replay_byte_count = self.replay_bytes.len() as u64;
        let replay_limit = replay_config.max_log_bytes.get();
        if replay_byte_count > u64::from(replay_limit) {
            return Err(RecoveryPointCodecError::ReplaySizeExceeded {
                actual: replay_byte_count,
                limit: replay_limit,
            });
        }
        let replay_length = u32::try_from(replay_byte_count)
            .expect("a replay bounded by NonZeroU32 always fits in u32");
        let mut encoded = Vec::with_capacity(ENVELOPE_OVERHEAD_BYTES + self.replay_bytes.len());
        encoded.extend_from_slice(RECOVERY_MAGIC);
        encoded.extend_from_slice(&RECOVERY_FORMAT_VERSION.to_be_bytes());
        encoded.extend_from_slice(&self.next_frame_id.get().to_be_bytes());
        encoded.extend_from_slice(&replay_length.to_be_bytes());
        encoded.extend_from_slice(&self.replay_bytes);
        let digest = recovery_digest(&encoded);
        encoded.extend_from_slice(&digest);
        Ok(encoded)
    }

    /// Decodes one complete, bounded, integrity-protected v1 envelope.
    ///
    /// All fields and the digest are validated from `encoded` before replay bytes
    /// are copied into the returned recovery point. Replay semantics and the
    /// accepted-event hash chain remain validated by engine restoration.
    pub fn from_envelope_bytes(
        encoded: &[u8],
        replay_config: ReplayConfig,
    ) -> Result<Self, RecoveryPointCodecError> {
        validate_replay_config(replay_config)?;
        let replay_limit = replay_config.max_log_bytes.get();
        let envelope_limit = u64::from(replay_limit) + ENVELOPE_OVERHEAD_BYTES as u64;
        let encoded_length = encoded.len() as u64;
        if encoded_length > envelope_limit {
            return Err(RecoveryPointCodecError::EnvelopeSizeExceeded {
                actual: encoded_length,
                limit: envelope_limit,
            });
        }
        if encoded.len() < ENVELOPE_OVERHEAD_BYTES {
            return Err(RecoveryPointCodecError::Truncated {
                actual: encoded_length,
                minimum: ENVELOPE_OVERHEAD_BYTES as u64,
            });
        }
        if encoded.get(..RECOVERY_MAGIC.len()) != Some(RECOVERY_MAGIC.as_slice()) {
            return Err(RecoveryPointCodecError::InvalidHeader);
        }

        let version = u16::from_be_bytes(
            encoded[VERSION_OFFSET..FRAME_OFFSET]
                .try_into()
                .expect("the fixed envelope prefix was length-checked"),
        );
        if version != RECOVERY_FORMAT_VERSION {
            return Err(RecoveryPointCodecError::UnsupportedVersion { found: version });
        }

        let next_frame_value = u64::from_be_bytes(
            encoded[FRAME_OFFSET..REPLAY_LENGTH_OFFSET]
                .try_into()
                .expect("the fixed envelope prefix was length-checked"),
        );
        let replay_length = u32::from_be_bytes(
            encoded[REPLAY_LENGTH_OFFSET..REPLAY_OFFSET]
                .try_into()
                .expect("the fixed envelope prefix was length-checked"),
        );
        if replay_length > replay_limit {
            return Err(RecoveryPointCodecError::ReplaySizeExceeded {
                actual: u64::from(replay_length),
                limit: replay_limit,
            });
        }

        let expected_length = ENVELOPE_OVERHEAD_BYTES as u64 + u64::from(replay_length);
        if encoded_length != expected_length {
            return Err(RecoveryPointCodecError::LengthMismatch {
                actual: encoded_length,
                expected: expected_length,
            });
        }
        let next_frame_id =
            FrameId::new(next_frame_value).map_err(|_| RecoveryPointCodecError::InvalidFrameId)?;
        let digest_offset = REPLAY_OFFSET + replay_length as usize;
        let expected_digest = recovery_digest(&encoded[..digest_offset]);
        if encoded[digest_offset..] != expected_digest {
            return Err(RecoveryPointCodecError::IntegrityMismatch);
        }

        Ok(Self::from_parts(
            encoded[REPLAY_OFFSET..digest_offset].to_vec(),
            next_frame_id,
        ))
    }
}

impl fmt::Debug for EngineRecoveryPoint {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("EngineRecoveryPoint")
            .field("replay_byte_count", &self.replay_bytes.len())
            .field("next_frame_id", &self.next_frame_id)
            .finish()
    }
}

/// Failure to encode or decode a bounded recovery-point envelope.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum RecoveryPointCodecError {
    /// The caller-supplied replay bound cannot represent a replay header.
    InvalidReplayConfig(ReplayConfigError),
    /// The byte stream is shorter than the fixed envelope overhead.
    Truncated {
        /// Supplied envelope byte count.
        actual: u64,
        /// Minimum envelope byte count.
        minimum: u64,
    },
    /// The envelope magic is not the Cogniform recovery-point magic.
    InvalidHeader,
    /// The envelope version is not supported by this implementation.
    UnsupportedVersion {
        /// Version encoded by the input.
        found: u16,
    },
    /// The complete envelope exceeds the configured replay bound plus overhead.
    EnvelopeSizeExceeded {
        /// Supplied envelope byte count.
        actual: u64,
        /// Maximum envelope byte count.
        limit: u64,
    },
    /// The replay payload exceeds the caller-supplied replay bound.
    ReplaySizeExceeded {
        /// Declared or supplied replay byte count.
        actual: u64,
        /// Maximum replay byte count.
        limit: u32,
    },
    /// The supplied byte count does not exactly match the declared replay length.
    LengthMismatch {
        /// Supplied envelope byte count.
        actual: u64,
        /// Envelope byte count implied by the replay length field.
        expected: u64,
    },
    /// The envelope contains the reserved zero frame identity.
    InvalidFrameId,
    /// The envelope digest does not match its encoded fields and replay bytes.
    IntegrityMismatch,
}

impl fmt::Display for RecoveryPointCodecError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidReplayConfig(error) => write!(formatter, "invalid replay config: {error}"),
            Self::Truncated { actual, minimum } => write!(
                formatter,
                "recovery envelope has {actual} bytes; at least {minimum} are required"
            ),
            Self::InvalidHeader => formatter.write_str("invalid recovery envelope header"),
            Self::UnsupportedVersion { found } => {
                write!(formatter, "unsupported recovery envelope version {found}")
            }
            Self::EnvelopeSizeExceeded { actual, limit } => write!(
                formatter,
                "recovery envelope has {actual} bytes; configured limit is {limit}"
            ),
            Self::ReplaySizeExceeded { actual, limit } => write!(
                formatter,
                "recovery replay has {actual} bytes; configured limit is {limit}"
            ),
            Self::LengthMismatch { actual, expected } => write!(
                formatter,
                "recovery envelope has {actual} bytes; declared length requires {expected}"
            ),
            Self::InvalidFrameId => formatter.write_str("invalid recovery envelope frame identity"),
            Self::IntegrityMismatch => {
                formatter.write_str("recovery envelope integrity check failed")
            }
        }
    }
}

impl std::error::Error for RecoveryPointCodecError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::InvalidReplayConfig(error) => Some(error),
            Self::Truncated { .. }
            | Self::InvalidHeader
            | Self::UnsupportedVersion { .. }
            | Self::EnvelopeSizeExceeded { .. }
            | Self::ReplaySizeExceeded { .. }
            | Self::LengthMismatch { .. }
            | Self::InvalidFrameId
            | Self::IntegrityMismatch => None,
        }
    }
}

fn validate_replay_config(config: ReplayConfig) -> Result<(), RecoveryPointCodecError> {
    config
        .validate()
        .map_err(RecoveryPointCodecError::InvalidReplayConfig)
}

fn recovery_digest(encoded_without_digest: &[u8]) -> [u8; DIGEST_BYTES] {
    let mut hasher = Sha256::new();
    hasher.update(RECOVERY_HASH_DOMAIN);
    hasher.update(encoded_without_digest);
    hasher.finalize().into()
}

#[cfg(test)]
mod tests {
    use core::num::NonZeroU32;

    use super::*;

    fn recovery() -> EngineRecoveryPoint {
        EngineRecoveryPoint::from_parts(
            b"CNFRPL1\nreplay-fixture".to_vec(),
            FrameId::new(17).unwrap(),
        )
    }

    #[test]
    fn envelope_is_repeatable_and_round_trips_exact_parts() {
        let recovery = recovery();
        let config = ReplayConfig::default();
        let first = recovery.to_envelope_bytes(config).unwrap();
        let second = recovery.to_envelope_bytes(config).unwrap();
        assert_eq!(first, second);
        assert_eq!(&first[..RECOVERY_MAGIC.len()], RECOVERY_MAGIC);
        assert_eq!(&first[VERSION_OFFSET..FRAME_OFFSET], &[0, 1]);
        assert_eq!(
            &first[first.len() - DIGEST_BYTES..],
            &[
                0xa8, 0x48, 0x6e, 0x8c, 0x9f, 0xf3, 0x27, 0x8e, 0x5c, 0xe0, 0x6e, 0x86, 0xc2, 0x5f,
                0x5a, 0x46, 0xd9, 0x08, 0x69, 0xab, 0x77, 0x96, 0x87, 0xf3, 0xa8, 0x75, 0xc3, 0xd1,
                0x4e, 0xfe, 0x50, 0xb2,
            ]
        );

        let decoded = EngineRecoveryPoint::from_envelope_bytes(&first, config).unwrap();
        assert_eq!(decoded, recovery);
    }

    #[test]
    fn every_single_byte_corruption_is_rejected() {
        let config = ReplayConfig::default();
        let encoded = recovery().to_envelope_bytes(config).unwrap();
        for index in 0..encoded.len() {
            let mut corrupted = encoded.clone();
            corrupted[index] ^= 1;
            assert!(
                EngineRecoveryPoint::from_envelope_bytes(&corrupted, config).is_err(),
                "corruption at byte {index} was accepted"
            );
        }
    }

    #[test]
    fn malformed_envelopes_return_typed_errors() {
        let config = ReplayConfig::default();
        let encoded = recovery().to_envelope_bytes(config).unwrap();

        assert!(matches!(
            EngineRecoveryPoint::from_envelope_bytes(
                &encoded[..ENVELOPE_OVERHEAD_BYTES - 1],
                config
            ),
            Err(RecoveryPointCodecError::Truncated { .. })
        ));
        assert!(matches!(
            EngineRecoveryPoint::from_envelope_bytes(&encoded[..encoded.len() - 1], config),
            Err(RecoveryPointCodecError::LengthMismatch { .. })
        ));

        let mut invalid_header = encoded.clone();
        invalid_header[0] ^= 1;
        assert_eq!(
            EngineRecoveryPoint::from_envelope_bytes(&invalid_header, config),
            Err(RecoveryPointCodecError::InvalidHeader)
        );

        let mut unsupported = encoded.clone();
        unsupported[VERSION_OFFSET..FRAME_OFFSET].copy_from_slice(&2_u16.to_be_bytes());
        assert_eq!(
            EngineRecoveryPoint::from_envelope_bytes(&unsupported, config),
            Err(RecoveryPointCodecError::UnsupportedVersion { found: 2 })
        );

        let mut zero_frame = encoded.clone();
        zero_frame[FRAME_OFFSET..REPLAY_LENGTH_OFFSET].fill(0);
        assert_eq!(
            EngineRecoveryPoint::from_envelope_bytes(&zero_frame, config),
            Err(RecoveryPointCodecError::InvalidFrameId)
        );

        let mut trailing = encoded.clone();
        trailing.push(0);
        assert!(matches!(
            EngineRecoveryPoint::from_envelope_bytes(&trailing, config),
            Err(RecoveryPointCodecError::LengthMismatch { .. })
        ));

        let mut mismatched_length = encoded.clone();
        let declared = u32::try_from(recovery().replay_bytes().len()).unwrap() + 1;
        mismatched_length[REPLAY_LENGTH_OFFSET..REPLAY_OFFSET]
            .copy_from_slice(&declared.to_be_bytes());
        assert!(matches!(
            EngineRecoveryPoint::from_envelope_bytes(&mismatched_length, config),
            Err(RecoveryPointCodecError::LengthMismatch { .. })
        ));

        let replay_index = REPLAY_OFFSET;
        let mut invalid_digest = encoded.clone();
        invalid_digest[replay_index] ^= 1;
        assert_eq!(
            EngineRecoveryPoint::from_envelope_bytes(&invalid_digest, config),
            Err(RecoveryPointCodecError::IntegrityMismatch)
        );
    }

    #[test]
    fn replay_and_envelope_bounds_are_enforced_before_copying() {
        let recovery = recovery();
        let default_config = ReplayConfig::default();
        let encoded = recovery.to_envelope_bytes(default_config).unwrap();

        let mut invalid_config = default_config;
        invalid_config.max_log_bytes = NonZeroU32::new(1).unwrap();
        assert!(matches!(
            recovery.to_envelope_bytes(invalid_config),
            Err(RecoveryPointCodecError::InvalidReplayConfig(_))
        ));

        let mut tight_config = default_config;
        tight_config.max_log_bytes = NonZeroU32::new(8).unwrap();
        assert!(matches!(
            recovery.to_envelope_bytes(tight_config),
            Err(RecoveryPointCodecError::ReplaySizeExceeded { .. })
        ));
        assert!(matches!(
            EngineRecoveryPoint::from_envelope_bytes(&encoded, tight_config),
            Err(RecoveryPointCodecError::EnvelopeSizeExceeded { .. })
        ));
        assert_eq!(
            EngineRecoveryPoint::envelope_byte_limit(tight_config).unwrap(),
            u64::from(tight_config.max_log_bytes.get()) + ENVELOPE_OVERHEAD_BYTES as u64
        );

        let mut declared_over_limit =
            EngineRecoveryPoint::from_parts(b"12345678".to_vec(), FrameId::new(1).unwrap())
                .to_envelope_bytes(tight_config)
                .unwrap();
        declared_over_limit[REPLAY_LENGTH_OFFSET..REPLAY_OFFSET]
            .copy_from_slice(&9_u32.to_be_bytes());
        assert_eq!(
            EngineRecoveryPoint::from_envelope_bytes(&declared_over_limit, tight_config),
            Err(RecoveryPointCodecError::ReplaySizeExceeded {
                actual: 9,
                limit: 8,
            })
        );
    }

    #[test]
    fn recovery_point_debug_redacts_replay_contents() {
        let recovery = EngineRecoveryPoint::from_parts(
            b"private-scene-marker".to_vec(),
            FrameId::new(7).unwrap(),
        );
        let debug = format!("{recovery:?}");
        assert!(!debug.contains("private-scene-marker"));
        assert!(debug.contains("replay_byte_count: 20"));
        assert!(debug.contains("next_frame_id"));
    }
}
