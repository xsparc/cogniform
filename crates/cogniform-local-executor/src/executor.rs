use core::num::{NonZeroU32, NonZeroU64};
use std::collections::{BTreeMap, BTreeSet, VecDeque};

use cogniform_engine::{
    EngineError, GatewayAdmission, GatewayError, GatewayResponse, LocalService, LocalServiceError,
    ObservationDelivery, ObservationError, ObservationPayload,
};
use cogniform_local_session::{
    LOCAL_SESSION_SCHEMA_VERSION, LocalSessionClientKind, LocalSessionError, LocalSessionLimits,
    LocalSessionServerKind, LocalSessionServerMessage, ObservationReference, PatchAdmission,
    PatchAdmissionStatus, PatchCompletion, QueryResponse, ServerHello, SessionClosed,
    SessionFailure, SessionFailureCode, decode_client_control_frame, server_control_frame,
};
use cogniform_local_transport::{LocalFrame, LocalFrameConfig, encode_frame};
use cogniform_protocol::{
    ApplyStatus, IdempotencyKey, ObservationId, ObservationMetadata, ObservationRequest,
    RuntimeLimits, ScenePatch, SceneQuery, SceneQueryResult,
};

use crate::LocalExecutorError;

/// Maximum frames returned by one caller-driven executor operation.
pub const MAX_OUTPUT_FRAMES_PER_CALL: usize = 2;

/// Explicit local receive and correlation bounds for one executor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalExecutorConfig {
    /// Local receive policy used before and during negotiation.
    pub receive_config: LocalFrameConfig,
    /// Maximum command and observation correlations live at once.
    pub max_live_correlations: NonZeroU32,
    /// Exact maximum frames returned by one handle or advance call.
    pub max_output_frames_per_call: NonZeroU32,
}

impl Default for LocalExecutorConfig {
    fn default() -> Self {
        Self {
            receive_config: LocalFrameConfig::default(),
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
        limits: LocalSessionLimits,
        frame_config: LocalFrameConfig,
    },
    Closed {
        limits: LocalSessionLimits,
        frame_config: LocalFrameConfig,
    },
}

struct SessionCore<S> {
    service: S,
    config: LocalExecutorConfig,
    state: SessionState,
    live_correlations: BTreeSet<NonZeroU64>,
    patch_correlations: BTreeMap<IdempotencyKey, NonZeroU64>,
    patch_order: VecDeque<IdempotencyKey>,
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
            patch_correlations: BTreeMap::new(),
            patch_order: VecDeque::new(),
            observation_correlations: BTreeMap::new(),
            observation_order: VecDeque::new(),
            observation_pending_reported: BTreeSet::new(),
        })
    }

    fn handle_frame(&mut self, frame: &LocalFrame) -> Result<Vec<LocalFrame>, LocalExecutorError> {
        let correlation_id = frame.correlation_id();
        let decode_config = self.current_frame_config().clone();
        let message = match decode_client_control_frame(frame, &decode_config) {
            Ok((_, message)) => message,
            Err(error) => {
                let code = classify_session_error(&error);
                return single_failure(correlation_id, code, &decode_config);
            }
        };

        match &self.state {
            SessionState::AwaitingHello => match message.message {
                LocalSessionClientKind::Hello(hello) => {
                    let Ok(limits) = hello.receive_limits.negotiate(&self.config.receive_config)
                    else {
                        return single_failure(
                            correlation_id,
                            SessionFailureCode::LimitExceeded,
                            &decode_config,
                        );
                    };
                    let frame_config = limits.to_frame_config().map_err(|_| {
                        LocalExecutorError::InvalidConfig {
                            reason: "negotiated limits are inconsistent",
                        }
                    })?;
                    let output = control_frame(
                        correlation_id,
                        LocalSessionServerKind::Hello(ServerHello {
                            effective_limits: limits,
                        }),
                        &frame_config,
                    )?;
                    self.state = SessionState::Active {
                        limits,
                        frame_config,
                    };
                    Ok(vec![output])
                }
                _ => single_failure(
                    correlation_id,
                    SessionFailureCode::ProtocolState,
                    &decode_config,
                ),
            },
            SessionState::Closed { .. } => single_failure(
                correlation_id,
                SessionFailureCode::ProtocolState,
                &decode_config,
            ),
            SessionState::Active { .. } => {
                if self.live_correlations.contains(&correlation_id) {
                    return single_failure(
                        correlation_id,
                        SessionFailureCode::ProtocolState,
                        &decode_config,
                    );
                }
                match message.message {
                    LocalSessionClientKind::Hello(_) => single_failure(
                        correlation_id,
                        SessionFailureCode::ProtocolState,
                        &decode_config,
                    ),
                    LocalSessionClientKind::SubmitPatch(submit) => {
                        self.submit_patch(correlation_id, submit.patch, &decode_config)
                    }
                    LocalSessionClientKind::Query(request) => {
                        self.query(correlation_id, &request.query, &decode_config)
                    }
                    LocalSessionClientKind::RequestObservation(request) => {
                        self.request_observation(correlation_id, request.request, &decode_config)
                    }
                    LocalSessionClientKind::Close(_) => self.close(correlation_id, &decode_config),
                }
            }
        }
    }

    fn submit_patch(
        &mut self,
        correlation_id: NonZeroU64,
        patch: ScenePatch,
        frame_config: &LocalFrameConfig,
    ) -> Result<Vec<LocalFrame>, LocalExecutorError> {
        if self.live_capacity_reached()
            && self.service.snapshot().command_depth < self.service.command_capacity()
        {
            return single_failure(
                correlation_id,
                SessionFailureCode::CapacityExceeded,
                frame_config,
            );
        }
        let admission = match self.service.submit_patch(patch) {
            Ok(admission) => admission,
            Err(code) => return single_failure(correlation_id, code, frame_config),
        };
        match admission {
            GatewayAdmission::Queued { idempotency_key } => {
                if count(self.patch_order.len()) >= self.service.command_capacity() {
                    return Err(LocalExecutorError::StateInvariant {
                        reason: "service queued a patch beyond its declared capacity",
                    });
                }
                let output = patch_admission_frame(
                    correlation_id,
                    idempotency_key,
                    PatchAdmissionStatus::Queued,
                    frame_config,
                )?;
                self.reserve_correlation(correlation_id)?;
                self.patch_correlations
                    .insert(idempotency_key, correlation_id);
                self.patch_order.push_back(idempotency_key);
                Ok(vec![output])
            }
            GatewayAdmission::AlreadyQueued { idempotency_key } => {
                if !self.patch_correlations.contains_key(&idempotency_key) {
                    return Err(LocalExecutorError::StateInvariant {
                        reason: "already-queued patch had no live correlation",
                    });
                }
                Ok(vec![patch_admission_frame(
                    correlation_id,
                    idempotency_key,
                    PatchAdmissionStatus::AlreadyQueued,
                    frame_config,
                )?])
            }
            GatewayAdmission::Dropped { idempotency_key } => Ok(vec![patch_admission_frame(
                correlation_id,
                idempotency_key,
                PatchAdmissionStatus::Dropped,
                frame_config,
            )?]),
            GatewayAdmission::Replayed { response } => {
                let GatewayResponse::PatchApplied { receipt } = *response else {
                    return Err(LocalExecutorError::StateInvariant {
                        reason: "patch admission replayed a non-patch response",
                    });
                };
                let idempotency_key = receipt.idempotency_key;
                Ok(vec![control_or_limit_failure(
                    correlation_id,
                    LocalSessionServerKind::PatchAdmission(PatchAdmission {
                        idempotency_key,
                        status: PatchAdmissionStatus::Replayed { receipt },
                    }),
                    frame_config,
                )?])
            }
            GatewayAdmission::Superseded {
                idempotency_key,
                superseded_idempotency_key,
            } => self.handle_supersession(
                correlation_id,
                idempotency_key,
                superseded_idempotency_key,
                frame_config,
            ),
        }
    }

    fn handle_supersession(
        &mut self,
        correlation_id: NonZeroU64,
        idempotency_key: IdempotencyKey,
        superseded_idempotency_key: IdempotencyKey,
        frame_config: &LocalFrameConfig,
    ) -> Result<Vec<LocalFrame>, LocalExecutorError> {
        let old_correlation = self
            .patch_correlations
            .get(&superseded_idempotency_key)
            .copied()
            .ok_or(LocalExecutorError::StateInvariant {
                reason: "superseded patch had no live correlation",
            })?;
        let rejected = failure_frame(
            old_correlation,
            SessionFailureCode::CommandRejected,
            frame_config,
        )?;
        let admitted = patch_admission_frame(
            correlation_id,
            idempotency_key,
            PatchAdmissionStatus::Superseded {
                superseded_idempotency_key,
            },
            frame_config,
        )?;
        let Some(position) = self
            .patch_order
            .iter()
            .position(|key| *key == superseded_idempotency_key)
        else {
            return Err(LocalExecutorError::StateInvariant {
                reason: "superseded patch was absent from local order",
            });
        };
        self.patch_order[position] = idempotency_key;
        self.patch_correlations.remove(&superseded_idempotency_key);
        self.release_correlation(old_correlation)?;
        self.reserve_correlation(correlation_id)?;
        self.patch_correlations
            .insert(idempotency_key, correlation_id);
        Ok(vec![rejected, admitted])
    }

    fn query(
        &self,
        correlation_id: NonZeroU64,
        query: &SceneQuery,
        frame_config: &LocalFrameConfig,
    ) -> Result<Vec<LocalFrame>, LocalExecutorError> {
        match self.service.query(query) {
            Ok(result) => Ok(vec![control_or_limit_failure(
                correlation_id,
                LocalSessionServerKind::QueryResult(QueryResponse { result }),
                frame_config,
            )?]),
            Err(code) => single_failure(correlation_id, code, frame_config),
        }
    }

    fn request_observation(
        &mut self,
        correlation_id: NonZeroU64,
        request: ObservationRequest,
        frame_config: &LocalFrameConfig,
    ) -> Result<Vec<LocalFrame>, LocalExecutorError> {
        if self.live_capacity_reached()
            || count(self.observation_order.len()) >= self.service.observation_capacity()
        {
            return single_failure(
                correlation_id,
                SessionFailureCode::CapacityExceeded,
                frame_config,
            );
        }
        if self
            .observation_correlations
            .contains_key(&request.observation_id)
        {
            return single_failure(
                correlation_id,
                SessionFailureCode::ObservationRejected,
                frame_config,
            );
        }
        let (width, height) = self.service.observation_dimensions();
        let pixels = u64::from(width).saturating_mul(u64::from(height));
        let limits = &frame_config.runtime_limits;
        if width > limits.max_observation_width.get()
            || height > limits.max_observation_height.get()
            || pixels > limits.max_observation_pixels.get()
        {
            return single_failure(
                correlation_id,
                SessionFailureCode::LimitExceeded,
                frame_config,
            );
        }
        let reference = observation_reference(request);
        let output = control_frame(
            correlation_id,
            LocalSessionServerKind::ObservationAccepted(reference),
            frame_config,
        )?;
        if let Err(code) = self.service.request_observation(request) {
            return single_failure(correlation_id, code, frame_config);
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
        frame_config: &LocalFrameConfig,
    ) -> Result<Vec<LocalFrame>, LocalExecutorError> {
        let snapshot = self.service.snapshot();
        if !self.live_correlations.is_empty()
            || !self.patch_order.is_empty()
            || !self.observation_order.is_empty()
            || snapshot.command_depth != 0
            || snapshot.outstanding_observations != 0
        {
            return single_failure(
                correlation_id,
                SessionFailureCode::ProtocolState,
                frame_config,
            );
        }
        let output = control_frame(
            correlation_id,
            LocalSessionServerKind::Closed(SessionClosed {}),
            frame_config,
        )?;
        let Some(limits) = self.negotiated_limits() else {
            return Err(LocalExecutorError::StateInvariant {
                reason: "close was handled outside the active state",
            });
        };
        self.state = SessionState::Closed {
            limits,
            frame_config: frame_config.clone(),
        };
        Ok(vec![output])
    }

    fn advance(&mut self) -> Result<Vec<LocalFrame>, LocalExecutorError> {
        let frame_config = match &self.state {
            SessionState::Active { frame_config, .. } => frame_config.clone(),
            SessionState::AwaitingHello | SessionState::Closed { .. } => return Ok(Vec::new()),
        };
        let mut output = Vec::with_capacity(MAX_OUTPUT_FRAMES_PER_CALL);
        if let Some(idempotency_key) = self.patch_order.pop_front() {
            let correlation_id = self.patch_correlations.remove(&idempotency_key).ok_or(
                LocalExecutorError::StateInvariant {
                    reason: "ordered patch had no live correlation",
                },
            )?;
            let message = match self.service.process_next() {
                Ok(Some(GatewayResponse::PatchApplied { receipt }))
                    if receipt.idempotency_key == idempotency_key =>
                {
                    if receipt.status == ApplyStatus::Applied {
                        LocalSessionServerKind::PatchCompleted(PatchCompletion { receipt })
                    } else {
                        LocalSessionServerKind::PatchAdmission(PatchAdmission {
                            idempotency_key,
                            status: PatchAdmissionStatus::Replayed { receipt },
                        })
                    }
                }
                Ok(Some(GatewayResponse::PatchApplied { .. })) => {
                    return Err(LocalExecutorError::StateInvariant {
                        reason: "processed patch identity did not match local order",
                    });
                }
                Ok(Some(GatewayResponse::ImaginationProcessed { .. })) => {
                    return Err(LocalExecutorError::StateInvariant {
                        reason: "patch order produced an imagination response",
                    });
                }
                Ok(None) => {
                    return Err(LocalExecutorError::StateInvariant {
                        reason: "service omitted an ordered patch response",
                    });
                }
                Err(code) => LocalSessionServerKind::Failure(SessionFailure { code }),
            };
            output.push(control_or_limit_failure(
                correlation_id,
                message,
                &frame_config,
            )?);
            self.release_correlation(correlation_id)?;
        }

        if !self.observation_order.is_empty() {
            match self.service.poll_observation() {
                Ok(Some(delivery)) => {
                    output.push(self.observation_delivery_frame(delivery, &frame_config)?);
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
                        output.push(control_frame(
                            pending.correlation_id,
                            LocalSessionServerKind::ObservationPending(reference),
                            &frame_config,
                        )?);
                        self.observation_pending_reported.insert(observation_id);
                    }
                }
                Err(code) => {
                    let observation_id = self.observation_order[0];
                    let correlation_id =
                        self.observation_correlations[&observation_id].correlation_id;
                    output.push(failure_frame(correlation_id, code, &frame_config)?);
                    self.release_observation(observation_id, correlation_id)?;
                }
            }
        }
        debug_assert!(output.len() <= MAX_OUTPUT_FRAMES_PER_CALL);
        Ok(output)
    }

    fn observation_delivery_frame(
        &mut self,
        delivery: ServiceObservationDelivery,
        frame_config: &LocalFrameConfig,
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
                if encode_frame(&frame, frame_config).is_ok() {
                    frame
                } else {
                    failure_frame(
                        correlation_id,
                        SessionFailureCode::LimitExceeded,
                        frame_config,
                    )?
                }
            }
            ServiceObservationDelivery::Failed { code, .. } => {
                failure_frame(correlation_id, code, frame_config)?
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
            pending_patches: count(self.patch_order.len()),
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
}

fn patch_admission_frame(
    correlation_id: NonZeroU64,
    idempotency_key: IdempotencyKey,
    status: PatchAdmissionStatus,
    frame_config: &LocalFrameConfig,
) -> Result<LocalFrame, LocalExecutorError> {
    control_frame(
        correlation_id,
        LocalSessionServerKind::PatchAdmission(PatchAdmission {
            idempotency_key,
            status,
        }),
        frame_config,
    )
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

fn control_frame(
    correlation_id: NonZeroU64,
    message: LocalSessionServerKind,
    frame_config: &LocalFrameConfig,
) -> Result<LocalFrame, LocalExecutorError> {
    let frame = server_control_frame(
        correlation_id,
        &LocalSessionServerMessage {
            schema_version: LOCAL_SESSION_SCHEMA_VERSION,
            message,
        },
        frame_config,
    )
    .map_err(|_| LocalExecutorError::OutputRejected)?;
    encode_frame(&frame, frame_config).map_err(|_| LocalExecutorError::OutputRejected)?;
    Ok(frame)
}

fn control_or_limit_failure(
    correlation_id: NonZeroU64,
    message: LocalSessionServerKind,
    frame_config: &LocalFrameConfig,
) -> Result<LocalFrame, LocalExecutorError> {
    match control_frame(correlation_id, message, frame_config) {
        Ok(frame) => Ok(frame),
        Err(LocalExecutorError::OutputRejected) => failure_frame(
            correlation_id,
            SessionFailureCode::LimitExceeded,
            frame_config,
        ),
        Err(error) => Err(error),
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
    fn submit_patch(&mut self, patch: ScenePatch) -> Result<GatewayAdmission, SessionFailureCode>;
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

    fn submit_patch(&mut self, patch: ScenePatch) -> Result<GatewayAdmission, SessionFailureCode> {
        self.submit_patch(patch)
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

    use cogniform_local_session::{
        ClientHello, LocalSessionClientMessage, QueryRequest, RequestObservation, SessionClose,
        SubmitPatch, client_control_frame, decode_server_control_frame,
    };
    use cogniform_protocol::{
        ApplyReceipt, ApplyTiming, ConflictPolicy, DeleteEntity, DeliverySemantic, FrameId,
        ImageDimensions, ObservationKind, ObservationQuality, ObservationStaleness, PatchBudget,
        SceneOperation, SceneRevision, SceneText, SchemaVersion, StableEntityId, TransactionId,
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
        outstanding_observations: Cell<u32>,
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

        fn process_next(&mut self) -> Result<Option<GatewayResponse>, SessionFailureCode> {
            self.command_depth
                .set(self.command_depth.get().saturating_sub(1));
            self.responses.get_mut().pop_front().unwrap_or(Ok(None))
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

    fn hello(core: &mut SessionCore<TestService>) {
        let config = LocalFrameConfig::default();
        let output = core
            .handle_frame(&client_frame(
                1,
                LocalSessionClientKind::Hello(ClientHello {
                    receive_limits: LocalSessionLimits::from_config(&config).unwrap(),
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

    fn server_kind(frame: &LocalFrame, config: &LocalFrameConfig) -> LocalSessionServerKind {
        decode_server_control_frame(frame, config)
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

        hello(&mut core);
        let duplicate = core
            .handle_frame(&client_frame(
                3,
                LocalSessionClientKind::Hello(ClientHello {
                    receive_limits: LocalSessionLimits::from_config(&config).unwrap(),
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
