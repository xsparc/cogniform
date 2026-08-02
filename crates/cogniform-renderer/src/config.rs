use core::num::NonZeroU32;
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
}
