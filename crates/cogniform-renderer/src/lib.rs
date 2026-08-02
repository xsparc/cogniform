//! Bounded, headless rendering for Cogniform.
//!
//! The renderer owns its `wgpu` device and all GPU resources. It renders to
//! offscreen textures and exposes no window, surface, or backend handle through
//! its public API. The first reference scene is intentionally small: one
//! built-in cube, one built-in camera, and color, depth, and renderer-local
//! entity-ID outputs.

mod asset;
mod config;
mod error;
mod frame;
mod renderer;
mod scene;

pub use asset::{AssetUploadAdmission, AssetUploadOutcome, RendererAssetStats};
pub use config::{
    AdapterPreference, MAX_READBACK_CAPACITY, MAX_READBACK_TIMEOUT, MAX_TARGET_DIMENSION,
    MAX_TARGET_PIXELS, REFERENCE_COLOR, REFERENCE_ENTITY_ID, RendererConfig,
};
pub use error::{CapabilityIssue, RenderTargetKind, RendererError};
pub use frame::{AdapterSummary, FrameMetadata, PendingFrame, RenderedFrame};
pub use renderer::HeadlessRenderer;
pub use scene::{RenderEntityId, SceneUpdateError, SceneUpdateSummary};
