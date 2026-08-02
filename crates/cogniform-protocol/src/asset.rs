use core::{fmt, str::FromStr};

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::{ValueError, ValueErrorKind};

/// Exact SHA-256 identity of immutable source asset bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ContentHash([u8; 32]);

impl ContentHash {
    /// Constructs a content hash from its exact digest bytes.
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

impl fmt::Display for ContentHash {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        for byte in self.0 {
            write!(formatter, "{byte:02x}")?;
        }
        Ok(())
    }
}

impl FromStr for ContentHash {
    type Err = ValueError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        if value.len() != 64
            || !value
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        {
            return Err(ValueError::new(ValueErrorKind::InvalidIdentifierEncoding));
        }
        let mut bytes = [0_u8; 32];
        for (target, pair) in bytes.iter_mut().zip(value.as_bytes().chunks_exact(2)) {
            let encoded = core::str::from_utf8(pair)
                .map_err(|_| ValueError::new(ValueErrorKind::InvalidIdentifierEncoding))?;
            *target = u8::from_str_radix(encoded, 16)
                .map_err(|_| ValueError::new(ValueErrorKind::InvalidIdentifierEncoding))?;
        }
        Ok(Self(bytes))
    }
}

impl Serialize for ContentHash {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for ContentHash {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let encoded = String::deserialize(deserializer)?;
        encoded.parse().map_err(serde::de::Error::custom)
    }
}

/// Hash-addressed mesh selection stored in logical scene state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AssetMeshComponent {
    /// SHA-256 identity of the immutable GLB source bytes.
    pub content_hash: ContentHash,
    /// Zero-based mesh index in the approved decoded asset.
    pub mesh_index: u32,
}
