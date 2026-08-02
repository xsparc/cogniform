use std::time::Duration;

/// Maximum width or height accepted for an offscreen target.
pub const MAX_TARGET_DIMENSION: u32 = 4_096;

/// Maximum number of pixels accepted for one offscreen target set.
pub const MAX_TARGET_PIXELS: u64 = 4_194_304;

/// Maximum time a caller may allow a reference-frame readback to wait.
pub const MAX_READBACK_TIMEOUT: Duration = Duration::from_mins(1);

/// Renderer-local ID written by the built-in reference cube.
///
/// This is deliberately not a [`cogniform_protocol::StableEntityId`]. CF005
/// will own the bounded mapping between stable world identity and compact GPU
/// identity.
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
}
