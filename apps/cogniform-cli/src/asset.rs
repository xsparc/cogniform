use std::{
    ffi::OsStr,
    io::{self, Write},
    path::Path,
};

use cogniform_protocol::ContentHash;
use cogniform_storage::AssetFileStore;

const INVALID_HASH: &str = "inspect-asset content hash must be 64 lowercase hexadecimal characters";

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
) -> Result<(), Box<dyn std::error::Error>> {
    let source = AssetFileStore::default().load(Path::new(path), expected_hash)?;
    let source_bytes = u64::try_from(source.len()).map_err(|_| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            "asset source length exceeds report range",
        )
    })?;
    drop(source);

    let encoded = format!(
        "Cogniform asset source inspection passed\ncontent hash: {expected_hash}\nsource bytes: {source_bytes}\n"
    );
    io::stdout().lock().write_all(encoded.as_bytes())?;
    Ok(())
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
