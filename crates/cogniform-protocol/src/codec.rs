use serde::{Serialize, de::DeserializeOwned};

use crate::{CodecError, JsonErrorCategory, RuntimeLimits, ValidationError};

pub(crate) trait Validate {
    fn validate(&self, limits: &RuntimeLimits) -> Result<(), ValidationError>;
}

pub(crate) fn encode<T>(value: &T, limits: &RuntimeLimits) -> Result<Vec<u8>, CodecError>
where
    T: Serialize + Validate,
{
    value.validate(limits).map_err(CodecError::InvalidMessage)?;

    let mut encoded = serde_json::to_vec(value).map_err(|_| CodecError::SerializationFailed)?;
    encoded.push(b'\n');
    enforce_encoded_size(encoded.len(), limits)?;
    Ok(encoded)
}

pub(crate) fn decode<T>(encoded: &[u8], limits: &RuntimeLimits) -> Result<T, CodecError>
where
    T: DeserializeOwned + Validate,
{
    enforce_encoded_size(encoded.len(), limits)?;
    enforce_nesting(encoded, limits)?;

    let value = serde_json::from_slice(encoded).map_err(|error| CodecError::MalformedJson {
        category: match error.classify() {
            serde_json::error::Category::Io => JsonErrorCategory::Io,
            serde_json::error::Category::Syntax => JsonErrorCategory::Syntax,
            serde_json::error::Category::Data => JsonErrorCategory::Data,
            serde_json::error::Category::Eof => JsonErrorCategory::EndOfFile,
        },
        line: error.line(),
        column: error.column(),
    })?;

    T::validate(&value, limits).map_err(CodecError::InvalidMessage)?;
    Ok(value)
}

fn enforce_encoded_size(encoded_len: usize, limits: &RuntimeLimits) -> Result<(), CodecError> {
    let actual = u64::try_from(encoded_len).unwrap_or(u64::MAX);
    let limit = limits.max_encoded_bytes.get();
    if actual > limit {
        return Err(CodecError::EncodedSizeExceeded { actual, limit });
    }
    Ok(())
}

fn enforce_nesting(encoded: &[u8], limits: &RuntimeLimits) -> Result<(), CodecError> {
    let limit = limits.max_json_nesting_depth.get();
    let mut depth = 0_u16;
    let mut in_string = false;
    let mut escaped = false;

    for byte in encoded {
        if in_string {
            if escaped {
                escaped = false;
            } else if *byte == b'\\' {
                escaped = true;
            } else if *byte == b'"' {
                in_string = false;
            }
            continue;
        }

        match *byte {
            b'"' => in_string = true,
            b'{' | b'[' => {
                depth = depth.saturating_add(1);
                if depth > limit {
                    return Err(CodecError::NestingLimitExceeded {
                        actual: depth,
                        limit,
                    });
                }
            }
            b'}' | b']' => depth = depth.saturating_sub(1),
            _ => {}
        }
    }

    Ok(())
}
