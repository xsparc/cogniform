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
    /// Maximum width or height accepted for one embedded texture image.
    pub max_texture_dimension_2d: NonZeroU32,
    /// Maximum pixel count accepted for one embedded texture image.
    pub max_texture_pixels: NonZeroU64,
    /// Maximum decoded RGBA8 bytes retained for one embedded texture.
    pub max_texture_decoded_bytes: NonZeroU64,
    /// Maximum temporary bytes the PNG decoder may allocate internally.
    pub max_texture_decoder_bytes: NonZeroU64,
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
            max_texture_dimension_2d: NonZeroU32::new(2_048).expect("constant is non-zero"),
            max_texture_pixels: NonZeroU64::new(4_194_304).expect("constant is non-zero"),
            max_texture_decoded_bytes: NonZeroU64::new(16 * 1_024 * 1_024)
                .expect("constant is non-zero"),
            max_texture_decoder_bytes: NonZeroU64::new(4 * 1_024 * 1_024)
                .expect("constant is non-zero"),
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
    /// An accessor encoding is outside the approved vertex/index subset.
    UnsupportedAccessor,
    /// A mesh primitive is not triangle-list geometry.
    UnsupportedPrimitiveMode,
    /// A decoded position is non-finite.
    NonFiniteVertex,
    /// A decoded normal is non-finite, zero-length, or inconsistent with its positions.
    InvalidNormal,
    /// A decoded source tangent is non-finite, zero-length, has invalid handedness, or is inconsistent.
    InvalidTangent,
    /// A decoded primary texture coordinate is non-finite or inconsistent with its positions.
    InvalidTexcoord,
    /// Embedded image bytes are malformed, truncated, or inconsistent with their PNG header.
    InvalidImage,
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

/// Exact decoded and GPU bytes in one interleaved asset vertex.
pub const ASSET_VERTEX_BYTES: u64 = 48;

/// One finite decoded position, unit normal, source tangent, and primary texture coordinate.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AssetVertex {
    /// XYZ position in mesh-local units.
    pub position: [FiniteF32; 3],
    /// Normalized XYZ normal in mesh-local units.
    pub normal: [FiniteF32; 3],
    /// Normalized source tangent XYZ and exact handedness in `w`.
    pub tangent: [FiniteF32; 4],
    /// Primary glTF texture coordinates retained without unit clamping.
    pub texcoord_0: [FiniteF32; 2],
}

/// Stable key for one mesh inside immutable source bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct AssetMeshKey {
    /// Source content identity.
    pub content_hash: ContentHash,
    /// Zero-based mesh selection.
    pub mesh_index: u32,
}

/// Deterministic alpha coverage retained from one imported glTF material.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum AssetAlphaMode {
    /// Ignore imported factor and texture alpha for coverage.
    Opaque,
    /// Discard fragments whose multiplied imported alpha is below the cutoff.
    Mask,
}

/// Immutable bounded material values imported with one mesh.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AssetMaterial {
    base_color: [UnitF32; 4],
    metallic: UnitF32,
    roughness: UnitF32,
    emissive: [f32; 3],
    texture_roles: u8,
    normal_scale: f32,
    alpha_mode: AssetAlphaMode,
    alpha_cutoff: f32,
    double_sided: bool,
}

impl AssetMaterial {
    const BASE_COLOR_TEXTURE: u8 = 1 << 0;
    const EMISSIVE_TEXTURE: u8 = 1 << 1;
    const METALLIC_ROUGHNESS_TEXTURE: u8 = 1 << 2;
    const NORMAL_TEXTURE: u8 = 1 << 3;

    /// Creates one validated linear metallic-roughness material with zero emission.
    #[must_use]
    pub const fn new(base_color: [UnitF32; 4], metallic: UnitF32, roughness: UnitF32) -> Self {
        Self {
            base_color,
            metallic,
            roughness,
            emissive: [0.0; 3],
            texture_roles: 0,
            normal_scale: 1.0,
            alpha_mode: AssetAlphaMode::Opaque,
            alpha_cutoff: 0.5,
            double_sided: false,
        }
    }

    pub(crate) const fn with_base_color_texture(mut self) -> Self {
        self.texture_roles |= Self::BASE_COLOR_TEXTURE;
        self
    }

    pub(crate) const fn with_metallic_roughness_texture(mut self) -> Self {
        self.texture_roles |= Self::METALLIC_ROUGHNESS_TEXTURE;
        self
    }

    pub(crate) const fn with_emissive_texture(mut self) -> Self {
        self.texture_roles |= Self::EMISSIVE_TEXTURE;
        self
    }

    pub(crate) fn with_emissive(mut self, emissive: [UnitF32; 3]) -> Self {
        self.emissive = emissive.map(UnitF32::get);
        self
    }

    pub(crate) fn with_normal_texture(mut self, scale: FiniteF32) -> Self {
        self.texture_roles |= Self::NORMAL_TEXTURE;
        self.normal_scale = scale.get();
        self
    }

    pub(crate) fn with_alpha_mask(mut self, cutoff: FiniteF32) -> Self {
        self.alpha_mode = AssetAlphaMode::Mask;
        self.alpha_cutoff = cutoff.get();
        self
    }

    pub(crate) const fn with_double_sided(mut self) -> Self {
        self.double_sided = true;
        self
    }

    /// Returns the imported linear base color.
    #[must_use]
    pub const fn base_color(self) -> [UnitF32; 4] {
        self.base_color
    }

    /// Returns the imported metallic factor.
    #[must_use]
    pub const fn metallic(self) -> UnitF32 {
        self.metallic
    }

    /// Returns the imported perceptual roughness factor.
    #[must_use]
    pub const fn roughness(self) -> UnitF32 {
        self.roughness
    }

    /// Returns the imported linear emissive RGB multiplier.
    #[must_use]
    pub const fn emissive(self) -> [f32; 3] {
        self.emissive
    }

    /// Returns whether this material samples the asset's shared base-color texture.
    #[must_use]
    pub const fn has_base_color_texture(self) -> bool {
        self.texture_roles & Self::BASE_COLOR_TEXTURE != 0
    }

    /// Returns whether this material samples the asset's shared emissive texture.
    #[must_use]
    pub const fn has_emissive_texture(self) -> bool {
        self.texture_roles & Self::EMISSIVE_TEXTURE != 0
    }

    /// Returns whether this material samples the asset's shared linear
    /// metallic-roughness texture.
    #[must_use]
    pub const fn has_metallic_roughness_texture(self) -> bool {
        self.texture_roles & Self::METALLIC_ROUGHNESS_TEXTURE != 0
    }

    /// Returns whether this material samples the asset's shared tangent-space normal texture.
    #[must_use]
    pub const fn has_normal_texture(self) -> bool {
        self.texture_roles & Self::NORMAL_TEXTURE != 0
    }

    /// Returns the finite glTF normal-texture XY scale.
    #[must_use]
    pub const fn normal_scale(self) -> f32 {
        self.normal_scale
    }

    /// Returns the imported deterministic alpha-coverage mode.
    #[must_use]
    pub const fn alpha_mode(self) -> AssetAlphaMode {
        self.alpha_mode
    }

    /// Returns the finite non-negative cutoff for imported mask coverage.
    #[must_use]
    pub const fn alpha_cutoff(self) -> Option<f32> {
        match self.alpha_mode {
            AssetAlphaMode::Opaque => None,
            AssetAlphaMode::Mask => Some(self.alpha_cutoff),
        }
    }

    /// Returns whether the imported material renders and lights both faces.
    #[must_use]
    pub const fn double_sided(self) -> bool {
        self.double_sided
    }
}

/// One immutable decoded image in tightly packed RGBA8 rows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssetTexture {
    width: NonZeroU32,
    height: NonZeroU32,
    rgba8: Arc<[u8]>,
}

impl AssetTexture {
    pub(crate) fn new(width: NonZeroU32, height: NonZeroU32, rgba8: Arc<[u8]>) -> Self {
        Self {
            width,
            height,
            rgba8,
        }
    }

    /// Returns the image width in pixels.
    #[must_use]
    pub const fn width(&self) -> u32 {
        self.width.get()
    }

    /// Returns the image height in pixels.
    #[must_use]
    pub const fn height(&self) -> u32 {
        self.height.get()
    }

    /// Returns tightly packed top-to-bottom RGBA8 texels.
    #[must_use]
    pub fn rgba8(&self) -> &[u8] {
        &self.rgba8
    }

    /// Returns the exact retained and upload texel bytes.
    #[must_use]
    pub fn byte_len(&self) -> u64 {
        u64::try_from(self.rgba8.len()).unwrap_or(u64::MAX)
    }
}

/// Immutable upload-ready expanded triangle mesh.
#[derive(Debug, Clone, PartialEq)]
pub struct AssetUploadJob {
    key: AssetMeshKey,
    vertices: Arc<[AssetVertex]>,
    material: AssetMaterial,
    base_color_texture: Option<AssetTexture>,
    emissive_texture: Option<AssetTexture>,
    metallic_roughness_texture: Option<AssetTexture>,
    normal_texture: Option<AssetTexture>,
}

impl AssetUploadJob {
    pub(crate) fn new(
        key: AssetMeshKey,
        vertices: Arc<[AssetVertex]>,
        material: AssetMaterial,
        base_color_texture: Option<AssetTexture>,
        emissive_texture: Option<AssetTexture>,
        metallic_roughness_texture: Option<AssetTexture>,
        normal_texture: Option<AssetTexture>,
    ) -> Self {
        Self {
            key,
            vertices,
            material,
            base_color_texture,
            emissive_texture,
            metallic_roughness_texture,
            normal_texture,
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
        self.material.base_color()
    }

    /// Returns the complete immutable imported material.
    #[must_use]
    pub const fn material(&self) -> AssetMaterial {
        self.material
    }

    /// Returns the immutable shared texture when this mesh's material references it.
    #[must_use]
    pub const fn base_color_texture(&self) -> Option<&AssetTexture> {
        self.base_color_texture.as_ref()
    }

    /// Returns the immutable shared emissive texture when this mesh's material references it.
    #[must_use]
    pub const fn emissive_texture(&self) -> Option<&AssetTexture> {
        self.emissive_texture.as_ref()
    }

    /// Returns the immutable shared linear metallic-roughness texture when
    /// this mesh's material references it.
    #[must_use]
    pub const fn metallic_roughness_texture(&self) -> Option<&AssetTexture> {
        self.metallic_roughness_texture.as_ref()
    }

    /// Returns the immutable shared normal texture when this mesh's material references it.
    #[must_use]
    pub const fn normal_texture(&self) -> Option<&AssetTexture> {
        self.normal_texture.as_ref()
    }

    /// Returns exact GPU vertex bytes required by this interleaved mesh.
    #[must_use]
    pub fn byte_len(&self) -> u64 {
        u64::try_from(self.vertices.len())
            .unwrap_or(u64::MAX)
            .saturating_mul(ASSET_VERTEX_BYTES)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct DecodedMesh {
    pub(crate) vertices: Arc<[AssetVertex]>,
    pub(crate) material: AssetMaterial,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct DecodedAsset {
    pub(crate) meshes: Vec<DecodedMesh>,
    pub(crate) base_color_texture: Option<AssetTexture>,
    pub(crate) emissive_texture: Option<AssetTexture>,
    pub(crate) metallic_roughness_texture: Option<AssetTexture>,
    pub(crate) normal_texture: Option<AssetTexture>,
    pub(crate) byte_len: u64,
}

impl DecodedAsset {
    pub(crate) fn texture_count(&self) -> u32 {
        u32::from(self.base_color_texture.is_some())
            + u32::from(self.emissive_texture.is_some())
            + u32::from(self.metallic_roughness_texture.is_some())
            + u32::from(self.normal_texture.is_some())
    }
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
    /// Monotonic elapsed microseconds for the oldest pending import.
    pub oldest_pending_import_age_micros: Option<u64>,
    /// Exact source bytes retained by pending imports.
    pub pending_source_bytes: u64,
    /// Decoded CPU mesh bytes retained by ready/proxy records.
    pub resident_cpu_bytes: u64,
}

/// Exact asset-store resources released for one content hash.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AssetStoreEviction {
    /// Immutable content identity selected for eviction.
    pub content_hash: ContentHash,
    /// Lifecycle state removed from the store, or `None` when already absent.
    pub previous_state: Option<AssetState>,
    /// Queued import records removed; currently zero or one.
    pub removed_pending_imports: u32,
    /// Exact queued source bytes released.
    pub released_pending_source_bytes: u64,
    /// Exact decoded CPU bytes released.
    pub released_resident_cpu_bytes: u64,
    /// Decoded mesh records released.
    pub removed_meshes: u32,
    /// Role-separated decoded textures released; currently zero to four.
    pub removed_textures: u32,
}

impl AssetStoreEviction {
    /// Returns whether the selected hash had no retained CPU-side state.
    #[must_use]
    pub const fn is_already_absent(self) -> bool {
        self.previous_state.is_none()
    }
}
