use core::{mem, num::NonZeroU64};
use std::io::{self, Read, Write};

use cogniform_observation::{decode_payload, encode_payload};
use cogniform_protocol::ObservationMetadata;
use sha2::{Digest, Sha256};

use crate::{
    LocalFrame, LocalFrameConfig, LocalFrameError, LocalFrameIoOperation, LocalFrameKind,
    LocalFrameSection,
};

const MAGIC: [u8; 8] = *b"COGLOC01";
const MAGIC_BYTES: usize = MAGIC.len();
const VERSION_BYTES: usize = mem::size_of::<u16>();
const KIND_BYTES: usize = mem::size_of::<u8>();
const RESERVED_BYTES: usize = mem::size_of::<u8>();
const CORRELATION_BYTES: usize = mem::size_of::<u64>();
const LENGTH_BYTES: usize = mem::size_of::<u64>();
const DIGEST_BYTES: usize = 32;

const VERSION_OFFSET: usize = MAGIC_BYTES;
const KIND_OFFSET: usize = VERSION_OFFSET + VERSION_BYTES;
const RESERVED_OFFSET: usize = KIND_OFFSET + KIND_BYTES;
const CORRELATION_OFFSET: usize = RESERVED_OFFSET + RESERVED_BYTES;
const CONTROL_LENGTH_OFFSET: usize = CORRELATION_OFFSET + CORRELATION_BYTES;
const BULK_LENGTH_OFFSET: usize = CONTROL_LENGTH_OFFSET + LENGTH_BYTES;
const DIGEST_OFFSET: usize = BULK_LENGTH_OFFSET + LENGTH_BYTES;

/// Current local stream frame version.
pub const LOCAL_FRAME_VERSION: u16 = 1;

/// Fixed bytes read before any version-one body allocation.
pub const LOCAL_FRAME_HEADER_BYTES: usize = DIGEST_OFFSET + DIGEST_BYTES;

#[derive(Debug, Clone, Copy)]
struct Header {
    kind: LocalFrameKind,
    correlation_id: NonZeroU64,
    control_bytes: u64,
    bulk_bytes: u64,
    frame_bytes: u64,
}

/// Encodes one validated frame completely in memory without performing I/O.
pub fn encode_frame(
    frame: &LocalFrame,
    config: &LocalFrameConfig,
) -> Result<Vec<u8>, LocalFrameError> {
    match frame {
        LocalFrame::Control {
            correlation_id,
            bytes,
        } => encode_sections(LocalFrameKind::Control, *correlation_id, bytes, &[], config),
        LocalFrame::Observation {
            correlation_id,
            metadata,
            payload,
        } => {
            let control = metadata
                .to_canonical_json(&config.runtime_limits)
                .map_err(|_| LocalFrameError::InvalidObservationMetadata)?;
            let bulk = encode_payload(
                metadata,
                payload,
                &config.runtime_limits,
                config.payload_limits,
            )?;
            encode_sections(
                LocalFrameKind::Observation,
                *correlation_id,
                &control,
                &bulk,
                config,
            )
        }
    }
}

/// Decodes one exact borrowed frame and rejects trailing bytes.
pub fn decode_frame(
    encoded: &[u8],
    config: &LocalFrameConfig,
) -> Result<LocalFrame, LocalFrameError> {
    if encoded.len() < LOCAL_FRAME_HEADER_BYTES {
        return Err(LocalFrameError::Truncated {
            section: LocalFrameSection::Header,
            expected: header_bytes_u64(),
            actual: u64::try_from(encoded.len()).unwrap_or(u64::MAX),
        });
    }
    let header_bytes = copy_array(&encoded[..LOCAL_FRAME_HEADER_BYTES]);
    let header = parse_header(&header_bytes, config)?;
    let actual = u64::try_from(encoded.len()).unwrap_or(u64::MAX);
    if actual < header.frame_bytes {
        let available_body = actual.saturating_sub(header_bytes_u64());
        let section = if available_body < header.control_bytes {
            LocalFrameSection::Control
        } else {
            LocalFrameSection::Bulk
        };
        let (expected, found) = match section {
            LocalFrameSection::Control => (header.control_bytes, available_body),
            LocalFrameSection::Bulk => (
                header.bulk_bytes,
                available_body.saturating_sub(header.control_bytes),
            ),
            LocalFrameSection::Header => unreachable!("header length was checked"),
        };
        return Err(LocalFrameError::Truncated {
            section,
            expected,
            actual: found,
        });
    }
    if actual > header.frame_bytes {
        return Err(LocalFrameError::TrailingBytes {
            expected: header.frame_bytes,
            actual,
        });
    }

    let control_end = LOCAL_FRAME_HEADER_BYTES + to_usize(header.control_bytes)?;
    let control = &encoded[LOCAL_FRAME_HEADER_BYTES..control_end];
    let bulk = &encoded[control_end..];
    verify_digest(&header_bytes, control, bulk)?;
    decode_borrowed_body(header, control, bulk, config)
}

/// Reads at most one frame from a caller-owned synchronous byte stream.
///
/// `Ok(None)` means clean end-of-stream before any header byte. End-of-stream
/// after a frame starts is a typed truncation error. The fixed header and all
/// declared limits are validated before body allocation.
pub fn read_frame<R: Read + ?Sized>(
    reader: &mut R,
    config: &LocalFrameConfig,
) -> Result<Option<LocalFrame>, LocalFrameError> {
    let Some(header_bytes) = read_header(reader)? else {
        return Ok(None);
    };
    let header = parse_header(&header_bytes, config)?;
    let mut control = allocate_zeroed(header.control_bytes)?;
    read_section(
        reader,
        &mut control,
        LocalFrameSection::Control,
        LocalFrameIoOperation::ReadControl,
    )?;
    let mut bulk = allocate_zeroed(header.bulk_bytes)?;
    read_section(
        reader,
        &mut bulk,
        LocalFrameSection::Bulk,
        LocalFrameIoOperation::ReadBulk,
    )?;
    verify_digest(&header_bytes, &control, &bulk)?;
    Ok(Some(decode_owned_body(header, control, &bulk, config)?))
}

/// Encodes one frame completely, then writes it to a caller-owned stream.
///
/// Successful validation never implies that the caller's stream can make the
/// eventual write atomic; an I/O failure can occur after a prefix was written.
pub fn write_frame<W: Write + ?Sized>(
    writer: &mut W,
    frame: &LocalFrame,
    config: &LocalFrameConfig,
) -> Result<(), LocalFrameError> {
    let encoded = encode_frame(frame, config)?;
    write_all(writer, &encoded)
}

fn encode_sections(
    kind: LocalFrameKind,
    correlation_id: NonZeroU64,
    control: &[u8],
    bulk: &[u8],
    config: &LocalFrameConfig,
) -> Result<Vec<u8>, LocalFrameError> {
    let control_bytes = u64::try_from(control.len()).unwrap_or(u64::MAX);
    let bulk_bytes = u64::try_from(bulk.len()).unwrap_or(u64::MAX);
    validate_layout(kind, control_bytes, bulk_bytes)?;
    let frame_bytes = validate_lengths(control_bytes, bulk_bytes, config)?;
    let capacity = to_usize(frame_bytes)?;
    let mut encoded = Vec::new();
    encoded
        .try_reserve_exact(capacity)
        .map_err(|_| LocalFrameError::AllocationFailed)?;
    append_header_prefix(
        &mut encoded,
        kind,
        correlation_id,
        control_bytes,
        bulk_bytes,
    );
    encoded.extend_from_slice(&[0_u8; DIGEST_BYTES]);
    encoded.extend_from_slice(control);
    encoded.extend_from_slice(bulk);
    let digest = integrity_digest(&encoded[..DIGEST_OFFSET], control, bulk);
    encoded[DIGEST_OFFSET..LOCAL_FRAME_HEADER_BYTES].copy_from_slice(&digest);
    Ok(encoded)
}

fn parse_header(
    encoded: &[u8; LOCAL_FRAME_HEADER_BYTES],
    config: &LocalFrameConfig,
) -> Result<Header, LocalFrameError> {
    if encoded[..MAGIC_BYTES] != MAGIC {
        return Err(LocalFrameError::InvalidMagic);
    }
    let version = read_u16(encoded, VERSION_OFFSET);
    if version != LOCAL_FRAME_VERSION {
        return Err(LocalFrameError::UnsupportedVersion { found: version });
    }
    let kind = decode_kind(encoded[KIND_OFFSET])?;
    if encoded[RESERVED_OFFSET] != 0 {
        return Err(LocalFrameError::NonCanonicalHeader);
    }
    let correlation_id = NonZeroU64::new(read_u64(encoded, CORRELATION_OFFSET))
        .ok_or(LocalFrameError::InvalidCorrelationId)?;
    let control_bytes = read_u64(encoded, CONTROL_LENGTH_OFFSET);
    let bulk_bytes = read_u64(encoded, BULK_LENGTH_OFFSET);
    validate_layout(kind, control_bytes, bulk_bytes)?;
    let frame_bytes = validate_lengths(control_bytes, bulk_bytes, config)?;
    Ok(Header {
        kind,
        correlation_id,
        control_bytes,
        bulk_bytes,
        frame_bytes,
    })
}

fn validate_layout(
    kind: LocalFrameKind,
    control_bytes: u64,
    bulk_bytes: u64,
) -> Result<(), LocalFrameError> {
    match kind {
        LocalFrameKind::Control if control_bytes == 0 => Err(LocalFrameError::EmptyControl),
        LocalFrameKind::Control if bulk_bytes != 0 => Err(LocalFrameError::InvalidSectionLayout),
        LocalFrameKind::Observation if control_bytes == 0 || bulk_bytes == 0 => {
            Err(LocalFrameError::InvalidSectionLayout)
        }
        _ => Ok(()),
    }
}

fn validate_lengths(
    control_bytes: u64,
    bulk_bytes: u64,
    config: &LocalFrameConfig,
) -> Result<u64, LocalFrameError> {
    let control_limit = config.frame_limits.max_control_bytes.get();
    if control_bytes > control_limit {
        return Err(LocalFrameError::ControlLimitExceeded {
            actual: control_bytes,
            limit: control_limit,
        });
    }
    let bulk_limit = config.frame_limits.max_bulk_bytes.get();
    if bulk_bytes > bulk_limit {
        return Err(LocalFrameError::BulkLimitExceeded {
            actual: bulk_bytes,
            limit: bulk_limit,
        });
    }
    let frame_bytes = header_bytes_u64()
        .checked_add(control_bytes)
        .and_then(|size| size.checked_add(bulk_bytes))
        .ok_or(LocalFrameError::SizeOverflow)?;
    let frame_limit = config.frame_limits.max_frame_bytes.get();
    if frame_bytes > frame_limit {
        return Err(LocalFrameError::FrameLimitExceeded {
            actual: frame_bytes,
            limit: frame_limit,
        });
    }
    Ok(frame_bytes)
}

fn decode_borrowed_body(
    header: Header,
    control: &[u8],
    bulk: &[u8],
    config: &LocalFrameConfig,
) -> Result<LocalFrame, LocalFrameError> {
    match header.kind {
        LocalFrameKind::Control => Ok(LocalFrame::Control {
            correlation_id: header.correlation_id,
            bytes: copy_owned(control)?,
        }),
        LocalFrameKind::Observation => {
            decode_observation(header.correlation_id, control, bulk, config)
        }
    }
}

fn decode_owned_body(
    header: Header,
    control: Vec<u8>,
    bulk: &[u8],
    config: &LocalFrameConfig,
) -> Result<LocalFrame, LocalFrameError> {
    match header.kind {
        LocalFrameKind::Control => Ok(LocalFrame::Control {
            correlation_id: header.correlation_id,
            bytes: control,
        }),
        LocalFrameKind::Observation => {
            decode_observation(header.correlation_id, &control, bulk, config)
        }
    }
}

fn decode_observation(
    correlation_id: NonZeroU64,
    control: &[u8],
    bulk: &[u8],
    config: &LocalFrameConfig,
) -> Result<LocalFrame, LocalFrameError> {
    let metadata = ObservationMetadata::from_json(control, &config.runtime_limits)
        .map_err(|_| LocalFrameError::InvalidObservationMetadata)?;
    let canonical = metadata
        .to_canonical_json(&config.runtime_limits)
        .map_err(|_| LocalFrameError::InvalidObservationMetadata)?;
    if canonical != control {
        return Err(LocalFrameError::InvalidObservationMetadata);
    }
    let payload = decode_payload(
        &metadata,
        bulk,
        &config.runtime_limits,
        config.payload_limits,
    )?;
    Ok(LocalFrame::Observation {
        correlation_id,
        metadata,
        payload,
    })
}

fn verify_digest(
    header: &[u8; LOCAL_FRAME_HEADER_BYTES],
    control: &[u8],
    bulk: &[u8],
) -> Result<(), LocalFrameError> {
    let digest = integrity_digest(&header[..DIGEST_OFFSET], control, bulk);
    if digest[..] != header[DIGEST_OFFSET..] {
        return Err(LocalFrameError::IntegrityMismatch);
    }
    Ok(())
}

fn integrity_digest(prefix: &[u8], control: &[u8], bulk: &[u8]) -> [u8; DIGEST_BYTES] {
    let mut hasher = Sha256::new();
    hasher.update(prefix);
    hasher.update(control);
    hasher.update(bulk);
    hasher.finalize().into()
}

fn append_header_prefix(
    encoded: &mut Vec<u8>,
    kind: LocalFrameKind,
    correlation_id: NonZeroU64,
    control_bytes: u64,
    bulk_bytes: u64,
) {
    encoded.extend_from_slice(&MAGIC);
    encoded.extend_from_slice(&LOCAL_FRAME_VERSION.to_be_bytes());
    encoded.push(encode_kind(kind));
    encoded.push(0);
    encoded.extend_from_slice(&correlation_id.get().to_be_bytes());
    encoded.extend_from_slice(&control_bytes.to_be_bytes());
    encoded.extend_from_slice(&bulk_bytes.to_be_bytes());
}

fn read_header<R: Read + ?Sized>(
    reader: &mut R,
) -> Result<Option<[u8; LOCAL_FRAME_HEADER_BYTES]>, LocalFrameError> {
    let mut header = [0_u8; LOCAL_FRAME_HEADER_BYTES];
    let mut read = 0_usize;
    while read < header.len() {
        match reader.read(&mut header[read..]) {
            Ok(0) if read == 0 => return Ok(None),
            Ok(0) => {
                return Err(LocalFrameError::Truncated {
                    section: LocalFrameSection::Header,
                    expected: header_bytes_u64(),
                    actual: u64::try_from(read).expect("header offset fits u64"),
                });
            }
            Ok(count) => read += count,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
            Err(error) => {
                return Err(LocalFrameError::Io {
                    operation: LocalFrameIoOperation::ReadHeader,
                    kind: error.kind(),
                });
            }
        }
    }
    Ok(Some(header))
}

fn read_section<R: Read + ?Sized>(
    reader: &mut R,
    destination: &mut [u8],
    section: LocalFrameSection,
    operation: LocalFrameIoOperation,
) -> Result<(), LocalFrameError> {
    let mut read = 0_usize;
    while read < destination.len() {
        match reader.read(&mut destination[read..]) {
            Ok(0) => {
                return Err(LocalFrameError::Truncated {
                    section,
                    expected: u64::try_from(destination.len()).unwrap_or(u64::MAX),
                    actual: u64::try_from(read).unwrap_or(u64::MAX),
                });
            }
            Ok(count) => read += count,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
            Err(error) => {
                return Err(LocalFrameError::Io {
                    operation,
                    kind: error.kind(),
                });
            }
        }
    }
    Ok(())
}

fn write_all<W: Write + ?Sized>(writer: &mut W, encoded: &[u8]) -> Result<(), LocalFrameError> {
    let mut written = 0_usize;
    while written < encoded.len() {
        match writer.write(&encoded[written..]) {
            Ok(0) => {
                return Err(LocalFrameError::Io {
                    operation: LocalFrameIoOperation::WriteFrame,
                    kind: io::ErrorKind::WriteZero,
                });
            }
            Ok(count) => written += count,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
            Err(error) => {
                return Err(LocalFrameError::Io {
                    operation: LocalFrameIoOperation::WriteFrame,
                    kind: error.kind(),
                });
            }
        }
    }
    Ok(())
}

fn allocate_zeroed(bytes: u64) -> Result<Vec<u8>, LocalFrameError> {
    let length = to_usize(bytes)?;
    let mut value = Vec::new();
    value
        .try_reserve_exact(length)
        .map_err(|_| LocalFrameError::AllocationFailed)?;
    value.resize(length, 0);
    Ok(value)
}

fn copy_owned(value: &[u8]) -> Result<Vec<u8>, LocalFrameError> {
    let mut owned = Vec::new();
    owned
        .try_reserve_exact(value.len())
        .map_err(|_| LocalFrameError::AllocationFailed)?;
    owned.extend_from_slice(value);
    Ok(owned)
}

const fn encode_kind(kind: LocalFrameKind) -> u8 {
    match kind {
        LocalFrameKind::Control => 1,
        LocalFrameKind::Observation => 2,
    }
}

fn decode_kind(encoded: u8) -> Result<LocalFrameKind, LocalFrameError> {
    match encoded {
        1 => Ok(LocalFrameKind::Control),
        2 => Ok(LocalFrameKind::Observation),
        found => Err(LocalFrameError::UnsupportedKind { found }),
    }
}

fn read_u16(encoded: &[u8], offset: usize) -> u16 {
    u16::from_be_bytes(copy_array(&encoded[offset..offset + VERSION_BYTES]))
}

fn read_u64(encoded: &[u8], offset: usize) -> u64 {
    u64::from_be_bytes(copy_array(&encoded[offset..offset + LENGTH_BYTES]))
}

fn copy_array<const N: usize>(encoded: &[u8]) -> [u8; N] {
    let mut value = [0_u8; N];
    value.copy_from_slice(encoded);
    value
}

fn to_usize(value: u64) -> Result<usize, LocalFrameError> {
    usize::try_from(value).map_err(|_| LocalFrameError::SizeOverflow)
}

fn header_bytes_u64() -> u64 {
    u64::try_from(LOCAL_FRAME_HEADER_BYTES).expect("header size fits u64")
}
