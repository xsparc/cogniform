#![doc = "Bounded content-addressed asset ingestion and upload-ready mesh values."]
#![forbid(unsafe_code)]

mod error;
mod glb;
mod store;
mod types;

pub use error::AssetError;
pub use store::AssetStore;
pub use types::{
    ASSET_VERTEX_BYTES, AssetAdmission, AssetDiagnostic, AssetDiagnosticCode, AssetLimits,
    AssetMaterial, AssetMeshKey, AssetProcessOutcome, AssetRecord, AssetState, AssetStoreConfig,
    AssetStoreStats, AssetUploadJob, AssetVertex, UnsupportedAssetPolicy,
};

use cogniform_protocol::ContentHash;
use sha2::{Digest, Sha256};

/// Computes the canonical SHA-256 identity of exact source bytes.
#[must_use]
pub fn content_hash(bytes: &[u8]) -> ContentHash {
    ContentHash::from_bytes(Sha256::digest(bytes).into())
}
