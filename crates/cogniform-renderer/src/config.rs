use core::num::{NonZeroU32, NonZeroU64};
use std::time::Duration;

/// Maximum width or height accepted for an offscreen target.
pub const MAX_TARGET_DIMENSION: u32 = 4_096;

/// Maximum number of pixels accepted for one offscreen target set.
pub const MAX_TARGET_PIXELS: u64 = 4_194_304;

/// Maximum fixed number of simultaneously in-flight readback sets.
pub const MAX_READBACK_CAPACITY: u32 = 16;

/// Maximum time a caller may allow a reference-frame readback to wait.
pub const MAX_READBACK_TIMEOUT: Duration = Duration::from_mins(1);

/// Renderer-local ID written by the built-in reference cube.
///
/// This is deliberately not a [`cogniform_protocol::StableEntityId`]. Extracted
/// scene frames retain an explicit compact-to-stable mapping instead.
pub const REFERENCE_ENTITY_ID: u32 = 7;

/// Expected linear RGBA8 color of the built-in reference cube.
pub const REFERENCE_COLOR: [u8; 4] = [51, 153, 230, 255];

/// Adapter preference used during headless initialization.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum AdapterPreference {
    /// Prefer a high-performance adapter, while still permitting a software
    /// adapter when it is the only compatible choice.
    #[default]
    HighPerformance,
    /// Prefer a lower-power adapter.
    LowPower,
    /// Require a fallback adapter, normally a software implementation.
    Fallback,
}

impl std::fmt::Display for AdapterPreference {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::HighPerformance => formatter.write_str("high-performance"),
            Self::LowPower => formatter.write_str("low-power"),
            Self::Fallback => formatter.write_str("fallback"),
        }
    }
}

/// Configuration for one fixed-size headless renderer.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RendererConfig {
    /// Width of every offscreen target in pixels.
    pub width: u32,
    /// Height of every offscreen target in pixels.
    pub height: u32,
    /// Adapter selection preference.
    pub adapter_preference: AdapterPreference,
    /// Maximum duration allowed while waiting for mapped readback buffers.
    pub readback_timeout: Duration,
    /// Fixed number of frames that may await readback concurrently.
    pub readback_capacity: NonZeroU32,
    /// Maximum number of renderer-owned extracted entity records.
    pub max_scene_entities: NonZeroU32,
    /// Maximum primitive draws admitted for one submitted frame.
    pub max_draws_per_frame: NonZeroU32,
    /// Maximum upload jobs awaiting explicit renderer processing.
    pub asset_upload_capacity: NonZeroU32,
    /// Maximum aggregate bytes reserved by pending upload jobs.
    pub max_pending_asset_upload_bytes: NonZeroU64,
    /// Maximum expanded vertices in one uploaded asset mesh.
    pub max_asset_vertices: NonZeroU32,
    /// Maximum bytes in one asset vertex buffer.
    pub max_asset_mesh_bytes: NonZeroU64,
    /// Maximum immutable asset meshes resident on the GPU.
    pub max_resident_asset_meshes: NonZeroU32,
    /// Maximum aggregate immutable asset vertex bytes resident on the GPU.
    pub max_resident_asset_bytes: NonZeroU64,
    /// Maximum width or height of one uploaded asset base-color texture.
    pub max_asset_texture_dimension_2d: NonZeroU32,
    /// Maximum bytes in one uploaded asset base-color texture.
    pub max_asset_texture_bytes: NonZeroU64,
    /// Maximum aggregate bytes reserved by pending unique texture uploads.
    pub max_pending_asset_texture_bytes: NonZeroU64,
    /// Maximum immutable asset base-color textures resident on the GPU.
    pub max_resident_asset_textures: NonZeroU32,
    /// Maximum aggregate immutable asset texture bytes resident on the GPU.
    pub max_resident_asset_texture_bytes: NonZeroU64,
}

impl RendererConfig {
    /// Creates a renderer configuration with bounded default behavior.
    #[must_use]
    pub const fn new(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            adapter_preference: AdapterPreference::HighPerformance,
            readback_timeout: Duration::from_secs(10),
            readback_capacity: NonZeroU32::new(2).expect("constant is non-zero"),
            max_scene_entities: NonZeroU32::new(65_536).expect("constant is non-zero"),
            max_draws_per_frame: NonZeroU32::new(4_096).expect("constant is non-zero"),
            asset_upload_capacity: NonZeroU32::new(64).expect("constant is non-zero"),
            max_pending_asset_upload_bytes: NonZeroU64::new(32 * 1_024 * 1_024)
                .expect("constant is non-zero"),
            max_asset_vertices: NonZeroU32::new(262_144).expect("constant is non-zero"),
            max_asset_mesh_bytes: NonZeroU64::new(16 * 1_024 * 1_024)
                .expect("constant is non-zero"),
            max_resident_asset_meshes: NonZeroU32::new(256).expect("constant is non-zero"),
            max_resident_asset_bytes: NonZeroU64::new(64 * 1_024 * 1_024)
                .expect("constant is non-zero"),
            max_asset_texture_dimension_2d: NonZeroU32::new(2_048).expect("constant is non-zero"),
            max_asset_texture_bytes: NonZeroU64::new(16 * 1_024 * 1_024)
                .expect("constant is non-zero"),
            max_pending_asset_texture_bytes: NonZeroU64::new(32 * 1_024 * 1_024)
                .expect("constant is non-zero"),
            max_resident_asset_textures: NonZeroU32::new(256).expect("constant is non-zero"),
            max_resident_asset_texture_bytes: NonZeroU64::new(64 * 1_024 * 1_024)
                .expect("constant is non-zero"),
        }
    }

    /// Selects an adapter preference.
    #[must_use]
    pub const fn with_adapter_preference(mut self, preference: AdapterPreference) -> Self {
        self.adapter_preference = preference;
        self
    }

    /// Sets the bounded readback timeout.
    #[must_use]
    pub const fn with_readback_timeout(mut self, timeout: Duration) -> Self {
        self.readback_timeout = timeout;
        self
    }

    /// Sets the fixed number of simultaneously in-flight readback sets.
    #[must_use]
    pub const fn with_readback_capacity(mut self, capacity: NonZeroU32) -> Self {
        self.readback_capacity = capacity;
        self
    }

    /// Sets the maximum renderer-owned extracted entity count.
    #[must_use]
    pub const fn with_max_scene_entities(mut self, capacity: NonZeroU32) -> Self {
        self.max_scene_entities = capacity;
        self
    }

    /// Sets the maximum primitive draws admitted for one frame.
    #[must_use]
    pub const fn with_max_draws_per_frame(mut self, capacity: NonZeroU32) -> Self {
        self.max_draws_per_frame = capacity;
        self
    }

    /// Sets the maximum count of explicitly processed upload jobs.
    #[must_use]
    pub const fn with_asset_upload_capacity(mut self, capacity: NonZeroU32) -> Self {
        self.asset_upload_capacity = capacity;
        self
    }

    /// Sets the aggregate pending upload-byte reservation.
    #[must_use]
    pub const fn with_max_pending_asset_upload_bytes(mut self, bytes: NonZeroU64) -> Self {
        self.max_pending_asset_upload_bytes = bytes;
        self
    }

    /// Sets the per-mesh expanded vertex limit.
    #[must_use]
    pub const fn with_max_asset_vertices(mut self, vertices: NonZeroU32) -> Self {
        self.max_asset_vertices = vertices;
        self
    }

    /// Sets the per-mesh GPU vertex-byte limit.
    #[must_use]
    pub const fn with_max_asset_mesh_bytes(mut self, bytes: NonZeroU64) -> Self {
        self.max_asset_mesh_bytes = bytes;
        self
    }

    /// Sets the aggregate GPU-resident mesh-count limit.
    #[must_use]
    pub const fn with_max_resident_asset_meshes(mut self, meshes: NonZeroU32) -> Self {
        self.max_resident_asset_meshes = meshes;
        self
    }

    /// Sets the aggregate GPU-resident asset vertex-byte limit.
    #[must_use]
    pub const fn with_max_resident_asset_bytes(mut self, bytes: NonZeroU64) -> Self {
        self.max_resident_asset_bytes = bytes;
        self
    }

    /// Sets the per-texture width and height limit.
    #[must_use]
    pub const fn with_max_asset_texture_dimension_2d(mut self, dimension: NonZeroU32) -> Self {
        self.max_asset_texture_dimension_2d = dimension;
        self
    }

    /// Sets the per-texture RGBA8 byte limit.
    #[must_use]
    pub const fn with_max_asset_texture_bytes(mut self, bytes: NonZeroU64) -> Self {
        self.max_asset_texture_bytes = bytes;
        self
    }

    /// Sets the aggregate pending unique-texture byte reservation.
    #[must_use]
    pub const fn with_max_pending_asset_texture_bytes(mut self, bytes: NonZeroU64) -> Self {
        self.max_pending_asset_texture_bytes = bytes;
        self
    }

    /// Sets the aggregate GPU-resident texture-count limit.
    #[must_use]
    pub const fn with_max_resident_asset_textures(mut self, textures: NonZeroU32) -> Self {
        self.max_resident_asset_textures = textures;
        self
    }

    /// Sets the aggregate GPU-resident texture-byte limit.
    #[must_use]
    pub const fn with_max_resident_asset_texture_bytes(mut self, bytes: NonZeroU64) -> Self {
        self.max_resident_asset_texture_bytes = bytes;
        self
    }
}
