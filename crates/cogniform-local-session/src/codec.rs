use core::num::NonZeroU64;
use std::io::{self, Write};

use cogniform_local_transport::{LOCAL_FRAME_HEADER_BYTES, LocalFrame, LocalFrameConfig};
use cogniform_protocol::{JsonErrorCategory, RuntimeLimits};
use serde::{Serialize, de::DeserializeOwned};

use crate::{
    LocalSessionClientMessage, LocalSessionError, LocalSessionServerMessage, SessionValidate,
};

/// Encodes one validated client message as canonical compact JSON followed by LF.
pub fn encode_client_message(
    message: &LocalSessionClientMessage,
    config: &LocalFrameConfig,
) -> Result<Vec<u8>, LocalSessionError> {
    encode(message, config)
}

/// Decodes one exact canonical client message under pre-allocation bounds.
pub fn decode_client_message(
    encoded: &[u8],
    config: &LocalFrameConfig,
) -> Result<LocalSessionClientMessage, LocalSessionError> {
    preflight(encoded, config)?;
    if serde_json::from_slice::<LocalSessionServerMessage>(encoded).is_ok() {
        return Err(LocalSessionError::WrongDirection);
    }
    decode(encoded, config)
}

/// Encodes one validated server message as canonical compact JSON followed by LF.
pub fn encode_server_message(
    message: &LocalSessionServerMessage,
    config: &LocalFrameConfig,
) -> Result<Vec<u8>, LocalSessionError> {
    encode(message, config)
}

/// Decodes one exact canonical server message under pre-allocation bounds.
pub fn decode_server_message(
    encoded: &[u8],
    config: &LocalFrameConfig,
) -> Result<LocalSessionServerMessage, LocalSessionError> {
    preflight(encoded, config)?;
    if serde_json::from_slice::<LocalSessionClientMessage>(encoded).is_ok() {
        return Err(LocalSessionError::WrongDirection);
    }
    decode(encoded, config)
}

/// Wraps one canonical client message in a CF039 control frame.
pub fn client_control_frame(
    correlation_id: NonZeroU64,
    message: &LocalSessionClientMessage,
    config: &LocalFrameConfig,
) -> Result<LocalFrame, LocalSessionError> {
    Ok(LocalFrame::Control {
        correlation_id,
        bytes: encode_client_message(message, config)?,
    })
}

/// Decodes one CF039 control frame as a client message.
pub fn decode_client_control_frame(
    frame: &LocalFrame,
    config: &LocalFrameConfig,
) -> Result<(NonZeroU64, LocalSessionClientMessage), LocalSessionError> {
    let LocalFrame::Control {
        correlation_id,
        bytes,
    } = frame
    else {
        return Err(LocalSessionError::WrongFrameKind);
    };
    Ok((*correlation_id, decode_client_message(bytes, config)?))
}

/// Wraps one canonical server message in a CF039 control frame.
pub fn server_control_frame(
    correlation_id: NonZeroU64,
    message: &LocalSessionServerMessage,
    config: &LocalFrameConfig,
) -> Result<LocalFrame, LocalSessionError> {
    Ok(LocalFrame::Control {
        correlation_id,
        bytes: encode_server_message(message, config)?,
    })
}

/// Decodes one CF039 control frame as a server message.
pub fn decode_server_control_frame(
    frame: &LocalFrame,
    config: &LocalFrameConfig,
) -> Result<(NonZeroU64, LocalSessionServerMessage), LocalSessionError> {
    let LocalFrame::Control {
        correlation_id,
        bytes,
    } = frame
    else {
        return Err(LocalSessionError::WrongFrameKind);
    };
    Ok((*correlation_id, decode_server_message(bytes, config)?))
}

fn encode<T>(value: &T, config: &LocalFrameConfig) -> Result<Vec<u8>, LocalSessionError>
where
    T: Serialize + SessionValidate,
{
    value
        .validate(config)
        .map_err(LocalSessionError::InvalidMessage)?;
    let limit = effective_message_limit(config)?;
    encode_validated(value, limit)
}

fn decode<T>(encoded: &[u8], config: &LocalFrameConfig) -> Result<T, LocalSessionError>
where
    T: Serialize + DeserializeOwned + SessionValidate,
{
    let limit = preflight(encoded, config)?;
    let value: T =
        serde_json::from_slice(encoded).map_err(|error| LocalSessionError::MalformedJson {
            category: match error.classify() {
                serde_json::error::Category::Io => JsonErrorCategory::Io,
                serde_json::error::Category::Syntax => JsonErrorCategory::Syntax,
                serde_json::error::Category::Data => JsonErrorCategory::Data,
                serde_json::error::Category::Eof => JsonErrorCategory::EndOfFile,
            },
            line: error.line(),
            column: error.column(),
        })?;
    value
        .validate(config)
        .map_err(LocalSessionError::InvalidMessage)?;
    let canonical = encode_validated(&value, limit)?;
    if canonical != encoded {
        return Err(LocalSessionError::NonCanonicalMessage);
    }
    Ok(value)
}

fn preflight(encoded: &[u8], config: &LocalFrameConfig) -> Result<u64, LocalSessionError> {
    let limit = effective_message_limit(config)?;
    enforce_size(encoded.len(), limit)?;
    enforce_nesting(encoded, &config.runtime_limits)?;
    Ok(limit)
}

fn effective_message_limit(config: &LocalFrameConfig) -> Result<u64, LocalSessionError> {
    let header = u64::try_from(LOCAL_FRAME_HEADER_BYTES).unwrap_or(u64::MAX);
    let body = config
        .frame_limits
        .max_frame_bytes
        .get()
        .checked_sub(header)
        .ok_or(LocalSessionError::InvalidConfig)?;
    let limit = body
        .min(config.frame_limits.max_control_bytes.get())
        .min(config.runtime_limits.max_encoded_bytes.get());
    if limit == 0 {
        Err(LocalSessionError::InvalidConfig)
    } else {
        Ok(limit)
    }
}

fn encode_validated<T: Serialize>(value: &T, limit: u64) -> Result<Vec<u8>, LocalSessionError> {
    let payload_limit = limit.saturating_sub(1);
    let mut writer = BoundedWriter::new(payload_limit);
    if serde_json::to_writer(&mut writer, value).is_err() {
        return Err(writer
            .error
            .unwrap_or(LocalSessionError::SerializationFailed));
    }
    writer.push_lf(limit)?;
    Ok(writer.bytes)
}

fn enforce_size(actual: usize, limit: u64) -> Result<(), LocalSessionError> {
    let actual = u64::try_from(actual).unwrap_or(u64::MAX);
    if actual > limit {
        Err(LocalSessionError::MessageLimitExceeded { actual, limit })
    } else {
        Ok(())
    }
}

fn enforce_nesting(encoded: &[u8], limits: &RuntimeLimits) -> Result<(), LocalSessionError> {
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
                    return Err(LocalSessionError::NestingLimitExceeded {
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

struct BoundedWriter {
    bytes: Vec<u8>,
    limit: u64,
    error: Option<LocalSessionError>,
}

impl BoundedWriter {
    const fn new(limit: u64) -> Self {
        Self {
            bytes: Vec::new(),
            limit,
            error: None,
        }
    }

    fn push_lf(&mut self, complete_limit: u64) -> Result<(), LocalSessionError> {
        let next =
            self.bytes
                .len()
                .checked_add(1)
                .ok_or(LocalSessionError::MessageLimitExceeded {
                    actual: u64::MAX,
                    limit: complete_limit,
                })?;
        enforce_size(next, complete_limit)?;
        self.bytes
            .try_reserve(1)
            .map_err(|_| LocalSessionError::AllocationFailed)?;
        self.bytes.push(b'\n');
        Ok(())
    }
}

impl Write for BoundedWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let actual = u64::try_from(self.bytes.len())
            .unwrap_or(u64::MAX)
            .saturating_add(u64::try_from(bytes.len()).unwrap_or(u64::MAX));
        if actual > self.limit {
            self.error = Some(LocalSessionError::MessageLimitExceeded {
                actual: actual.saturating_add(1),
                limit: self.limit.saturating_add(1),
            });
            return Err(io::Error::other("bounded session message limit exceeded"));
        }
        if self.bytes.try_reserve(bytes.len()).is_err() {
            self.error = Some(LocalSessionError::AllocationFailed);
            return Err(io::Error::other(
                "bounded session message allocation failed",
            ));
        }
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
