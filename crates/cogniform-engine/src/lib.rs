//! Composition boundary for Cogniform's world, render, and observation domains.
//!
//! The engine applies world transactions, forwards compact immutable render
//! extractions, submits revision-linked frames, and completes observations on
//! a bounded worker path. It exposes no mutable ECS or GPU handle.

#![forbid(unsafe_code)]

mod engine;
mod error;
mod gateway;
mod measurement;
mod observation;
mod scenario;
mod service;

pub use cogniform_renderer::AdapterSummary;
pub use engine::{CogniformEngine, EngineConfig, EngineRecoveryPoint};
pub use error::{EngineError, GatewayError, LocalServiceError, ObservationError};
pub use gateway::{
    GatewayAdmission, GatewayCommand, GatewayConfig, GatewayQueueStats, GatewayResponse,
    LocalGateway,
};
pub use measurement::{
    MeasurementError, MeasurementProfile, TimingDistribution, WorldMeasurement,
    measure_controlled_world_fixture,
};
pub use observation::{
    EntityVisibility, Observation, ObservationPayload, ObservationQueue, ObservationRequest,
};
pub use scenario::{
    CanonicalScenarioConfig, CanonicalScenarioError, CanonicalScenarioReport, ObservationEvidence,
    run_canonical_scenario,
};
pub use service::{LocalService, LocalServiceConfig, LocalServiceStatus};
