//! Backend-neutral contracts for Cogniform.
//!
//! The crate defines versioned, bounded values shared by world, renderer, and
//! service domains. It intentionally contains no ECS, GPU, network, or
//! generated transport handles.

#![forbid(unsafe_code)]

mod codec;
mod component;
mod error;
mod id;
mod limits;
mod message;
mod render;

pub use component::{
    CameraComponent, ColorRgb, ColorRgba, ComponentKind, ComponentValue, LightComponent, LightKind,
    LocalTransform, MaterialComponent, NameComponent, PositiveVec3, PrimitiveComponent,
    PrimitiveShape, Quaternion, Vec3,
};
pub use error::{
    CodecError, DiagnosticCode, JsonErrorCategory, ValidationError, ValueError, ValueErrorKind,
};
pub use id::{
    FiniteF32, FrameId, IdempotencyKey, NonNegativeF32, ObservationId, PositiveF32, SceneRevision,
    SceneText, SchemaVersion, StableEntityId, TransactionId, UnitF32,
};
pub use limits::{PatchBudget, RuntimeLimits};
pub use message::{
    ApplyReceipt, ApplyStatus, ApplyTiming, ConflictPolicy, CreateEntity, DeleteEntity,
    DeliverySemantic, Diagnostic, DiagnosticSeverity, ImageDimensions, ObservationKind,
    ObservationMetadata, ObservationQuality, ObservationStaleness, QueueConfig, RemoveComponent,
    ReparentEntity, SceneOperation, ScenePatch, SetComponent,
};
pub use render::{RenderChange, RenderContractError, RenderEntity, RenderExtraction};
