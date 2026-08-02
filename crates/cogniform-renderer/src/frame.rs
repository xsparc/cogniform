use std::{
    collections::BTreeMap,
    fmt,
    sync::mpsc,
    time::{Duration, Instant},
};

use cogniform_protocol::{FrameId, SceneRevision, StableEntityId};

use crate::{
    RendererError,
    renderer::{GpuRetirementGuard, ReadbackLayout, ReadbackLease},
};

/// Public, backend-neutral description of the selected adapter.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AdapterSummary {
    /// Human-readable adapter name reported by the platform.
    pub name: String,
    /// Stable backend label such as `vulkan` or `dx12`.
    pub backend: String,
    /// Stable device class such as `discrete-gpu` or `cpu`.
    pub device_type: String,
    /// Whether the adapter reports WebGPU-compliant downlevel capabilities.
    pub webgpu_compliant: bool,
}

impl AdapterSummary {
    pub(crate) fn from_adapter(adapter: &wgpu::Adapter) -> Self {
        let info = adapter.get_info();
        let device_type = match info.device_type {
            wgpu::DeviceType::Other => "other",
            wgpu::DeviceType::IntegratedGpu => "integrated-gpu",
            wgpu::DeviceType::DiscreteGpu => "discrete-gpu",
            wgpu::DeviceType::VirtualGpu => "virtual-gpu",
            wgpu::DeviceType::Cpu => "cpu",
        };

        Self {
            name: info.name,
            backend: info.backend.to_string(),
            device_type: device_type.to_owned(),
            webgpu_compliant: adapter.get_downlevel_capabilities().is_webgpu_compliant(),
        }
    }
}

/// Exact causal source of one submitted extracted-scene frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FrameMetadata {
    /// Monotonic identity allocated when submission succeeds.
    pub frame_id: FrameId,
    /// Latest fully consumed world revision used by this frame.
    pub scene_revision: SceneRevision,
    /// Stable camera identity used to build the view.
    pub camera_id: StableEntityId,
    /// Monotonic extraction generation consumed by the renderer.
    pub extraction_generation: u64,
}

/// Completed color, depth, and renderer-local entity-ID outputs.
#[derive(Clone, Debug, PartialEq)]
pub struct RenderedFrame {
    width: u32,
    height: u32,
    adapter: AdapterSummary,
    metadata: FrameMetadata,
    color: Vec<[u8; 4]>,
    depth: Vec<f32>,
    entity_ids: Vec<u32>,
    stable_entity_ids: Vec<Option<StableEntityId>>,
}

impl RenderedFrame {
    /// Width of every output in pixels.
    #[must_use]
    pub const fn width(&self) -> u32 {
        self.width
    }

    /// Height of every output in pixels.
    #[must_use]
    pub const fn height(&self) -> u32 {
        self.height
    }

    /// Adapter that produced this frame.
    #[must_use]
    pub const fn adapter(&self) -> &AdapterSummary {
        &self.adapter
    }

    /// Returns exact revision, frame, camera, and extraction causality.
    #[must_use]
    pub const fn metadata(&self) -> FrameMetadata {
        self.metadata
    }

    /// Linear RGBA8 pixels in row-major order.
    #[must_use]
    pub fn color(&self) -> &[[u8; 4]] {
        &self.color
    }

    /// Normalized depth pixels in row-major order.
    #[must_use]
    pub fn depth(&self) -> &[f32] {
        &self.depth
    }

    /// Renderer-local entity IDs in row-major order.
    #[must_use]
    pub fn entity_ids(&self) -> &[u32] {
        &self.entity_ids
    }

    /// Stable world identities corresponding to the entity-ID attachment.
    ///
    /// Background pixels are `None`; compact renderer identities never cross
    /// this boundary without their frame-local stable mapping.
    #[must_use]
    pub fn stable_entity_ids(&self) -> &[Option<StableEntityId>] {
        &self.stable_entity_ids
    }

    /// Returns the color at `(x, y)`, or `None` when outside the frame.
    #[must_use]
    pub fn color_at(&self, x: u32, y: u32) -> Option<[u8; 4]> {
        self.pixel_index(x, y).map(|index| self.color[index])
    }

    /// Returns the depth at `(x, y)`, or `None` when outside the frame.
    #[must_use]
    pub fn depth_at(&self, x: u32, y: u32) -> Option<f32> {
        self.pixel_index(x, y).map(|index| self.depth[index])
    }

    /// Returns the renderer-local entity ID at `(x, y)`, or `None` when outside the frame.
    #[must_use]
    pub fn entity_id_at(&self, x: u32, y: u32) -> Option<u32> {
        self.pixel_index(x, y).map(|index| self.entity_ids[index])
    }

    /// Returns the stable world identity at `(x, y)`, or `None` for background
    /// and coordinates outside the frame.
    #[must_use]
    pub fn stable_entity_id_at(&self, x: u32, y: u32) -> Option<StableEntityId> {
        self.pixel_index(x, y)
            .and_then(|index| self.stable_entity_ids[index])
    }

    /// Consumes the frame and returns its color payload.
    #[must_use]
    pub fn into_color(self) -> Vec<[u8; 4]> {
        self.color
    }

    /// Consumes the frame and returns its depth payload.
    #[must_use]
    pub fn into_depth(self) -> Vec<f32> {
        self.depth
    }

    /// Consumes the frame and returns exact stable entity identities.
    #[must_use]
    pub fn into_stable_entity_ids(self) -> Vec<Option<StableEntityId>> {
        self.stable_entity_ids
    }

    fn pixel_index(&self, x: u32, y: u32) -> Option<usize> {
        if x >= self.width || y >= self.height {
            return None;
        }
        Some((y as usize) * (self.width as usize) + (x as usize))
    }
}

/// Submitted reference frame whose readback is not yet consumed.
///
/// Submission does not wait for the GPU. Calling [`PendingFrame::read`] is the
/// explicit synchronization point used by deterministic tests and diagnostic
/// clients.
pub struct PendingFrame {
    pub(crate) device: wgpu::Device,
    pub(crate) submission: wgpu::SubmissionIndex,
    pub(crate) readback: ReadbackLease,
    pub(crate) layout: ReadbackLayout,
    pub(crate) adapter: AdapterSummary,
    pub(crate) metadata: FrameMetadata,
    pub(crate) id_lookup: BTreeMap<u32, StableEntityId>,
    pub(crate) timeout: Duration,
    // This field must drop after the frame's GPU resources. It keeps final
    // device/queue destruction on the retirement worker when the renderer is
    // dropped before readback.
    pub(crate) _gpu_retirement: GpuRetirementGuard,
}

impl fmt::Debug for PendingFrame {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PendingFrame")
            .field("width", &self.layout.width)
            .field("height", &self.layout.height)
            .field("adapter", &self.adapter)
            .field("metadata", &self.metadata)
            .field("timeout", &self.timeout)
            .finish_non_exhaustive()
    }
}

impl PendingFrame {
    /// Waits for this submission, maps its three output buffers, and returns
    /// tightly packed CPU data.
    pub fn read(mut self) -> Result<RenderedFrame, RendererError> {
        let deadline = Instant::now() + self.timeout;
        let (sender, receiver) = mpsc::sync_channel(3);
        self.readback.begin_mapping();
        schedule_map(self.readback.color(), "color-map", &sender);
        schedule_map(self.readback.depth(), "depth-map", &sender);
        schedule_map(self.readback.entity_ids(), "entity-id-map", &sender);
        drop(sender);

        let poll_timeout = deadline.saturating_duration_since(Instant::now());
        self.device
            .poll(wgpu::PollType::Wait {
                submission_index: Some(self.submission.clone()),
                timeout: Some(poll_timeout),
            })
            .map_err(|error| RendererError::ReadbackFailed {
                stage: "device-poll",
                reason: error.to_string(),
            })?;

        for _ in 0..3 {
            let remaining = deadline.saturating_duration_since(Instant::now());
            let (stage, result) = receiver.recv_timeout(remaining).map_err(|error| {
                RendererError::ReadbackFailed {
                    stage: "mapping-callback",
                    reason: error.to_string(),
                }
            })?;
            result.map_err(|error| RendererError::ReadbackFailed {
                stage,
                reason: error.to_string(),
            })?;
        }

        let color_bytes = read_tightly_packed(self.readback.color(), self.layout, "color-range")?;
        let depth_bytes = read_tightly_packed(self.readback.depth(), self.layout, "depth-range")?;
        let entity_id_bytes =
            read_tightly_packed(self.readback.entity_ids(), self.layout, "entity-id-range")?;

        let color = color_bytes
            .chunks_exact(4)
            .map(|bytes| [bytes[0], bytes[1], bytes[2], bytes[3]])
            .collect();

        let mut depth = Vec::with_capacity(self.layout.pixel_count());
        for (pixel_index, bytes) in depth_bytes.chunks_exact(4).enumerate() {
            let value = f32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
            if !value.is_finite() || !(0.0..=1.0).contains(&value) {
                return Err(RendererError::InvalidDepthOutput { pixel_index });
            }
            depth.push(value);
        }

        let entity_ids = entity_id_bytes
            .chunks_exact(4)
            .map(|bytes| u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
            .collect::<Vec<_>>();
        let stable_entity_ids = entity_ids
            .iter()
            .map(|&render_entity_id| {
                if render_entity_id == 0 {
                    Ok(None)
                } else {
                    self.id_lookup
                        .get(&render_entity_id)
                        .copied()
                        .map(Some)
                        .ok_or(RendererError::UnknownRenderEntityId { render_entity_id })
                }
            })
            .collect::<Result<Vec<_>, _>>()?;
        self.readback.finish_mapping();

        Ok(RenderedFrame {
            width: self.layout.width,
            height: self.layout.height,
            adapter: self.adapter,
            metadata: self.metadata,
            color,
            depth,
            entity_ids,
            stable_entity_ids,
        })
    }
}

fn schedule_map(
    buffer: &wgpu::Buffer,
    stage: &'static str,
    sender: &mpsc::SyncSender<(&'static str, Result<(), wgpu::BufferAsyncError>)>,
) {
    let sender = sender.clone();
    buffer
        .slice(..)
        .map_async(wgpu::MapMode::Read, move |result| {
            let _ = sender.send((stage, result));
        });
}

fn read_tightly_packed(
    buffer: &wgpu::Buffer,
    layout: ReadbackLayout,
    stage: &'static str,
) -> Result<Vec<u8>, RendererError> {
    let mapped =
        buffer
            .slice(..)
            .get_mapped_range()
            .map_err(|error| RendererError::ReadbackFailed {
                stage,
                reason: error.to_string(),
            })?;
    let mut packed = Vec::with_capacity(layout.unpadded_size());
    for row in mapped
        .chunks_exact(layout.padded_bytes_per_row as usize)
        .take(layout.height as usize)
    {
        packed.extend_from_slice(&row[..layout.unpadded_bytes_per_row as usize]);
    }
    drop(mapped);
    Ok(packed)
}
