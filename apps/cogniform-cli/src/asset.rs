use std::{
    ffi::OsStr,
    io::{self, Write},
    path::Path,
};

use cogniform_protocol::ContentHash;
use cogniform_storage::AssetFileStore;
use serde::Serialize;

const SCHEMA_VERSION: u32 = 1;
const INVALID_HASH: &str = "inspect-asset content hash must be 64 lowercase hexadecimal characters";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AssetOutput {
    Human,
    Json,
}

pub(crate) fn parse_content_hash(encoded: &OsStr) -> io::Result<ContentHash> {
    encoded
        .to_str()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, INVALID_HASH))?
        .parse()
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, INVALID_HASH))
}

pub(crate) fn run(
    expected_hash: ContentHash,
    path: &OsStr,
    output: AssetOutput,
) -> Result<(), Box<dyn std::error::Error>> {
    let source = AssetFileStore::default().load(Path::new(path), expected_hash)?;
    let source_bytes = u64::try_from(source.len()).map_err(|_| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            "asset source length exceeds report range",
        )
    })?;
    drop(source);

    let encoded = match output {
        AssetOutput::Human => format!(
            "Cogniform asset source inspection passed\ncontent hash: {expected_hash}\nsource bytes: {source_bytes}\n"
        )
        .into_bytes(),
        AssetOutput::Json => {
            let report = AssetInspectionReport {
                schema_version: SCHEMA_VERSION,
                content_hash: expected_hash.to_string(),
                source_bytes,
            };
            let mut encoded = serde_json::to_vec(&report)?;
            encoded.push(b'\n');
            encoded
        }
    };
    io::stdout().lock().write_all(&encoded)?;
    Ok(())
}

#[derive(Serialize)]
struct AssetInspectionReport {
    schema_version: u32,
    content_hash: String,
    source_bytes: u64,
}

#[cfg(test)]
mod tests {
    use std::ffi::OsStr;

    use super::parse_content_hash;

    #[test]
    fn content_hash_requires_exact_lowercase_hexadecimal() {
        let valid = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
        assert_eq!(
            parse_content_hash(OsStr::new(valid)).unwrap().to_string(),
            valid
        );

        for invalid in [
            "",
            "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcde",
            "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef0",
            "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdeg",
            "0123456789ABCDEF0123456789ABCDEF0123456789ABCDEF0123456789ABCDEF",
        ] {
            let error = parse_content_hash(OsStr::new(invalid)).unwrap_err();
            assert_eq!(error.kind(), std::io::ErrorKind::InvalidInput);
            assert_eq!(
                error.to_string(),
                "inspect-asset content hash must be 64 lowercase hexadecimal characters"
            );
        }
    }
}
