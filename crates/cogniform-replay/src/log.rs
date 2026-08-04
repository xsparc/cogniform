use core::{fmt, num::NonZeroU32};

use cogniform_protocol::{ApplyStatus, FrameId, RuntimeLimits, ScenePatch, SceneRevision};
use cogniform_world::{AuthoritativeWorld, LogicalSceneHash, WorldConfig};
use sha2::{Digest, Sha256};

use crate::{
    ReplayError, ReplayIntegrityError, ReplayIntegrityErrorKind, ReplayRevisionError,
    ReplayTailError, ReplayTailErrorKind,
};

const REPLAY_HEADER: &[u8; 8] = b"CNFRPL1\n";
const REPLAY_HEADER_BYTES: u32 = 8;
const ENTRY_HASH_DOMAIN: &[u8] = b"cogniform.replay-entry\0";
const ENTRY_FORMAT_VERSION: u16 = 1;
const FRAME_LENGTH_BYTES: u64 = 4;
const FIXED_ENTRY_BODY_BYTES: u64 = 166;
pub(crate) const ENTRY_ENVELOPE_BYTES: u64 = FRAME_LENGTH_BYTES + FIXED_ENTRY_BODY_BYTES;

/// Explicit byte and entry bounds for one in-memory replay log.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReplayConfig {
    /// Maximum accepted entries.
    pub max_entries: NonZeroU32,
    /// Maximum encoded bytes for one length-prefixed entry.
    pub max_entry_bytes: NonZeroU32,
    /// Maximum encoded bytes for the header and all entries.
    pub max_log_bytes: NonZeroU32,
}

impl ReplayConfig {
    /// Verifies that the total log bound can represent the mandatory header.
    pub const fn validate(self) -> Result<(), ReplayConfigError> {
        if self.max_log_bytes.get() < REPLAY_HEADER_BYTES {
            Err(ReplayConfigError {
                actual: self.max_log_bytes.get(),
                minimum: REPLAY_HEADER_BYTES,
            })
        } else {
            Ok(())
        }
    }
}

impl Default for ReplayConfig {
    fn default() -> Self {
        Self {
            max_entries: NonZeroU32::new(4_096).expect("constant is non-zero"),
            max_entry_bytes: NonZeroU32::new(1_049_000).expect("constant is non-zero"),
            max_log_bytes: NonZeroU32::new(67_108_864).expect("constant is non-zero"),
        }
    }
}

/// Reports a replay configuration that cannot represent an empty stream.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReplayConfigError {
    actual: u32,
    minimum: u32,
}

impl ReplayConfigError {
    /// Returns the configured total byte bound.
    #[must_use]
    pub const fn actual(self) -> u32 {
        self.actual
    }

    /// Returns the minimum total byte bound.
    #[must_use]
    pub const fn minimum(self) -> u32 {
        self.minimum
    }
}

impl fmt::Display for ReplayConfigError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "replay log byte limit {} is below minimum {}",
            self.actual, self.minimum
        )
    }
}

impl std::error::Error for ReplayConfigError {}

/// SHA-256 digest of one versioned replay entry and its predecessor digest.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ReplayEntryHash([u8; 32]);

impl ReplayEntryHash {
    /// Empty predecessor used by the first entry.
    pub const ZERO: Self = Self([0; 32]);

    /// Constructs a digest from exact bytes.
    #[must_use]
    pub const fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    /// Returns the exact digest bytes.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

impl fmt::Display for ReplayEntryHash {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        for byte in self.0 {
            write!(formatter, "{byte:02x}")?;
        }
        Ok(())
    }
}

/// One accepted canonical patch and its integrity and causality evidence.
#[derive(Debug, Clone, PartialEq)]
pub struct ReplayEntry {
    sequence: u64,
    patch: ScenePatch,
    patch_bytes: Vec<u8>,
    estimated_visible_frame: FrameId,
    previous_revision: SceneRevision,
    new_revision: SceneRevision,
    previous_scene_hash: LogicalSceneHash,
    new_scene_hash: LogicalSceneHash,
    previous_entry_hash: ReplayEntryHash,
    entry_hash: ReplayEntryHash,
}

#[derive(Clone, Copy)]
pub(crate) struct ReplayEntryMetadata {
    pub sequence: u64,
    pub estimated_visible_frame: FrameId,
    pub previous_revision: SceneRevision,
    pub new_revision: SceneRevision,
    pub previous_scene_hash: LogicalSceneHash,
    pub new_scene_hash: LogicalSceneHash,
    pub previous_entry_hash: ReplayEntryHash,
}

impl ReplayEntry {
    pub(crate) fn new(
        patch: ScenePatch,
        patch_bytes: Vec<u8>,
        metadata: ReplayEntryMetadata,
    ) -> Self {
        let mut entry = Self {
            sequence: metadata.sequence,
            patch,
            patch_bytes,
            estimated_visible_frame: metadata.estimated_visible_frame,
            previous_revision: metadata.previous_revision,
            new_revision: metadata.new_revision,
            previous_scene_hash: metadata.previous_scene_hash,
            new_scene_hash: metadata.new_scene_hash,
            previous_entry_hash: metadata.previous_entry_hash,
            entry_hash: ReplayEntryHash::ZERO,
        };
        entry.entry_hash = entry.compute_hash();
        entry
    }

    /// Returns the one-based append sequence.
    #[must_use]
    pub const fn sequence(&self) -> u64 {
        self.sequence
    }

    /// Returns the decoded canonical patch.
    #[must_use]
    pub const fn patch(&self) -> &ScenePatch {
        &self.patch
    }

    /// Returns the recorded render-visible frame estimate.
    #[must_use]
    pub const fn estimated_visible_frame(&self) -> FrameId {
        self.estimated_visible_frame
    }

    /// Returns the scene revision before the patch.
    #[must_use]
    pub const fn previous_revision(&self) -> SceneRevision {
        self.previous_revision
    }

    /// Returns the scene revision after the patch.
    #[must_use]
    pub const fn new_revision(&self) -> SceneRevision {
        self.new_revision
    }

    /// Returns the canonical logical hash before the patch.
    #[must_use]
    pub const fn previous_scene_hash(&self) -> LogicalSceneHash {
        self.previous_scene_hash
    }

    /// Returns the canonical logical hash after the patch.
    #[must_use]
    pub const fn new_scene_hash(&self) -> LogicalSceneHash {
        self.new_scene_hash
    }

    /// Returns the preceding replay entry digest.
    #[must_use]
    pub const fn previous_entry_hash(&self) -> ReplayEntryHash {
        self.previous_entry_hash
    }

    /// Returns this entry's canonical digest.
    #[must_use]
    pub const fn entry_hash(&self) -> ReplayEntryHash {
        self.entry_hash
    }

    pub(crate) fn encoded_bytes(&self) -> u64 {
        ENTRY_ENVELOPE_BYTES + self.patch_bytes.len() as u64
    }

    fn compute_hash(&self) -> ReplayEntryHash {
        let mut hasher = Sha256::new();
        hasher.update(ENTRY_HASH_DOMAIN);
        self.update_hash_fields(&mut hasher);
        ReplayEntryHash(hasher.finalize().into())
    }

    fn update_hash_fields(&self, hasher: &mut Sha256) {
        hasher.update(ENTRY_FORMAT_VERSION.to_be_bytes());
        hasher.update(self.sequence.to_be_bytes());
        hasher.update(self.estimated_visible_frame.get().to_be_bytes());
        hasher.update(self.previous_revision.get().to_be_bytes());
        hasher.update(self.new_revision.get().to_be_bytes());
        hasher.update(self.previous_scene_hash.as_bytes());
        hasher.update(self.new_scene_hash.as_bytes());
        hasher.update(self.previous_entry_hash.as_bytes());
        hasher.update(
            u32::try_from(self.patch_bytes.len())
                .expect("protocol encoded byte bound fits u32")
                .to_be_bytes(),
        );
        hasher.update(&self.patch_bytes);
    }

    fn encode_into(&self, encoded: &mut Vec<u8>) {
        let body_len = FIXED_ENTRY_BODY_BYTES + self.patch_bytes.len() as u64;
        encoded.extend_from_slice(
            &u32::try_from(body_len)
                .expect("admitted replay entry length fits u32")
                .to_be_bytes(),
        );
        encoded.extend_from_slice(&ENTRY_FORMAT_VERSION.to_be_bytes());
        encoded.extend_from_slice(&self.sequence.to_be_bytes());
        encoded.extend_from_slice(&self.estimated_visible_frame.get().to_be_bytes());
        encoded.extend_from_slice(&self.previous_revision.get().to_be_bytes());
        encoded.extend_from_slice(&self.new_revision.get().to_be_bytes());
        encoded.extend_from_slice(self.previous_scene_hash.as_bytes());
        encoded.extend_from_slice(self.new_scene_hash.as_bytes());
        encoded.extend_from_slice(self.previous_entry_hash.as_bytes());
        encoded.extend_from_slice(
            &u32::try_from(self.patch_bytes.len())
                .expect("protocol encoded byte bound fits u32")
                .to_be_bytes(),
        );
        encoded.extend_from_slice(&self.patch_bytes);
        encoded.extend_from_slice(self.entry_hash.as_bytes());
    }
}

/// Summary produced after verifying every entry in a complete log.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReplayVerification {
    entry_count: u32,
    final_revision: SceneRevision,
    final_scene_hash: Option<LogicalSceneHash>,
    final_entry_hash: ReplayEntryHash,
}

impl ReplayVerification {
    /// Returns the number of verified entries.
    #[must_use]
    pub const fn entry_count(self) -> u32 {
        self.entry_count
    }

    /// Returns the final recorded scene revision.
    #[must_use]
    pub const fn final_revision(self) -> SceneRevision {
        self.final_revision
    }

    /// Returns the final logical hash, or `None` for an empty log.
    #[must_use]
    pub const fn final_scene_hash(self) -> Option<LogicalSceneHash> {
        self.final_scene_hash
    }

    /// Returns the final entry hash, or the zero predecessor for an empty log.
    #[must_use]
    pub const fn final_entry_hash(self) -> ReplayEntryHash {
        self.final_entry_hash
    }
}

/// Append-only canonical replay entries with bounded encoded size.
#[derive(Debug, Clone, PartialEq)]
pub struct ReplayLog {
    config: ReplayConfig,
    entries: Vec<ReplayEntry>,
    encoded_bytes: u64,
}

impl ReplayLog {
    pub(crate) fn new(config: ReplayConfig) -> Self {
        Self {
            config,
            entries: Vec::new(),
            encoded_bytes: REPLAY_HEADER.len() as u64,
        }
    }

    /// Returns entries in immutable append order.
    #[must_use]
    pub fn entries(&self) -> &[ReplayEntry] {
        &self.entries
    }

    /// Returns the number of recorded entries.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Reports whether the log contains no entries.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Returns the exact encoded byte count including the stream header.
    #[must_use]
    pub const fn encoded_len(&self) -> u64 {
        self.encoded_bytes
    }

    /// Verifies sequence, revision, predecessor, scene, and entry-hash chains.
    pub fn verify(&self) -> Result<ReplayVerification, ReplayIntegrityError> {
        let mut previous_entry_hash = ReplayEntryHash::ZERO;
        let mut previous_scene_hash = None;
        let mut previous_revision = SceneRevision::INITIAL;
        for (index, entry) in self.entries.iter().enumerate() {
            let entry_index = u32::try_from(index).expect("replay entry bound fits u32");
            let expected_sequence = u64::from(entry_index) + 1;
            if entry.sequence != expected_sequence {
                return Err(ReplayIntegrityError::new(
                    entry_index,
                    ReplayIntegrityErrorKind::SequenceGap,
                ));
            }
            if entry.previous_revision != previous_revision
                || entry.previous_revision.checked_next().ok() != Some(entry.new_revision)
            {
                return Err(ReplayIntegrityError::new(
                    entry_index,
                    ReplayIntegrityErrorKind::RevisionGap,
                ));
            }
            if entry.previous_entry_hash != previous_entry_hash {
                return Err(ReplayIntegrityError::new(
                    entry_index,
                    ReplayIntegrityErrorKind::PreviousEntryHashMismatch,
                ));
            }
            if previous_scene_hash.is_some_and(|hash| hash != entry.previous_scene_hash) {
                return Err(ReplayIntegrityError::new(
                    entry_index,
                    ReplayIntegrityErrorKind::PreviousSceneHashMismatch,
                ));
            }
            if entry.entry_hash != entry.compute_hash() {
                return Err(ReplayIntegrityError::new(
                    entry_index,
                    ReplayIntegrityErrorKind::EntryHashMismatch,
                ));
            }
            previous_entry_hash = entry.entry_hash;
            previous_scene_hash = Some(entry.new_scene_hash);
            previous_revision = entry.new_revision;
        }
        Ok(ReplayVerification {
            entry_count: u32::try_from(self.entries.len()).expect("replay entry bound fits u32"),
            final_revision: previous_revision,
            final_scene_hash: previous_scene_hash,
            final_entry_hash: previous_entry_hash,
        })
    }

    /// Encodes the verified append-only log into its version-one byte stream.
    #[must_use]
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut encoded = Vec::with_capacity(
            usize::try_from(self.encoded_bytes).expect("configured log byte bound fits usize"),
        );
        encoded.extend_from_slice(REPLAY_HEADER);
        for entry in &self.entries {
            entry.encode_into(&mut encoded);
        }
        encoded
    }

    /// Encodes the complete retained prefix ending at an exact scene revision.
    ///
    /// Revision zero produces the mandatory header with no entries. Retained
    /// revisions are contiguous, so the revision also identifies the number of
    /// entries in the returned independently valid replay stream.
    pub fn to_bytes_through_revision(
        &self,
        revision: SceneRevision,
    ) -> Result<Vec<u8>, ReplayRevisionError> {
        let latest = self
            .entries
            .last()
            .map_or(SceneRevision::INITIAL, ReplayEntry::new_revision);
        if revision > latest {
            return Err(ReplayRevisionError::new(revision, latest));
        }

        let entry_count = usize::try_from(revision.get())
            .expect("a retained revision count is bounded by ReplayConfig");
        let entries = &self.entries[..entry_count];
        let encoded_bytes = entries
            .iter()
            .fold(REPLAY_HEADER.len() as u64, |total, entry| {
                total
                    .checked_add(entry.encoded_bytes())
                    .expect("retained entries fit the configured replay byte bound")
            });
        let mut encoded = Vec::with_capacity(
            usize::try_from(encoded_bytes).expect("configured log byte bound fits usize"),
        );
        encoded.extend_from_slice(REPLAY_HEADER);
        for entry in entries {
            entry.encode_into(&mut encoded);
        }
        Ok(encoded)
    }

    /// Loads and verifies the longest valid prefix of a bounded byte stream.
    #[must_use]
    pub fn load_prefix(
        encoded: &[u8],
        config: ReplayConfig,
        runtime_limits: &RuntimeLimits,
    ) -> ReplayLoad {
        let mut log = Self::new(config);
        if let Err(error) = config.validate() {
            return ReplayLoad::with_tail(log, 0, ReplayTailErrorKind::InvalidConfig(error));
        }
        if encoded.len() as u64 > u64::from(config.max_log_bytes.get()) {
            return ReplayLoad::with_tail(log, 0, ReplayTailErrorKind::LogSizeExceeded);
        }
        if encoded.get(..REPLAY_HEADER.len()) != Some(REPLAY_HEADER.as_slice()) {
            return ReplayLoad::with_tail(log, 0, ReplayTailErrorKind::InvalidHeader);
        }

        let mut offset = REPLAY_HEADER.len();
        while offset < encoded.len() {
            let frame_offset = offset;
            let Some(length_bytes) = encoded.get(offset..offset + 4) else {
                return ReplayLoad::with_tail(log, frame_offset, ReplayTailErrorKind::Truncated);
            };
            let body_len = u32::from_be_bytes(
                length_bytes
                    .try_into()
                    .expect("four-byte length slice has exact size"),
            );
            let framed_len = u64::from(body_len) + FRAME_LENGTH_BYTES;
            if framed_len > u64::from(config.max_entry_bytes.get()) {
                return ReplayLoad::with_tail(
                    log,
                    frame_offset,
                    ReplayTailErrorKind::EntrySizeExceeded,
                );
            }
            if log.entries.len()
                >= usize::try_from(config.max_entries.get()).expect("u32 fits usize")
            {
                return ReplayLoad::with_tail(
                    log,
                    frame_offset,
                    ReplayTailErrorKind::EntryCapacityExceeded,
                );
            }
            offset += 4;
            let body_end = match offset.checked_add(body_len as usize) {
                Some(end) if end <= encoded.len() => end,
                _ => {
                    return ReplayLoad::with_tail(
                        log,
                        frame_offset,
                        ReplayTailErrorKind::Truncated,
                    );
                }
            };
            let entry = match decode_entry(&encoded[offset..body_end], runtime_limits) {
                Ok(entry) => entry,
                Err(kind) => return ReplayLoad::with_tail(log, frame_offset, kind),
            };
            if let Err(error) = log.verify_next(&entry) {
                return ReplayLoad::with_tail(
                    log,
                    frame_offset,
                    ReplayTailErrorKind::Integrity(error.kind()),
                );
            }
            log.entries.push(entry);
            log.encoded_bytes = log.encoded_bytes.saturating_add(framed_len);
            offset = body_end;
        }
        ReplayLoad { log, tail: None }
    }

    /// Replays every verified entry into a fresh authoritative world.
    pub fn replay(&self, world_config: WorldConfig) -> Result<AuthoritativeWorld, ReplayError> {
        self.verify().map_err(ReplayError::Integrity)?;
        let mut world = AuthoritativeWorld::new(world_config);
        for (index, entry) in self.entries.iter().enumerate() {
            let entry_index = u32::try_from(index).expect("replay entry bound fits u32");
            let previous_hash = world
                .logical_hash()
                .map_err(|source| ReplayError::Invariant {
                    entry_index,
                    source,
                })?;
            if world.revision() != entry.previous_revision
                || previous_hash != entry.previous_scene_hash
            {
                return Err(ReplayError::SceneHashMismatch { entry_index });
            }
            let receipt = world
                .apply_patch(&entry.patch, entry.estimated_visible_frame)
                .map_err(|source| ReplayError::World {
                    entry_index,
                    source,
                })?;
            if receipt.status != ApplyStatus::Applied
                || receipt.previous_revision != entry.previous_revision
                || receipt.new_revision != entry.new_revision
            {
                return Err(ReplayError::RevisionMismatch { entry_index });
            }
            let new_hash = world
                .logical_hash()
                .map_err(|source| ReplayError::Invariant {
                    entry_index,
                    source,
                })?;
            if new_hash != entry.new_scene_hash {
                return Err(ReplayError::SceneHashMismatch { entry_index });
            }
        }
        Ok(world)
    }

    pub(crate) fn config(&self) -> ReplayConfig {
        self.config
    }

    fn verify_next(&self, entry: &ReplayEntry) -> Result<(), ReplayIntegrityError> {
        let entry_index = u32::try_from(self.entries.len()).expect("replay entry bound fits u32");
        if entry.sequence != u64::from(entry_index) + 1 {
            return Err(ReplayIntegrityError::new(
                entry_index,
                ReplayIntegrityErrorKind::SequenceGap,
            ));
        }
        let previous_revision = self
            .entries
            .last()
            .map_or(SceneRevision::INITIAL, ReplayEntry::new_revision);
        if entry.previous_revision != previous_revision
            || entry.previous_revision.checked_next().ok() != Some(entry.new_revision)
        {
            return Err(ReplayIntegrityError::new(
                entry_index,
                ReplayIntegrityErrorKind::RevisionGap,
            ));
        }
        if entry.previous_entry_hash != self.last_entry_hash() {
            return Err(ReplayIntegrityError::new(
                entry_index,
                ReplayIntegrityErrorKind::PreviousEntryHashMismatch,
            ));
        }
        if self
            .entries
            .last()
            .is_some_and(|previous| previous.new_scene_hash != entry.previous_scene_hash)
        {
            return Err(ReplayIntegrityError::new(
                entry_index,
                ReplayIntegrityErrorKind::PreviousSceneHashMismatch,
            ));
        }
        if entry.entry_hash != entry.compute_hash() {
            return Err(ReplayIntegrityError::new(
                entry_index,
                ReplayIntegrityErrorKind::EntryHashMismatch,
            ));
        }
        Ok(())
    }

    pub(crate) fn last_entry_hash(&self) -> ReplayEntryHash {
        self.entries
            .last()
            .map_or(ReplayEntryHash::ZERO, ReplayEntry::entry_hash)
    }

    pub(crate) fn append(&mut self, entry: ReplayEntry) {
        self.encoded_bytes = self.encoded_bytes.saturating_add(entry.encoded_bytes());
        self.entries.push(entry);
    }
}

/// Result of loading a replay stream, including any unverified tail.
#[derive(Debug, Clone, PartialEq)]
pub struct ReplayLoad {
    log: ReplayLog,
    tail: Option<ReplayTailError>,
}

impl ReplayLoad {
    fn with_tail(log: ReplayLog, offset: usize, kind: ReplayTailErrorKind) -> Self {
        let verified_entries =
            u32::try_from(log.len()).expect("configured replay entry bound fits u32");
        Self {
            log,
            tail: Some(ReplayTailError::new(verified_entries, offset as u64, kind)),
        }
    }

    /// Returns the complete verified prefix.
    #[must_use]
    pub const fn log(&self) -> &ReplayLog {
        &self.log
    }

    /// Returns the first tail failure, if the whole stream was not valid.
    #[must_use]
    pub const fn tail_error(&self) -> Option<&ReplayTailError> {
        self.tail.as_ref()
    }

    /// Splits the verified prefix from its optional tail diagnostic.
    #[must_use]
    pub fn into_parts(self) -> (ReplayLog, Option<ReplayTailError>) {
        (self.log, self.tail)
    }
}

fn decode_entry(
    encoded: &[u8],
    runtime_limits: &RuntimeLimits,
) -> Result<ReplayEntry, ReplayTailErrorKind> {
    if (encoded.len() as u64) < FIXED_ENTRY_BODY_BYTES {
        return Err(ReplayTailErrorKind::MalformedEntry);
    }
    let mut reader = EntryReader::new(encoded);
    if reader.read_u16()? != ENTRY_FORMAT_VERSION {
        return Err(ReplayTailErrorKind::MalformedEntry);
    }
    let sequence = reader.read_u64()?;
    let estimated_visible_frame =
        FrameId::new(reader.read_u64()?).map_err(|_| ReplayTailErrorKind::MalformedEntry)?;
    let previous_revision = SceneRevision::new(reader.read_u64()?);
    let new_revision = SceneRevision::new(reader.read_u64()?);
    let previous_scene_hash = LogicalSceneHash::from_bytes(reader.read_array()?);
    let new_scene_hash = LogicalSceneHash::from_bytes(reader.read_array()?);
    let previous_entry_hash = ReplayEntryHash::from_bytes(reader.read_array()?);
    let patch_len = reader.read_u32()? as usize;
    if reader.remaining() != patch_len.saturating_add(32) {
        return Err(ReplayTailErrorKind::MalformedEntry);
    }
    let patch_bytes = reader.read_bytes(patch_len)?.to_vec();
    let entry_hash = ReplayEntryHash::from_bytes(reader.read_array()?);
    let patch = ScenePatch::from_json(&patch_bytes, runtime_limits)
        .map_err(ReplayTailErrorKind::InvalidPatch)?;
    let canonical = patch
        .to_canonical_json(runtime_limits)
        .map_err(ReplayTailErrorKind::InvalidPatch)?;
    if canonical != patch_bytes {
        return Err(ReplayTailErrorKind::NonCanonicalPatch);
    }
    Ok(ReplayEntry {
        sequence,
        patch,
        patch_bytes,
        estimated_visible_frame,
        previous_revision,
        new_revision,
        previous_scene_hash,
        new_scene_hash,
        previous_entry_hash,
        entry_hash,
    })
}

struct EntryReader<'a> {
    encoded: &'a [u8],
    position: usize,
}

impl<'a> EntryReader<'a> {
    const fn new(encoded: &'a [u8]) -> Self {
        Self {
            encoded,
            position: 0,
        }
    }

    fn remaining(&self) -> usize {
        self.encoded.len().saturating_sub(self.position)
    }

    fn read_bytes(&mut self, length: usize) -> Result<&'a [u8], ReplayTailErrorKind> {
        let end = self
            .position
            .checked_add(length)
            .ok_or(ReplayTailErrorKind::MalformedEntry)?;
        let value = self
            .encoded
            .get(self.position..end)
            .ok_or(ReplayTailErrorKind::MalformedEntry)?;
        self.position = end;
        Ok(value)
    }

    fn read_array<const N: usize>(&mut self) -> Result<[u8; N], ReplayTailErrorKind> {
        self.read_bytes(N)?
            .try_into()
            .map_err(|_| ReplayTailErrorKind::MalformedEntry)
    }

    fn read_u16(&mut self) -> Result<u16, ReplayTailErrorKind> {
        self.read_array().map(u16::from_be_bytes)
    }

    fn read_u32(&mut self) -> Result<u32, ReplayTailErrorKind> {
        self.read_array().map(u32::from_be_bytes)
    }

    fn read_u64(&mut self) -> Result<u64, ReplayTailErrorKind> {
        self.read_array().map(u64::from_be_bytes)
    }
}
