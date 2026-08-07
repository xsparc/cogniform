use core::mem;

use cogniform_protocol::{ObservationKind, ObservationMetadata, RuntimeLimits, StableEntityId};
use sha2::{Digest, Sha256};

use crate::{
    EntityVisibility, ObservationEnvelopeError, ObservationPayload, ObservationPayloadLimits,
};

const MAGIC: [u8; 8] = *b"COGOBS01";
const MAGIC_BYTES: usize = MAGIC.len();
const VERSION_BYTES: usize = mem::size_of::<u16>();
const KIND_BYTES: usize = mem::size_of::<u8>();
const RESERVED_BYTES: usize = mem::size_of::<u8>();
const COUNT_BYTES: usize = mem::size_of::<u64>();
const LENGTH_BYTES: usize = mem::size_of::<u64>();
const DIGEST_BYTES: usize = 32;
const VERSION_OFFSET: usize = MAGIC_BYTES;
const KIND_OFFSET: usize = VERSION_OFFSET + VERSION_BYTES;
const RESERVED_OFFSET: usize = KIND_OFFSET + KIND_BYTES;
const COUNT_OFFSET: usize = RESERVED_OFFSET + RESERVED_BYTES;
const LENGTH_OFFSET: usize = COUNT_OFFSET + COUNT_BYTES;
const DIGEST_OFFSET: usize = LENGTH_OFFSET + LENGTH_BYTES;
/// Maximum accepted deviation of an encoded normal's squared length from one.
pub const OBSERVATION_NORMAL_LENGTH_SQUARED_TOLERANCE: f32 = 1.0e-3;

/// Current binary observation-payload envelope version.
pub const OBSERVATION_PAYLOAD_ENVELOPE_VERSION: u16 = 1;

/// Fixed bytes before the first version-one payload byte.
pub const OBSERVATION_PAYLOAD_ENVELOPE_HEADER_BYTES: usize = DIGEST_OFFSET + DIGEST_BYTES;

/// Encodes one validated payload into the deterministic version-one envelope.
pub fn encode_payload(
    metadata: &ObservationMetadata,
    payload: &ObservationPayload,
    runtime_limits: &RuntimeLimits,
    payload_limits: ObservationPayloadLimits,
) -> Result<Vec<u8>, ObservationEnvelopeError> {
    let metadata_json = canonical_metadata(metadata, runtime_limits)?;
    let kind = payload.kind();
    if metadata.kind != kind {
        return Err(ObservationEnvelopeError::KindMismatch {
            metadata: metadata.kind,
            payload: kind,
        });
    }

    let item_count = payload.item_count();
    validate_count(metadata, kind, item_count, runtime_limits, payload_limits)?;
    validate_values(payload, runtime_limits)?;

    let payload_bytes = encoded_payload_bytes(kind, item_count)?;
    let envelope_bytes = total_envelope_bytes(payload_bytes)?;
    enforce_envelope_limit(envelope_bytes, payload_limits)?;

    let capacity =
        usize::try_from(envelope_bytes).map_err(|_| ObservationEnvelopeError::SizeOverflow)?;
    let mut encoded = Vec::new();
    encoded
        .try_reserve_exact(capacity)
        .map_err(|_| ObservationEnvelopeError::AllocationFailed)?;
    append_header_prefix(&mut encoded, kind, item_count, payload_bytes);
    encoded.extend_from_slice(&[0_u8; DIGEST_BYTES]);
    append_payload(&mut encoded, payload);

    let digest = integrity_digest(
        &encoded[..DIGEST_OFFSET],
        &metadata_json,
        &encoded[OBSERVATION_PAYLOAD_ENVELOPE_HEADER_BYTES..],
    );
    encoded[DIGEST_OFFSET..OBSERVATION_PAYLOAD_ENVELOPE_HEADER_BYTES].copy_from_slice(&digest);
    Ok(encoded)
}

/// Decodes one exact version-one envelope after bounds and integrity checks.
pub fn decode_payload(
    metadata: &ObservationMetadata,
    encoded: &[u8],
    runtime_limits: &RuntimeLimits,
    payload_limits: ObservationPayloadLimits,
) -> Result<ObservationPayload, ObservationEnvelopeError> {
    let actual_bytes = u64::try_from(encoded.len()).unwrap_or(u64::MAX);
    enforce_envelope_limit(actual_bytes, payload_limits)?;
    let minimum =
        u64::try_from(OBSERVATION_PAYLOAD_ENVELOPE_HEADER_BYTES).expect("header size fits u64");
    if actual_bytes < minimum {
        return Err(ObservationEnvelopeError::Truncated {
            expected: minimum,
            actual: actual_bytes,
        });
    }
    if encoded[..MAGIC_BYTES] != MAGIC {
        return Err(ObservationEnvelopeError::InvalidMagic);
    }

    let version = read_u16(encoded, VERSION_OFFSET);
    if version != OBSERVATION_PAYLOAD_ENVELOPE_VERSION {
        return Err(ObservationEnvelopeError::UnsupportedVersion { found: version });
    }
    let kind = decode_kind(encoded[KIND_OFFSET])?;
    if encoded[RESERVED_OFFSET] != 0 {
        return Err(ObservationEnvelopeError::NonCanonicalEncoding);
    }

    let item_count = read_u64(encoded, COUNT_OFFSET);
    let payload_bytes = read_u64(encoded, LENGTH_OFFSET);
    let expected_total = total_envelope_bytes(payload_bytes)?;
    if actual_bytes < expected_total {
        return Err(ObservationEnvelopeError::Truncated {
            expected: expected_total,
            actual: actual_bytes,
        });
    }
    if actual_bytes > expected_total {
        return Err(ObservationEnvelopeError::TrailingBytes {
            expected: expected_total,
            actual: actual_bytes,
        });
    }

    let required_payload_bytes = encoded_payload_bytes(kind, item_count)?;
    if payload_bytes != required_payload_bytes {
        return Err(ObservationEnvelopeError::PayloadLengthMismatch {
            expected: required_payload_bytes,
            actual: payload_bytes,
        });
    }

    let metadata_json = canonical_metadata(metadata, runtime_limits)?;
    if metadata.kind != kind {
        return Err(ObservationEnvelopeError::KindMismatch {
            metadata: metadata.kind,
            payload: kind,
        });
    }
    validate_count(metadata, kind, item_count, runtime_limits, payload_limits)?;

    let payload = &encoded[OBSERVATION_PAYLOAD_ENVELOPE_HEADER_BYTES..];
    let digest = integrity_digest(&encoded[..DIGEST_OFFSET], &metadata_json, payload);
    if digest[..] != encoded[DIGEST_OFFSET..OBSERVATION_PAYLOAD_ENVELOPE_HEADER_BYTES] {
        return Err(ObservationEnvelopeError::IntegrityMismatch);
    }

    let decoded = decode_values(kind, item_count, payload, runtime_limits)?;
    validate_values(&decoded, runtime_limits)?;
    Ok(decoded)
}

fn canonical_metadata(
    metadata: &ObservationMetadata,
    runtime_limits: &RuntimeLimits,
) -> Result<Vec<u8>, ObservationEnvelopeError> {
    metadata
        .to_canonical_json(runtime_limits)
        .map_err(|_| ObservationEnvelopeError::InvalidMetadata)
}

fn validate_count(
    metadata: &ObservationMetadata,
    kind: ObservationKind,
    item_count: u64,
    runtime_limits: &RuntimeLimits,
    payload_limits: ObservationPayloadLimits,
) -> Result<(), ObservationEnvelopeError> {
    if kind == ObservationKind::Visibility {
        let limit = payload_limits.max_visibility_entries.get();
        if item_count > u64::from(limit) {
            return Err(ObservationEnvelopeError::VisibilityEntryLimitExceeded {
                actual: item_count,
                limit,
            });
        }
        return Ok(());
    }

    let expected = metadata
        .dimensions
        .ok_or(ObservationEnvelopeError::InvalidMetadata)?
        .pixel_count();
    if item_count != expected {
        return Err(ObservationEnvelopeError::ItemCountMismatch {
            expected,
            actual: item_count,
        });
    }
    if item_count > runtime_limits.max_observation_pixels.get() {
        return Err(ObservationEnvelopeError::ItemCountMismatch {
            expected: runtime_limits.max_observation_pixels.get(),
            actual: item_count,
        });
    }
    Ok(())
}

fn validate_values(
    payload: &ObservationPayload,
    runtime_limits: &RuntimeLimits,
) -> Result<(), ObservationEnvelopeError> {
    match payload {
        ObservationPayload::Color(_) | ObservationPayload::EntityId(_) => Ok(()),
        ObservationPayload::Depth(values) => {
            for (index, value) in values.iter().copied().enumerate() {
                if !canonical_f32(value) || !(0.0..=1.0).contains(&value) {
                    return Err(invalid_value(ObservationKind::Depth, index));
                }
            }
            Ok(())
        }
        ObservationPayload::Normal(values) => {
            for (index, value) in values.iter().copied().enumerate() {
                if let Some([x, y, z]) = value {
                    if !canonical_f32(x) || !canonical_f32(y) || !canonical_f32(z) {
                        return Err(invalid_value(ObservationKind::Normal, index));
                    }
                    let length_squared = x.mul_add(x, y.mul_add(y, z * z));
                    if !length_squared.is_finite()
                        || (length_squared - 1.0).abs()
                            > OBSERVATION_NORMAL_LENGTH_SQUARED_TOLERANCE
                    {
                        return Err(invalid_value(ObservationKind::Normal, index));
                    }
                }
            }
            Ok(())
        }
        ObservationPayload::Visibility(values) => {
            let mut previous = None;
            let mut visible_pixels = 0_u64;
            for (index, value) in values.iter().enumerate() {
                if value.visible_pixels == 0
                    || previous.is_some_and(|entity_id| entity_id >= value.entity_id)
                {
                    return Err(invalid_value(ObservationKind::Visibility, index));
                }
                previous = Some(value.entity_id);
                visible_pixels = visible_pixels
                    .checked_add(value.visible_pixels)
                    .ok_or(ObservationEnvelopeError::SizeOverflow)?;
            }
            let limit = runtime_limits.max_observation_pixels.get();
            if visible_pixels > limit {
                return Err(ObservationEnvelopeError::VisibilityPixelLimitExceeded {
                    actual: visible_pixels,
                    limit,
                });
            }
            Ok(())
        }
    }
}

fn canonical_f32(value: f32) -> bool {
    value.is_finite() && value.to_bits() != (-0.0_f32).to_bits()
}

fn invalid_value(kind: ObservationKind, index: usize) -> ObservationEnvelopeError {
    ObservationEnvelopeError::InvalidPayloadValue {
        kind,
        index: u64::try_from(index).unwrap_or(u64::MAX),
    }
}

fn encoded_payload_bytes(
    kind: ObservationKind,
    item_count: u64,
) -> Result<u64, ObservationEnvelopeError> {
    item_count
        .checked_mul(item_bytes(kind))
        .ok_or(ObservationEnvelopeError::SizeOverflow)
}

const fn item_bytes(kind: ObservationKind) -> u64 {
    match kind {
        ObservationKind::Color | ObservationKind::Depth => 4,
        ObservationKind::Normal => 13,
        ObservationKind::EntityId => 17,
        ObservationKind::Visibility => 24,
    }
}

fn total_envelope_bytes(payload_bytes: u64) -> Result<u64, ObservationEnvelopeError> {
    u64::try_from(OBSERVATION_PAYLOAD_ENVELOPE_HEADER_BYTES)
        .expect("header size fits u64")
        .checked_add(payload_bytes)
        .ok_or(ObservationEnvelopeError::SizeOverflow)
}

fn enforce_envelope_limit(
    actual: u64,
    limits: ObservationPayloadLimits,
) -> Result<(), ObservationEnvelopeError> {
    let limit = limits.max_envelope_bytes.get();
    if actual > limit {
        return Err(ObservationEnvelopeError::EnvelopeLimitExceeded { actual, limit });
    }
    Ok(())
}

fn append_header_prefix(
    encoded: &mut Vec<u8>,
    kind: ObservationKind,
    item_count: u64,
    payload_bytes: u64,
) {
    encoded.extend_from_slice(&MAGIC);
    encoded.extend_from_slice(&OBSERVATION_PAYLOAD_ENVELOPE_VERSION.to_be_bytes());
    encoded.push(encode_kind(kind));
    encoded.push(0);
    encoded.extend_from_slice(&item_count.to_be_bytes());
    encoded.extend_from_slice(&payload_bytes.to_be_bytes());
}

fn append_payload(encoded: &mut Vec<u8>, payload: &ObservationPayload) {
    match payload {
        ObservationPayload::Color(values) => {
            for value in values {
                encoded.extend_from_slice(value);
            }
        }
        ObservationPayload::Depth(values) => {
            for value in values {
                encoded.extend_from_slice(&value.to_bits().to_be_bytes());
            }
        }
        ObservationPayload::Normal(values) => {
            for value in values {
                match value {
                    None => encoded.extend_from_slice(&[0_u8; 13]),
                    Some(components) => {
                        encoded.push(1);
                        for component in components {
                            encoded.extend_from_slice(&component.to_bits().to_be_bytes());
                        }
                    }
                }
            }
        }
        ObservationPayload::EntityId(values) => {
            for value in values {
                match value {
                    None => encoded.extend_from_slice(&[0_u8; 17]),
                    Some(entity_id) => {
                        encoded.push(1);
                        encoded.extend_from_slice(&entity_id.get().to_be_bytes());
                    }
                }
            }
        }
        ObservationPayload::Visibility(values) => {
            for value in values {
                encoded.extend_from_slice(&value.entity_id.get().to_be_bytes());
                encoded.extend_from_slice(&value.visible_pixels.to_be_bytes());
            }
        }
    }
}

fn decode_values(
    kind: ObservationKind,
    item_count: u64,
    payload: &[u8],
    runtime_limits: &RuntimeLimits,
) -> Result<ObservationPayload, ObservationEnvelopeError> {
    match kind {
        ObservationKind::Color => decode_color(item_count, payload),
        ObservationKind::Depth => decode_depth(item_count, payload),
        ObservationKind::Normal => decode_normals(item_count, payload),
        ObservationKind::EntityId => decode_entity_ids(item_count, payload),
        ObservationKind::Visibility => decode_visibility(item_count, payload, runtime_limits),
    }
}

fn decode_color(
    item_count: u64,
    payload: &[u8],
) -> Result<ObservationPayload, ObservationEnvelopeError> {
    let mut values = reserved_vec(item_count)?;
    for chunk in payload.chunks_exact(4) {
        values.push([chunk[0], chunk[1], chunk[2], chunk[3]]);
    }
    Ok(ObservationPayload::Color(values))
}

fn decode_depth(
    item_count: u64,
    payload: &[u8],
) -> Result<ObservationPayload, ObservationEnvelopeError> {
    let mut values = reserved_vec(item_count)?;
    for (index, chunk) in payload.chunks_exact(4).enumerate() {
        let value = f32::from_bits(u32::from_be_bytes(copy_array(chunk)));
        if !canonical_f32(value) || !(0.0..=1.0).contains(&value) {
            return Err(invalid_value(ObservationKind::Depth, index));
        }
        values.push(value);
    }
    Ok(ObservationPayload::Depth(values))
}

fn decode_normals(
    item_count: u64,
    payload: &[u8],
) -> Result<ObservationPayload, ObservationEnvelopeError> {
    let mut values = reserved_vec(item_count)?;
    for (index, chunk) in payload.chunks_exact(13).enumerate() {
        let value = match chunk[0] {
            0 => {
                if chunk[1..].iter().any(|byte| *byte != 0) {
                    return Err(ObservationEnvelopeError::NonCanonicalEncoding);
                }
                None
            }
            1 => {
                let x = f32::from_bits(u32::from_be_bytes(copy_array(&chunk[1..5])));
                let y = f32::from_bits(u32::from_be_bytes(copy_array(&chunk[5..9])));
                let z = f32::from_bits(u32::from_be_bytes(copy_array(&chunk[9..13])));
                if !canonical_f32(x) || !canonical_f32(y) || !canonical_f32(z) {
                    return Err(invalid_value(ObservationKind::Normal, index));
                }
                let length_squared = x.mul_add(x, y.mul_add(y, z * z));
                if !length_squared.is_finite()
                    || (length_squared - 1.0).abs() > OBSERVATION_NORMAL_LENGTH_SQUARED_TOLERANCE
                {
                    return Err(invalid_value(ObservationKind::Normal, index));
                }
                Some([x, y, z])
            }
            _ => return Err(ObservationEnvelopeError::NonCanonicalEncoding),
        };
        values.push(value);
    }
    Ok(ObservationPayload::Normal(values))
}

fn decode_entity_ids(
    item_count: u64,
    payload: &[u8],
) -> Result<ObservationPayload, ObservationEnvelopeError> {
    let mut values = reserved_vec(item_count)?;
    for (index, chunk) in payload.chunks_exact(17).enumerate() {
        let value = match chunk[0] {
            0 => {
                if chunk[1..].iter().any(|byte| *byte != 0) {
                    return Err(ObservationEnvelopeError::NonCanonicalEncoding);
                }
                None
            }
            1 => {
                let entity_id = u128::from_be_bytes(copy_array(&chunk[1..17]));
                Some(
                    StableEntityId::new(entity_id)
                        .map_err(|_| invalid_value(ObservationKind::EntityId, index))?,
                )
            }
            _ => return Err(ObservationEnvelopeError::NonCanonicalEncoding),
        };
        values.push(value);
    }
    Ok(ObservationPayload::EntityId(values))
}

fn decode_visibility(
    item_count: u64,
    payload: &[u8],
    runtime_limits: &RuntimeLimits,
) -> Result<ObservationPayload, ObservationEnvelopeError> {
    let mut values = reserved_vec(item_count)?;
    let mut previous = None;
    let mut total_pixels = 0_u64;
    for (index, chunk) in payload.chunks_exact(24).enumerate() {
        let entity_id = StableEntityId::new(u128::from_be_bytes(copy_array(&chunk[..16])))
            .map_err(|_| invalid_value(ObservationKind::Visibility, index))?;
        let visible_pixels = u64::from_be_bytes(copy_array(&chunk[16..24]));
        if visible_pixels == 0 || previous.is_some_and(|prior| prior >= entity_id) {
            return Err(invalid_value(ObservationKind::Visibility, index));
        }
        previous = Some(entity_id);
        total_pixels = total_pixels
            .checked_add(visible_pixels)
            .ok_or(ObservationEnvelopeError::SizeOverflow)?;
        values.push(EntityVisibility {
            entity_id,
            visible_pixels,
        });
    }
    let limit = runtime_limits.max_observation_pixels.get();
    if total_pixels > limit {
        return Err(ObservationEnvelopeError::VisibilityPixelLimitExceeded {
            actual: total_pixels,
            limit,
        });
    }
    Ok(ObservationPayload::Visibility(values))
}

fn reserved_vec<T>(item_count: u64) -> Result<Vec<T>, ObservationEnvelopeError> {
    let capacity =
        usize::try_from(item_count).map_err(|_| ObservationEnvelopeError::SizeOverflow)?;
    let mut values = Vec::new();
    values
        .try_reserve_exact(capacity)
        .map_err(|_| ObservationEnvelopeError::AllocationFailed)?;
    Ok(values)
}

fn integrity_digest(prefix: &[u8], metadata: &[u8], payload: &[u8]) -> [u8; DIGEST_BYTES] {
    let mut hasher = Sha256::new();
    hasher.update(prefix);
    hasher.update(metadata);
    hasher.update(payload);
    hasher.finalize().into()
}

const fn encode_kind(kind: ObservationKind) -> u8 {
    match kind {
        ObservationKind::Color => 1,
        ObservationKind::Depth => 2,
        ObservationKind::Normal => 3,
        ObservationKind::EntityId => 4,
        ObservationKind::Visibility => 5,
    }
}

fn decode_kind(encoded: u8) -> Result<ObservationKind, ObservationEnvelopeError> {
    match encoded {
        1 => Ok(ObservationKind::Color),
        2 => Ok(ObservationKind::Depth),
        3 => Ok(ObservationKind::Normal),
        4 => Ok(ObservationKind::EntityId),
        5 => Ok(ObservationKind::Visibility),
        found => Err(ObservationEnvelopeError::UnsupportedKind { found }),
    }
}

fn read_u16(encoded: &[u8], offset: usize) -> u16 {
    u16::from_be_bytes(copy_array(&encoded[offset..offset + VERSION_BYTES]))
}

fn read_u64(encoded: &[u8], offset: usize) -> u64 {
    u64::from_be_bytes(copy_array(&encoded[offset..offset + COUNT_BYTES]))
}

fn copy_array<const N: usize>(encoded: &[u8]) -> [u8; N] {
    let mut value = [0_u8; N];
    value.copy_from_slice(encoded);
    value
}
