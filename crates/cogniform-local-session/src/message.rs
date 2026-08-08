use core::num::{NonZeroU32, NonZeroU64};

use cogniform_compilation::{CompilationLimits, CompilationResult};
use cogniform_local_transport::{LOCAL_FRAME_HEADER_BYTES, LocalFrameConfig, LocalFrameLimits};
use cogniform_protocol::{
    ApplyReceipt, ApplyStatus, IdempotencyKey, ImaginationEnvelope, ImaginationId, ObservationId,
    ObservationRequest, RuntimeLimits, ScenePatch, SceneQuery, SceneQueryResult, SceneRevision,
};
use serde::{Deserialize, Serialize};

use crate::{LocalSessionValidationError, LocalSessionValidationKind};

/// Original local-session schema retained byte-for-byte for existing callers.
pub const LOCAL_SESSION_SCHEMA_VERSION: u16 = 1;
/// Local-session schema version that adds bounded semantic imagination.
pub const LOCAL_SESSION_SCHEMA_VERSION_V2: u16 = 2;

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

    /// Selects the field-wise intersection of advertised and local receive limits.
    pub fn negotiate(
        &self,
        local_config: &LocalFrameConfig,
    ) -> Result<Self, LocalSessionValidationError> {
        self.validate()?;
        let local = Self::from_config(local_config)?;
        self.intersect(&local)
    }

    /// Selects the internally consistent field-wise intersection of two peers.
    pub fn intersect(&self, other: &Self) -> Result<Self, LocalSessionValidationError> {
        self.validate()?;
        other.validate()?;
        let runtime_limits = intersect_runtime_limits(self.runtime_limits, other.runtime_limits);
        let max_frame_bytes = self.max_frame_bytes.min(other.max_frame_bytes);
        let frame_body = max_frame_bytes
            .get()
            .checked_sub(u64::try_from(LOCAL_FRAME_HEADER_BYTES).unwrap_or(u64::MAX))
            .ok_or_else(|| invalid_limits("limits.max_frame_bytes"))?;
        let max_bulk_bytes = self.max_bulk_bytes.min(other.max_bulk_bytes);
        let limits = Self {
            max_frame_bytes,
            max_control_message_bytes: self
                .max_control_message_bytes
                .min(other.max_control_message_bytes)
                .min(runtime_limits.max_encoded_bytes)
                .min(
                    NonZeroU64::new(frame_body)
                        .ok_or_else(|| invalid_limits("limits.max_control_message_bytes"))?,
                ),
            max_bulk_bytes,
            max_observation_envelope_bytes: self
                .max_observation_envelope_bytes
                .min(other.max_observation_envelope_bytes)
                .min(max_bulk_bytes),
            max_visibility_entries: self
                .max_visibility_entries
                .min(other.max_visibility_entries),
            runtime_limits,
        };
        limits.validate()?;
        Ok(limits)
    }

    /// Converts negotiated limits into the exact frame configuration they describe.
    pub fn to_frame_config(&self) -> Result<LocalFrameConfig, LocalSessionValidationError> {
        self.validate()?;
        Ok(LocalFrameConfig::with_payload_bounds(
            LocalFrameLimits::new(
                self.max_frame_bytes,
                self.max_control_message_bytes,
                self.max_bulk_bytes,
            ),
            self.runtime_limits,
            self.max_observation_envelope_bytes,
            self.max_visibility_entries,
        ))
    }

    /// Validates internal consistency without comparing against local policy.
    pub fn validate(&self) -> Result<(), LocalSessionValidationError> {
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

fn intersect_runtime_limits(left: RuntimeLimits, right: RuntimeLimits) -> RuntimeLimits {
    let max_components = left.max_components.min(right.max_components);
    RuntimeLimits {
        max_encoded_bytes: left.max_encoded_bytes.min(right.max_encoded_bytes),
        max_decoded_bytes: left.max_decoded_bytes.min(right.max_decoded_bytes),
        max_json_nesting_depth: left
            .max_json_nesting_depth
            .min(right.max_json_nesting_depth),
        max_operations: left.max_operations.min(right.max_operations),
        max_components,
        max_components_per_entity: left
            .max_components_per_entity
            .min(right.max_components_per_entity)
            .min(max_components),
        max_text_bytes: left.max_text_bytes.min(right.max_text_bytes),
        max_diagnostics: left.max_diagnostics.min(right.max_diagnostics),
        max_queue_capacity: left.max_queue_capacity.min(right.max_queue_capacity),
        max_imagination_entities: left
            .max_imagination_entities
            .min(right.max_imagination_entities),
        max_imagination_relations: left
            .max_imagination_relations
            .min(right.max_imagination_relations),
        max_imagination_constraints: left
            .max_imagination_constraints
            .min(right.max_imagination_constraints),
        max_query_entities: left.max_query_entities.min(right.max_query_entities),
        max_observation_width: left.max_observation_width.min(right.max_observation_width),
        max_observation_height: left
            .max_observation_height
            .min(right.max_observation_height),
        max_observation_pixels: left
            .max_observation_pixels
            .min(right.max_observation_pixels),
    }
}

/// Selects the internally consistent field-wise intersection of compilation limits.
pub fn intersect_compilation_limits(
    left: CompilationLimits,
    right: CompilationLimits,
) -> Result<CompilationLimits, LocalSessionValidationError> {
    validate_compilation_limits(&left)?;
    validate_compilation_limits(&right)?;
    let patch_limits = intersect_runtime_limits(left.patch_limits, right.patch_limits);
    let limits = CompilationLimits {
        max_encoded_bytes: left.max_encoded_bytes.min(right.max_encoded_bytes),
        max_decoded_bytes: left.max_decoded_bytes.min(right.max_decoded_bytes),
        max_json_nesting_depth: left
            .max_json_nesting_depth
            .min(right.max_json_nesting_depth),
        max_text_bytes: left.max_text_bytes.min(right.max_text_bytes),
        max_decisions: left.max_decisions.min(right.max_decisions),
        max_unresolved_constraints: left
            .max_unresolved_constraints
            .min(right.max_unresolved_constraints),
        patch_limits,
    };
    validate_compilation_limits(&limits)?;
    Ok(limits)
}

/// Validates one advertised compilation-limit value.
pub fn validate_compilation_limits(
    limits: &CompilationLimits,
) -> Result<(), LocalSessionValidationError> {
    if limits.patch_limits.max_components_per_entity > limits.patch_limits.max_components {
        return Err(LocalSessionValidationError::new(
            LocalSessionValidationKind::InvalidCompilationLimits,
            "compilation_limits.patch_limits.max_components_per_entity",
        ));
    }
    Ok(())
}

/// Reports whether proposed compilation limits fit within available policy.
#[must_use]
pub fn compilation_limits_fit(proposed: &CompilationLimits, available: &CompilationLimits) -> bool {
    proposed.max_encoded_bytes <= available.max_encoded_bytes
        && proposed.max_decoded_bytes <= available.max_decoded_bytes
        && proposed.max_json_nesting_depth <= available.max_json_nesting_depth
        && proposed.max_text_bytes <= available.max_text_bytes
        && proposed.max_decisions <= available.max_decisions
        && proposed.max_unresolved_constraints <= available.max_unresolved_constraints
        && runtime_limits_fit(&proposed.patch_limits, &available.patch_limits)
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

/// Client-to-service message variants across supported session versions.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LocalSessionClientKind {
    /// Opens negotiation and advertises client receive limits.
    Hello(ClientHello),
    /// Submits one bounded core scene patch.
    SubmitPatch(SubmitPatch),
    /// Submits one bounded semantic imagination under schema version two.
    SubmitImagination(SubmitImagination),
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
    /// Bounds compilation results the client accepts; required only in version two.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub compilation_receive_limits: Option<CompilationLimits>,
}

/// One scene-patch submission.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SubmitPatch {
    /// Core schema-owned patch value.
    pub patch: ScenePatch,
}

/// One semantic-imagination submission.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SubmitImagination {
    /// Core schema-owned semantic request.
    pub imagination: ImaginationEnvelope,
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

/// Service-to-client message variants across supported session versions.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LocalSessionServerKind {
    /// Completes negotiation with effective limits.
    Hello(ServerHello),
    /// Reports immediate patch admission without implying completion.
    PatchAdmission(PatchAdmission),
    /// Reports the committed result of one admitted patch.
    PatchCompleted(PatchCompletion),
    /// Reports immediate imagination admission without implying compilation.
    ImaginationAdmission(ImaginationAdmission),
    /// Reports the deterministic compilation and optional committed patch receipt.
    ImaginationCompleted(ImaginationCompletion),
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
    /// Effective compilation-result limits; present only in version two.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effective_compilation_limits: Option<CompilationLimits>,
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

/// Immediate result of semantic-imagination admission.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ImaginationAdmission {
    /// Identity of the submitted semantic request.
    pub imagination_id: ImaginationId,
    /// Idempotency key identifying the submitted request.
    pub idempotency_key: IdempotencyKey,
    /// Admission outcome without compilation or world-mutation side effects.
    pub status: ImaginationAdmissionStatus,
}

/// Stable semantic-imagination admission outcomes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ImaginationAdmissionStatus {
    /// New semantic work was queued.
    Queued,
    /// Identical semantic work was already queued.
    AlreadyQueued,
    /// New latest-value work replaced older uncommitted work.
    Superseded {
        /// Key of the discarded uncommitted command.
        superseded_idempotency_key: IdempotencyKey,
    },
    /// Best-effort work was dropped without admission.
    Dropped,
    /// A retained exact result was returned without recompilation or mutation.
    Replayed {
        /// Cached compilation and replay-marked receipt, when compilation produced a patch.
        completion: Box<ImaginationCompletion>,
    },
}

/// Completed semantic compilation and its optional patch-application result.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ImaginationCompletion {
    /// Identity of the source semantic request.
    pub imagination_id: ImaginationId,
    /// Idempotency key of the source semantic request.
    pub idempotency_key: IdempotencyKey,
    /// Exact bounded deterministic compiler result.
    pub compilation: CompilationResult,
    /// Apply receipt when and only when compilation produced a patch.
    pub receipt: Option<ApplyReceipt>,
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
    fn validate(
        &self,
        config: &LocalFrameConfig,
        compilation_limits: Option<&CompilationLimits>,
    ) -> Result<(), LocalSessionValidationError>;
}

impl SessionValidate for LocalSessionClientMessage {
    fn validate(
        &self,
        config: &LocalFrameConfig,
        _compilation_limits: Option<&CompilationLimits>,
    ) -> Result<(), LocalSessionValidationError> {
        validate_version(self.schema_version)?;
        let limits = &config.runtime_limits;
        match &self.message {
            LocalSessionClientKind::Hello(hello) => {
                hello.receive_limits.validate()?;
                validate_client_hello(self.schema_version, hello)
            }
            LocalSessionClientKind::SubmitPatch(value) => {
                value.patch.validate_with_limits(limits).map_err(|error| {
                    LocalSessionValidationError::protocol("message.submit_patch.patch", error)
                })
            }
            LocalSessionClientKind::SubmitImagination(value) => {
                require_v2(self.schema_version, "message.submit_imagination")?;
                value
                    .imagination
                    .validate_with_limits(limits)
                    .map_err(|error| {
                        LocalSessionValidationError::protocol(
                            "message.submit_imagination.imagination",
                            error,
                        )
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
    fn validate(
        &self,
        config: &LocalFrameConfig,
        compilation_limits: Option<&CompilationLimits>,
    ) -> Result<(), LocalSessionValidationError> {
        validate_version(self.schema_version)?;
        let requires_explicit_compilation_limits = matches!(
            &self.message,
            LocalSessionServerKind::Hello(_)
                | LocalSessionServerKind::ImaginationCompleted(_)
                | LocalSessionServerKind::ImaginationAdmission(ImaginationAdmission {
                    status: ImaginationAdmissionStatus::Replayed { .. },
                    ..
                })
        );
        if self.schema_version == LOCAL_SESSION_SCHEMA_VERSION_V2
            && compilation_limits.is_none()
            && requires_explicit_compilation_limits
        {
            return Err(LocalSessionValidationError::new(
                LocalSessionValidationKind::InvalidCompilationLimits,
                "message.version_two.explicit_compilation_limits",
            ));
        }
        let limits = &config.runtime_limits;
        let default_compilation_limits = CompilationLimits::for_runtime_limits(*limits);
        let compilation_limits = compilation_limits.unwrap_or(&default_compilation_limits);
        validate_compilation_limits(compilation_limits)?;
        match &self.message {
            LocalSessionServerKind::Hello(hello) => {
                hello.effective_limits.validate()?;
                hello.effective_limits.fits_within_config(config)?;
                validate_server_hello(self.schema_version, hello, compilation_limits)
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
            LocalSessionServerKind::ImaginationAdmission(value) => {
                require_v2(self.schema_version, "message.imagination_admission")?;
                value.validate(limits, compilation_limits)
            }
            LocalSessionServerKind::ImaginationCompleted(value) => {
                require_v2(self.schema_version, "message.imagination_completed")?;
                value.validate(limits, compilation_limits, ApplyStatus::Applied)
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

impl LocalSessionClientMessage {
    /// Validates one message under explicit frame and compilation-result bounds.
    pub fn validate_with_limits(
        &self,
        config: &LocalFrameConfig,
        compilation_limits: &CompilationLimits,
    ) -> Result<(), LocalSessionValidationError> {
        self.validate(config, Some(compilation_limits))
    }
}

impl LocalSessionServerMessage {
    /// Validates one message under explicit negotiated frame and compilation bounds.
    pub fn validate_with_limits(
        &self,
        config: &LocalFrameConfig,
        compilation_limits: &CompilationLimits,
    ) -> Result<(), LocalSessionValidationError> {
        self.validate(config, Some(compilation_limits))
    }
}

fn validate_client_hello(
    version: u16,
    hello: &ClientHello,
) -> Result<(), LocalSessionValidationError> {
    match (version, hello.compilation_receive_limits.as_ref()) {
        (LOCAL_SESSION_SCHEMA_VERSION, None) => Ok(()),
        (LOCAL_SESSION_SCHEMA_VERSION_V2, Some(limits)) => {
            validate_compilation_limits(limits)?;
            if runtime_limits_fit(&limits.patch_limits, &hello.receive_limits.runtime_limits) {
                Ok(())
            } else {
                Err(LocalSessionValidationError::new(
                    LocalSessionValidationKind::InvalidCompilationLimits,
                    "message.hello.compilation_receive_limits.patch_limits",
                ))
            }
        }
        _ => Err(LocalSessionValidationError::new(
            LocalSessionValidationKind::InvalidVersionVariant,
            "message.hello.compilation_receive_limits",
        )),
    }
}

fn validate_server_hello(
    version: u16,
    hello: &ServerHello,
    available: &CompilationLimits,
) -> Result<(), LocalSessionValidationError> {
    match (version, hello.effective_compilation_limits.as_ref()) {
        (LOCAL_SESSION_SCHEMA_VERSION, None) => Ok(()),
        (LOCAL_SESSION_SCHEMA_VERSION_V2, Some(limits)) => {
            validate_compilation_limits(limits)?;
            if compilation_limits_fit(limits, available)
                && runtime_limits_fit(&limits.patch_limits, &hello.effective_limits.runtime_limits)
            {
                Ok(())
            } else {
                Err(LocalSessionValidationError::new(
                    LocalSessionValidationKind::InvalidCompilationLimits,
                    "message.hello.effective_compilation_limits",
                ))
            }
        }
        _ => Err(LocalSessionValidationError::new(
            LocalSessionValidationKind::InvalidVersionVariant,
            "message.hello.effective_compilation_limits",
        )),
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

impl ImaginationAdmission {
    fn validate(
        &self,
        runtime_limits: &RuntimeLimits,
        compilation_limits: &CompilationLimits,
    ) -> Result<(), LocalSessionValidationError> {
        if let ImaginationAdmissionStatus::Superseded {
            superseded_idempotency_key,
        } = &self.status
            && *superseded_idempotency_key == self.idempotency_key
        {
            return Err(LocalSessionValidationError::new(
                LocalSessionValidationKind::InvalidImaginationAdmission,
                "message.imagination_admission.status.superseded.superseded_idempotency_key",
            ));
        }
        if let ImaginationAdmissionStatus::Replayed { completion } = &self.status {
            if completion.imagination_id != self.imagination_id
                || completion.idempotency_key != self.idempotency_key
            {
                return Err(LocalSessionValidationError::new(
                    LocalSessionValidationKind::InvalidImaginationAdmission,
                    "message.imagination_admission.status.replayed.completion",
                ));
            }
            completion.validate(
                runtime_limits,
                compilation_limits,
                ApplyStatus::IdempotentReplay,
            )?;
        }
        Ok(())
    }
}

impl ImaginationCompletion {
    fn validate(
        &self,
        runtime_limits: &RuntimeLimits,
        compilation_limits: &CompilationLimits,
        expected_status: ApplyStatus,
    ) -> Result<(), LocalSessionValidationError> {
        self.compilation
            .to_canonical_json(compilation_limits)
            .map_err(|_| {
                LocalSessionValidationError::new(
                    LocalSessionValidationKind::InvalidCompilationResult,
                    "message.imagination_completed.compilation",
                )
            })?;
        if self.compilation.imagination_id != self.imagination_id {
            return Err(invalid_imagination_completion(
                "message.imagination_completed.imagination_id",
            ));
        }
        match (&self.compilation.patch, &self.receipt) {
            (Some(patch), Some(receipt)) => {
                receipt
                    .validate_with_limits(runtime_limits)
                    .map_err(|error| {
                        LocalSessionValidationError::protocol(
                            "message.imagination_completed.receipt",
                            error,
                        )
                    })?;
                let operation_count = u32::try_from(patch.operations.len()).unwrap_or(u32::MAX);
                if receipt.status != expected_status
                    || self.idempotency_key != patch.idempotency_key
                    || receipt.idempotency_key != self.idempotency_key
                    || receipt.transaction_id != patch.transaction_id
                    || receipt.previous_revision != self.compilation.scene_revision
                    || patch.base_revision != self.compilation.scene_revision
                    || receipt.operation_count.get() != operation_count
                {
                    return Err(invalid_imagination_completion(
                        "message.imagination_completed.receipt",
                    ));
                }
                Ok(())
            }
            (None, None) => Ok(()),
            _ => Err(invalid_imagination_completion(
                "message.imagination_completed.receipt",
            )),
        }
    }
}

fn validate_version(version: u16) -> Result<(), LocalSessionValidationError> {
    if version == LOCAL_SESSION_SCHEMA_VERSION || version == LOCAL_SESSION_SCHEMA_VERSION_V2 {
        Ok(())
    } else {
        Err(LocalSessionValidationError::new(
            LocalSessionValidationKind::UnsupportedVersion,
            "schema_version",
        ))
    }
}

fn require_v2(version: u16, field: &'static str) -> Result<(), LocalSessionValidationError> {
    if version == LOCAL_SESSION_SCHEMA_VERSION_V2 {
        Ok(())
    } else {
        Err(LocalSessionValidationError::new(
            LocalSessionValidationKind::InvalidVersionVariant,
            field,
        ))
    }
}

fn invalid_imagination_completion(field: &'static str) -> LocalSessionValidationError {
    LocalSessionValidationError::new(
        LocalSessionValidationKind::InvalidImaginationCompletion,
        field,
    )
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
