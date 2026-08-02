//! Backend-neutral contracts for Cogniform.
//!
//! The crate defines versioned, bounded values shared by world, renderer, and
//! service domains. It intentionally contains no ECS, GPU, network, or
//! generated transport handles.

#![forbid(unsafe_code)]

mod asset;
mod codec;
mod component;
mod error;
mod id;
mod imagination;
mod limits;
mod message;
mod query;
mod render;

pub use asset::{AssetMeshComponent, ContentHash};
pub use component::{
    CameraComponent, ColorRgb, ColorRgba, ComponentKind, ComponentValue, LightComponent, LightKind,
    LocalTransform, MaterialComponent, NameComponent, PositiveVec3, PrimitiveComponent,
    PrimitiveShape, Quaternion, Vec3,
};
pub use error::{
    CodecError, DiagnosticCode, JsonErrorCategory, ValidationError, ValueError, ValueErrorKind,
};
pub use id::{
    FiniteF32, FrameId, IdempotencyKey, ImaginationId, NonNegativeF32, ObservationId, PositiveF32,
    ProcedureId, SceneRevision, SceneText, SchemaVersion, StableEntityId, TransactionId, UnitF32,
};
pub use imagination::{
    ImaginationBudget, ImaginationConstraint, ImaginationEnvelope, ImaginationRelation,
    ImaginedEntity,
};
pub use limits::{PatchBudget, RuntimeLimits};
pub use message::{
    ApplyReceipt, ApplyStatus, ApplyTiming, ConflictPolicy, CreateEntity, DeleteEntity,
    DeliverySemantic, Diagnostic, DiagnosticSeverity, ImageDimensions, ObservationKind,
    ObservationMetadata, ObservationQuality, ObservationStaleness, QueueConfig, RemoveComponent,
    ReparentEntity, SceneOperation, ScenePatch, SetComponent,
};
pub use query::{SceneEntityView, SceneQuery, SceneQueryResult};
pub use render::{
    RenderChange, RenderComponents, RenderContractError, RenderEntity, RenderExtraction,
};
