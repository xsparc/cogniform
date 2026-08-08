//! Composition boundary for Cogniform's world, render, and observation domains.
//!
//! The engine applies world transactions, forwards compact immutable render
//! extractions and asset-upload values, submits revision-linked frames, and
//! completes observations on a bounded worker path. The local service also
//! owns caller-driven bounded asset ingestion. No API exposes a mutable ECS or
//! GPU handle.

#![forbid(unsafe_code)]

mod engine;
mod error;
mod gateway;
mod measurement;
mod observation;
mod recovery;
mod scenario;
mod service;

pub use cogniform_assets::{
    AssetAdmission, AssetDiagnostic, AssetDiagnosticCode, AssetError, AssetLimits, AssetMaterial,
    AssetMeshKey, AssetProcessOutcome, AssetRecord, AssetState, AssetStoreConfig,
    AssetStoreEviction, AssetStoreStats, AssetUploadJob, AssetVertex, UnsupportedAssetPolicy,
    content_hash,
};
pub use cogniform_observation::{
    EntityVisibility, ObservationEnvelopeError, ObservationPayload, ObservationPayloadLimits,
};
pub use cogniform_procedural::{
    BuiltinProcedure, CuboidGrid, ProcedureError, ProcedureLimits, ProcedureRequest,
};
pub use cogniform_protocol::ObservationRequest;
pub use cogniform_renderer::{
    AdapterSummary, AssetUploadAdmission, AssetUploadOutcome, RendererAssetEviction,
    RendererAssetStats, RendererError,
};
pub use engine::{CogniformEngine, EngineConfig, inspect_recovery_point};
pub use error::{EngineError, GatewayError, LocalRevertError, LocalServiceError, ObservationError};
pub use gateway::{
    GatewayAdmission, GatewayCommand, GatewayConfig, GatewayQueueStats, GatewayResponse,
    LocalGateway,
};
pub use measurement::{
    MeasurementError, MeasurementProfile, TimingDistribution, WorldMeasurement,
    measure_controlled_world_fixture,
};
pub use observation::{Observation, ObservationQueue};
pub use recovery::{EngineRecoveryPoint, RecoveryInspection, RecoveryPointCodecError};
pub use scenario::{
    CanonicalScenarioConfig, CanonicalScenarioError, CanonicalScenarioReport, ObservationEvidence,
    run_canonical_scenario,
};
pub use service::{
    LocalAssetEvictionOutcome, LocalAssetStatus, LocalRevertReceipt, LocalService,
    LocalServiceConfig, LocalServiceStatus, ProcedureSubmission,
};
