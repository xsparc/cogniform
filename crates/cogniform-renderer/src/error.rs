use crate::{AdapterPreference, AdapterSummary};
use cogniform_assets::AssetMeshKey;
use cogniform_protocol::{PrimitiveShape, StableEntityId};

/// Offscreen target whose required capability was unavailable.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RenderTargetKind {
    /// Linear RGBA8 color output.
    Color,
    /// 32-bit floating-point depth output.
    Depth,
    /// 32-bit unsigned renderer-local entity-ID output.
    EntityId,
}

impl std::fmt::Display for RenderTargetKind {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Color => formatter.write_str("color"),
            Self::Depth => formatter.write_str("depth"),
            Self::EntityId => formatter.write_str("entity-id"),
        }
    }
}

/// One missing adapter capability.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CapabilityIssue {
    /// A required WebGPU limit exceeded the adapter's supported value.
    Limit {
        /// Stable `wgpu` limit name.
        name: &'static str,
        /// Minimum value required by the renderer.
        required: u64,
        /// Value reported by the adapter.
        supported: u64,
    },
    /// A target format lacks render-attachment or copy-source usage.
    TextureUsage {
        /// Affected renderer target.
        target: RenderTargetKind,
        /// Stable description of the required usages.
        required: &'static str,
    },
}

impl std::fmt::Display for CapabilityIssue {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Limit {
                name,
                required,
                supported,
            } => write!(
                formatter,
                "limit {name} requires {required}, adapter reports {supported}"
            ),
            Self::TextureUsage { target, required } => {
                write!(formatter, "{target} target requires {required}")
            }
        }
    }
}

/// Structured renderer initialization, submission, or readback failure.
#[derive(Debug)]
pub enum RendererError {
    /// No native backend was compiled for the current platform.
    BackendUnavailable,
    /// Target dimensions were zero or exceeded the fixed project bounds.
    InvalidTarget {
        /// Requested width.
        width: u32,
        /// Requested height.
        height: u32,
        /// Stable reason code.
        reason: &'static str,
    },
    /// The configured readback timeout was zero or exceeded the project bound.
    InvalidReadbackTimeout,
    /// The configured readback pool capacity was zero or exceeded its bound.
    InvalidReadbackCapacity,
    /// Asset upload or residency limits are internally inconsistent.
    InvalidAssetConfig {
        /// Stable configuration reason.
        reason: &'static str,
    },
    /// No readback buffer set was immediately available for submission.
    ReadbackPoolExhausted {
        /// Fixed number of in-flight frames supported by this renderer.
        capacity: u32,
    },
    /// No adapter matched the requested preference.
    AdapterUnavailable {
        /// Preference used for the failed request.
        preference: AdapterPreference,
        /// Backend request diagnostic.
        reason: String,
    },
    /// The selected adapter lacks one or more required capabilities.
    UnsupportedCapabilities {
        /// Selected adapter summary.
        adapter: AdapterSummary,
        /// Complete bounded list of detected issues.
        issues: Vec<CapabilityIssue>,
    },
    /// Logical device creation failed after capability validation.
    DeviceRequestFailed {
        /// Selected adapter summary.
        adapter: AdapterSummary,
        /// Device request diagnostic.
        reason: String,
    },
    /// Built-in shader or pipeline validation failed.
    PipelineCreationFailed {
        /// Validation diagnostic.
        reason: String,
    },
    /// The requested stable camera is absent or has no camera component.
    CameraUnavailable {
        /// Requested stable camera identity.
        camera_id: StableEntityId,
    },
    /// The extracted camera transform cannot be inverted.
    CameraTransformNotInvertible {
        /// Requested stable camera identity.
        camera_id: StableEntityId,
    },
    /// An extracted matrix cannot be represented by the GPU's f32 inputs.
    GpuTransformOutOfRange {
        /// Affected stable entity.
        entity_id: StableEntityId,
    },
    /// A built-in primitive is not yet supported by the bounded draw path.
    UnsupportedPrimitive {
        /// Affected stable entity.
        entity_id: StableEntityId,
        /// Unsupported built-in shape.
        shape: PrimitiveShape,
    },
    /// The extracted scene contains more drawable primitives than configured.
    DrawCapacityExceeded {
        /// Configured maximum draws per frame.
        limit: u32,
    },
    /// A CPU upload job does not contain a supported expanded triangle mesh.
    InvalidAssetMesh {
        /// Immutable mesh identity.
        key: AssetMeshKey,
        /// Stable validation reason.
        reason: &'static str,
    },
    /// One decoded mesh exceeds the configured vertex limit.
    AssetVertexLimitExceeded {
        /// Immutable mesh identity.
        key: AssetMeshKey,
        /// Supplied vertex count.
        actual: u32,
        /// Configured maximum.
        limit: u32,
    },
    /// One decoded mesh exceeds the configured per-buffer byte limit.
    AssetMeshBytesExceeded {
        /// Immutable mesh identity.
        key: AssetMeshKey,
        /// Supplied byte count.
        actual: u64,
        /// Configured maximum.
        limit: u64,
    },
    /// The bounded renderer upload queue is full.
    AssetUploadCapacityExceeded {
        /// Configured job capacity.
        capacity: u32,
    },
    /// Pending upload byte reservations would exceed their bound.
    AssetUploadBytesExceeded {
        /// Projected reserved bytes.
        actual: u64,
        /// Configured maximum.
        limit: u64,
    },
    /// Resident plus reserved mesh count would exceed its bound.
    AssetResidencyCapacityExceeded {
        /// Configured mesh capacity.
        capacity: u32,
    },
    /// Resident plus reserved GPU asset bytes would exceed their bound.
    AssetResidencyBytesExceeded {
        /// Projected bytes.
        actual: u64,
        /// Configured maximum.
        limit: u64,
    },
    /// An extracted asset mesh is not GPU-resident and has no explicit primitive proxy.
    AssetUnavailable {
        /// Affected stable scene entity.
        entity_id: StableEntityId,
        /// Missing immutable mesh identity.
        key: AssetMeshKey,
    },
    /// The monotonic frame identity cannot be incremented.
    FrameIdOverflow,
    /// GPU completion or buffer mapping failed.
    ReadbackFailed {
        /// Stable readback stage.
        stage: &'static str,
        /// Backend diagnostic.
        reason: String,
    },
    /// A mapped output contained an invalid depth value.
    InvalidDepthOutput {
        /// Pixel index containing the invalid value.
        pixel_index: usize,
    },
    /// A non-background attachment value had no stable-ID mapping for the frame.
    UnknownRenderEntityId {
        /// Unrecognized renderer-local attachment value.
        render_entity_id: u32,
    },
}

impl std::fmt::Display for RendererError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::BackendUnavailable => formatter
                .write_str("no supported native wgpu backend is compiled for this platform"),
            Self::InvalidTarget {
                width,
                height,
                reason,
            } => write!(formatter, "invalid {width}x{height} target: {reason}"),
            Self::InvalidReadbackTimeout => formatter
                .write_str("readback timeout must be greater than zero and at most 60 seconds"),
            Self::InvalidReadbackCapacity => {
                formatter.write_str("readback capacity must be greater than zero and at most 16")
            }
            error @ (Self::InvalidAssetConfig { .. }
            | Self::InvalidAssetMesh { .. }
            | Self::AssetVertexLimitExceeded { .. }
            | Self::AssetMeshBytesExceeded { .. }
            | Self::AssetUploadCapacityExceeded { .. }
            | Self::AssetUploadBytesExceeded { .. }
            | Self::AssetResidencyCapacityExceeded { .. }
            | Self::AssetResidencyBytesExceeded { .. }
            | Self::AssetUnavailable { .. }) => format_asset_error(error, formatter),
            Self::ReadbackPoolExhausted { capacity } => write!(
                formatter,
                "all {capacity} bounded readback slots are in flight"
            ),
            Self::AdapterUnavailable { preference, reason } => {
                write!(formatter, "no {preference} adapter is available: {reason}")
            }
            Self::UnsupportedCapabilities { adapter, issues } => {
                write!(
                    formatter,
                    "adapter {} ({}/{}) is missing {} required capability or capabilities",
                    adapter.name,
                    adapter.backend,
                    adapter.device_type,
                    issues.len()
                )
            }
            Self::DeviceRequestFailed { adapter, reason } => write!(
                formatter,
                "device request failed for {} ({}/{}): {reason}",
                adapter.name, adapter.backend, adapter.device_type
            ),
            Self::PipelineCreationFailed { reason } => {
                write!(formatter, "reference pipeline creation failed: {reason}")
            }
            Self::CameraUnavailable { camera_id } => {
                write!(
                    formatter,
                    "camera {camera_id} is unavailable in extracted renderer state"
                )
            }
            Self::CameraTransformNotInvertible { camera_id } => write!(
                formatter,
                "camera {camera_id} has a non-invertible world transform"
            ),
            Self::GpuTransformOutOfRange { entity_id } => write!(
                formatter,
                "entity {entity_id} has a transform outside finite GPU f32 range"
            ),
            Self::UnsupportedPrimitive { entity_id, shape } => write!(
                formatter,
                "entity {entity_id} uses unsupported primitive {shape:?}"
            ),
            Self::DrawCapacityExceeded { limit } => {
                write!(formatter, "frame draw capacity {limit} would be exceeded")
            }
            Self::FrameIdOverflow => formatter.write_str("frame identifier overflow"),
            Self::ReadbackFailed { stage, reason } => {
                write!(formatter, "readback failed during {stage}: {reason}")
            }
            Self::InvalidDepthOutput { pixel_index } => {
                write!(
                    formatter,
                    "depth output at pixel {pixel_index} is not finite or normalized"
                )
            }
            Self::UnknownRenderEntityId { render_entity_id } => write!(
                formatter,
                "frame contains unknown renderer entity ID {render_entity_id}"
            ),
        }
    }
}

fn format_asset_error(
    error: &RendererError,
    formatter: &mut std::fmt::Formatter<'_>,
) -> std::fmt::Result {
    match error {
        RendererError::InvalidAssetConfig { reason } => {
            write!(formatter, "invalid renderer asset config: {reason}")
        }
        RendererError::InvalidAssetMesh { key, reason } => {
            write!(formatter, "asset mesh {key:?} is invalid: {reason}")
        }
        RendererError::AssetVertexLimitExceeded { key, actual, limit } => write!(
            formatter,
            "asset mesh {key:?} has {actual} vertices; limit is {limit}"
        ),
        RendererError::AssetMeshBytesExceeded { key, actual, limit } => write!(
            formatter,
            "asset mesh {key:?} has {actual} bytes; limit is {limit}"
        ),
        RendererError::AssetUploadCapacityExceeded { capacity } => {
            write!(formatter, "asset upload capacity {capacity} is full")
        }
        RendererError::AssetUploadBytesExceeded { actual, limit } => write!(
            formatter,
            "pending asset uploads reserve {actual} bytes; limit is {limit}"
        ),
        RendererError::AssetResidencyCapacityExceeded { capacity } => {
            write!(formatter, "asset residency capacity {capacity} is full")
        }
        RendererError::AssetResidencyBytesExceeded { actual, limit } => write!(
            formatter,
            "asset residency would reserve {actual} bytes; limit is {limit}"
        ),
        RendererError::AssetUnavailable { entity_id, key } => write!(
            formatter,
            "entity {entity_id} references unavailable asset mesh {key:?}"
        ),
        _ => unreachable!("only asset errors are delegated"),
    }
}

impl std::error::Error for RendererError {}
