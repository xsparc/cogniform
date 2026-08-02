use core::num::{NonZeroU32, NonZeroU64};
use std::sync::Arc;

use cogniform_protocol::{ContentHash, FiniteF32, UnitF32};

/// Fixed admission and decoded-output limits for one asset store.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AssetLimits {
    /// Maximum exact GLB source bytes accepted for one asset.
    pub max_source_bytes: NonZeroU64,
    /// Maximum aggregate queued source bytes.
    pub max_pending_source_bytes: NonZeroU64,
    /// Maximum JSON chunk bytes in one GLB.
    pub max_json_bytes: NonZeroU64,
    /// Maximum BIN chunk bytes in one GLB.
    pub max_bin_bytes: NonZeroU64,
    /// Maximum retained records, including rejected assets.
    pub max_assets: NonZeroU32,
    /// Maximum unprocessed imports.
    pub max_pending_imports: NonZeroU32,
    /// Maximum meshes decoded from one GLB.
    pub max_meshes: NonZeroU32,
    /// Maximum buffer-view records parsed from one GLB.
    pub max_buffer_views: NonZeroU32,
    /// Maximum accessor records parsed from one GLB.
    pub max_accessors: NonZeroU32,
    /// Maximum material records parsed from one GLB.
    pub max_materials: NonZeroU32,
    /// Maximum primitives accepted in one mesh.
    pub max_primitives_per_mesh: NonZeroU32,
    /// Maximum expanded triangle vertices in one mesh.
    pub max_vertices_per_mesh: NonZeroU32,
    /// Maximum source indices read for one mesh.
    pub max_indices_per_mesh: NonZeroU32,
    /// Maximum decoded upload bytes retained for one asset.
    pub max_asset_decoded_bytes: NonZeroU64,
    /// Maximum aggregate decoded CPU mesh bytes retained by the store.
    pub max_resident_cpu_bytes: NonZeroU64,
}

impl Default for AssetLimits {
    fn default() -> Self {
        Self {
            max_source_bytes: NonZeroU64::new(16 * 1_024 * 1_024).expect("constant is non-zero"),
            max_pending_source_bytes: NonZeroU64::new(32 * 1_024 * 1_024)
                .expect("constant is non-zero"),
            max_json_bytes: NonZeroU64::new(1_024 * 1_024).expect("constant is non-zero"),
            max_bin_bytes: NonZeroU64::new(16 * 1_024 * 1_024).expect("constant is non-zero"),
            max_assets: NonZeroU32::new(256).expect("constant is non-zero"),
            max_pending_imports: NonZeroU32::new(64).expect("constant is non-zero"),
            max_meshes: NonZeroU32::new(64).expect("constant is non-zero"),
            max_buffer_views: NonZeroU32::new(256).expect("constant is non-zero"),
            max_accessors: NonZeroU32::new(256).expect("constant is non-zero"),
            max_materials: NonZeroU32::new(256).expect("constant is non-zero"),
            max_primitives_per_mesh: NonZeroU32::new(1).expect("constant is non-zero"),
            max_vertices_per_mesh: NonZeroU32::new(262_144).expect("constant is non-zero"),
            max_indices_per_mesh: NonZeroU32::new(786_432).expect("constant is non-zero"),
            max_asset_decoded_bytes: NonZeroU64::new(16 * 1_024 * 1_024)
                .expect("constant is non-zero"),
            max_resident_cpu_bytes: NonZeroU64::new(64 * 1_024 * 1_024)
                .expect("constant is non-zero"),
        }
    }
}

/// Policy used only for syntactically valid but unsupported GLB features.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum UnsupportedAssetPolicy {
    /// Retain a rejected record and its structured diagnostic.
    #[default]
    Reject,
    /// Retain a conspicuous unit-cube proxy and the original diagnostic.
    ProxyCuboid,
}

/// Configuration for one bounded caller-driven asset store.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct AssetStoreConfig {
    /// All source, collection, and retained-memory limits.
    pub limits: AssetLimits,
    /// Explicit behavior for supported fallback classifications.
    pub unsupported_policy: UnsupportedAssetPolicy,
}

/// Stable asset lifecycle state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssetState {
    /// Verified source bytes await caller-driven decoding.
    Queued,
    /// The strict GLB subset decoded successfully.
    Ready,
    /// An unsupported GLB produced the configured deterministic proxy.
    ProxyReady,
    /// Import failed and no decoded mesh is available.
    Rejected,
}

/// Stable classification for one asset import diagnostic.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum AssetDiagnosticCode {
    /// GLB framing, version, or chunk order is invalid.
    InvalidGlb,
    /// The declared GLB or chunk length does not match available bytes.
    InvalidLength,
    /// JSON is malformed or outside the strict schema subset.
    InvalidJson,
    /// One or more glTF extensions were declared.
    UnsupportedExtension,
    /// A valid glTF feature is outside the approved subset.
    UnsupportedFeature,
    /// A buffer, view, accessor, or index references bytes outside its bounds.
    InvalidBufferRange,
    /// An accessor encoding is outside the approved scalar/position subset.
    UnsupportedAccessor,
    /// A mesh primitive is not triangle-list geometry.
    UnsupportedPrimitiveMode,
    /// A decoded position is non-finite.
    NonFiniteVertex,
    /// A decoded index is outside its position accessor.
    InvalidIndex,
    /// A configured mesh, primitive, vertex, or index count was exceeded.
    CollectionLimitExceeded,
    /// A configured source, chunk, decoded-asset, or CPU-residency byte limit was exceeded.
    ByteLimitExceeded,
}

impl AssetDiagnosticCode {
    pub(crate) const fn permits_proxy(self) -> bool {
        matches!(
            self,
            Self::UnsupportedExtension
                | Self::UnsupportedFeature
                | Self::UnsupportedAccessor
                | Self::UnsupportedPrimitiveMode
        )
    }
}

/// Bounded diagnostic that never retains parser input or an unbounded error string.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssetDiagnostic {
    /// Stable diagnostic classification.
    pub code: AssetDiagnosticCode,
    /// Static schema/import location.
    pub location: &'static str,
    /// Optional zero-based collection index.
    pub index: Option<u32>,
}

impl AssetDiagnostic {
    pub(crate) const fn new(
        code: AssetDiagnosticCode,
        location: &'static str,
        index: Option<u32>,
    ) -> Self {
        Self {
            code,
            location,
            index,
        }
    }
}

/// One finite decoded position used by the baseline asset pipeline.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AssetVertex {
    /// XYZ position in mesh-local units.
    pub position: [FiniteF32; 3],
}

/// Stable key for one mesh inside immutable source bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct AssetMeshKey {
    /// Source content identity.
    pub content_hash: ContentHash,
    /// Zero-based mesh selection.
    pub mesh_index: u32,
}

/// Immutable upload-ready expanded triangle mesh.
#[derive(Debug, Clone, PartialEq)]
pub struct AssetUploadJob {
    key: AssetMeshKey,
    vertices: Arc<[AssetVertex]>,
    base_color: [UnitF32; 4],
}

impl AssetUploadJob {
    pub(crate) fn new(
        key: AssetMeshKey,
        vertices: Arc<[AssetVertex]>,
        base_color: [UnitF32; 4],
    ) -> Self {
        Self {
            key,
            vertices,
            base_color,
        }
    }

    /// Returns the immutable mesh identity.
    #[must_use]
    pub const fn key(&self) -> AssetMeshKey {
        self.key
    }

    /// Returns finite expanded triangle vertices.
    #[must_use]
    pub fn vertices(&self) -> &[AssetVertex] {
        &self.vertices
    }

    /// Returns the imported linear base color.
    #[must_use]
    pub const fn base_color(&self) -> [UnitF32; 4] {
        self.base_color
    }

    /// Returns exact GPU vertex bytes required by this baseline mesh.
    #[must_use]
    pub fn byte_len(&self) -> u64 {
        u64::try_from(self.vertices.len())
            .unwrap_or(u64::MAX)
            .saturating_mul(12)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct DecodedMesh {
    pub(crate) vertices: Arc<[AssetVertex]>,
    pub(crate) base_color: [UnitF32; 4],
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct DecodedAsset {
    pub(crate) meshes: Vec<DecodedMesh>,
    pub(crate) byte_len: u64,
}

/// Immediate result of verified source admission.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssetAdmission {
    /// New verified source bytes were queued.
    Queued {
        /// Verified immutable content identity.
        content_hash: ContentHash,
    },
    /// This content hash already has a retained immutable record.
    AlreadyKnown {
        /// Existing immutable content identity.
        content_hash: ContentHash,
        /// Current retained lifecycle state.
        state: AssetState,
    },
}

/// Result of processing at most one queued import.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AssetProcessOutcome {
    /// Processed source content identity.
    pub content_hash: ContentHash,
    /// Resulting retained lifecycle state.
    pub state: AssetState,
    /// Available decoded mesh count, or zero on rejection.
    pub mesh_count: u32,
}

/// Read-only retained asset record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssetRecord {
    /// Immutable source content identity.
    pub content_hash: ContentHash,
    /// Current lifecycle state.
    pub state: AssetState,
    /// Exact admitted source byte count.
    pub source_bytes: u64,
    /// Decoded CPU mesh bytes retained for upload.
    pub decoded_bytes: u64,
    /// Available mesh count.
    pub mesh_count: u32,
    /// Bounded import diagnostics; currently zero or one.
    pub diagnostics: Vec<AssetDiagnostic>,
}

/// Aggregate bounded store occupancy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AssetStoreStats {
    /// Total retained records.
    pub records: u32,
    /// Imports awaiting explicit processing.
    pub pending_imports: u32,
    /// Exact source bytes retained by pending imports.
    pub pending_source_bytes: u64,
    /// Decoded CPU mesh bytes retained by ready/proxy records.
    pub resident_cpu_bytes: u64,
}
