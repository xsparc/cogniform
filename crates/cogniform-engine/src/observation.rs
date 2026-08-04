use core::num::NonZeroU32;
use std::{
    collections::BTreeMap,
    sync::{
        Arc,
        atomic::{AtomicU32, Ordering},
        mpsc::{self, Receiver, SyncSender, TryRecvError, TrySendError},
    },
    thread,
    time::{Instant, SystemTime, UNIX_EPOCH},
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

/// Stable visibility summary for one entity in one frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EntityVisibility {
    /// Stable world identity.
    pub entity_id: StableEntityId,
    /// Exact number of pixels carrying this identity.
    pub visible_pixels: u64,
}

/// Owned bulk data associated with one causal observation envelope.
#[derive(Debug, Clone, PartialEq)]
pub enum ObservationPayload {
    /// Linear RGBA8 pixels in row-major order.
    Color(Vec<[u8; 4]>),
    /// Normalized f32 depth pixels in row-major order.
    Depth(Vec<f32>),
    /// Flat world-space unit normals; background pixels are `None`.
    Normal(Vec<Option<[f32; 3]>>),
    /// Exact stable identity per pixel; background is `None`.
    EntityId(Vec<Option<StableEntityId>>),
    /// Stable-identity visibility counts sorted by identity.
    Visibility(Vec<EntityVisibility>),
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
        formatter
            .debug_struct("ObservationQueue")
            .field("capacity", &self.slots.capacity())
            .field("outstanding", &self.slots.in_use())
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
}

impl ObservationSlots {
    fn new(capacity: NonZeroU32) -> Self {
        Self {
            inner: Arc::new(ObservationSlotState {
                capacity: capacity.get(),
                in_use: AtomicU32::new(0),
            }),
        }
    }

    fn capacity(&self) -> u32 {
        self.inner.capacity
    }

    fn in_use(&self) -> u32 {
        self.inner.in_use.load(Ordering::Acquire)
    }

    fn try_acquire(&self) -> Option<ObservationPermit> {
        let mut current = self.in_use();
        loop {
            if current >= self.capacity() {
                return None;
            }
            match self.inner.in_use.compare_exchange_weak(
                current,
                current + 1,
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                Ok(_) => {
                    return Some(ObservationPermit {
                        slots: self.clone(),
                    });
                }
                Err(observed) => current = observed,
            }
        }
    }
}

pub(crate) struct ObservationPermit {
    slots: ObservationSlots,
}

impl Drop for ObservationPermit {
    fn drop(&mut self) {
        let previous = self.slots.inner.in_use.fetch_sub(1, Ordering::AcqRel);
        debug_assert!(previous > 0);
    }
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
}
