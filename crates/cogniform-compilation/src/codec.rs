use std::io::{self, Write};

use cogniform_protocol::JsonErrorCategory;

use crate::{CompilationCodecError, CompilationLimits, CompilationResult};

pub(crate) fn encode(
    value: &CompilationResult,
    limits: &CompilationLimits,
) -> Result<Vec<u8>, CompilationCodecError> {
    value
        .validate(limits)
        .map_err(CompilationCodecError::InvalidResult)?;
    encode_validated(value, limits)
}

pub(crate) fn decode(
    encoded: &[u8],
    limits: &CompilationLimits,
) -> Result<CompilationResult, CompilationCodecError> {
    enforce_size(encoded.len(), limits.max_encoded_bytes.get())?;
    enforce_nesting(encoded, limits.max_json_nesting_depth.get())?;
    let value: CompilationResult =
        serde_json::from_slice(encoded).map_err(|error| CompilationCodecError::MalformedJson {
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
        .validate(limits)
        .map_err(CompilationCodecError::InvalidResult)?;
    if encode_validated(&value, limits)? != encoded {
        return Err(CompilationCodecError::NonCanonicalResult);
    }
    Ok(value)
}

fn encode_validated(
    value: &CompilationResult,
    limits: &CompilationLimits,
) -> Result<Vec<u8>, CompilationCodecError> {
    let limit = limits.max_encoded_bytes.get();
    let mut writer = BoundedWriter::new(limit.saturating_sub(1));
    if serde_json::to_writer(&mut writer, value).is_err() {
        return Err(writer
            .error
            .unwrap_or(CompilationCodecError::SerializationFailed));
    }
    writer.push_lf(limit)?;
    enforce_nesting(&writer.bytes, limits.max_json_nesting_depth.get())?;
    Ok(writer.bytes)
}

fn enforce_size(actual: usize, limit: u64) -> Result<(), CompilationCodecError> {
    let actual = u64::try_from(actual).unwrap_or(u64::MAX);
    if actual > limit {
        Err(CompilationCodecError::EncodedSizeExceeded { actual, limit })
    } else {
        Ok(())
    }
}

fn enforce_nesting(encoded: &[u8], limit: u16) -> Result<(), CompilationCodecError> {
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
                    return Err(CompilationCodecError::NestingLimitExceeded {
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
    error: Option<CompilationCodecError>,
}

impl BoundedWriter {
    const fn new(limit: u64) -> Self {
        Self {
            bytes: Vec::new(),
            limit,
            error: None,
        }
    }

    fn push_lf(&mut self, complete_limit: u64) -> Result<(), CompilationCodecError> {
        let next =
            self.bytes
                .len()
                .checked_add(1)
                .ok_or(CompilationCodecError::EncodedSizeExceeded {
                    actual: u64::MAX,
                    limit: complete_limit,
                })?;
        enforce_size(next, complete_limit)?;
        self.bytes
            .try_reserve(1)
            .map_err(|_| CompilationCodecError::AllocationFailed)?;
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
            self.error = Some(CompilationCodecError::EncodedSizeExceeded {
                actual: actual.saturating_add(1),
                limit: self.limit.saturating_add(1),
            });
            return Err(io::Error::other(
                "bounded compilation-result limit exceeded",
            ));
        }
        if self.bytes.try_reserve(bytes.len()).is_err() {
            self.error = Some(CompilationCodecError::AllocationFailed);
            return Err(io::Error::other(
                "bounded compilation-result allocation failed",
            ));
        }
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
