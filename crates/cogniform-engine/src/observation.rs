use core::num::NonZeroU32;
use std::{
    collections::BTreeMap,
    sync::{
        Arc, Mutex, MutexGuard,
        atomic::{AtomicU32, Ordering},
        mpsc::{self, Receiver, SyncSender, TryRecvError, TrySendError},
    },
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use cogniform_observation::{
    EntityVisibility, ObservationEnvelopeError, ObservationPayload, ObservationPayloadLimits,
    encode_payload,
};
use cogniform_protocol::{
    ImageDimensions, ObservationId, ObservationKind, ObservationMetadata, ObservationQuality,
    ObservationStaleness, RuntimeLimits, SceneRevision, SchemaVersion, StableEntityId,
};
use cogniform_renderer::{PendingFrame, RenderedFrame};

use crate::ObservationError;

/// One bounded request for a machine-readable frame observation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ObservationRequest {
    /// Public identity of the requested result.
    pub observation_id: ObservationId,
    /// Stable extracted camera identity.
    pub camera_id: StableEntityId,
    /// Requested payload kind.
    pub kind: ObservationKind,
    /// Requested quality tier.
    pub quality: ObservationQuality,
}

/// Completed bounded observation with exact source causality.
#[derive(Debug, Clone, PartialEq)]
pub struct Observation {
    metadata: ObservationMetadata,
    payload: ObservationPayload,
}

impl Observation {
    /// Returns the validated public causal envelope.
    #[must_use]
    pub const fn metadata(&self) -> &ObservationMetadata {
        &self.metadata
    }

    /// Returns the owned payload selected by the request.
    #[must_use]
    pub const fn payload(&self) -> &ObservationPayload {
        &self.payload
    }

    /// Explicitly encodes the owned payload without performing I/O.
    pub fn to_payload_envelope(
        &self,
        runtime_limits: &RuntimeLimits,
        payload_limits: ObservationPayloadLimits,
    ) -> Result<Vec<u8>, ObservationEnvelopeError> {
        encode_payload(
            &self.metadata,
            &self.payload,
            runtime_limits,
            payload_limits,
        )
    }
}

/// Fixed-capacity asynchronous readback and observation worker.
///
/// One global permit is held from admission through delivery, so queued jobs,
/// active readback, and completed results cannot collectively exceed capacity.
pub struct ObservationQueue {
    sender: SyncSender<ObservationJob>,
    receiver: Receiver<ObservationCompletion>,
    slots: ObservationSlots,
    limits: RuntimeLimits,
}

impl std::fmt::Debug for ObservationQueue {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let (outstanding, oldest_age_micros) = self.status_at(Instant::now());
        formatter
            .debug_struct("ObservationQueue")
            .field("capacity", &self.slots.capacity())
            .field("outstanding", &outstanding)
            .field("oldest_outstanding_age_micros", &oldest_age_micros)
            .finish_non_exhaustive()
    }
}

impl ObservationQueue {
    /// Starts one worker with a fixed maximum number of outstanding requests.
    pub fn new(capacity: NonZeroU32, limits: RuntimeLimits) -> Result<Self, ObservationError> {
        if capacity.get() > limits.max_queue_capacity.get() {
            return Err(ObservationError::CapacityExceeded {
                capacity: limits.max_queue_capacity.get(),
            });
        }
        let channel_capacity = usize::try_from(capacity.get()).expect("u32 capacity fits usize");
        let (sender, jobs) = mpsc::sync_channel::<ObservationJob>(channel_capacity);
        let (results, receiver) = mpsc::sync_channel::<ObservationCompletion>(channel_capacity);
        thread::Builder::new()
            .name("cogniform-observation".to_owned())
            .spawn(move || observation_worker(&jobs, &results))
            .map_err(|error| ObservationError::WorkerStartFailed {
                reason: error.to_string(),
            })?;
        Ok(Self {
            sender,
            receiver,
            slots: ObservationSlots::new(capacity),
            limits,
        })
    }

    /// Returns the fixed total outstanding-request capacity.
    #[must_use]
    pub fn capacity(&self) -> u32 {
        self.slots.capacity()
    }

    /// Returns the current number of queued, active, or completed requests.
    #[must_use]
    pub fn outstanding(&self) -> u32 {
        self.slots.in_use()
    }

    /// Returns monotonic elapsed microseconds for the oldest outstanding request.
    #[must_use]
    pub fn oldest_outstanding_age_micros(&self) -> Option<u64> {
        self.oldest_outstanding_age_micros_at(Instant::now())
    }

    pub(crate) fn oldest_outstanding_age_micros_at(&self, sampled_at: Instant) -> Option<u64> {
        self.slots.oldest_age_micros_at(sampled_at)
    }

    pub(crate) fn status_at(&self, sampled_at: Instant) -> (u32, Option<u64>) {
        self.slots.status_at(sampled_at)
    }

    /// Admits a submitted frame without waiting for worker or consumer progress.
    pub fn try_submit(
        &self,
        pending: PendingFrame,
        request: ObservationRequest,
    ) -> Result<(), ObservationError> {
        let permit = self.try_reserve()?;
        self.submit_reserved(permit, pending, request)
    }

    pub(crate) fn try_reserve(&self) -> Result<ObservationPermit, ObservationError> {
        self.slots
            .try_acquire()
            .ok_or(ObservationError::CapacityExceeded {
                capacity: self.slots.capacity(),
            })
    }

    pub(crate) fn submit_reserved(
        &self,
        permit: ObservationPermit,
        pending: PendingFrame,
        request: ObservationRequest,
    ) -> Result<(), ObservationError> {
        let job = ObservationJob {
            permit,
            pending,
            request,
            started_at: Instant::now(),
        };
        self.sender.try_send(job).map_err(|error| match error {
            TrySendError::Full(_) => ObservationError::CapacityExceeded {
                capacity: self.slots.capacity(),
            },
            TrySendError::Disconnected(_) => ObservationError::WorkerUnavailable,
        })
    }

    /// Polls one completion without waiting for GPU, worker, or consumer state.
    pub fn try_receive(
        &self,
        latest_known_revision: SceneRevision,
    ) -> Result<Option<Observation>, ObservationError> {
        let completion = match self.receiver.try_recv() {
            Ok(completion) => completion,
            Err(TryRecvError::Empty) => return Ok(None),
            Err(TryRecvError::Disconnected) => return Err(ObservationError::WorkerUnavailable),
        };
        let ObservationCompletion {
            permit: _permit,
            request,
            observed_at_unix_micros,
            production_latency_micros,
            frame,
        } = completion;
        let frame = frame.map_err(ObservationError::Renderer)?;
        let source = frame.metadata();
        if request.camera_id != source.camera_id {
            return Err(ObservationError::CameraMismatch {
                requested: request.camera_id,
                rendered: source.camera_id,
            });
        }
        if latest_known_revision < source.scene_revision {
            return Err(ObservationError::SourceRevisionAhead {
                source: source.scene_revision,
                latest: latest_known_revision,
            });
        }
        let dimensions = (request.kind != ObservationKind::Visibility).then(|| ImageDimensions {
            width: NonZeroU32::new(frame.width()).expect("validated frame width is non-zero"),
            height: NonZeroU32::new(frame.height()).expect("validated frame height is non-zero"),
        });
        let payload = payload_for(request.kind, frame);
        let metadata = ObservationMetadata {
            schema_version: SchemaVersion::V1,
            observation_id: request.observation_id,
            scene_revision: source.scene_revision,
            frame_id: source.frame_id,
            camera_id: source.camera_id,
            kind: request.kind,
            dimensions,
            quality: request.quality,
            observed_at_unix_micros,
            production_latency_micros,
            staleness: ObservationStaleness {
                latest_known_revision,
                revisions_behind: latest_known_revision.get() - source.scene_revision.get(),
            },
        };
        metadata
            .validate_with_limits(&self.limits)
            .map_err(ObservationError::InvalidMetadata)?;
        Ok(Some(Observation { metadata, payload }))
    }
}

fn payload_for(kind: ObservationKind, frame: RenderedFrame) -> ObservationPayload {
    match kind {
        ObservationKind::Color => ObservationPayload::Color(frame.into_color()),
        ObservationKind::Depth => ObservationPayload::Depth(frame.into_depth()),
        ObservationKind::Normal => ObservationPayload::Normal(frame.into_normals()),
        ObservationKind::EntityId => ObservationPayload::EntityId(frame.into_stable_entity_ids()),
        ObservationKind::Visibility => {
            let mut counts = BTreeMap::<StableEntityId, u64>::new();
            for &entity_id in frame.stable_entity_ids().iter().flatten() {
                *counts.entry(entity_id).or_default() += 1;
            }
            ObservationPayload::Visibility(
                counts
                    .into_iter()
                    .map(|(entity_id, visible_pixels)| EntityVisibility {
                        entity_id,
                        visible_pixels,
                    })
                    .collect(),
            )
        }
    }
}

struct ObservationJob {
    permit: ObservationPermit,
    pending: PendingFrame,
    request: ObservationRequest,
    started_at: Instant,
}

struct ObservationCompletion {
    permit: ObservationPermit,
    request: ObservationRequest,
    observed_at_unix_micros: u64,
    production_latency_micros: u64,
    frame: Result<RenderedFrame, cogniform_renderer::RendererError>,
}

fn observation_worker(
    jobs: &Receiver<ObservationJob>,
    results: &SyncSender<ObservationCompletion>,
) {
    while let Ok(job) = jobs.recv() {
        let frame = job.pending.read();
        let production_latency_micros =
            u64::try_from(job.started_at.elapsed().as_micros()).unwrap_or(u64::MAX);
        let observed_at_unix_micros = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |duration| {
                u64::try_from(duration.as_micros()).unwrap_or(u64::MAX)
            });
        let completion = ObservationCompletion {
            permit: job.permit,
            request: job.request,
            observed_at_unix_micros,
            production_latency_micros,
            frame,
        };
        if results.send(completion).is_err() {
            break;
        }
    }
}

#[derive(Clone)]
struct ObservationSlots {
    inner: Arc<ObservationSlotState>,
}

struct ObservationSlotState {
    capacity: u32,
    in_use: AtomicU32,
    pending: Mutex<Vec<Arc<ObservationAdmission>>>,
}

struct ObservationAdmission {
    reserved_at: Instant,
}

impl ObservationSlots {
    fn new(capacity: NonZeroU32) -> Self {
        Self {
            inner: Arc::new(ObservationSlotState {
                capacity: capacity.get(),
                in_use: AtomicU32::new(0),
                pending: Mutex::new(Vec::with_capacity(
                    usize::try_from(capacity.get()).expect("u32 capacity fits usize"),
                )),
            }),
        }
    }

    fn capacity(&self) -> u32 {
        self.inner.capacity
    }

    fn in_use(&self) -> u32 {
        let pending = self.pending();
        let in_use = self.inner.in_use.load(Ordering::Acquire);
        debug_assert_eq!(
            usize::try_from(in_use).expect("u32 count fits usize"),
            pending.len()
        );
        in_use
    }

    fn try_acquire(&self) -> Option<ObservationPermit> {
        let mut pending = self.pending();
        let current = self.inner.in_use.load(Ordering::Acquire);
        if current >= self.capacity() {
            return None;
        }
        Some(self.acquire(&mut pending, current, Instant::now()))
    }

    #[cfg(test)]
    fn try_acquire_at(&self, reserved_at: Instant) -> Option<ObservationPermit> {
        let mut pending = self.pending();
        let current = self.inner.in_use.load(Ordering::Acquire);
        if current >= self.capacity() {
            return None;
        }
        Some(self.acquire(&mut pending, current, reserved_at))
    }

    fn acquire(
        &self,
        pending: &mut Vec<Arc<ObservationAdmission>>,
        current: u32,
        reserved_at: Instant,
    ) -> ObservationPermit {
        let admission = Arc::new(ObservationAdmission { reserved_at });
        pending.push(Arc::clone(&admission));
        let previous = self.inner.in_use.fetch_add(1, Ordering::AcqRel);
        debug_assert_eq!(previous, current);
        ObservationPermit {
            slots: self.clone(),
            admission,
        }
    }

    fn oldest_age_micros_at(&self, sampled_at: Instant) -> Option<u64> {
        self.status_at(sampled_at).1
    }

    fn status_at(&self, sampled_at: Instant) -> (u32, Option<u64>) {
        let pending = self.pending();
        let in_use = self.inner.in_use.load(Ordering::Acquire);
        debug_assert_eq!(
            usize::try_from(in_use).expect("u32 count fits usize"),
            pending.len()
        );
        let oldest_age_micros = pending
            .iter()
            .map(|admission| admission.reserved_at)
            .min()
            .map(|reserved_at| elapsed_micros(sampled_at, reserved_at));
        (in_use, oldest_age_micros)
    }

    fn pending(&self) -> MutexGuard<'_, Vec<Arc<ObservationAdmission>>> {
        self.inner
            .pending
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

pub(crate) struct ObservationPermit {
    slots: ObservationSlots,
    admission: Arc<ObservationAdmission>,
}

impl Drop for ObservationPermit {
    fn drop(&mut self) {
        let mut pending = self.slots.pending();
        let position = pending
            .iter()
            .position(|admission| Arc::ptr_eq(admission, &self.admission))
            .expect("outstanding observation permit retains its age record");
        pending.swap_remove(position);
        let previous = self.slots.inner.in_use.fetch_sub(1, Ordering::AcqRel);
        debug_assert!(previous > 0);
    }
}

fn elapsed_micros(sampled_at: Instant, started_at: Instant) -> u64 {
    duration_micros(sampled_at.saturating_duration_since(started_at))
}

fn duration_micros(duration: Duration) -> u64 {
    u64::try_from(duration.as_micros()).unwrap_or(u64::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn global_slots_enforce_capacity_until_delivery_is_released() {
        let slots = ObservationSlots::new(NonZeroU32::new(2).unwrap());
        let first = slots.try_acquire().unwrap();
        let second = slots.try_acquire().unwrap();
        assert_eq!(slots.in_use(), 2);
        assert!(slots.try_acquire().is_none());
        drop(first);
        assert_eq!(slots.in_use(), 1);
        let replacement = slots.try_acquire().unwrap();
        assert_eq!(slots.in_use(), 2);
        drop((second, replacement));
        assert_eq!(slots.in_use(), 0);
    }

    #[test]
    fn observation_age_spans_each_live_permit_and_releases_exactly() {
        let started_at = Instant::now();
        let slots = ObservationSlots::new(NonZeroU32::new(2).unwrap());
        assert_eq!(slots.oldest_age_micros_at(started_at), None);
        let first = slots.try_acquire_at(started_at).unwrap();
        let second = slots
            .try_acquire_at(started_at + Duration::from_micros(4))
            .unwrap();
        assert!(
            slots
                .try_acquire_at(started_at + Duration::from_micros(6))
                .is_none()
        );
        let sampled_at = started_at + Duration::from_micros(11);
        assert_eq!(slots.oldest_age_micros_at(sampled_at), Some(11));
        drop(first);
        assert_eq!(slots.oldest_age_micros_at(sampled_at), Some(7));
        drop(second);
        assert_eq!(slots.oldest_age_micros_at(sampled_at), None);
        assert_eq!(duration_micros(Duration::MAX), u64::MAX);
    }

    #[test]
    fn renderer_error_delivery_releases_observation_age_and_capacity() {
        let started_at = Instant::now();
        let slots = ObservationSlots::new(NonZeroU32::new(1).unwrap());
        let permit = slots.try_acquire_at(started_at).unwrap();
        let (sender, _jobs) = mpsc::sync_channel::<ObservationJob>(1);
        let (results, receiver) = mpsc::sync_channel(1);
        let queue = ObservationQueue {
            sender,
            receiver,
            slots: slots.clone(),
            limits: RuntimeLimits::default(),
        };
        results
            .send(ObservationCompletion {
                permit,
                request: ObservationRequest {
                    observation_id: ObservationId::new(1).unwrap(),
                    camera_id: StableEntityId::new(1).unwrap(),
                    kind: ObservationKind::Color,
                    quality: ObservationQuality::Low,
                },
                observed_at_unix_micros: 0,
                production_latency_micros: 0,
                frame: Err(cogniform_renderer::RendererError::ReadbackFailed {
                    stage: "test",
                    reason: "injected failure".to_owned(),
                }),
            })
            .unwrap();
        assert!(slots.oldest_age_micros_at(started_at).is_some());

        assert!(matches!(
            queue.try_receive(SceneRevision::INITIAL),
            Err(ObservationError::Renderer(
                cogniform_renderer::RendererError::ReadbackFailed { .. }
            ))
        ));
        assert_eq!(slots.status_at(started_at), (0, None));
    }

    #[test]
    fn visibility_counts_are_sorted_and_ignore_background() {
        let first = StableEntityId::new(1).unwrap();
        let second = StableEntityId::new(2).unwrap();
        let mut counts = BTreeMap::<StableEntityId, u64>::new();
        for entity_id in [Some(second), None, Some(first), Some(second)]
            .into_iter()
            .flatten()
        {
            *counts.entry(entity_id).or_default() += 1;
        }
        let visibility = counts.into_iter().collect::<Vec<_>>();
        assert_eq!(visibility, vec![(first, 1), (second, 2)]);
    }

    #[test]
    fn completed_observation_explicitly_encodes_its_bound_payload() {
        let metadata = ObservationMetadata {
            schema_version: SchemaVersion::V1,
            observation_id: ObservationId::new(1).unwrap(),
            scene_revision: SceneRevision::new(2),
            frame_id: cogniform_protocol::FrameId::new(3).unwrap(),
            camera_id: StableEntityId::new(4).unwrap(),
            kind: ObservationKind::Color,
            dimensions: Some(ImageDimensions {
                width: NonZeroU32::new(1).unwrap(),
                height: NonZeroU32::new(1).unwrap(),
            }),
            quality: ObservationQuality::Low,
            observed_at_unix_micros: 5,
            production_latency_micros: 6,
            staleness: ObservationStaleness {
                latest_known_revision: SceneRevision::new(2),
                revisions_behind: 0,
            },
        };
        let observation = Observation {
            metadata,
            payload: ObservationPayload::Color(vec![[1, 2, 3, 4]]),
        };
        let runtime_limits = RuntimeLimits::default();
        let payload_limits = ObservationPayloadLimits::default();
        let encoded = observation
            .to_payload_envelope(&runtime_limits, payload_limits)
            .unwrap();

        assert_eq!(
            cogniform_observation::decode_payload(
                observation.metadata(),
                &encoded,
                &runtime_limits,
                payload_limits,
            )
            .unwrap(),
            observation.payload
        );
    }
}
