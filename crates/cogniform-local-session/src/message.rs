use core::num::{NonZeroU32, NonZeroU64};

use cogniform_local_transport::{LOCAL_FRAME_HEADER_BYTES, LocalFrameConfig};
use cogniform_protocol::{
    ApplyReceipt, ApplyStatus, IdempotencyKey, ObservationId, ObservationRequest, RuntimeLimits,
    ScenePatch, SceneQuery, SceneQueryResult, SceneRevision,
};
use serde::{Deserialize, Serialize};

use crate::{LocalSessionValidationError, LocalSessionValidationKind};

/// The only local-session control-message schema supported by this crate.
pub const LOCAL_SESSION_SCHEMA_VERSION: u16 = 1;

/// Bounded receive limits exchanged during the explicit hello handshake.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LocalSessionLimits {
    /// Maximum complete CF039 frame bytes accepted by the receiver.
    pub max_frame_bytes: NonZeroU64,
    /// Effective maximum canonical session control-message bytes, including LF.
    pub max_control_message_bytes: NonZeroU64,
    /// Maximum CF039 observation bulk-section bytes accepted by the receiver.
    pub max_bulk_bytes: NonZeroU64,
    /// Maximum complete CF038 observation-payload envelope bytes.
    pub max_observation_envelope_bytes: NonZeroU64,
    /// Maximum entries in one structured visibility observation.
    pub max_visibility_entries: NonZeroU32,
    /// Core protocol admission limits used for nested values.
    pub runtime_limits: RuntimeLimits,
}

impl LocalSessionLimits {
    /// Derives the exact receive limits enforced by a local frame configuration.
    pub fn from_config(config: &LocalFrameConfig) -> Result<Self, LocalSessionValidationError> {
        let header = u64::try_from(LOCAL_FRAME_HEADER_BYTES).unwrap_or(u64::MAX);
        let frame_body = config
            .frame_limits
            .max_frame_bytes
            .get()
            .checked_sub(header)
            .ok_or_else(|| invalid_limits("limits.max_frame_bytes"))?;
        let effective_control = frame_body
            .min(config.frame_limits.max_control_bytes.get())
            .min(config.runtime_limits.max_encoded_bytes.get());
        let max_control_message_bytes = NonZeroU64::new(effective_control)
            .ok_or_else(|| invalid_limits("limits.max_control_message_bytes"))?;
        let effective_bulk = frame_body.min(config.frame_limits.max_bulk_bytes.get());
        let max_bulk_bytes = NonZeroU64::new(effective_bulk)
            .ok_or_else(|| invalid_limits("limits.max_bulk_bytes"))?;
        let max_observation_envelope_bytes =
            NonZeroU64::new(effective_bulk.min(config.payload_limits.max_envelope_bytes.get()))
                .ok_or_else(|| invalid_limits("limits.max_observation_envelope_bytes"))?;
        let limits = Self {
            max_frame_bytes: config.frame_limits.max_frame_bytes,
            max_control_message_bytes,
            max_bulk_bytes,
            max_observation_envelope_bytes,
            max_visibility_entries: config.payload_limits.max_visibility_entries,
            runtime_limits: config.runtime_limits,
        };
        limits.validate()?;
        Ok(limits)
    }

    pub(crate) fn validate(&self) -> Result<(), LocalSessionValidationError> {
        let header = u64::try_from(LOCAL_FRAME_HEADER_BYTES).unwrap_or(u64::MAX);
        let Some(frame_body) = self.max_frame_bytes.get().checked_sub(header) else {
            return Err(invalid_limits("limits.max_frame_bytes"));
        };
        if self.max_control_message_bytes.get() > frame_body
            || self.max_control_message_bytes.get() > self.runtime_limits.max_encoded_bytes.get()
        {
            return Err(invalid_limits("limits.max_control_message_bytes"));
        }
        if self.max_bulk_bytes.get() > frame_body {
            return Err(invalid_limits("limits.max_bulk_bytes"));
        }
        if self.max_observation_envelope_bytes.get() > self.max_bulk_bytes.get() {
            return Err(invalid_limits("limits.max_observation_envelope_bytes"));
        }
        if self.runtime_limits.max_components_per_entity.get()
            > self.runtime_limits.max_components.get()
        {
            return Err(invalid_limits(
                "limits.runtime_limits.max_components_per_entity",
            ));
        }
        Ok(())
    }

    fn fits_within_config(
        &self,
        config: &LocalFrameConfig,
    ) -> Result<(), LocalSessionValidationError> {
        let available = Self::from_config(config)?;
        if self.max_frame_bytes.get() > available.max_frame_bytes.get()
            || self.max_control_message_bytes.get() > available.max_control_message_bytes.get()
            || self.max_bulk_bytes.get() > available.max_bulk_bytes.get()
            || self.max_observation_envelope_bytes.get()
                > available.max_observation_envelope_bytes.get()
            || self.max_visibility_entries.get() > available.max_visibility_entries.get()
            || !runtime_limits_fit(&self.runtime_limits, &available.runtime_limits)
        {
            return Err(invalid_limits("message.hello.effective_limits"));
        }
        Ok(())
    }
}

/// One versioned client-to-service local control message.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LocalSessionClientMessage {
    /// Local-session schema version.
    pub schema_version: u16,
    /// Direction-specific client message.
    pub message: LocalSessionClientKind,
}

/// Client-to-service message variants admitted by schema version one.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LocalSessionClientKind {
    /// Opens negotiation and advertises client receive limits.
    Hello(ClientHello),
    /// Submits one bounded core scene patch.
    SubmitPatch(SubmitPatch),
    /// Requests one exact-revision logical query.
    Query(QueryRequest),
    /// Requests one exact-revision machine observation.
    RequestObservation(RequestObservation),
    /// Requests orderly local-session closure.
    Close(SessionClose),
}

/// Initial client receive-limit advertisement.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClientHello {
    /// Bounds the client promises to enforce for server output.
    pub receive_limits: LocalSessionLimits,
}

/// One scene-patch submission.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SubmitPatch {
    /// Core schema-owned patch value.
    pub patch: ScenePatch,
}

/// One exact-revision query request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QueryRequest {
    /// Core schema-owned query value.
    pub query: SceneQuery,
}

/// One exact-revision observation request.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RequestObservation {
    /// Core schema-owned observation request.
    pub request: ObservationRequest,
}

/// Empty close payload; unknown fields remain rejected.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SessionClose {}

/// One versioned service-to-client local control message.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LocalSessionServerMessage {
    /// Local-session schema version.
    pub schema_version: u16,
    /// Direction-specific server message.
    pub message: LocalSessionServerKind,
}

/// Service-to-client message variants emitted by schema version one.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LocalSessionServerKind {
    /// Completes negotiation with effective limits.
    Hello(ServerHello),
    /// Reports immediate patch admission without implying completion.
    PatchAdmission(PatchAdmission),
    /// Reports the committed result of one admitted patch.
    PatchCompleted(PatchCompletion),
    /// Returns one exact-revision logical query result.
    QueryResult(QueryResponse),
    /// Confirms observation work was accepted.
    ObservationAccepted(ObservationReference),
    /// Reports that an accepted observation is not complete yet.
    ObservationPending(ObservationReference),
    /// Reports one stable payload-redacted failure.
    Failure(SessionFailure),
    /// Confirms orderly local-session closure.
    Closed(SessionClosed),
}

/// Server-selected receive and output limits.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ServerHello {
    /// Effective limits for this local session.
    pub effective_limits: LocalSessionLimits,
}

/// Immediate result of patch admission.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PatchAdmission {
    /// Key identifying the submitted patch.
    pub idempotency_key: IdempotencyKey,
    /// Admission outcome without queue execution side effects.
    pub status: PatchAdmissionStatus,
}

/// Stable patch-admission outcomes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PatchAdmissionStatus {
    /// New patch work was queued.
    Queued,
    /// Identical patch work was already queued.
    AlreadyQueued,
    /// New latest-value work replaced older uncommitted work.
    Superseded {
        /// Key of the discarded uncommitted patch.
        superseded_idempotency_key: IdempotencyKey,
    },
    /// Best-effort work was dropped without admission.
    Dropped,
    /// A prior committed result was returned without re-execution.
    Replayed {
        /// Original receipt marked as an idempotent replay.
        receipt: ApplyReceipt,
    },
}

/// Completed committed patch result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PatchCompletion {
    /// Newly applied receipt; replay responses are admission outcomes instead.
    pub receipt: ApplyReceipt,
}

/// One exact-revision query response.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QueryResponse {
    /// Core schema-owned query result.
    pub result: SceneQueryResult,
}

/// Stable identity of accepted or pending observation work.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ObservationReference {
    /// Identity of the eventual observation frame.
    pub observation_id: ObservationId,
    /// Exact authoritative scene revision being observed.
    pub scene_revision: SceneRevision,
}

/// Payload-redacted local-session failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SessionFailure {
    /// Stable machine-readable failure classification.
    pub code: SessionFailureCode,
}

/// Stable failure codes that do not expose input, parser text, or internals.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum SessionFailureCode {
    /// Control bytes did not match the session schema.
    InvalidMessage,
    /// The declared session schema version is unsupported.
    UnsupportedVersion,
    /// The message is invalid in the current session state.
    ProtocolState,
    /// A configured or negotiated resource limit was exceeded.
    LimitExceeded,
    /// An exact scene revision did not match current state.
    RevisionMismatch,
    /// A bounded queue or result capacity was exhausted.
    CapacityExceeded,
    /// A submitted command was rejected.
    CommandRejected,
    /// A logical query was rejected.
    QueryRejected,
    /// An observation request was rejected.
    ObservationRejected,
    /// The local service is temporarily unavailable.
    ServiceUnavailable,
    /// A non-sensitive internal failure occurred.
    Internal,
}

/// Empty orderly-close acknowledgement.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SessionClosed {}

pub(crate) trait SessionValidate {
    fn validate(&self, config: &LocalFrameConfig) -> Result<(), LocalSessionValidationError>;
}

impl SessionValidate for LocalSessionClientMessage {
    fn validate(&self, config: &LocalFrameConfig) -> Result<(), LocalSessionValidationError> {
        validate_version(self.schema_version)?;
        let limits = &config.runtime_limits;
        match &self.message {
            LocalSessionClientKind::Hello(hello) => hello.receive_limits.validate(),
            LocalSessionClientKind::SubmitPatch(value) => {
                value.patch.validate_with_limits(limits).map_err(|error| {
                    LocalSessionValidationError::protocol("message.submit_patch.patch", error)
                })
            }
            LocalSessionClientKind::Query(value) => {
                value.query.validate_with_limits(limits).map_err(|error| {
                    LocalSessionValidationError::protocol("message.query.query", error)
                })
            }
            LocalSessionClientKind::RequestObservation(value) => {
                value.request.validate_with_limits(limits).map_err(|error| {
                    LocalSessionValidationError::protocol(
                        "message.request_observation.request",
                        error,
                    )
                })
            }
            LocalSessionClientKind::Close(_) => Ok(()),
        }
    }
}

impl SessionValidate for LocalSessionServerMessage {
    fn validate(&self, config: &LocalFrameConfig) -> Result<(), LocalSessionValidationError> {
        validate_version(self.schema_version)?;
        let limits = &config.runtime_limits;
        match &self.message {
            LocalSessionServerKind::Hello(hello) => {
                hello.effective_limits.validate()?;
                hello.effective_limits.fits_within_config(config)
            }
            LocalSessionServerKind::PatchAdmission(value) => value.validate(limits),
            LocalSessionServerKind::PatchCompleted(value) => {
                value
                    .receipt
                    .validate_with_limits(limits)
                    .map_err(|error| {
                        LocalSessionValidationError::protocol(
                            "message.patch_completed.receipt",
                            error,
                        )
                    })?;
                if value.receipt.status != ApplyStatus::Applied {
                    return Err(LocalSessionValidationError::new(
                        LocalSessionValidationKind::InvalidPatchCompletion,
                        "message.patch_completed.receipt.status",
                    ));
                }
                Ok(())
            }
            LocalSessionServerKind::QueryResult(value) => {
                value.result.validate_with_limits(limits).map_err(|error| {
                    LocalSessionValidationError::protocol("message.query_result.result", error)
                })
            }
            LocalSessionServerKind::ObservationAccepted(_)
            | LocalSessionServerKind::ObservationPending(_)
            | LocalSessionServerKind::Failure(_)
            | LocalSessionServerKind::Closed(_) => Ok(()),
        }
    }
}

impl PatchAdmission {
    fn validate(&self, limits: &RuntimeLimits) -> Result<(), LocalSessionValidationError> {
        if let PatchAdmissionStatus::Superseded {
            superseded_idempotency_key,
        } = &self.status
            && *superseded_idempotency_key == self.idempotency_key
        {
            return Err(LocalSessionValidationError::new(
                LocalSessionValidationKind::InvalidPatchAdmission,
                "message.patch_admission.status.superseded.superseded_idempotency_key",
            ));
        }
        if let PatchAdmissionStatus::Replayed { receipt } = &self.status {
            receipt.validate_with_limits(limits).map_err(|error| {
                LocalSessionValidationError::protocol(
                    "message.patch_admission.status.replayed.receipt",
                    error,
                )
            })?;
            if receipt.status != ApplyStatus::IdempotentReplay
                || receipt.idempotency_key != self.idempotency_key
            {
                return Err(LocalSessionValidationError::new(
                    LocalSessionValidationKind::InvalidPatchAdmission,
                    "message.patch_admission.status.replayed.receipt",
                ));
            }
        }
        Ok(())
    }
}

fn validate_version(version: u16) -> Result<(), LocalSessionValidationError> {
    if version == LOCAL_SESSION_SCHEMA_VERSION {
        Ok(())
    } else {
        Err(LocalSessionValidationError::new(
            LocalSessionValidationKind::UnsupportedVersion,
            "schema_version",
        ))
    }
}

fn invalid_limits(field: &'static str) -> LocalSessionValidationError {
    LocalSessionValidationError::new(LocalSessionValidationKind::InvalidLimits, field)
}

fn runtime_limits_fit(proposed: &RuntimeLimits, available: &RuntimeLimits) -> bool {
    proposed.max_encoded_bytes <= available.max_encoded_bytes
        && proposed.max_decoded_bytes <= available.max_decoded_bytes
        && proposed.max_json_nesting_depth <= available.max_json_nesting_depth
        && proposed.max_operations <= available.max_operations
        && proposed.max_components <= available.max_components
        && proposed.max_components_per_entity <= available.max_components_per_entity
        && proposed.max_text_bytes <= available.max_text_bytes
        && proposed.max_diagnostics <= available.max_diagnostics
        && proposed.max_queue_capacity <= available.max_queue_capacity
        && proposed.max_imagination_entities <= available.max_imagination_entities
        && proposed.max_imagination_relations <= available.max_imagination_relations
        && proposed.max_imagination_constraints <= available.max_imagination_constraints
        && proposed.max_query_entities <= available.max_query_entities
        && proposed.max_observation_width <= available.max_observation_width
        && proposed.max_observation_height <= available.max_observation_height
        && proposed.max_observation_pixels <= available.max_observation_pixels
}
