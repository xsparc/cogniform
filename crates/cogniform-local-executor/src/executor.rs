use core::num::{NonZeroU32, NonZeroU64};
use std::collections::{BTreeMap, BTreeSet, VecDeque};

use cogniform_compilation::{CompilationLimits, CompilationResult};
use cogniform_engine::{
    EngineError, GatewayAdmission, GatewayError, GatewayResponse, LocalService, LocalServiceError,
    ObservationDelivery, ObservationError, ObservationPayload,
};
use cogniform_local_session::{
    ImaginationAdmission, ImaginationAdmissionStatus, ImaginationCompletion,
    LOCAL_SESSION_SCHEMA_VERSION, LOCAL_SESSION_SCHEMA_VERSION_V2, LocalSessionClientKind,
    LocalSessionError, LocalSessionLimits, LocalSessionServerKind, LocalSessionServerMessage,
    ObservationReference, PatchAdmission, PatchAdmissionStatus, PatchCompletion, QueryResponse,
    ServerHello, SessionClosed, SessionFailure, SessionFailureCode, decode_client_control_frame,
    decode_client_control_frame_with_limits, intersect_compilation_limits, server_control_frame,
    server_control_frame_with_limits,
};
use cogniform_local_transport::{LocalFrame, LocalFrameConfig, encode_frame};
use cogniform_protocol::{
    ApplyStatus, IdempotencyKey, ImaginationEnvelope, ImaginationId, ObservationId,
    ObservationMetadata, ObservationRequest, RuntimeLimits, ScenePatch, SceneQuery,
    SceneQueryResult, SceneRevision, TransactionId,
};

use crate::LocalExecutorError;

/// Maximum frames returned by one caller-driven executor operation.
pub const MAX_OUTPUT_FRAMES_PER_CALL: usize = 2;

/// Explicit local receive and correlation bounds for one executor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalExecutorConfig {
    /// Local receive policy used before and during negotiation.
    pub receive_config: LocalFrameConfig,
    /// Local policy for compilation results sent through schema version two.
    pub compilation_limits: CompilationLimits,
    /// Maximum command and observation correlations live at once.
    pub max_live_correlations: NonZeroU32,
    /// Exact maximum frames returned by one handle or advance call.
    pub max_output_frames_per_call: NonZeroU32,
}

impl Default for LocalExecutorConfig {
    fn default() -> Self {
        Self {
            receive_config: LocalFrameConfig::default(),
            compilation_limits: CompilationLimits::default(),
            max_live_correlations: NonZeroU32::new(1_024).expect("constant is non-zero"),
            max_output_frames_per_call: NonZeroU32::new(
                u32::try_from(MAX_OUTPUT_FRAMES_PER_CALL).expect("constant fits u32"),
            )
            .expect("constant is non-zero"),
        }
    }
}

/// Public local-session lifecycle phase.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LocalExecutorPhase {
    /// Exactly one client hello is required before service work.
    AwaitingHello,
    /// Negotiation completed and ordinary client messages are admitted.
    Active,
    /// An orderly close completed; later messages are rejected.
    Closed,
}

/// Bounded executor occupancy without request or payload contents.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LocalExecutorStatus {
    /// Current protocol lifecycle phase.
    pub phase: LocalExecutorPhase,
    /// Command and observation correlations awaiting terminal output.
    pub live_correlations: u32,
    /// Fixed live-correlation capacity checked against service capacities.
    pub live_correlation_capacity: u32,
    /// Exact maximum frames returned by one operation.
    pub max_output_frames_per_call: u32,
    /// Admitted patches awaiting one-command advancement.
    pub pending_patches: u32,
    /// Admitted semantic requests awaiting one-command advancement.
    pub pending_imaginations: u32,
    /// Accepted observations awaiting completion delivery.
    pub pending_observations: u32,
}

/// One caller-driven local session over an owned typed service.
pub struct LocalSessionExecutor {
    core: SessionCore<LocalService>,
}

impl std::fmt::Debug for LocalSessionExecutor {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("LocalSessionExecutor")
            .field("status", &self.status())
            .finish_non_exhaustive()
    }
}

impl LocalSessionExecutor {
    /// Wraps one quiescent service without initializing I/O or background runtime work.
    pub fn new(
        service: LocalService,
        config: LocalExecutorConfig,
    ) -> Result<Self, LocalExecutorError> {
        Ok(Self {
            core: SessionCore::new(service, config)?,
        })
    }

    /// Handles one already-framed client value and returns bounded immediate output.
    pub fn handle_frame(
        &mut self,
        frame: &LocalFrame,
    ) -> Result<Vec<LocalFrame>, LocalExecutorError> {
        self.core.handle_frame(frame)
    }

    /// Processes at most one command and polls at most one observation completion.
    pub fn advance(&mut self) -> Result<Vec<LocalFrame>, LocalExecutorError> {
        self.core.advance()
    }

    /// Returns bounded session-owned lifecycle and occupancy counters.
    #[must_use]
    pub fn status(&self) -> LocalExecutorStatus {
        self.core.status()
    }

    /// Returns the effective limits after successful hello negotiation.
    #[must_use]
    pub const fn negotiated_limits(&self) -> Option<LocalSessionLimits> {
        self.core.negotiated_limits()
    }

    /// Returns effective compilation-result limits after a version-two hello.
    #[must_use]
    pub const fn negotiated_compilation_limits(&self) -> Option<CompilationLimits> {
        self.core.negotiated_compilation_limits()
    }

    /// Returns read-only access to the owned local service.
    #[must_use]
    pub const fn service(&self) -> &LocalService {
        &self.core.service
    }

    /// Releases the owned service only after orderly close and complete correlation release.
    pub fn into_service(self) -> Result<LocalService, Box<Self>> {
        if self.status().phase == LocalExecutorPhase::Closed && self.status().live_correlations == 0
        {
            Ok(self.core.service)
        } else {
            Err(Box::new(self))
        }
    }
}

#[derive(Debug, Clone)]
enum SessionState {
    AwaitingHello,
    Active {
        schema_version: u16,
        limits: LocalSessionLimits,
        compilation_limits: Option<CompilationLimits>,
        frame_config: LocalFrameConfig,
    },
    Closed {
        schema_version: u16,
        limits: LocalSessionLimits,
        compilation_limits: Option<CompilationLimits>,
        frame_config: LocalFrameConfig,
    },
}

struct SessionCore<S> {
    service: S,
    config: LocalExecutorConfig,
    state: SessionState,
    live_correlations: BTreeSet<NonZeroU64>,
    command_correlations: BTreeMap<IdempotencyKey, PendingCommand>,
    command_order: VecDeque<IdempotencyKey>,
    observation_correlations: BTreeMap<ObservationId, PendingObservation>,
    observation_order: VecDeque<ObservationId>,
    observation_pending_reported: BTreeSet<ObservationId>,
}

impl<S: SessionService> SessionCore<S> {
    fn new(service: S, mut config: LocalExecutorConfig) -> Result<Self, LocalExecutorError> {
        let configured_limits =
            LocalSessionLimits::from_config(&config.receive_config).map_err(|_| {
                LocalExecutorError::InvalidConfig {
                    reason: "receive limits are inconsistent",
                }
            })?;
        let service_runtime_limits = service.runtime_limits();
        let service_limits = LocalSessionLimits {
            max_control_message_bytes: configured_limits
                .max_control_message_bytes
                .min(service_runtime_limits.max_encoded_bytes),
            runtime_limits: service_runtime_limits,
            ..configured_limits
        };
        let effective_local_limits =
            configured_limits.intersect(&service_limits).map_err(|_| {
                LocalExecutorError::InvalidConfig {
                    reason: "service and receive limits are inconsistent",
                }
            })?;
        config.receive_config = effective_local_limits.to_frame_config().map_err(|_| {
            LocalExecutorError::InvalidConfig {
                reason: "effective local receive limits are inconsistent",
            }
        })?;
        let service_compilation_limits =
            CompilationLimits::for_runtime_limits(service_runtime_limits);
        let receive_compilation_limits =
            CompilationLimits::for_runtime_limits(effective_local_limits.runtime_limits);
        config.compilation_limits =
            intersect_compilation_limits(config.compilation_limits, service_compilation_limits)
                .and_then(|limits| intersect_compilation_limits(limits, receive_compilation_limits))
                .map_err(|_| LocalExecutorError::InvalidConfig {
                    reason: "compilation limits are inconsistent",
                })?;
        config.max_live_correlations = NonZeroU32::new(
            config.max_live_correlations.get().min(
                config
                    .receive_config
                    .runtime_limits
                    .max_queue_capacity
                    .get(),
            ),
        )
        .expect("the intersection of non-zero capacities is non-zero");
        if usize::try_from(config.max_output_frames_per_call.get()).unwrap_or(usize::MAX)
            != MAX_OUTPUT_FRAMES_PER_CALL
        {
            return Err(LocalExecutorError::InvalidConfig {
                reason: "output frame capacity must match the protocol maximum",
            });
        }
        let snapshot = service.snapshot();
        if snapshot.command_depth != 0 || snapshot.outstanding_observations != 0 {
            return Err(LocalExecutorError::ServiceNotQuiescent {
                commands: snapshot.command_depth,
                observations: snapshot.outstanding_observations,
            });
        }
        if service.command_capacity() > service.runtime_limits().max_queue_capacity.get()
            || service.observation_capacity() > service.runtime_limits().max_queue_capacity.get()
        {
            return Err(LocalExecutorError::InvalidConfig {
                reason: "service capacity exceeds its runtime queue bound",
            });
        }
        let service_correlation_capacity = service
            .command_capacity()
            .checked_add(service.observation_capacity())
            .ok_or(LocalExecutorError::InvalidConfig {
                reason: "combined service capacity overflowed",
            })?;
        config.max_live_correlations = NonZeroU32::new(
            config
                .max_live_correlations
                .get()
                .min(service_correlation_capacity),
        )
        .ok_or(LocalExecutorError::InvalidConfig {
            reason: "combined service capacity is zero",
        })?;
        Ok(Self {
            service,
            config,
            state: SessionState::AwaitingHello,
            live_correlations: BTreeSet::new(),
            command_correlations: BTreeMap::new(),
            command_order: VecDeque::new(),
            observation_correlations: BTreeMap::new(),
            observation_order: VecDeque::new(),
            observation_pending_reported: BTreeSet::new(),
        })
    }

    fn handle_frame(&mut self, frame: &LocalFrame) -> Result<Vec<LocalFrame>, LocalExecutorError> {
        let correlation_id = frame.correlation_id();
        let decode_config = self.current_frame_config().clone();
        let negotiated_compilation_limits = self.negotiated_compilation_limits();
        let decoded = if let Some(limits) = negotiated_compilation_limits.as_ref() {
            decode_client_control_frame_with_limits(frame, &decode_config, limits)
        } else {
            decode_client_control_frame(frame, &decode_config)
        };
        let message = match decoded {
            Ok((_, message)) => message,
            Err(error) => {
                let code = classify_session_error(&error);
                return match self.state {
                    SessionState::AwaitingHello => {
                        single_failure(correlation_id, code, &decode_config)
                    }
                    SessionState::Active { .. } | SessionState::Closed { .. } => {
                        single_failure_for(correlation_id, code, &self.current_output_context())
                    }
                };
            }
        };
        match self.status().phase {
            LocalExecutorPhase::AwaitingHello => self.handle_hello(
                correlation_id,
                message.schema_version,
                &message.message,
                &decode_config,
            ),
            LocalExecutorPhase::Active => {
                self.handle_active(correlation_id, message.schema_version, message.message)
            }
            LocalExecutorPhase::Closed => single_failure_for(
                correlation_id,
                SessionFailureCode::ProtocolState,
                &self.current_output_context(),
            ),
        }
    }

    fn handle_hello(
        &mut self,
        correlation_id: NonZeroU64,
        schema_version: u16,
        message: &LocalSessionClientKind,
        decode_config: &LocalFrameConfig,
    ) -> Result<Vec<LocalFrame>, LocalExecutorError> {
        let pre_session_context = SessionOutputContext {
            schema_version,
            frame_config: decode_config.clone(),
            compilation_limits: None,
        };
        let LocalSessionClientKind::Hello(hello) = message else {
            return single_failure_for(
                correlation_id,
                SessionFailureCode::ProtocolState,
                &pre_session_context,
            );
        };
        let Ok(limits) = hello.receive_limits.negotiate(&self.config.receive_config) else {
            return single_failure_for(
                correlation_id,
                SessionFailureCode::LimitExceeded,
                &pre_session_context,
            );
        };
        let frame_config =
            limits
                .to_frame_config()
                .map_err(|_| LocalExecutorError::InvalidConfig {
                    reason: "negotiated limits are inconsistent",
                })?;
        let compilation_limits = if schema_version == LOCAL_SESSION_SCHEMA_VERSION_V2 {
            let Some(advertised) = hello.compilation_receive_limits else {
                return single_failure_for(
                    correlation_id,
                    SessionFailureCode::InvalidMessage,
                    &pre_session_context,
                );
            };
            match intersect_compilation_limits(advertised, self.config.compilation_limits) {
                Ok(limits) => Some(limits),
                Err(_) => {
                    return single_failure_for(
                        correlation_id,
                        SessionFailureCode::LimitExceeded,
                        &pre_session_context,
                    );
                }
            }
        } else {
            None
        };
        if let Some(compilation_limits) = compilation_limits
            && let Err(code) = self
                .service
                .configure_compilation_limits(compilation_limits)
        {
            return single_failure_for(
                correlation_id,
                code,
                &SessionOutputContext {
                    schema_version,
                    frame_config,
                    compilation_limits: Some(compilation_limits),
                },
            );
        }
        let output = control_frame_for(
            correlation_id,
            LocalSessionServerKind::Hello(ServerHello {
                effective_limits: limits,
                effective_compilation_limits: compilation_limits,
            }),
            schema_version,
            &frame_config,
            compilation_limits.as_ref(),
        )?;
        self.state = SessionState::Active {
            schema_version,
            limits,
            compilation_limits,
            frame_config,
        };
        Ok(vec![output])
    }

    fn handle_active(
        &mut self,
        correlation_id: NonZeroU64,
        schema_version: u16,
        message: LocalSessionClientKind,
    ) -> Result<Vec<LocalFrame>, LocalExecutorError> {
        let output_context = self.current_output_context();
        if schema_version != output_context.schema_version {
            return single_failure_for(
                correlation_id,
                SessionFailureCode::UnsupportedVersion,
                &output_context,
            );
        }
        if self.live_correlations.contains(&correlation_id) {
            return single_failure_for(
                correlation_id,
                SessionFailureCode::ProtocolState,
                &output_context,
            );
        }
        match message {
            LocalSessionClientKind::Hello(_) => single_failure_for(
                correlation_id,
                SessionFailureCode::ProtocolState,
                &output_context,
            ),
            LocalSessionClientKind::SubmitPatch(submit) => {
                self.submit_patch(correlation_id, submit.patch, &output_context)
            }
            LocalSessionClientKind::SubmitImagination(submit) => {
                self.submit_imagination(correlation_id, submit.imagination, &output_context)
            }
            LocalSessionClientKind::Query(request) => {
                self.query(correlation_id, &request.query, &output_context)
            }
            LocalSessionClientKind::RequestObservation(request) => {
                self.request_observation(correlation_id, request.request, &output_context)
            }
            LocalSessionClientKind::Close(_) => self.close(correlation_id, &output_context),
        }
    }

    fn submit_patch(
        &mut self,
        correlation_id: NonZeroU64,
        patch: ScenePatch,
        context: &SessionOutputContext,
    ) -> Result<Vec<LocalFrame>, LocalExecutorError> {
        if self.live_capacity_reached()
            && self.service.snapshot().command_depth < self.service.command_capacity()
        {
            return single_failure_for(
                correlation_id,
                SessionFailureCode::CapacityExceeded,
                context,
            );
        }
        let submitted_key = patch.idempotency_key;
        let admission = match self.service.submit_patch(patch) {
            Ok(admission) => admission,
            Err(code) => return single_failure_for(correlation_id, code, context),
        };
        match admission {
            GatewayAdmission::Queued { idempotency_key } => {
                ensure_admission_key(submitted_key, idempotency_key)?;
                if count(self.command_order.len()) >= self.service.command_capacity() {
                    return Err(LocalExecutorError::StateInvariant {
                        reason: "service queued a patch beyond its declared capacity",
                    });
                }
                let output = patch_admission_frame(
                    correlation_id,
                    idempotency_key,
                    PatchAdmissionStatus::Queued,
                    context,
                )?;
                self.reserve_correlation(correlation_id)?;
                self.command_correlations
                    .insert(idempotency_key, PendingCommand::patch(correlation_id));
                self.command_order.push_back(idempotency_key);
                Ok(vec![output])
            }
            GatewayAdmission::AlreadyQueued { idempotency_key } => {
                ensure_admission_key(submitted_key, idempotency_key)?;
                if !matches!(
                    self.command_correlations.get(&idempotency_key),
                    Some(PendingCommand {
                        kind: PendingCommandKind::Patch,
                        ..
                    })
                ) {
                    return Err(LocalExecutorError::StateInvariant {
                        reason: "already-queued patch had no live correlation",
                    });
                }
                Ok(vec![patch_admission_frame(
                    correlation_id,
                    idempotency_key,
                    PatchAdmissionStatus::AlreadyQueued,
                    context,
                )?])
            }
            GatewayAdmission::Dropped { idempotency_key } => {
                ensure_admission_key(submitted_key, idempotency_key)?;
                Ok(vec![patch_admission_frame(
                    correlation_id,
                    idempotency_key,
                    PatchAdmissionStatus::Dropped,
                    context,
                )?])
            }
            GatewayAdmission::Replayed { response } => {
                let GatewayResponse::PatchApplied { receipt } = *response else {
                    return Err(LocalExecutorError::StateInvariant {
                        reason: "patch admission replayed a non-patch response",
                    });
                };
                let idempotency_key = receipt.idempotency_key;
                ensure_admission_key(submitted_key, idempotency_key)?;
                Ok(vec![control_or_limit_failure_for(
                    correlation_id,
                    LocalSessionServerKind::PatchAdmission(PatchAdmission {
                        idempotency_key,
                        status: PatchAdmissionStatus::Replayed { receipt },
                    }),
                    context,
                )?])
            }
            GatewayAdmission::Superseded {
                idempotency_key,
                superseded_idempotency_key,
            } => {
                ensure_admission_key(submitted_key, idempotency_key)?;
                self.handle_supersession(
                    correlation_id,
                    idempotency_key,
                    superseded_idempotency_key,
                    context,
                )
            }
        }
    }

    fn submit_imagination(
        &mut self,
        correlation_id: NonZeroU64,
        imagination: ImaginationEnvelope,
        context: &SessionOutputContext,
    ) -> Result<Vec<LocalFrame>, LocalExecutorError> {
        if context.schema_version != LOCAL_SESSION_SCHEMA_VERSION_V2
            || context.compilation_limits.is_none()
        {
            return single_failure_for(
                correlation_id,
                SessionFailureCode::UnsupportedVersion,
                context,
            );
        }
        if self.live_capacity_reached()
            && self.service.snapshot().command_depth < self.service.command_capacity()
        {
            return single_failure_for(
                correlation_id,
                SessionFailureCode::CapacityExceeded,
                context,
            );
        }
        let submitted = SubmittedImagination::from_envelope(&imagination);
        let admission = match self.service.submit_imagination(imagination) {
            Ok(admission) => admission,
            Err(code) => return single_failure_for(correlation_id, code, context),
        };
        self.handle_imagination_admission(correlation_id, submitted, admission, context)
    }

    fn handle_imagination_admission(
        &mut self,
        correlation_id: NonZeroU64,
        submitted: SubmittedImagination,
        admission: GatewayAdmission,
        context: &SessionOutputContext,
    ) -> Result<Vec<LocalFrame>, LocalExecutorError> {
        match admission {
            GatewayAdmission::Queued { idempotency_key } => {
                ensure_admission_key(submitted.idempotency_key, idempotency_key)?;
                if count(self.command_order.len()) >= self.service.command_capacity() {
                    return Err(LocalExecutorError::StateInvariant {
                        reason: "service queued an imagination beyond its declared capacity",
                    });
                }
                let output = imagination_admission_frame(
                    correlation_id,
                    submitted.imagination_id,
                    idempotency_key,
                    ImaginationAdmissionStatus::Queued,
                    context,
                )?;
                self.reserve_correlation(correlation_id)?;
                self.command_correlations.insert(
                    idempotency_key,
                    PendingCommand::imagination(correlation_id, submitted),
                );
                self.command_order.push_back(idempotency_key);
                Ok(vec![output])
            }
            GatewayAdmission::AlreadyQueued { idempotency_key } => {
                ensure_admission_key(submitted.idempotency_key, idempotency_key)?;
                if !matches!(
                    self.command_correlations.get(&idempotency_key),
                    Some(PendingCommand {
                        kind: PendingCommandKind::Imagination { submitted: pending },
                        ..
                    }) if *pending == submitted
                ) {
                    return Err(LocalExecutorError::StateInvariant {
                        reason: "already-queued imagination had no matching live correlation",
                    });
                }
                Ok(vec![imagination_admission_frame(
                    correlation_id,
                    submitted.imagination_id,
                    idempotency_key,
                    ImaginationAdmissionStatus::AlreadyQueued,
                    context,
                )?])
            }
            GatewayAdmission::Dropped { idempotency_key } => {
                ensure_admission_key(submitted.idempotency_key, idempotency_key)?;
                Ok(vec![imagination_admission_frame(
                    correlation_id,
                    submitted.imagination_id,
                    idempotency_key,
                    ImaginationAdmissionStatus::Dropped,
                    context,
                )?])
            }
            GatewayAdmission::Replayed { response } => {
                self.handle_imagination_replay(correlation_id, submitted, *response, context)
            }
            GatewayAdmission::Superseded {
                idempotency_key,
                superseded_idempotency_key,
            } => {
                ensure_admission_key(submitted.idempotency_key, idempotency_key)?;
                self.handle_imagination_supersession(
                    correlation_id,
                    submitted,
                    idempotency_key,
                    superseded_idempotency_key,
                    context,
                )
            }
        }
    }

    fn handle_imagination_replay(
        &self,
        correlation_id: NonZeroU64,
        submitted: SubmittedImagination,
        response: GatewayResponse,
        context: &SessionOutputContext,
    ) -> Result<Vec<LocalFrame>, LocalExecutorError> {
        let GatewayResponse::ImaginationProcessed {
            compilation,
            receipt,
        } = response
        else {
            return Err(LocalExecutorError::StateInvariant {
                reason: "imagination admission replayed a non-imagination response",
            });
        };
        let completion = imagination_completion(
            submitted,
            *compilation,
            receipt,
            ApplyStatus::IdempotentReplay,
            &CompilationLimits::for_runtime_limits(self.service.runtime_limits()),
            context,
        );
        let completion = match completion {
            Ok(completion) => completion,
            Err(LocalExecutorError::StateInvariant { .. }) => {
                return single_failure_for(correlation_id, SessionFailureCode::Internal, context);
            }
            Err(error) => return Err(error),
        };
        Ok(vec![imagination_admission_frame(
            correlation_id,
            submitted.imagination_id,
            submitted.idempotency_key,
            ImaginationAdmissionStatus::Replayed {
                completion: Box::new(completion),
            },
            context,
        )?])
    }

    fn handle_imagination_supersession(
        &mut self,
        correlation_id: NonZeroU64,
        submitted: SubmittedImagination,
        idempotency_key: IdempotencyKey,
        superseded_idempotency_key: IdempotencyKey,
        context: &SessionOutputContext,
    ) -> Result<Vec<LocalFrame>, LocalExecutorError> {
        let old_correlation = self
            .command_correlations
            .get(&superseded_idempotency_key)
            .map(|pending| pending.correlation_id)
            .ok_or(LocalExecutorError::StateInvariant {
                reason: "superseded command had no live correlation",
            })?;
        let rejected = failure_frame_for(
            old_correlation,
            SessionFailureCode::CommandRejected,
            context,
        )?;
        let admitted = imagination_admission_frame(
            correlation_id,
            submitted.imagination_id,
            idempotency_key,
            ImaginationAdmissionStatus::Superseded {
                superseded_idempotency_key,
            },
            context,
        )?;
        let Some(position) = self
            .command_order
            .iter()
            .position(|key| *key == superseded_idempotency_key)
        else {
            return Err(LocalExecutorError::StateInvariant {
                reason: "superseded command was absent from local order",
            });
        };
        self.command_order[position] = idempotency_key;
        self.command_correlations
            .remove(&superseded_idempotency_key);
        self.release_correlation(old_correlation)?;
        self.reserve_correlation(correlation_id)?;
        self.command_correlations.insert(
            idempotency_key,
            PendingCommand::imagination(correlation_id, submitted),
        );
        Ok(vec![rejected, admitted])
    }

    fn handle_supersession(
        &mut self,
        correlation_id: NonZeroU64,
        idempotency_key: IdempotencyKey,
        superseded_idempotency_key: IdempotencyKey,
        context: &SessionOutputContext,
    ) -> Result<Vec<LocalFrame>, LocalExecutorError> {
        let old_correlation = self
            .command_correlations
            .get(&superseded_idempotency_key)
            .map(|pending| pending.correlation_id)
            .ok_or(LocalExecutorError::StateInvariant {
                reason: "superseded patch had no live correlation",
            })?;
        let rejected = failure_frame_for(
            old_correlation,
            SessionFailureCode::CommandRejected,
            context,
        )?;
        let admitted = patch_admission_frame(
            correlation_id,
            idempotency_key,
            PatchAdmissionStatus::Superseded {
                superseded_idempotency_key,
            },
            context,
        )?;
        let Some(position) = self
            .command_order
            .iter()
            .position(|key| *key == superseded_idempotency_key)
        else {
            return Err(LocalExecutorError::StateInvariant {
                reason: "superseded patch was absent from local order",
            });
        };
        self.command_order[position] = idempotency_key;
        self.command_correlations
            .remove(&superseded_idempotency_key);
        self.release_correlation(old_correlation)?;
        self.reserve_correlation(correlation_id)?;
        self.command_correlations
            .insert(idempotency_key, PendingCommand::patch(correlation_id));
        Ok(vec![rejected, admitted])
    }

    fn query(
        &self,
        correlation_id: NonZeroU64,
        query: &SceneQuery,
        context: &SessionOutputContext,
    ) -> Result<Vec<LocalFrame>, LocalExecutorError> {
        match self.service.query(query) {
            Ok(result) => Ok(vec![control_or_limit_failure_for(
                correlation_id,
                LocalSessionServerKind::QueryResult(QueryResponse { result }),
                context,
            )?]),
            Err(code) => single_failure_for(correlation_id, code, context),
        }
    }

    fn request_observation(
        &mut self,
        correlation_id: NonZeroU64,
        request: ObservationRequest,
        context: &SessionOutputContext,
    ) -> Result<Vec<LocalFrame>, LocalExecutorError> {
        if self.live_capacity_reached()
            || count(self.observation_order.len()) >= self.service.observation_capacity()
        {
            return single_failure_for(
                correlation_id,
                SessionFailureCode::CapacityExceeded,
                context,
            );
        }
        if self
            .observation_correlations
            .contains_key(&request.observation_id)
        {
            return single_failure_for(
                correlation_id,
                SessionFailureCode::ObservationRejected,
                context,
            );
        }
        let (width, height) = self.service.observation_dimensions();
        let pixels = u64::from(width).saturating_mul(u64::from(height));
        let limits = &context.frame_config.runtime_limits;
        if width > limits.max_observation_width.get()
            || height > limits.max_observation_height.get()
            || pixels > limits.max_observation_pixels.get()
        {
            return single_failure_for(correlation_id, SessionFailureCode::LimitExceeded, context);
        }
        let reference = observation_reference(request);
        let output = control_frame_for_context(
            correlation_id,
            LocalSessionServerKind::ObservationAccepted(reference),
            context,
        )?;
        if let Err(code) = self.service.request_observation(request) {
            return single_failure_for(correlation_id, code, context);
        }
        self.reserve_correlation(correlation_id)?;
        self.observation_correlations.insert(
            request.observation_id,
            PendingObservation {
                correlation_id,
                scene_revision: request.scene_revision,
            },
        );
        self.observation_order.push_back(request.observation_id);
        Ok(vec![output])
    }

    fn close(
        &mut self,
        correlation_id: NonZeroU64,
        context: &SessionOutputContext,
    ) -> Result<Vec<LocalFrame>, LocalExecutorError> {
        let snapshot = self.service.snapshot();
        if !self.live_correlations.is_empty()
            || !self.command_order.is_empty()
            || !self.observation_order.is_empty()
            || snapshot.command_depth != 0
            || snapshot.outstanding_observations != 0
        {
            return single_failure_for(correlation_id, SessionFailureCode::ProtocolState, context);
        }
        let output = control_frame_for_context(
            correlation_id,
            LocalSessionServerKind::Closed(SessionClosed {}),
            context,
        )?;
        let Some(limits) = self.negotiated_limits() else {
            return Err(LocalExecutorError::StateInvariant {
                reason: "close was handled outside the active state",
            });
        };
        self.state = SessionState::Closed {
            schema_version: context.schema_version,
            limits,
            compilation_limits: context.compilation_limits,
            frame_config: context.frame_config.clone(),
        };
        Ok(vec![output])
    }

    fn advance(&mut self) -> Result<Vec<LocalFrame>, LocalExecutorError> {
        let context = match &self.state {
            SessionState::Active { .. } => self.current_output_context(),
            SessionState::AwaitingHello | SessionState::Closed { .. } => return Ok(Vec::new()),
        };
        let mut output = Vec::with_capacity(MAX_OUTPUT_FRAMES_PER_CALL);
        if let Some(command) = self.advance_command(&context)? {
            output.push(command);
        }

        if !self.observation_order.is_empty() {
            match self.service.poll_observation() {
                Ok(Some(delivery)) => {
                    output.push(self.observation_delivery_frame(delivery, &context)?);
                }
                Ok(None) => {
                    if let Some(observation_id) = self
                        .observation_order
                        .iter()
                        .copied()
                        .find(|id| !self.observation_pending_reported.contains(id))
                    {
                        let pending = self.observation_correlations[&observation_id];
                        let reference = ObservationReference {
                            observation_id,
                            scene_revision: pending.scene_revision,
                        };
                        output.push(control_frame_for_context(
                            pending.correlation_id,
                            LocalSessionServerKind::ObservationPending(reference),
                            &context,
                        )?);
                        self.observation_pending_reported.insert(observation_id);
                    }
                }
                Err(code) => {
                    let observation_id = self.observation_order[0];
                    let correlation_id =
                        self.observation_correlations[&observation_id].correlation_id;
                    output.push(failure_frame_for(correlation_id, code, &context)?);
                    self.release_observation(observation_id, correlation_id)?;
                }
            }
        }
        debug_assert!(output.len() <= MAX_OUTPUT_FRAMES_PER_CALL);
        Ok(output)
    }

    fn advance_command(
        &mut self,
        context: &SessionOutputContext,
    ) -> Result<Option<LocalFrame>, LocalExecutorError> {
        let Some(idempotency_key) = self.command_order.pop_front() else {
            return Ok(None);
        };
        let pending = self.command_correlations.remove(&idempotency_key).ok_or(
            LocalExecutorError::StateInvariant {
                reason: "ordered command had no live correlation",
            },
        )?;
        let correlation_id = pending.correlation_id;
        let service_compilation_limits =
            CompilationLimits::for_runtime_limits(self.service.runtime_limits());
        let message = match (pending.kind, self.service.process_next()) {
            (PendingCommandKind::Patch, Ok(Some(GatewayResponse::PatchApplied { receipt })))
                if receipt.idempotency_key == idempotency_key
                    && receipt.status == ApplyStatus::Applied =>
            {
                Ok(LocalSessionServerKind::PatchCompleted(PatchCompletion {
                    receipt,
                }))
            }
            (
                PendingCommandKind::Imagination { submitted },
                Ok(Some(GatewayResponse::ImaginationProcessed {
                    compilation,
                    receipt,
                })),
            ) => imagination_completion(
                submitted,
                *compilation,
                receipt,
                ApplyStatus::Applied,
                &service_compilation_limits,
                context,
            )
            .map(LocalSessionServerKind::ImaginationCompleted),
            (PendingCommandKind::Patch, Ok(Some(GatewayResponse::PatchApplied { .. }))) => {
                Err(LocalExecutorError::StateInvariant {
                    reason: "processed patch receipt did not match local order",
                })
            }
            (PendingCommandKind::Patch, Ok(Some(GatewayResponse::ImaginationProcessed { .. })))
            | (
                PendingCommandKind::Imagination { .. },
                Ok(Some(GatewayResponse::PatchApplied { .. })),
            ) => Err(LocalExecutorError::StateInvariant {
                reason: "processed command kind did not match local order",
            }),
            (_, Ok(None)) => Err(LocalExecutorError::StateInvariant {
                reason: "service omitted an ordered command response",
            }),
            (_, Err(code)) => Ok(LocalSessionServerKind::Failure(SessionFailure { code })),
        };
        let message = match message {
            Ok(message) => message,
            Err(LocalExecutorError::StateInvariant { .. }) => {
                LocalSessionServerKind::Failure(SessionFailure {
                    code: SessionFailureCode::Internal,
                })
            }
            Err(error) => return Err(error),
        };
        let output = control_or_limit_failure_for(correlation_id, message, context)?;
        self.release_correlation(correlation_id)?;
        Ok(Some(output))
    }

    fn observation_delivery_frame(
        &mut self,
        delivery: ServiceObservationDelivery,
        context: &SessionOutputContext,
    ) -> Result<LocalFrame, LocalExecutorError> {
        let observation_id = delivery.observation_id();
        let correlation_id = self
            .observation_correlations
            .get(&observation_id)
            .map(|pending| pending.correlation_id)
            .ok_or(LocalExecutorError::StateInvariant {
                reason: "observation completion had no live correlation",
            })?;
        let output = match delivery {
            ServiceObservationDelivery::Completed { metadata, payload } => {
                let frame = LocalFrame::Observation {
                    correlation_id,
                    metadata,
                    payload,
                };
                if encode_frame(&frame, &context.frame_config).is_ok() {
                    frame
                } else {
                    failure_frame_for(correlation_id, SessionFailureCode::LimitExceeded, context)?
                }
            }
            ServiceObservationDelivery::Failed { code, .. } => {
                failure_frame_for(correlation_id, code, context)?
            }
        };
        self.release_observation(observation_id, correlation_id)?;
        Ok(output)
    }

    fn release_observation(
        &mut self,
        observation_id: ObservationId,
        correlation_id: NonZeroU64,
    ) -> Result<(), LocalExecutorError> {
        self.observation_correlations.remove(&observation_id);
        self.observation_pending_reported.remove(&observation_id);
        let Some(position) = self
            .observation_order
            .iter()
            .position(|id| *id == observation_id)
        else {
            return Err(LocalExecutorError::StateInvariant {
                reason: "completed observation was absent from local order",
            });
        };
        self.observation_order.remove(position);
        self.release_correlation(correlation_id)
    }

    fn current_frame_config(&self) -> &LocalFrameConfig {
        match &self.state {
            SessionState::AwaitingHello => &self.config.receive_config,
            SessionState::Active { frame_config, .. }
            | SessionState::Closed { frame_config, .. } => frame_config,
        }
    }

    fn current_output_context(&self) -> SessionOutputContext {
        match &self.state {
            SessionState::AwaitingHello => SessionOutputContext {
                schema_version: LOCAL_SESSION_SCHEMA_VERSION,
                frame_config: self.config.receive_config.clone(),
                compilation_limits: None,
            },
            SessionState::Active {
                schema_version,
                frame_config,
                compilation_limits,
                ..
            }
            | SessionState::Closed {
                schema_version,
                frame_config,
                compilation_limits,
                ..
            } => SessionOutputContext {
                schema_version: *schema_version,
                frame_config: frame_config.clone(),
                compilation_limits: *compilation_limits,
            },
        }
    }

    fn live_capacity_reached(&self) -> bool {
        u32::try_from(self.live_correlations.len()).unwrap_or(u32::MAX)
            >= self.config.max_live_correlations.get()
    }

    fn reserve_correlation(
        &mut self,
        correlation_id: NonZeroU64,
    ) -> Result<(), LocalExecutorError> {
        if self.live_capacity_reached() || !self.live_correlations.insert(correlation_id) {
            return Err(LocalExecutorError::StateInvariant {
                reason: "live correlation reservation failed",
            });
        }
        Ok(())
    }

    fn release_correlation(
        &mut self,
        correlation_id: NonZeroU64,
    ) -> Result<(), LocalExecutorError> {
        if !self.live_correlations.remove(&correlation_id) {
            return Err(LocalExecutorError::StateInvariant {
                reason: "terminal correlation was not live",
            });
        }
        Ok(())
    }

    fn status(&self) -> LocalExecutorStatus {
        LocalExecutorStatus {
            phase: match self.state {
                SessionState::AwaitingHello => LocalExecutorPhase::AwaitingHello,
                SessionState::Active { .. } => LocalExecutorPhase::Active,
                SessionState::Closed { .. } => LocalExecutorPhase::Closed,
            },
            live_correlations: count(self.live_correlations.len()),
            live_correlation_capacity: self.config.max_live_correlations.get(),
            max_output_frames_per_call: self.config.max_output_frames_per_call.get(),
            pending_patches: count(
                self.command_correlations
                    .values()
                    .filter(|pending| matches!(pending.kind, PendingCommandKind::Patch))
                    .count(),
            ),
            pending_imaginations: count(
                self.command_correlations
                    .values()
                    .filter(|pending| {
                        matches!(pending.kind, PendingCommandKind::Imagination { .. })
                    })
                    .count(),
            ),
            pending_observations: count(self.observation_order.len()),
        }
    }

    const fn negotiated_limits(&self) -> Option<LocalSessionLimits> {
        match &self.state {
            SessionState::AwaitingHello => None,
            SessionState::Active { limits, .. } | SessionState::Closed { limits, .. } => {
                Some(*limits)
            }
        }
    }

    const fn negotiated_compilation_limits(&self) -> Option<CompilationLimits> {
        match &self.state {
            SessionState::AwaitingHello => None,
            SessionState::Active {
                compilation_limits, ..
            }
            | SessionState::Closed {
                compilation_limits, ..
            } => *compilation_limits,
        }
    }
}

fn patch_admission_frame(
    correlation_id: NonZeroU64,
    idempotency_key: IdempotencyKey,
    status: PatchAdmissionStatus,
    context: &SessionOutputContext,
) -> Result<LocalFrame, LocalExecutorError> {
    control_frame_for_context(
        correlation_id,
        LocalSessionServerKind::PatchAdmission(PatchAdmission {
            idempotency_key,
            status,
        }),
        context,
    )
}

fn imagination_admission_frame(
    correlation_id: NonZeroU64,
    imagination_id: ImaginationId,
    idempotency_key: IdempotencyKey,
    status: ImaginationAdmissionStatus,
    context: &SessionOutputContext,
) -> Result<LocalFrame, LocalExecutorError> {
    control_or_limit_failure_for(
        correlation_id,
        LocalSessionServerKind::ImaginationAdmission(ImaginationAdmission {
            imagination_id,
            idempotency_key,
            status,
        }),
        context,
    )
}

fn imagination_completion(
    submitted: SubmittedImagination,
    compilation: CompilationResult,
    receipt: Option<cogniform_protocol::ApplyReceipt>,
    expected_status: ApplyStatus,
    service_compilation_limits: &CompilationLimits,
    context: &SessionOutputContext,
) -> Result<ImaginationCompletion, LocalExecutorError> {
    if context.compilation_limits.is_none() {
        return Err(LocalExecutorError::StateInvariant {
            reason: "imagination completion had no negotiated compilation limits",
        });
    }
    compilation
        .to_canonical_json(service_compilation_limits)
        .map_err(|_| LocalExecutorError::StateInvariant {
            reason: "service produced an invalid compilation result",
        })?;
    if compilation.imagination_id != submitted.imagination_id {
        return Err(LocalExecutorError::StateInvariant {
            reason: "processed imagination identity did not match local order",
        });
    }
    if compilation.scene_revision != submitted.base_revision {
        return Err(LocalExecutorError::StateInvariant {
            reason: "processed imagination revision did not match submitted work",
        });
    }
    match (&compilation.patch, &receipt) {
        (Some(patch), Some(receipt))
            if patch.idempotency_key == submitted.idempotency_key
                && patch.transaction_id == submitted.transaction_id
                && receipt.idempotency_key == submitted.idempotency_key
                && receipt.status == expected_status => {}
        (None, None) => {}
        _ => {
            return Err(LocalExecutorError::StateInvariant {
                reason: "processed imagination result and receipt roles were inconsistent",
            });
        }
    }
    Ok(ImaginationCompletion {
        imagination_id: submitted.imagination_id,
        idempotency_key: submitted.idempotency_key,
        compilation,
        receipt,
    })
}

fn ensure_admission_key(
    submitted: IdempotencyKey,
    admitted: IdempotencyKey,
) -> Result<(), LocalExecutorError> {
    if submitted == admitted {
        Ok(())
    } else {
        Err(LocalExecutorError::StateInvariant {
            reason: "gateway admission identity did not match submitted command",
        })
    }
}

fn single_failure(
    correlation_id: NonZeroU64,
    code: SessionFailureCode,
    frame_config: &LocalFrameConfig,
) -> Result<Vec<LocalFrame>, LocalExecutorError> {
    Ok(vec![failure_frame(correlation_id, code, frame_config)?])
}

fn failure_frame(
    correlation_id: NonZeroU64,
    code: SessionFailureCode,
    frame_config: &LocalFrameConfig,
) -> Result<LocalFrame, LocalExecutorError> {
    control_frame(
        correlation_id,
        LocalSessionServerKind::Failure(SessionFailure { code }),
        frame_config,
    )
}

fn single_failure_for(
    correlation_id: NonZeroU64,
    code: SessionFailureCode,
    context: &SessionOutputContext,
) -> Result<Vec<LocalFrame>, LocalExecutorError> {
    Ok(vec![failure_frame_for(correlation_id, code, context)?])
}

fn failure_frame_for(
    correlation_id: NonZeroU64,
    code: SessionFailureCode,
    context: &SessionOutputContext,
) -> Result<LocalFrame, LocalExecutorError> {
    control_frame_for_context(
        correlation_id,
        LocalSessionServerKind::Failure(SessionFailure { code }),
        context,
    )
}

fn control_frame(
    correlation_id: NonZeroU64,
    message: LocalSessionServerKind,
    frame_config: &LocalFrameConfig,
) -> Result<LocalFrame, LocalExecutorError> {
    control_frame_for(
        correlation_id,
        message,
        LOCAL_SESSION_SCHEMA_VERSION,
        frame_config,
        None,
    )
}

fn control_frame_for_context(
    correlation_id: NonZeroU64,
    message: LocalSessionServerKind,
    context: &SessionOutputContext,
) -> Result<LocalFrame, LocalExecutorError> {
    control_frame_for(
        correlation_id,
        message,
        context.schema_version,
        &context.frame_config,
        context.compilation_limits.as_ref(),
    )
}

fn control_frame_for(
    correlation_id: NonZeroU64,
    message: LocalSessionServerKind,
    schema_version: u16,
    frame_config: &LocalFrameConfig,
    compilation_limits: Option<&CompilationLimits>,
) -> Result<LocalFrame, LocalExecutorError> {
    let message = LocalSessionServerMessage {
        schema_version,
        message,
    };
    let frame = if let Some(limits) = compilation_limits {
        server_control_frame_with_limits(correlation_id, &message, frame_config, limits)
    } else {
        server_control_frame(correlation_id, &message, frame_config)
    }
    .map_err(|_| LocalExecutorError::OutputRejected)?;
    encode_frame(&frame, frame_config).map_err(|_| LocalExecutorError::OutputRejected)?;
    Ok(frame)
}

fn control_or_limit_failure_for(
    correlation_id: NonZeroU64,
    message: LocalSessionServerKind,
    context: &SessionOutputContext,
) -> Result<LocalFrame, LocalExecutorError> {
    match control_frame_for_context(correlation_id, message, context) {
        Ok(frame) => Ok(frame),
        Err(LocalExecutorError::OutputRejected) => {
            failure_frame_for(correlation_id, SessionFailureCode::LimitExceeded, context)
        }
        Err(error) => Err(error),
    }
}

#[derive(Debug, Clone)]
struct SessionOutputContext {
    schema_version: u16,
    frame_config: LocalFrameConfig,
    compilation_limits: Option<CompilationLimits>,
}

#[derive(Debug, Clone, Copy)]
struct PendingCommand {
    correlation_id: NonZeroU64,
    kind: PendingCommandKind,
}

impl PendingCommand {
    const fn patch(correlation_id: NonZeroU64) -> Self {
        Self {
            correlation_id,
            kind: PendingCommandKind::Patch,
        }
    }

    const fn imagination(correlation_id: NonZeroU64, submitted: SubmittedImagination) -> Self {
        Self {
            correlation_id,
            kind: PendingCommandKind::Imagination { submitted },
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum PendingCommandKind {
    Patch,
    Imagination { submitted: SubmittedImagination },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct SubmittedImagination {
    imagination_id: ImaginationId,
    transaction_id: TransactionId,
    idempotency_key: IdempotencyKey,
    base_revision: SceneRevision,
}

impl SubmittedImagination {
    const fn from_envelope(imagination: &ImaginationEnvelope) -> Self {
        Self {
            imagination_id: imagination.imagination_id,
            transaction_id: imagination.transaction_id,
            idempotency_key: imagination.idempotency_key,
            base_revision: imagination.base_revision,
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct PendingObservation {
    correlation_id: NonZeroU64,
    scene_revision: cogniform_protocol::SceneRevision,
}

#[derive(Debug, Clone, Copy)]
struct ServiceSnapshot {
    command_depth: u32,
    outstanding_observations: u32,
}

enum ServiceObservationDelivery {
    Completed {
        metadata: ObservationMetadata,
        payload: ObservationPayload,
    },
    Failed {
        observation_id: ObservationId,
        code: SessionFailureCode,
    },
}

impl ServiceObservationDelivery {
    const fn observation_id(&self) -> ObservationId {
        match self {
            Self::Completed { metadata, .. } => metadata.observation_id,
            Self::Failed { observation_id, .. } => *observation_id,
        }
    }
}

trait SessionService {
    fn runtime_limits(&self) -> RuntimeLimits;
    fn command_capacity(&self) -> u32;
    fn observation_capacity(&self) -> u32;
    fn observation_dimensions(&self) -> (u32, u32);
    fn snapshot(&self) -> ServiceSnapshot;
    fn configure_compilation_limits(
        &mut self,
        limits: CompilationLimits,
    ) -> Result<(), SessionFailureCode>;
    fn submit_patch(&mut self, patch: ScenePatch) -> Result<GatewayAdmission, SessionFailureCode>;
    fn submit_imagination(
        &mut self,
        imagination: ImaginationEnvelope,
    ) -> Result<GatewayAdmission, SessionFailureCode>;
    fn process_next(&mut self) -> Result<Option<GatewayResponse>, SessionFailureCode>;
    fn query(&self, query: &SceneQuery) -> Result<SceneQueryResult, SessionFailureCode>;
    fn request_observation(
        &mut self,
        request: ObservationRequest,
    ) -> Result<(), SessionFailureCode>;
    fn poll_observation(&self) -> Result<Option<ServiceObservationDelivery>, SessionFailureCode>;
}

impl SessionService for LocalService {
    fn runtime_limits(&self) -> RuntimeLimits {
        self.runtime_limits()
    }

    fn command_capacity(&self) -> u32 {
        self.command_capacity()
    }

    fn observation_capacity(&self) -> u32 {
        self.status().observation_capacity
    }

    fn observation_dimensions(&self) -> (u32, u32) {
        self.observation_dimensions()
    }

    fn snapshot(&self) -> ServiceSnapshot {
        let status = self.status();
        ServiceSnapshot {
            command_depth: status.command_queue.depth,
            outstanding_observations: status.outstanding_observations,
        }
    }

    fn configure_compilation_limits(
        &mut self,
        limits: CompilationLimits,
    ) -> Result<(), SessionFailureCode> {
        self.configure_compilation_limits(limits)
            .map_err(|error| classify_service_error(&error, ServiceOperation::Command))
    }

    fn submit_patch(&mut self, patch: ScenePatch) -> Result<GatewayAdmission, SessionFailureCode> {
        self.submit_patch(patch)
            .map_err(|error| classify_service_error(&error, ServiceOperation::Command))
    }

    fn submit_imagination(
        &mut self,
        imagination: ImaginationEnvelope,
    ) -> Result<GatewayAdmission, SessionFailureCode> {
        self.submit_imagination(imagination)
            .map_err(|error| classify_service_error(&error, ServiceOperation::Command))
    }

    fn process_next(&mut self) -> Result<Option<GatewayResponse>, SessionFailureCode> {
        self.process_next()
            .map_err(|error| classify_service_error(&error, ServiceOperation::Command))
    }

    fn query(&self, query: &SceneQuery) -> Result<SceneQueryResult, SessionFailureCode> {
        self.query(query)
            .map_err(|error| classify_service_error(&error, ServiceOperation::Query))
    }

    fn request_observation(
        &mut self,
        request: ObservationRequest,
    ) -> Result<(), SessionFailureCode> {
        self.request_observation(request)
            .map_err(|error| classify_service_error(&error, ServiceOperation::Observation))
    }

    fn poll_observation(&self) -> Result<Option<ServiceObservationDelivery>, SessionFailureCode> {
        self.try_receive_observation_delivery()
            .map(|delivery| {
                delivery.map(|delivery| match delivery {
                    ObservationDelivery::Completed(observation) => {
                        let (metadata, payload) = observation.into_parts();
                        ServiceObservationDelivery::Completed { metadata, payload }
                    }
                    ObservationDelivery::Failed {
                        observation_id,
                        error,
                    } => ServiceObservationDelivery::Failed {
                        observation_id,
                        code: classify_observation_error(&error),
                    },
                })
            })
            .map_err(|error| classify_service_error(&error, ServiceOperation::Observation))
    }
}

#[derive(Clone, Copy)]
enum ServiceOperation {
    Command,
    Query,
    Observation,
}

fn classify_service_error(
    error: &LocalServiceError,
    operation: ServiceOperation,
) -> SessionFailureCode {
    match error {
        LocalServiceError::Gateway(error) => classify_gateway_error(error, operation),
        LocalServiceError::Engine(error) => classify_engine_error(error, operation),
        LocalServiceError::Asset(_)
        | LocalServiceError::Procedure(_)
        | LocalServiceError::Revert(_) => default_failure(operation),
    }
}

fn classify_gateway_error(error: &GatewayError, operation: ServiceOperation) -> SessionFailureCode {
    if error.is_compilation_limit_exceeded() {
        return SessionFailureCode::LimitExceeded;
    }
    match error {
        GatewayError::CommandCapacityExceeded { .. }
        | GatewayError::IdempotencyCapacityExceeded
        | GatewayError::QueryResultCapacityExceeded { .. } => SessionFailureCode::CapacityExceeded,
        GatewayError::QueryRevisionMismatch { .. } => SessionFailureCode::RevisionMismatch,
        GatewayError::Engine(error) => classify_engine_error(error, operation),
        GatewayError::InvalidConfig { .. }
        | GatewayError::Compiler(_)
        | GatewayError::WorldView(_) => SessionFailureCode::Internal,
        GatewayError::InvalidCommand(_)
        | GatewayError::InvalidCommandEncoding(_)
        | GatewayError::IdempotencyConflict { .. }
        | GatewayError::InvalidQueryResult(_) => default_failure(operation),
    }
}

fn classify_engine_error(error: &EngineError, operation: ServiceOperation) -> SessionFailureCode {
    match error {
        EngineError::Observation(error) => classify_observation_error(error),
        EngineError::Renderer(_) | EngineError::SceneUpdate(_) => {
            SessionFailureCode::ServiceUnavailable
        }
        EngineError::WorldApply(_) => default_failure(operation),
        EngineError::InvalidConfig { .. }
        | EngineError::RecoveryFrameBehindReplay { .. }
        | EngineError::ReplayConfig(_)
        | EngineError::ReplayRevision(_)
        | EngineError::ReplayRecord(_)
        | EngineError::Replay(_)
        | EngineError::WorldExtraction(_)
        | EngineError::WorldInvariant(_) => SessionFailureCode::Internal,
    }
}

fn classify_observation_error(error: &ObservationError) -> SessionFailureCode {
    match error {
        ObservationError::RequestRevisionMismatch { .. } => SessionFailureCode::RevisionMismatch,
        ObservationError::CapacityExceeded { .. } => SessionFailureCode::CapacityExceeded,
        ObservationError::WorkerUnavailable
        | ObservationError::WorkerStartFailed { .. }
        | ObservationError::Renderer(_) => SessionFailureCode::ServiceUnavailable,
        ObservationError::InvalidRequest(_)
        | ObservationError::SourceRevisionAhead { .. }
        | ObservationError::SourceRevisionMismatch { .. }
        | ObservationError::CameraMismatch { .. }
        | ObservationError::InvalidMetadata(_) => SessionFailureCode::ObservationRejected,
    }
}

const fn default_failure(operation: ServiceOperation) -> SessionFailureCode {
    match operation {
        ServiceOperation::Command => SessionFailureCode::CommandRejected,
        ServiceOperation::Query => SessionFailureCode::QueryRejected,
        ServiceOperation::Observation => SessionFailureCode::ObservationRejected,
    }
}

fn classify_session_error(error: &LocalSessionError) -> SessionFailureCode {
    match error {
        LocalSessionError::InvalidConfig
        | LocalSessionError::MessageLimitExceeded { .. }
        | LocalSessionError::NestingLimitExceeded { .. }
        | LocalSessionError::AllocationFailed => SessionFailureCode::LimitExceeded,
        LocalSessionError::InvalidMessage(error)
            if error.kind()
                == cogniform_local_session::LocalSessionValidationKind::UnsupportedVersion =>
        {
            SessionFailureCode::UnsupportedVersion
        }
        LocalSessionError::MalformedJson { .. }
        | LocalSessionError::InvalidMessage(_)
        | LocalSessionError::NonCanonicalMessage
        | LocalSessionError::WrongDirection
        | LocalSessionError::WrongFrameKind
        | LocalSessionError::SerializationFailed => SessionFailureCode::InvalidMessage,
    }
}

const fn observation_reference(request: ObservationRequest) -> ObservationReference {
    ObservationReference {
        observation_id: request.observation_id,
        scene_revision: request.scene_revision,
    }
}

fn count(value: usize) -> u32 {
    u32::try_from(value).unwrap_or(u32::MAX)
}

#[cfg(test)]
mod tests {
    use core::num::NonZeroU32;
    use std::collections::VecDeque;

    use cogniform_compilation::{UnresolvedConstraint, UnresolvedConstraintCode};
    use cogniform_local_session::{
        ClientHello, LocalSessionClientMessage, QueryRequest, RequestObservation, SessionClose,
        SubmitImagination, SubmitPatch, client_control_frame, decode_server_control_frame,
        decode_server_control_frame_with_limits,
    };
    use cogniform_protocol::{
        ApplyReceipt, ApplyTiming, ConflictPolicy, DeleteEntity, DeliverySemantic, FrameId,
        ImageDimensions, ImaginationBudget, ImaginedEntity, ObservationKind, ObservationQuality,
        ObservationStaleness, PatchBudget, PositiveF32, PositiveVec3, PrimitiveComponent,
        PrimitiveShape, SceneOperation, SceneRevision, SceneText, SchemaVersion, StableEntityId,
        TransactionId,
    };

    use super::*;

    use std::cell::{Cell, RefCell};

    #[derive(Default)]
    struct TestService {
        runtime_limits: Option<RuntimeLimits>,
        command_capacity: Option<u32>,
        observation_capacity: Option<u32>,
        observation_dimensions: Option<(u32, u32)>,
        command_depth: Cell<u32>,
        process_calls: Cell<u32>,
        outstanding_observations: Cell<u32>,
        compilation_limits: Cell<Option<CompilationLimits>>,
        admissions: RefCell<VecDeque<Result<GatewayAdmission, SessionFailureCode>>>,
        responses: RefCell<VecDeque<Result<Option<GatewayResponse>, SessionFailureCode>>>,
        query_results: RefCell<VecDeque<Result<SceneQueryResult, SessionFailureCode>>>,
        observation_requests: RefCell<VecDeque<Result<(), SessionFailureCode>>>,
        deliveries:
            RefCell<VecDeque<Result<Option<ServiceObservationDelivery>, SessionFailureCode>>>,
    }

    impl SessionService for TestService {
        fn runtime_limits(&self) -> RuntimeLimits {
            self.runtime_limits.unwrap_or_default()
        }

        fn command_capacity(&self) -> u32 {
            self.command_capacity.unwrap_or(64)
        }

        fn observation_capacity(&self) -> u32 {
            self.observation_capacity.unwrap_or(8)
        }

        fn observation_dimensions(&self) -> (u32, u32) {
            self.observation_dimensions.unwrap_or((1, 1))
        }

        fn snapshot(&self) -> ServiceSnapshot {
            ServiceSnapshot {
                command_depth: self.command_depth.get(),
                outstanding_observations: self.outstanding_observations.get(),
            }
        }

        fn configure_compilation_limits(
            &mut self,
            limits: CompilationLimits,
        ) -> Result<(), SessionFailureCode> {
            self.compilation_limits.set(Some(limits));
            Ok(())
        }

        fn submit_patch(
            &mut self,
            patch: ScenePatch,
        ) -> Result<GatewayAdmission, SessionFailureCode> {
            let result = self.admissions.get_mut().pop_front().unwrap_or({
                Ok(GatewayAdmission::Queued {
                    idempotency_key: patch.idempotency_key,
                })
            });
            if matches!(result, Ok(GatewayAdmission::Queued { .. })) {
                self.command_depth.set(self.command_depth.get() + 1);
            }
            result
        }

        fn submit_imagination(
            &mut self,
            imagination: ImaginationEnvelope,
        ) -> Result<GatewayAdmission, SessionFailureCode> {
            let result = self.admissions.get_mut().pop_front().unwrap_or({
                Ok(GatewayAdmission::Queued {
                    idempotency_key: imagination.idempotency_key,
                })
            });
            if matches!(result, Ok(GatewayAdmission::Queued { .. })) {
                self.command_depth.set(self.command_depth.get() + 1);
            }
            result
        }

        fn process_next(&mut self) -> Result<Option<GatewayResponse>, SessionFailureCode> {
            self.process_calls.set(self.process_calls.get() + 1);
            self.command_depth
                .set(self.command_depth.get().saturating_sub(1));
            let result = self.responses.get_mut().pop_front().unwrap_or(Ok(None));
            if let (
                Some(limits),
                Ok(Some(GatewayResponse::ImaginationProcessed { compilation, .. })),
            ) = (self.compilation_limits.get(), &result)
                && compilation.to_canonical_json(&limits).is_err()
            {
                return Err(SessionFailureCode::LimitExceeded);
            }
            result
        }

        fn query(&self, query: &SceneQuery) -> Result<SceneQueryResult, SessionFailureCode> {
            self.query_results
                .borrow_mut()
                .pop_front()
                .unwrap_or_else(|| {
                    Ok(SceneQueryResult {
                        schema_version: SchemaVersion::V1,
                        scene_revision: query.scene_revision,
                        entities: Vec::new(),
                    })
                })
        }

        fn request_observation(
            &mut self,
            _request: ObservationRequest,
        ) -> Result<(), SessionFailureCode> {
            let result = self
                .observation_requests
                .get_mut()
                .pop_front()
                .unwrap_or(Ok(()));
            if result.is_ok() {
                self.outstanding_observations
                    .set(self.outstanding_observations.get() + 1);
            }
            result
        }

        fn poll_observation(
            &self,
        ) -> Result<Option<ServiceObservationDelivery>, SessionFailureCode> {
            let result = self.deliveries.borrow_mut().pop_front().unwrap_or(Ok(None));
            if matches!(result, Ok(Some(_))) || result.is_err() {
                self.outstanding_observations
                    .set(self.outstanding_observations.get().saturating_sub(1));
            }
            result
        }
    }

    fn core(service: TestService) -> SessionCore<TestService> {
        SessionCore::new(service, LocalExecutorConfig::default()).unwrap()
    }

    fn correlation(value: u64) -> NonZeroU64 {
        NonZeroU64::new(value).unwrap()
    }

    fn patch(nonce: u128, delivery: DeliverySemantic) -> ScenePatch {
        ScenePatch {
            schema_version: SchemaVersion::V1,
            transaction_id: TransactionId::new(nonce).unwrap(),
            idempotency_key: IdempotencyKey::new(nonce + 100).unwrap(),
            base_revision: SceneRevision::new(7),
            conflict_policy: ConflictPolicy::RequireExactBase,
            delivery,
            declared_budget: PatchBudget::default(),
            operations: vec![SceneOperation::Delete(DeleteEntity {
                entity_id: StableEntityId::new(nonce + 1_000).unwrap(),
            })],
        }
    }

    fn imagination(nonce: u128, delivery: DeliverySemantic) -> ImaginationEnvelope {
        ImaginationEnvelope {
            schema_version: SchemaVersion::V1,
            imagination_id: ImaginationId::new(nonce + 200).unwrap(),
            transaction_id: TransactionId::new(nonce).unwrap(),
            idempotency_key: IdempotencyKey::new(nonce + 100).unwrap(),
            base_revision: SceneRevision::new(7),
            delivery,
            seed: 42,
            declared_budget: ImaginationBudget::default(),
            entities: vec![ImaginedEntity {
                key: SceneText::new("table").unwrap(),
                preferred_id: None,
                name: None,
                primitive: PrimitiveComponent {
                    shape: PrimitiveShape::Cuboid,
                    dimensions: PositiveVec3 {
                        x: PositiveF32::new(1.0).unwrap(),
                        y: PositiveF32::new(1.0).unwrap(),
                        z: PositiveF32::new(1.0).unwrap(),
                    },
                },
                transform: None,
                material: None,
            }],
            relations: Vec::new(),
            constraints: Vec::new(),
        }
    }

    fn compilation(
        imagination: &ImaginationEnvelope,
        patch: Option<ScenePatch>,
    ) -> CompilationResult {
        CompilationResult {
            schema_version: cogniform_compilation::COMPILATION_SCHEMA_VERSION,
            imagination_id: imagination.imagination_id,
            scene_revision: imagination.base_revision,
            patch,
            decisions: Vec::new(),
            unresolved: Vec::new(),
        }
    }

    fn unresolved_compilation(imagination: &ImaginationEnvelope) -> CompilationResult {
        CompilationResult {
            schema_version: cogniform_compilation::COMPILATION_SCHEMA_VERSION,
            imagination_id: imagination.imagination_id,
            scene_revision: imagination.base_revision,
            patch: None,
            decisions: Vec::new(),
            unresolved: vec![UnresolvedConstraint {
                code: UnresolvedConstraintCode::RequiredEntityMissing,
                relation_index: None,
                constraint_index: Some(0),
                entity_key: None,
                related_key: None,
                entity_id: Some(StableEntityId::new(999).unwrap()),
            }],
        }
    }

    fn query() -> SceneQuery {
        SceneQuery {
            schema_version: SchemaVersion::V1,
            scene_revision: SceneRevision::new(7),
            entity_ids: Vec::new(),
            component_kinds: Vec::new(),
            limit: NonZeroU32::new(1).unwrap(),
        }
    }

    fn observation_request(observation_id: u128) -> ObservationRequest {
        ObservationRequest {
            schema_version: SchemaVersion::V1,
            observation_id: ObservationId::new(observation_id).unwrap(),
            scene_revision: SceneRevision::new(7),
            camera_id: StableEntityId::new(8).unwrap(),
            kind: ObservationKind::Visibility,
            quality: ObservationQuality::Low,
        }
    }

    fn receipt(patch: &ScenePatch, status: ApplyStatus) -> ApplyReceipt {
        ApplyReceipt {
            schema_version: SchemaVersion::V1,
            transaction_id: patch.transaction_id,
            idempotency_key: patch.idempotency_key,
            status,
            previous_revision: SceneRevision::new(7),
            new_revision: SceneRevision::new(8),
            operation_count: NonZeroU32::new(1).unwrap(),
            diagnostics: Vec::new(),
            timing: ApplyTiming {
                decode_micros: 1,
                validate_micros: 2,
                commit_micros: 3,
            },
            estimated_visible_frame: FrameId::new(9).unwrap(),
        }
    }

    fn client_frame(
        correlation_id: u64,
        message: LocalSessionClientKind,
        config: &LocalFrameConfig,
    ) -> LocalFrame {
        client_control_frame(
            correlation(correlation_id),
            &LocalSessionClientMessage {
                schema_version: LOCAL_SESSION_SCHEMA_VERSION,
                message,
            },
            config,
        )
        .unwrap()
    }

    fn client_frame_v2(
        correlation_id: u64,
        message: LocalSessionClientKind,
        config: &LocalFrameConfig,
    ) -> LocalFrame {
        client_control_frame(
            correlation(correlation_id),
            &LocalSessionClientMessage {
                schema_version: LOCAL_SESSION_SCHEMA_VERSION_V2,
                message,
            },
            config,
        )
        .unwrap()
    }

    fn hello(core: &mut SessionCore<TestService>) {
        let config = LocalFrameConfig::default();
        let output = core
            .handle_frame(&client_frame(
                1,
                LocalSessionClientKind::Hello(ClientHello {
                    receive_limits: LocalSessionLimits::from_config(&config).unwrap(),
                    compilation_receive_limits: None,
                }),
                &config,
            ))
            .unwrap();
        assert_eq!(output.len(), 1);
        assert!(matches!(
            server_kind(&output[0], &config),
            LocalSessionServerKind::Hello(_)
        ));
    }

    fn hello_v2(core: &mut SessionCore<TestService>) {
        let config = LocalFrameConfig::default();
        let compilation_limits = CompilationLimits::default();
        let output = core
            .handle_frame(&client_frame_v2(
                1,
                LocalSessionClientKind::Hello(ClientHello {
                    receive_limits: LocalSessionLimits::from_config(&config).unwrap(),
                    compilation_receive_limits: Some(compilation_limits),
                }),
                &config,
            ))
            .unwrap();
        assert_eq!(output.len(), 1);
        assert!(matches!(
            server_kind_v2(&output[0], &config, &compilation_limits),
            LocalSessionServerKind::Hello(ServerHello {
                effective_compilation_limits: Some(limits),
                ..
            }) if limits == compilation_limits
        ));
    }

    fn server_kind(frame: &LocalFrame, config: &LocalFrameConfig) -> LocalSessionServerKind {
        decode_server_control_frame(frame, config)
            .unwrap()
            .1
            .message
    }

    fn server_kind_v2(
        frame: &LocalFrame,
        config: &LocalFrameConfig,
        compilation_limits: &CompilationLimits,
    ) -> LocalSessionServerKind {
        decode_server_control_frame_with_limits(frame, config, compilation_limits)
            .unwrap()
            .1
            .message
    }

    fn assert_failure(frame: &LocalFrame, expected: SessionFailureCode, config: &LocalFrameConfig) {
        assert!(matches!(
            server_kind(frame, config),
            LocalSessionServerKind::Failure(SessionFailure { code }) if code == expected
        ));
    }

    #[test]
    fn constructor_requires_quiescent_service_and_bounded_config() {
        let busy = TestService {
            command_depth: Cell::new(1),
            ..TestService::default()
        };
        assert!(matches!(
            SessionCore::new(busy, LocalExecutorConfig::default()),
            Err(LocalExecutorError::ServiceNotQuiescent { commands: 1, .. })
        ));

        let output_config = LocalExecutorConfig {
            max_output_frames_per_call: NonZeroU32::new(1).unwrap(),
            ..LocalExecutorConfig::default()
        };
        assert!(matches!(
            SessionCore::new(TestService::default(), output_config),
            Err(LocalExecutorError::InvalidConfig { .. })
        ));

        let bounded = core(TestService::default());
        assert_eq!(bounded.status().live_correlation_capacity, 72);
    }

    #[test]
    fn configured_live_capacity_rejects_before_service_work_and_releases_for_reuse() {
        let config = LocalFrameConfig::default();
        let first_patch = patch(1, DeliverySemantic::MustApply);
        let mut core = SessionCore::new(
            TestService::default(),
            LocalExecutorConfig {
                max_live_correlations: NonZeroU32::new(1).unwrap(),
                ..LocalExecutorConfig::default()
            },
        )
        .unwrap();
        hello(&mut core);

        core.handle_frame(&client_frame(
            70,
            LocalSessionClientKind::SubmitPatch(SubmitPatch {
                patch: first_patch.clone(),
            }),
            &config,
        ))
        .unwrap();
        assert_eq!(core.status().live_correlations, 1);
        assert_eq!(core.status().live_correlation_capacity, 1);

        let rejected = core
            .handle_frame(&client_frame(
                71,
                LocalSessionClientKind::RequestObservation(RequestObservation {
                    request: observation_request(71),
                }),
                &config,
            ))
            .unwrap();
        assert_failure(&rejected[0], SessionFailureCode::CapacityExceeded, &config);
        assert_eq!(core.service.snapshot().outstanding_observations, 0);

        core.service
            .responses
            .get_mut()
            .push_back(Ok(Some(GatewayResponse::PatchApplied {
                receipt: receipt(&first_patch, ApplyStatus::Applied),
            })));
        core.advance().unwrap();
        assert_eq!(core.status().live_correlations, 0);

        core.handle_frame(&client_frame(
            71,
            LocalSessionClientKind::RequestObservation(RequestObservation {
                request: observation_request(71),
            }),
            &config,
        ))
        .unwrap();
        assert_eq!(core.status().live_correlations, 1);
        assert_eq!(core.service.snapshot().outstanding_observations, 1);
    }

    #[test]
    fn service_runtime_limits_participate_in_hello_negotiation() {
        let runtime_limits = RuntimeLimits {
            max_query_entities: NonZeroU32::new(1).unwrap(),
            ..RuntimeLimits::default()
        };
        let service = TestService {
            runtime_limits: Some(runtime_limits),
            ..TestService::default()
        };
        let mut core = core(service);
        hello(&mut core);
        assert_eq!(
            core.negotiated_limits()
                .unwrap()
                .runtime_limits
                .max_query_entities
                .get(),
            1
        );
    }

    #[test]
    fn version_two_compilation_limits_negotiate_fieldwise() {
        let frame_config = LocalFrameConfig::default();
        let executor_config = LocalExecutorConfig {
            compilation_limits: CompilationLimits {
                max_decisions: NonZeroU32::new(100).unwrap(),
                ..CompilationLimits::default()
            },
            ..LocalExecutorConfig::default()
        };
        let advertised = CompilationLimits {
            max_encoded_bytes: NonZeroU64::new(2_000_000).unwrap(),
            ..CompilationLimits::default()
        };
        let expected =
            intersect_compilation_limits(executor_config.compilation_limits, advertised).unwrap();
        let mut core = SessionCore::new(TestService::default(), executor_config).unwrap();
        let output = core
            .handle_frame(&client_frame_v2(
                1,
                LocalSessionClientKind::Hello(ClientHello {
                    receive_limits: LocalSessionLimits::from_config(&frame_config).unwrap(),
                    compilation_receive_limits: Some(advertised),
                }),
                &frame_config,
            ))
            .unwrap();
        assert_eq!(core.negotiated_compilation_limits(), Some(expected));
        assert!(matches!(
            server_kind_v2(&output[0], &frame_config, &expected),
            LocalSessionServerKind::Hello(ServerHello {
                effective_compilation_limits: Some(limits),
                ..
            }) if limits == expected
        ));
    }

    #[test]
    fn exactly_one_hello_precedes_service_work_and_close_is_terminal() {
        let config = LocalFrameConfig::default();
        let mut core = core(TestService::default());
        let prehello = core
            .handle_frame(&client_frame(
                2,
                LocalSessionClientKind::Query(QueryRequest { query: query() }),
                &config,
            ))
            .unwrap();
        assert_failure(&prehello[0], SessionFailureCode::ProtocolState, &config);

        let prehello_v2 = core
            .handle_frame(&client_frame_v2(
                20,
                LocalSessionClientKind::Query(QueryRequest { query: query() }),
                &config,
            ))
            .unwrap();
        assert!(matches!(
            server_kind_v2(&prehello_v2[0], &config, &CompilationLimits::default()),
            LocalSessionServerKind::Failure(SessionFailure {
                code: SessionFailureCode::ProtocolState,
            })
        ));

        hello(&mut core);
        let duplicate = core
            .handle_frame(&client_frame(
                3,
                LocalSessionClientKind::Hello(ClientHello {
                    receive_limits: LocalSessionLimits::from_config(&config).unwrap(),
                    compilation_receive_limits: None,
                }),
                &config,
            ))
            .unwrap();
        assert_failure(&duplicate[0], SessionFailureCode::ProtocolState, &config);

        let closed = core
            .handle_frame(&client_frame(
                4,
                LocalSessionClientKind::Close(SessionClose {}),
                &config,
            ))
            .unwrap();
        assert!(matches!(
            server_kind(&closed[0], &config),
            LocalSessionServerKind::Closed(_)
        ));
        assert_eq!(core.status().phase, LocalExecutorPhase::Closed);
        let postclose = core
            .handle_frame(&client_frame(
                5,
                LocalSessionClientKind::Query(QueryRequest { query: query() }),
                &config,
            ))
            .unwrap();
        assert_failure(&postclose[0], SessionFailureCode::ProtocolState, &config);
        assert!(core.advance().unwrap().is_empty());
    }

    #[test]
    fn imagination_completion_replay_and_version_lock_are_exact_once() {
        let config = LocalFrameConfig::default();
        let limits = CompilationLimits::default();
        let request = imagination(20, DeliverySemantic::MustApply);
        let normalized_patch = patch(20, DeliverySemantic::MustApply);
        let compiled = compilation(&request, Some(normalized_patch.clone()));
        let applied = receipt(&normalized_patch, ApplyStatus::Applied);
        let mut replay_receipt = applied.clone();
        replay_receipt.status = ApplyStatus::IdempotentReplay;
        let service = TestService::default();
        service
            .responses
            .borrow_mut()
            .push_back(Ok(Some(GatewayResponse::ImaginationProcessed {
                compilation: Box::new(compiled.clone()),
                receipt: Some(applied.clone()),
            })));
        let mut core = core(service);
        hello_v2(&mut core);

        let admitted = core
            .handle_frame(&client_frame_v2(
                2,
                LocalSessionClientKind::SubmitImagination(SubmitImagination {
                    imagination: request.clone(),
                }),
                &config,
            ))
            .unwrap();
        assert!(matches!(
            server_kind_v2(&admitted[0], &config, &limits),
            LocalSessionServerKind::ImaginationAdmission(ImaginationAdmission {
                status: ImaginationAdmissionStatus::Queued,
                ..
            })
        ));
        assert_eq!(core.status().pending_imaginations, 1);
        assert_eq!(core.status().pending_patches, 0);

        let completed = core.advance().unwrap();
        assert_eq!(completed[0].correlation_id(), correlation(2));
        assert!(matches!(
            server_kind_v2(&completed[0], &config, &limits),
            LocalSessionServerKind::ImaginationCompleted(ImaginationCompletion {
                imagination_id,
                idempotency_key,
                compilation,
                receipt: Some(receipt),
            }) if imagination_id == request.imagination_id
                && idempotency_key == request.idempotency_key
                && compilation == compiled
                && receipt == applied
        ));
        assert_eq!(core.status().live_correlations, 0);
        assert_eq!(core.service.process_calls.get(), 1);

        core.service
            .admissions
            .borrow_mut()
            .push_back(Ok(GatewayAdmission::Replayed {
                response: Box::new(GatewayResponse::ImaginationProcessed {
                    compilation: Box::new(compiled.clone()),
                    receipt: Some(replay_receipt.clone()),
                }),
            }));
        let replayed = core
            .handle_frame(&client_frame_v2(
                3,
                LocalSessionClientKind::SubmitImagination(SubmitImagination {
                    imagination: request.clone(),
                }),
                &config,
            ))
            .unwrap();
        let LocalSessionServerKind::ImaginationAdmission(admission) =
            server_kind_v2(&replayed[0], &config, &limits)
        else {
            panic!("expected imagination replay admission");
        };
        assert_eq!(admission.imagination_id, request.imagination_id);
        assert_eq!(admission.idempotency_key, request.idempotency_key);
        let ImaginationAdmissionStatus::Replayed { completion } = admission.status else {
            panic!("expected retained completion");
        };
        assert_eq!(completion.compilation, compiled);
        assert_eq!(completion.receipt, Some(replay_receipt));
        assert_eq!(core.service.process_calls.get(), 1);
        assert_eq!(core.status().live_correlations, 0);

        let mixed_version = core
            .handle_frame(&client_frame(
                4,
                LocalSessionClientKind::Query(QueryRequest { query: query() }),
                &config,
            ))
            .unwrap();
        assert!(matches!(
            server_kind_v2(&mixed_version[0], &config, &limits),
            LocalSessionServerKind::Failure(SessionFailure {
                code: SessionFailureCode::UnsupportedVersion,
            })
        ));
    }

    #[test]
    fn imagination_already_queued_and_dropped_admissions_are_terminal() {
        let config = LocalFrameConfig::default();
        let limits = CompilationLimits::default();
        let request = imagination(25, DeliverySemantic::MustApply);
        let mut core = core(TestService::default());
        hello_v2(&mut core);
        core.handle_frame(&client_frame_v2(
            2,
            LocalSessionClientKind::SubmitImagination(SubmitImagination {
                imagination: request.clone(),
            }),
            &config,
        ))
        .unwrap();

        core.service
            .admissions
            .borrow_mut()
            .push_back(Ok(GatewayAdmission::AlreadyQueued {
                idempotency_key: request.idempotency_key,
            }));
        let already_queued = core
            .handle_frame(&client_frame_v2(
                3,
                LocalSessionClientKind::SubmitImagination(SubmitImagination {
                    imagination: request,
                }),
                &config,
            ))
            .unwrap();
        assert!(matches!(
            server_kind_v2(&already_queued[0], &config, &limits),
            LocalSessionServerKind::ImaginationAdmission(ImaginationAdmission {
                status: ImaginationAdmissionStatus::AlreadyQueued,
                ..
            })
        ));
        assert_eq!(core.status().live_correlations, 1);

        let dropped_request = imagination(26, DeliverySemantic::BestEffort);
        core.service
            .admissions
            .borrow_mut()
            .push_back(Ok(GatewayAdmission::Dropped {
                idempotency_key: dropped_request.idempotency_key,
            }));
        let dropped = core
            .handle_frame(&client_frame_v2(
                4,
                LocalSessionClientKind::SubmitImagination(SubmitImagination {
                    imagination: dropped_request,
                }),
                &config,
            ))
            .unwrap();
        assert!(matches!(
            server_kind_v2(&dropped[0], &config, &limits),
            LocalSessionServerKind::ImaginationAdmission(ImaginationAdmission {
                status: ImaginationAdmissionStatus::Dropped,
                ..
            })
        ));
        assert_eq!(core.status().live_correlations, 1);
    }

    #[test]
    fn active_v2_decode_failure_preserves_the_negotiated_version() {
        let config = LocalFrameConfig::default();
        let limits = CompilationLimits::default();
        let mut core = core(TestService::default());
        hello_v2(&mut core);
        let malformed = LocalFrame::Control {
            correlation_id: correlation(5),
            bytes: b"{}\n".to_vec(),
        };
        let malformed = core.handle_frame(&malformed).unwrap();
        assert!(matches!(
            server_kind_v2(&malformed[0], &config, &limits),
            LocalSessionServerKind::Failure(SessionFailure {
                code: SessionFailureCode::InvalidMessage,
            })
        ));
    }

    #[test]
    fn imagination_supersession_releases_old_correlation_and_retains_new_kind() {
        let config = LocalFrameConfig::default();
        let limits = CompilationLimits::default();
        let delivery = DeliverySemantic::LatestWins {
            supersession_key: SceneText::new("draft/scene").unwrap(),
        };
        let first = imagination(30, delivery.clone());
        let second = imagination(31, delivery);
        let first_key = first.idempotency_key;
        let second_key = second.idempotency_key;
        let second_patch = patch(31, second.delivery.clone());
        let second_compilation = compilation(&second, Some(second_patch.clone()));
        let service = TestService::default();
        service.admissions.borrow_mut().extend([
            Ok(GatewayAdmission::Queued {
                idempotency_key: first_key,
            }),
            Ok(GatewayAdmission::Superseded {
                idempotency_key: second_key,
                superseded_idempotency_key: first_key,
            }),
        ]);
        service
            .responses
            .borrow_mut()
            .push_back(Ok(Some(GatewayResponse::ImaginationProcessed {
                compilation: Box::new(second_compilation.clone()),
                receipt: Some(receipt(&second_patch, ApplyStatus::Applied)),
            })));
        let mut executor = core(service);
        hello_v2(&mut executor);
        executor
            .handle_frame(&client_frame_v2(
                2,
                LocalSessionClientKind::SubmitImagination(SubmitImagination { imagination: first }),
                &config,
            ))
            .unwrap();
        let superseded = executor
            .handle_frame(&client_frame_v2(
                3,
                LocalSessionClientKind::SubmitImagination(SubmitImagination {
                    imagination: second,
                }),
                &config,
            ))
            .unwrap();
        assert_eq!(superseded.len(), 2);
        assert_eq!(superseded[0].correlation_id(), correlation(2));
        assert!(matches!(
            server_kind_v2(&superseded[0], &config, &limits),
            LocalSessionServerKind::Failure(SessionFailure {
                code: SessionFailureCode::CommandRejected,
            })
        ));
        assert_eq!(superseded[1].correlation_id(), correlation(3));
        assert!(matches!(
            server_kind_v2(&superseded[1], &config, &limits),
            LocalSessionServerKind::ImaginationAdmission(ImaginationAdmission {
                status: ImaginationAdmissionStatus::Superseded {
                    superseded_idempotency_key,
                },
                ..
            }) if superseded_idempotency_key == first_key
        ));
        assert_eq!(executor.status().live_correlations, 1);
        assert_eq!(executor.status().pending_imaginations, 1);
        let completed = executor.advance().unwrap();
        assert_eq!(completed[0].correlation_id(), correlation(3));
        assert!(matches!(
            server_kind_v2(&completed[0], &config, &limits),
            LocalSessionServerKind::ImaginationCompleted(ImaginationCompletion {
                compilation,
                ..
            }) if compilation == second_compilation
        ));
        assert_eq!(executor.status().live_correlations, 0);
    }

    #[test]
    fn patch_to_imagination_supersession_replaces_pending_kind() {
        let config = LocalFrameConfig::default();
        let limits = CompilationLimits::default();
        let delivery = DeliverySemantic::LatestWins {
            supersession_key: SceneText::new("draft/cross-kind").unwrap(),
        };

        let old_patch = patch(50, delivery.clone());
        let new_imagination = imagination(51, delivery.clone());
        let normalized_patch = patch(51, delivery.clone());
        let compiled = compilation(&new_imagination, Some(normalized_patch.clone()));
        let service = TestService::default();
        service.admissions.borrow_mut().extend([
            Ok(GatewayAdmission::Queued {
                idempotency_key: old_patch.idempotency_key,
            }),
            Ok(GatewayAdmission::Superseded {
                idempotency_key: new_imagination.idempotency_key,
                superseded_idempotency_key: old_patch.idempotency_key,
            }),
        ]);
        service
            .responses
            .borrow_mut()
            .push_back(Ok(Some(GatewayResponse::ImaginationProcessed {
                compilation: Box::new(compiled),
                receipt: Some(receipt(&normalized_patch, ApplyStatus::Applied)),
            })));
        let mut patch_to_imagination = core(service);
        hello_v2(&mut patch_to_imagination);
        patch_to_imagination
            .handle_frame(&client_frame_v2(
                2,
                LocalSessionClientKind::SubmitPatch(SubmitPatch {
                    patch: old_patch.clone(),
                }),
                &config,
            ))
            .unwrap();
        let replacement = patch_to_imagination
            .handle_frame(&client_frame_v2(
                3,
                LocalSessionClientKind::SubmitImagination(SubmitImagination {
                    imagination: new_imagination,
                }),
                &config,
            ))
            .unwrap();
        assert_eq!(replacement[0].correlation_id(), correlation(2));
        assert_eq!(replacement[1].correlation_id(), correlation(3));
        assert_eq!(patch_to_imagination.status().pending_patches, 0);
        assert_eq!(patch_to_imagination.status().pending_imaginations, 1);
        let completed = patch_to_imagination.advance().unwrap();
        assert_eq!(completed[0].correlation_id(), correlation(3));
        assert!(matches!(
            server_kind_v2(&completed[0], &config, &limits),
            LocalSessionServerKind::ImaginationCompleted(_)
        ));
    }

    #[test]
    fn imagination_to_patch_supersession_replaces_pending_kind() {
        let config = LocalFrameConfig::default();
        let limits = CompilationLimits::default();
        let delivery = DeliverySemantic::LatestWins {
            supersession_key: SceneText::new("draft/cross-kind").unwrap(),
        };
        let old_imagination = imagination(60, delivery.clone());
        let new_patch = patch(61, delivery);
        let service = TestService::default();
        service.admissions.borrow_mut().extend([
            Ok(GatewayAdmission::Queued {
                idempotency_key: old_imagination.idempotency_key,
            }),
            Ok(GatewayAdmission::Superseded {
                idempotency_key: new_patch.idempotency_key,
                superseded_idempotency_key: old_imagination.idempotency_key,
            }),
        ]);
        service
            .responses
            .borrow_mut()
            .push_back(Ok(Some(GatewayResponse::PatchApplied {
                receipt: receipt(&new_patch, ApplyStatus::Applied),
            })));
        let mut imagination_to_patch = core(service);
        hello_v2(&mut imagination_to_patch);
        imagination_to_patch
            .handle_frame(&client_frame_v2(
                2,
                LocalSessionClientKind::SubmitImagination(SubmitImagination {
                    imagination: old_imagination,
                }),
                &config,
            ))
            .unwrap();
        let replacement = imagination_to_patch
            .handle_frame(&client_frame_v2(
                3,
                LocalSessionClientKind::SubmitPatch(SubmitPatch { patch: new_patch }),
                &config,
            ))
            .unwrap();
        assert_eq!(replacement[0].correlation_id(), correlation(2));
        assert_eq!(replacement[1].correlation_id(), correlation(3));
        assert_eq!(imagination_to_patch.status().pending_patches, 1);
        assert_eq!(imagination_to_patch.status().pending_imaginations, 0);
        let completed = imagination_to_patch.advance().unwrap();
        assert_eq!(completed[0].correlation_id(), correlation(3));
        assert!(matches!(
            server_kind_v2(&completed[0], &config, &limits),
            LocalSessionServerKind::PatchCompleted(_)
        ));
        assert_eq!(imagination_to_patch.status().live_correlations, 0);
    }

    #[test]
    fn unresolved_imagination_and_service_error_are_terminal_without_receipts() {
        let config = LocalFrameConfig::default();
        let limits = CompilationLimits::default();
        let unresolved_request = imagination(40, DeliverySemantic::MustApply);
        let unresolved = unresolved_compilation(&unresolved_request);
        let failed_request = imagination(41, DeliverySemantic::MustApply);
        let service = TestService::default();
        service.responses.borrow_mut().extend([
            Ok(Some(GatewayResponse::ImaginationProcessed {
                compilation: Box::new(unresolved.clone()),
                receipt: None,
            })),
            Err(SessionFailureCode::Internal),
        ]);
        let mut core = core(service);
        hello_v2(&mut core);

        core.handle_frame(&client_frame_v2(
            2,
            LocalSessionClientKind::SubmitImagination(SubmitImagination {
                imagination: unresolved_request,
            }),
            &config,
        ))
        .unwrap();
        let completion = core.advance().unwrap();
        assert!(matches!(
            server_kind_v2(&completion[0], &config, &limits),
            LocalSessionServerKind::ImaginationCompleted(ImaginationCompletion {
                compilation,
                receipt: None,
                ..
            }) if compilation == unresolved
        ));
        assert_eq!(core.status().live_correlations, 0);

        core.handle_frame(&client_frame_v2(
            3,
            LocalSessionClientKind::SubmitImagination(SubmitImagination {
                imagination: failed_request,
            }),
            &config,
        ))
        .unwrap();
        let failed = core.advance().unwrap();
        assert!(matches!(
            server_kind_v2(&failed[0], &config, &limits),
            LocalSessionServerKind::Failure(SessionFailure {
                code: SessionFailureCode::Internal,
            })
        ));
        assert_eq!(core.status().live_correlations, 0);
        assert_eq!(core.status().pending_imaginations, 0);
    }

    #[test]
    fn wrong_direction_and_frame_kind_fail_without_service_work() {
        let config = LocalFrameConfig::default();
        let mut core = core(TestService::default());
        let wrong_direction = server_control_frame(
            correlation(60),
            &LocalSessionServerMessage {
                schema_version: LOCAL_SESSION_SCHEMA_VERSION,
                message: LocalSessionServerKind::Failure(SessionFailure {
                    code: SessionFailureCode::Internal,
                }),
            },
            &config,
        )
        .unwrap();
        let rejected = core.handle_frame(&wrong_direction).unwrap();
        assert_failure(&rejected[0], SessionFailureCode::InvalidMessage, &config);
        assert_eq!(core.service.snapshot().command_depth, 0);
        assert_eq!(core.service.snapshot().outstanding_observations, 0);

        hello(&mut core);
        let request = observation_request(61);
        let wrong_kind = LocalFrame::Observation {
            correlation_id: correlation(61),
            metadata: ObservationMetadata {
                schema_version: SchemaVersion::V1,
                observation_id: request.observation_id,
                scene_revision: request.scene_revision,
                frame_id: FrameId::new(1).unwrap(),
                camera_id: request.camera_id,
                kind: ObservationKind::Visibility,
                dimensions: None,
                quality: request.quality,
                observed_at_unix_micros: 1,
                production_latency_micros: 1,
                staleness: ObservationStaleness {
                    latest_known_revision: request.scene_revision,
                    revisions_behind: 0,
                },
            },
            payload: ObservationPayload::Visibility(Vec::new()),
        };
        let rejected = core.handle_frame(&wrong_kind).unwrap();
        assert_failure(&rejected[0], SessionFailureCode::InvalidMessage, &config);
        assert_eq!(core.status().live_correlations, 0);
    }

    #[test]
    fn negotiated_result_limits_fail_before_completion_and_release_once() {
        let config = LocalFrameConfig::default();
        let request = imagination(70, DeliverySemantic::MustApply);
        let normalized_patch = patch(70, DeliverySemantic::MustApply);
        let compiled = compilation(&request, Some(normalized_patch.clone()));
        let default_limits = CompilationLimits::default();
        let encoded_bytes = compiled.to_canonical_json(&default_limits).unwrap().len();
        let mut encoded_limited = default_limits;
        encoded_limited.max_encoded_bytes = NonZeroU64::new(
            u64::try_from(encoded_bytes)
                .unwrap()
                .checked_sub(1)
                .unwrap(),
        )
        .unwrap();
        let mut nesting_limited = default_limits;
        nesting_limited.max_json_nesting_depth = core::num::NonZeroU16::new(1).unwrap();

        for negotiated in [encoded_limited, nesting_limited] {
            let service = TestService::default();
            service.responses.borrow_mut().push_back(Ok(Some(
                GatewayResponse::ImaginationProcessed {
                    compilation: Box::new(compiled.clone()),
                    receipt: Some(receipt(&normalized_patch, ApplyStatus::Applied)),
                },
            )));
            let mut core = core(service);
            core.handle_frame(&client_frame_v2(
                1,
                LocalSessionClientKind::Hello(ClientHello {
                    receive_limits: LocalSessionLimits::from_config(&config).unwrap(),
                    compilation_receive_limits: Some(negotiated),
                }),
                &config,
            ))
            .unwrap();
            assert_eq!(core.service.compilation_limits.get(), Some(negotiated));

            core.handle_frame(&client_frame_v2(
                2,
                LocalSessionClientKind::SubmitImagination(SubmitImagination {
                    imagination: request.clone(),
                }),
                &config,
            ))
            .unwrap();
            let failed = core.advance().unwrap();
            assert!(matches!(
                server_kind_v2(&failed[0], &config, &negotiated),
                LocalSessionServerKind::Failure(SessionFailure {
                    code: SessionFailureCode::LimitExceeded,
                })
            ));
            assert_eq!(core.status().live_correlations, 0);
            assert_eq!(core.status().pending_imaginations, 0);
            assert_eq!(core.service.process_calls.get(), 1);

            let mut replay_receipt = receipt(&normalized_patch, ApplyStatus::IdempotentReplay);
            replay_receipt.status = ApplyStatus::IdempotentReplay;
            core.service
                .admissions
                .borrow_mut()
                .push_back(Ok(GatewayAdmission::Replayed {
                    response: Box::new(GatewayResponse::ImaginationProcessed {
                        compilation: Box::new(compiled.clone()),
                        receipt: Some(replay_receipt),
                    }),
                }));
            let replay = core
                .handle_frame(&client_frame_v2(
                    3,
                    LocalSessionClientKind::SubmitImagination(SubmitImagination {
                        imagination: request.clone(),
                    }),
                    &config,
                ))
                .unwrap();
            assert!(matches!(
                server_kind_v2(&replay[0], &config, &negotiated),
                LocalSessionServerKind::Failure(SessionFailure {
                    code: SessionFailureCode::LimitExceeded,
                })
            ));
            assert_eq!(core.service.process_calls.get(), 1);
            assert_eq!(core.status().live_correlations, 0);
        }
    }

    #[test]
    fn imagination_completion_binds_submitted_transaction_and_revision() {
        let config = LocalFrameConfig::default();
        let request = imagination(80, DeliverySemantic::MustApply);
        let normalized_patch = patch(80, DeliverySemantic::MustApply);

        let mut wrong_revision = compilation(&request, Some(normalized_patch.clone()));
        wrong_revision.scene_revision = SceneRevision::new(8);
        wrong_revision.patch.as_mut().unwrap().base_revision = SceneRevision::new(8);
        let mut wrong_revision_receipt = receipt(&normalized_patch, ApplyStatus::Applied);
        wrong_revision_receipt.previous_revision = SceneRevision::new(8);
        wrong_revision_receipt.new_revision = SceneRevision::new(9);
        let service = TestService::default();
        service
            .responses
            .borrow_mut()
            .push_back(Ok(Some(GatewayResponse::ImaginationProcessed {
                compilation: Box::new(wrong_revision),
                receipt: Some(wrong_revision_receipt),
            })));
        let mut executor = core(service);
        hello_v2(&mut executor);
        executor
            .handle_frame(&client_frame_v2(
                2,
                LocalSessionClientKind::SubmitImagination(SubmitImagination {
                    imagination: request.clone(),
                }),
                &config,
            ))
            .unwrap();
        let failed = executor.advance().unwrap();
        assert!(matches!(
            server_kind_v2(&failed[0], &config, &CompilationLimits::default()),
            LocalSessionServerKind::Failure(SessionFailure {
                code: SessionFailureCode::Internal,
            })
        ));
        assert_eq!(executor.status().live_correlations, 0);

        let mut wrong_transaction = compilation(&request, Some(normalized_patch.clone()));
        wrong_transaction.patch.as_mut().unwrap().transaction_id = TransactionId::new(999).unwrap();
        let mut wrong_transaction_receipt =
            receipt(&normalized_patch, ApplyStatus::IdempotentReplay);
        wrong_transaction_receipt.transaction_id = TransactionId::new(999).unwrap();
        let service = TestService::default();
        service
            .admissions
            .borrow_mut()
            .push_back(Ok(GatewayAdmission::Replayed {
                response: Box::new(GatewayResponse::ImaginationProcessed {
                    compilation: Box::new(wrong_transaction),
                    receipt: Some(wrong_transaction_receipt),
                }),
            }));
        let mut executor = core(service);
        hello_v2(&mut executor);
        let failed = executor
            .handle_frame(&client_frame_v2(
                2,
                LocalSessionClientKind::SubmitImagination(SubmitImagination {
                    imagination: request.clone(),
                }),
                &config,
            ))
            .unwrap();
        assert!(matches!(
            server_kind_v2(&failed[0], &config, &CompilationLimits::default()),
            LocalSessionServerKind::Failure(SessionFailure {
                code: SessionFailureCode::Internal,
            })
        ));
        assert_eq!(executor.status().live_correlations, 0);

        let mut unresolved = unresolved_compilation(&request);
        unresolved.scene_revision = SceneRevision::new(8);
        let service = TestService::default();
        service
            .admissions
            .borrow_mut()
            .push_back(Ok(GatewayAdmission::Replayed {
                response: Box::new(GatewayResponse::ImaginationProcessed {
                    compilation: Box::new(unresolved),
                    receipt: None,
                }),
            }));
        let mut executor = core(service);
        hello_v2(&mut executor);
        let failed = executor
            .handle_frame(&client_frame_v2(
                2,
                LocalSessionClientKind::SubmitImagination(SubmitImagination {
                    imagination: request,
                }),
                &config,
            ))
            .unwrap();
        assert!(matches!(
            server_kind_v2(&failed[0], &config, &CompilationLimits::default()),
            LocalSessionServerKind::Failure(SessionFailure {
                code: SessionFailureCode::Internal,
            })
        ));
        assert_eq!(executor.status().live_correlations, 0);
    }

    #[test]
    fn observation_identity_and_dimensions_reject_before_duplicate_service_work() {
        let config = LocalFrameConfig::default();
        let request = observation_request(62);
        let mut core = core(TestService::default());
        hello(&mut core);
        core.handle_frame(&client_frame(
            62,
            LocalSessionClientKind::RequestObservation(RequestObservation { request }),
            &config,
        ))
        .unwrap();
        let duplicate = core
            .handle_frame(&client_frame(
                63,
                LocalSessionClientKind::RequestObservation(RequestObservation { request }),
                &config,
            ))
            .unwrap();
        assert_failure(
            &duplicate[0],
            SessionFailureCode::ObservationRejected,
            &config,
        );
        assert_eq!(core.service.snapshot().outstanding_observations, 1);

        let mut narrow = LocalFrameConfig::default();
        narrow.runtime_limits.max_observation_width = NonZeroU32::new(1).unwrap();
        narrow.runtime_limits.max_observation_pixels = NonZeroU64::new(1).unwrap();
        let oversized_service = TestService {
            observation_dimensions: Some((2, 1)),
            ..TestService::default()
        };
        let mut oversized = SessionCore::new(
            oversized_service,
            LocalExecutorConfig {
                receive_config: narrow.clone(),
                ..LocalExecutorConfig::default()
            },
        )
        .unwrap();
        let hello_limits = LocalSessionLimits::from_config(&narrow).unwrap();
        oversized
            .handle_frame(&client_frame(
                1,
                LocalSessionClientKind::Hello(ClientHello {
                    receive_limits: hello_limits,
                    compilation_receive_limits: None,
                }),
                &narrow,
            ))
            .unwrap();
        let rejected = oversized
            .handle_frame(&client_frame(
                64,
                LocalSessionClientKind::RequestObservation(RequestObservation {
                    request: observation_request(64),
                }),
                &narrow,
            ))
            .unwrap();
        assert_failure(&rejected[0], SessionFailureCode::LimitExceeded, &narrow);
        assert_eq!(oversized.service.snapshot().outstanding_observations, 0);
    }

    #[test]
    fn patch_completion_releases_exact_correlation_and_allows_reuse() {
        let config = LocalFrameConfig::default();
        let submitted = patch(1, DeliverySemantic::MustApply);
        let service = TestService {
            command_capacity: Some(1),
            ..TestService::default()
        };
        service
            .responses
            .borrow_mut()
            .push_back(Ok(Some(GatewayResponse::PatchApplied {
                receipt: receipt(&submitted, ApplyStatus::Applied),
            })));
        let mut core = SessionCore::new(
            service,
            LocalExecutorConfig {
                max_live_correlations: NonZeroU32::new(1).unwrap(),
                ..LocalExecutorConfig::default()
            },
        )
        .unwrap();
        hello(&mut core);

        let admitted = core
            .handle_frame(&client_frame(
                7,
                LocalSessionClientKind::SubmitPatch(SubmitPatch {
                    patch: submitted.clone(),
                }),
                &config,
            ))
            .unwrap();
        assert!(matches!(
            server_kind(&admitted[0], &config),
            LocalSessionServerKind::PatchAdmission(PatchAdmission {
                status: PatchAdmissionStatus::Queued,
                ..
            })
        ));
        assert_eq!(core.status().live_correlations, 1);

        let duplicate = core
            .handle_frame(&client_frame(
                7,
                LocalSessionClientKind::Query(QueryRequest { query: query() }),
                &config,
            ))
            .unwrap();
        assert_failure(&duplicate[0], SessionFailureCode::ProtocolState, &config);

        let completed = core.advance().unwrap();
        assert!(matches!(
            server_kind(&completed[0], &config),
            LocalSessionServerKind::PatchCompleted(_)
        ));
        assert_eq!(completed[0].correlation_id(), correlation(7));
        assert_eq!(core.status().live_correlations, 0);

        let reused = core
            .handle_frame(&client_frame(
                7,
                LocalSessionClientKind::Query(QueryRequest { query: query() }),
                &config,
            ))
            .unwrap();
        assert!(matches!(
            server_kind(&reused[0], &config),
            LocalSessionServerKind::QueryResult(_)
        ));
    }

    #[test]
    fn supersession_terminates_old_correlation_and_retains_new_queue_position() {
        let config = LocalFrameConfig::default();
        let first = patch(
            10,
            DeliverySemantic::LatestWins {
                supersession_key: SceneText::new("camera/main").unwrap(),
            },
        );
        let second = patch(
            11,
            DeliverySemantic::LatestWins {
                supersession_key: SceneText::new("camera/main").unwrap(),
            },
        );
        let service = TestService {
            command_capacity: Some(1),
            ..TestService::default()
        };
        service.admissions.borrow_mut().extend([
            Ok(GatewayAdmission::Queued {
                idempotency_key: first.idempotency_key,
            }),
            Ok(GatewayAdmission::Superseded {
                idempotency_key: second.idempotency_key,
                superseded_idempotency_key: first.idempotency_key,
            }),
        ]);
        service
            .responses
            .borrow_mut()
            .push_back(Ok(Some(GatewayResponse::PatchApplied {
                receipt: receipt(&second, ApplyStatus::Applied),
            })));
        let mut core = SessionCore::new(
            service,
            LocalExecutorConfig {
                max_live_correlations: NonZeroU32::new(1).unwrap(),
                ..LocalExecutorConfig::default()
            },
        )
        .unwrap();
        hello(&mut core);
        core.handle_frame(&client_frame(
            10,
            LocalSessionClientKind::SubmitPatch(SubmitPatch { patch: first }),
            &config,
        ))
        .unwrap();
        let superseded = core
            .handle_frame(&client_frame(
                11,
                LocalSessionClientKind::SubmitPatch(SubmitPatch { patch: second }),
                &config,
            ))
            .unwrap();
        assert_eq!(superseded.len(), MAX_OUTPUT_FRAMES_PER_CALL);
        assert_eq!(superseded[0].correlation_id(), correlation(10));
        assert_failure(&superseded[0], SessionFailureCode::CommandRejected, &config);
        assert_eq!(superseded[1].correlation_id(), correlation(11));
        assert!(matches!(
            server_kind(&superseded[1], &config),
            LocalSessionServerKind::PatchAdmission(PatchAdmission {
                status: PatchAdmissionStatus::Superseded { .. },
                ..
            })
        ));
        assert_eq!(core.status().live_correlations, 1);
        assert_eq!(core.advance().unwrap()[0].correlation_id(), correlation(11));
        assert_eq!(core.status().live_correlations, 0);
    }

    #[test]
    fn already_queued_dropped_and_replayed_admissions_are_terminal() {
        let config = LocalFrameConfig::default();
        let queued = patch(50, DeliverySemantic::MustApply);
        let dropped = patch(51, DeliverySemantic::BestEffort);
        let replayed = patch(52, DeliverySemantic::MustApply);
        let service = TestService {
            command_capacity: Some(1),
            ..TestService::default()
        };
        service.admissions.borrow_mut().extend([
            Ok(GatewayAdmission::Queued {
                idempotency_key: queued.idempotency_key,
            }),
            Ok(GatewayAdmission::AlreadyQueued {
                idempotency_key: queued.idempotency_key,
            }),
            Ok(GatewayAdmission::Dropped {
                idempotency_key: dropped.idempotency_key,
            }),
            Ok(GatewayAdmission::Replayed {
                response: Box::new(GatewayResponse::PatchApplied {
                    receipt: receipt(&replayed, ApplyStatus::IdempotentReplay),
                }),
            }),
        ]);
        let mut core = SessionCore::new(
            service,
            LocalExecutorConfig {
                max_live_correlations: NonZeroU32::new(1).unwrap(),
                ..LocalExecutorConfig::default()
            },
        )
        .unwrap();
        hello(&mut core);

        for (correlation_id, patch, expected) in [
            (50, queued.clone(), PatchAdmissionStatus::Queued),
            (51, queued, PatchAdmissionStatus::AlreadyQueued),
            (52, dropped, PatchAdmissionStatus::Dropped),
        ] {
            let output = core
                .handle_frame(&client_frame(
                    correlation_id,
                    LocalSessionClientKind::SubmitPatch(SubmitPatch { patch }),
                    &config,
                ))
                .unwrap();
            assert!(matches!(
                server_kind(&output[0], &config),
                LocalSessionServerKind::PatchAdmission(PatchAdmission { status, .. })
                    if status == expected
            ));
        }
        let replay = core
            .handle_frame(&client_frame(
                53,
                LocalSessionClientKind::SubmitPatch(SubmitPatch { patch: replayed }),
                &config,
            ))
            .unwrap();
        assert!(matches!(
            server_kind(&replay[0], &config),
            LocalSessionServerKind::PatchAdmission(PatchAdmission {
                status: PatchAdmissionStatus::Replayed { .. },
                ..
            })
        ));
        assert_eq!(core.status().live_correlations, 1);
        assert_eq!(core.status().pending_patches, 1);
    }

    #[test]
    fn observation_pending_is_once_and_completion_releases_exact_identity() {
        let config = LocalFrameConfig::default();
        let request = observation_request(20);
        let metadata = ObservationMetadata {
            schema_version: SchemaVersion::V1,
            observation_id: request.observation_id,
            scene_revision: request.scene_revision,
            frame_id: FrameId::new(2).unwrap(),
            camera_id: request.camera_id,
            kind: ObservationKind::Visibility,
            dimensions: None,
            quality: request.quality,
            observed_at_unix_micros: 1,
            production_latency_micros: 2,
            staleness: ObservationStaleness {
                latest_known_revision: request.scene_revision,
                revisions_behind: 0,
            },
        };
        let service = TestService::default();
        service.deliveries.borrow_mut().extend([
            Ok(None),
            Ok(None),
            Ok(Some(ServiceObservationDelivery::Completed {
                metadata,
                payload: ObservationPayload::Visibility(Vec::new()),
            })),
        ]);
        let mut core = core(service);
        hello(&mut core);
        let accepted = core
            .handle_frame(&client_frame(
                20,
                LocalSessionClientKind::RequestObservation(RequestObservation { request }),
                &config,
            ))
            .unwrap();
        assert!(matches!(
            server_kind(&accepted[0], &config),
            LocalSessionServerKind::ObservationAccepted(_)
        ));
        let pending = core.advance().unwrap();
        assert!(matches!(
            server_kind(&pending[0], &config),
            LocalSessionServerKind::ObservationPending(reference)
                if reference.scene_revision == SceneRevision::new(7)
        ));
        assert!(core.advance().unwrap().is_empty());
        let completed = core.advance().unwrap();
        assert!(matches!(completed[0], LocalFrame::Observation { .. }));
        assert_eq!(completed[0].correlation_id(), correlation(20));
        assert_eq!(core.status().live_correlations, 0);
    }

    #[test]
    fn oversized_observation_becomes_redacted_limit_failure() {
        let mut receive = LocalFrameConfig::default();
        receive.frame_limits.max_bulk_bytes = NonZeroU64::new(1).unwrap();
        receive.payload_limits.max_envelope_bytes = NonZeroU64::new(1).unwrap();
        let config = LocalExecutorConfig {
            receive_config: receive.clone(),
            ..LocalExecutorConfig::default()
        };
        let request = ObservationRequest {
            kind: ObservationKind::Color,
            ..observation_request(30)
        };
        let metadata = ObservationMetadata {
            schema_version: SchemaVersion::V1,
            observation_id: request.observation_id,
            scene_revision: request.scene_revision,
            frame_id: FrameId::new(2).unwrap(),
            camera_id: request.camera_id,
            kind: request.kind,
            dimensions: Some(ImageDimensions {
                width: NonZeroU32::new(1).unwrap(),
                height: NonZeroU32::new(1).unwrap(),
            }),
            quality: request.quality,
            observed_at_unix_micros: 1,
            production_latency_micros: 2,
            staleness: ObservationStaleness {
                latest_known_revision: request.scene_revision,
                revisions_behind: 0,
            },
        };
        let service = TestService::default();
        service.deliveries.borrow_mut().push_back(Ok(Some(
            ServiceObservationDelivery::Completed {
                metadata,
                payload: ObservationPayload::Color(vec![[0, 0, 0, 255]]),
            },
        )));
        let mut core = SessionCore::new(service, config).unwrap();
        let output = core
            .handle_frame(&client_frame(
                1,
                LocalSessionClientKind::Hello(ClientHello {
                    receive_limits: LocalSessionLimits::from_config(&receive).unwrap(),
                    compilation_receive_limits: None,
                }),
                &receive,
            ))
            .unwrap();
        assert!(matches!(
            server_kind(&output[0], &receive),
            LocalSessionServerKind::Hello(_)
        ));
        core.handle_frame(&client_frame(
            30,
            LocalSessionClientKind::RequestObservation(RequestObservation { request }),
            &receive,
        ))
        .unwrap();
        let completed = core.advance().unwrap();
        assert_failure(&completed[0], SessionFailureCode::LimitExceeded, &receive);
        assert_eq!(core.status().live_correlations, 0);
    }

    #[test]
    fn failed_command_and_observation_paths_release_their_live_correlations() {
        let config = LocalFrameConfig::default();
        let submitted = patch(40, DeliverySemantic::MustApply);
        let request = observation_request(41);
        let service = TestService::default();
        service
            .responses
            .borrow_mut()
            .push_back(Err(SessionFailureCode::CommandRejected));
        service
            .deliveries
            .borrow_mut()
            .push_back(Ok(Some(ServiceObservationDelivery::Failed {
                observation_id: request.observation_id,
                code: SessionFailureCode::ServiceUnavailable,
            })));
        let mut core = core(service);
        hello(&mut core);
        core.handle_frame(&client_frame(
            40,
            LocalSessionClientKind::SubmitPatch(SubmitPatch { patch: submitted }),
            &config,
        ))
        .unwrap();
        core.handle_frame(&client_frame(
            41,
            LocalSessionClientKind::RequestObservation(RequestObservation { request }),
            &config,
        ))
        .unwrap();
        let output = core.advance().unwrap();
        assert_eq!(output.len(), MAX_OUTPUT_FRAMES_PER_CALL);
        assert_failure(&output[0], SessionFailureCode::CommandRejected, &config);
        assert_failure(&output[1], SessionFailureCode::ServiceUnavailable, &config);
        assert_eq!(core.status().live_correlations, 0);
    }
}
