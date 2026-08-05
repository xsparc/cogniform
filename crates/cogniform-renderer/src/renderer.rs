use std::{
    sync::{Arc, Mutex, TryLockError, mpsc},
    thread,
};

use cogniform_assets::AssetUploadJob;
use cogniform_protocol::{FrameId, RenderExtraction, SceneRevision, StableEntityId};

use crate::{
    AdapterPreference, AdapterSummary, AssetUploadAdmission, AssetUploadOutcome, CapabilityIssue,
    FrameMetadata, MAX_READBACK_CAPACITY, MAX_READBACK_TIMEOUT, MAX_TARGET_DIMENSION,
    MAX_TARGET_PIXELS, PendingFrame, REFERENCE_COLOR, REFERENCE_ENTITY_ID, RenderTargetKind,
    RendererAssetStats, RendererConfig, RendererError, SceneUpdateError, SceneUpdateSummary,
    asset::RendererAssets,
    scene::{PreparedDraw, PreparedGeometry, PreparedScene, RenderScene},
};

const COLOR_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;
const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;
const ENTITY_ID_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::R32Uint;
const NORMAL_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;
const BYTES_PER_PIXEL: u32 = 4;
const COPY_ROW_ALIGNMENT: u32 = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
const CUBE_VERTEX_COUNT: u32 = 36;
const PLANE_VERTEX_COUNT: u32 = 6;
const ASSET_VERTEX_ATTRIBUTES: [wgpu::VertexAttribute; 2] =
    wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3];
const CUBE_POSITIONS: [[f32; 3]; 36] = [
    [-0.5, -0.5, -0.5],
    [0.5, -0.5, -0.5],
    [0.5, 0.5, -0.5],
    [-0.5, -0.5, -0.5],
    [0.5, 0.5, -0.5],
    [-0.5, 0.5, -0.5],
    [0.5, -0.5, 0.5],
    [-0.5, -0.5, 0.5],
    [-0.5, 0.5, 0.5],
    [0.5, -0.5, 0.5],
    [-0.5, 0.5, 0.5],
    [0.5, 0.5, 0.5],
    [-0.5, -0.5, 0.5],
    [-0.5, -0.5, -0.5],
    [-0.5, 0.5, -0.5],
    [-0.5, -0.5, 0.5],
    [-0.5, 0.5, -0.5],
    [-0.5, 0.5, 0.5],
    [0.5, -0.5, -0.5],
    [0.5, -0.5, 0.5],
    [0.5, 0.5, 0.5],
    [0.5, -0.5, -0.5],
    [0.5, 0.5, 0.5],
    [0.5, 0.5, -0.5],
    [-0.5, 0.5, -0.5],
    [0.5, 0.5, -0.5],
    [0.5, 0.5, 0.5],
    [-0.5, 0.5, -0.5],
    [0.5, 0.5, 0.5],
    [-0.5, 0.5, 0.5],
    [-0.5, -0.5, 0.5],
    [0.5, -0.5, 0.5],
    [0.5, -0.5, -0.5],
    [-0.5, -0.5, 0.5],
    [0.5, -0.5, -0.5],
    [-0.5, -0.5, -0.5],
];
const PLANE_POSITIONS: [[f32; 3]; 6] = [
    [-0.5, -0.5, 0.0],
    [0.5, -0.5, 0.0],
    [0.5, 0.5, 0.0],
    [-0.5, -0.5, 0.0],
    [0.5, 0.5, 0.0],
    [-0.5, 0.5, 0.0],
];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ReadbackLayout {
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) unpadded_bytes_per_row: u32,
    pub(crate) padded_bytes_per_row: u32,
    pub(crate) buffer_size: u64,
}

impl ReadbackLayout {
    fn new(width: u32, height: u32) -> Result<Self, RendererError> {
        let unpadded_bytes_per_row =
            width
                .checked_mul(BYTES_PER_PIXEL)
                .ok_or(RendererError::InvalidTarget {
                    width,
                    height,
                    reason: "row byte count overflowed",
                })?;
        let padded_bytes_per_row = align_up(unpadded_bytes_per_row, COPY_ROW_ALIGNMENT).ok_or(
            RendererError::InvalidTarget {
                width,
                height,
                reason: "padded row byte count overflowed",
            },
        )?;
        let buffer_size = u64::from(padded_bytes_per_row)
            .checked_mul(u64::from(height))
            .ok_or(RendererError::InvalidTarget {
                width,
                height,
                reason: "readback buffer size overflowed",
            })?;

        Ok(Self {
            width,
            height,
            unpadded_bytes_per_row,
            padded_bytes_per_row,
            buffer_size,
        })
    }

    pub(crate) fn pixel_count(self) -> usize {
        (self.width as usize) * (self.height as usize)
    }

    pub(crate) fn unpadded_size(self) -> usize {
        (self.unpadded_bytes_per_row as usize) * (self.height as usize)
    }
}

/// Fixed-size headless renderer for the deterministic primitive reference scene.
pub struct HeadlessRenderer {
    config: RendererConfig,
    adapter: AdapterSummary,
    device: wgpu::Device,
    queue: wgpu::Queue,
    pipeline: wgpu::RenderPipeline,
    draw_layout: wgpu::BindGroupLayout,
    cube_vertices: wgpu::Buffer,
    plane_vertices: wgpu::Buffer,
    assets: RendererAssets,
    readback_layout: ReadbackLayout,
    readback_pool: ReadbackPool,
    scene: RenderScene,
    next_frame_id: u64,
    // This field must drop after the renderer's GPU resources.
    gpu_retirement: GpuRetirementGuard,
}

impl std::fmt::Debug for HeadlessRenderer {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("HeadlessRenderer")
            .field("config", &self.config)
            .field("adapter", &self.adapter)
            .field("assets", &self.assets)
            .finish_non_exhaustive()
    }
}

impl HeadlessRenderer {
    /// Negotiates an adapter and device without creating a window or surface.
    pub async fn new(config: RendererConfig) -> Result<Self, RendererError> {
        Self::new_with_next_frame_id(
            config,
            FrameId::new(1).expect("initial frame identity is non-zero"),
        )
        .await
    }

    /// Negotiates a headless renderer with an explicit next frame identity.
    ///
    /// This constructor supports composition roots that restore frame-sequence
    /// causality from a previously captured engine recovery point.
    pub async fn new_with_next_frame_id(
        config: RendererConfig,
        next_frame_id: FrameId,
    ) -> Result<Self, RendererError> {
        validate_config(&config)?;
        let readback_layout = ReadbackLayout::new(config.width, config.height)?;
        let adapter = request_adapter(config.adapter_preference).await?;
        let adapter_summary = AdapterSummary::from_adapter(&adapter);
        let (device, queue) =
            request_device(&adapter, &adapter_summary, &config, readback_layout).await?;
        let gpu_retirement = GpuRetirementGuard::start(device.clone(), queue.clone())?;
        let (pipeline, draw_layout) = create_reference_pipeline(&device).await?;
        let cube_vertices =
            create_builtin_vertex_buffer(&device, "cogniform-cube-vertices", &CUBE_POSITIONS);
        let plane_vertices =
            create_builtin_vertex_buffer(&device, "cogniform-plane-vertices", &PLANE_POSITIONS);
        let readback_pool = ReadbackPool::new(
            &device,
            readback_layout.buffer_size,
            config.readback_capacity,
        );
        let scene = RenderScene::new(config.max_scene_entities);

        Ok(Self {
            config,
            adapter: adapter_summary,
            device,
            queue,
            pipeline,
            draw_layout,
            cube_vertices,
            plane_vertices,
            assets: RendererAssets::new(),
            readback_layout,
            readback_pool,
            scene,
            next_frame_id: next_frame_id.get(),
            gpu_retirement,
        })
    }

    /// Returns the validated configuration used by this renderer.
    #[must_use]
    pub const fn config(&self) -> &RendererConfig {
        &self.config
    }

    /// Returns a backend-neutral summary of the selected adapter.
    #[must_use]
    pub const fn adapter(&self) -> &AdapterSummary {
        &self.adapter
    }

    /// Returns the latest fully consumed extracted scene revision.
    #[must_use]
    pub const fn scene_revision(&self) -> SceneRevision {
        self.scene.revision()
    }

    /// Returns the latest consumed extraction generation.
    #[must_use]
    pub const fn extraction_generation(&self) -> u64 {
        self.scene.generation()
    }

    /// Returns the renderer-owned extracted record count.
    #[must_use]
    pub fn extracted_entity_count(&self) -> usize {
        self.scene.entity_count()
    }

    /// Returns the compact identity assigned to one current stable entity.
    #[must_use]
    pub fn compact_entity_id(&self, entity_id: StableEntityId) -> Option<crate::RenderEntityId> {
        self.scene.compact_id(entity_id)
    }

    /// Returns the frame identity that the next successful submission will use.
    pub fn next_frame_id(&self) -> Result<FrameId, RendererError> {
        FrameId::new(self.next_frame_id).map_err(|_| RendererError::FrameIdOverflow)
    }

    /// Atomically consumes one ordered world extraction packet.
    pub fn apply_extraction(
        &mut self,
        extraction: &RenderExtraction,
    ) -> Result<SceneUpdateSummary, SceneUpdateError> {
        self.scene.apply(extraction)
    }

    /// Admits one immutable CPU mesh while reserving all pending and residency capacity.
    pub fn enqueue_asset_upload(
        &mut self,
        job: AssetUploadJob,
    ) -> Result<AssetUploadAdmission, RendererError> {
        self.assets.enqueue(job, &self.config)
    }

    /// Processes at most one admitted upload job on the renderer domain.
    ///
    /// This method never decodes source assets and is never called implicitly by
    /// frame submission.
    pub fn process_next_asset_upload(&mut self) -> Option<AssetUploadOutcome> {
        self.assets.process_next(&self.device)
    }

    /// Returns aggregate upload and GPU residency occupancy.
    #[must_use]
    pub fn asset_stats(&self) -> RendererAssetStats {
        self.assets.stats()
    }

    /// Encodes and submits the built-in cube reference scene without waiting
    /// for GPU completion or CPU readback.
    pub fn submit_reference_scene(&mut self) -> Result<PendingFrame, RendererError> {
        let camera_id = StableEntityId::new(1).expect("reference camera ID is non-zero");
        let entity_id = StableEntityId::new(u128::from(REFERENCE_ENTITY_ID))
            .expect("reference entity ID is non-zero");
        let prepared = PreparedScene {
            draws: vec![PreparedDraw {
                geometry: PreparedGeometry::Cuboid,
                model: [
                    1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
                ],
                view_projection: [
                    0.70, 0.00, 0.00, 0.00, 0.00, 0.70, 0.00, 0.00, 0.20, 0.15, 0.40, 0.00, 0.00,
                    0.00, 0.50, 1.00,
                ],
                color: REFERENCE_COLOR.map(|channel| f32::from(channel) / 255.0),
                compact_id: REFERENCE_ENTITY_ID,
            }],
            id_lookup: [(REFERENCE_ENTITY_ID, entity_id)].into_iter().collect(),
        };
        self.submit_prepared(camera_id, SceneRevision::INITIAL, 0, prepared)
    }

    /// Submits the current extracted scene without waiting for GPU completion,
    /// CPU readback, observation processing, or downstream consumers.
    pub fn submit_scene(
        &mut self,
        camera_id: StableEntityId,
    ) -> Result<PendingFrame, RendererError> {
        let prepared = self.scene.prepare(
            camera_id,
            self.config.width,
            self.config.height,
            self.config.max_draws_per_frame,
            |key| {
                self.assets
                    .mesh(key)
                    .map(super::asset::GpuAssetMesh::base_color)
            },
        )?;
        self.submit_prepared(
            camera_id,
            self.scene.revision(),
            self.scene.generation(),
            prepared,
        )
    }

    fn submit_prepared(
        &mut self,
        camera_id: StableEntityId,
        scene_revision: SceneRevision,
        extraction_generation: u64,
        prepared: PreparedScene,
    ) -> Result<PendingFrame, RendererError> {
        let readback = self.readback_pool.try_acquire()?;
        let frame_id = self.next_frame_id()?;
        let next_frame_id = self.next_frame_id.checked_add(1).unwrap_or(0);
        let size = wgpu::Extent3d {
            width: self.config.width,
            height: self.config.height,
            depth_or_array_layers: 1,
        };
        let targets = RenderTargets::new(&self.device, size);
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("cogniform-reference-scene-encoder"),
            });
        encode_scene_pass(
            &mut encoder,
            &ScenePassResources {
                device: &self.device,
                queue: &self.queue,
                pipeline: &self.pipeline,
                draw_layout: &self.draw_layout,
                cube_vertices: &self.cube_vertices,
                plane_vertices: &self.plane_vertices,
                assets: &self.assets,
                targets: &targets,
            },
            &prepared.draws,
        );

        copy_target_to_buffer(
            &mut encoder,
            &targets.color,
            wgpu::TextureAspect::All,
            readback.color(),
            self.readback_layout,
            size,
        );
        copy_target_to_buffer(
            &mut encoder,
            &targets.depth,
            wgpu::TextureAspect::DepthOnly,
            readback.depth(),
            self.readback_layout,
            size,
        );
        copy_target_to_buffer(
            &mut encoder,
            &targets.normals,
            wgpu::TextureAspect::All,
            readback.normals(),
            self.readback_layout,
            size,
        );
        copy_target_to_buffer(
            &mut encoder,
            &targets.entity_ids,
            wgpu::TextureAspect::All,
            readback.entity_ids(),
            self.readback_layout,
            size,
        );

        let submission = self.queue.submit([encoder.finish()]);
        self.next_frame_id = next_frame_id;
        Ok(PendingFrame {
            device: self.device.clone(),
            submission,
            readback,
            layout: self.readback_layout,
            adapter: self.adapter.clone(),
            metadata: FrameMetadata {
                frame_id,
                scene_revision,
                camera_id,
                extraction_generation,
            },
            id_lookup: prepared.id_lookup,
            timeout: self.config.readback_timeout,
            _gpu_retirement: self.gpu_retirement.clone(),
        })
    }
}

#[derive(Clone)]
pub(crate) struct GpuRetirementGuard {
    _keep_alive: mpsc::SyncSender<()>,
}

impl GpuRetirementGuard {
    fn start(device: wgpu::Device, queue: wgpu::Queue) -> Result<Self, RendererError> {
        let (keep_alive, disconnected) = mpsc::sync_channel(0);
        thread::Builder::new()
            .name("cogniform-gpu-retirement".to_owned())
            .spawn(move || {
                let _ = disconnected.recv();
                drop(queue);
                drop(device);
            })
            .map_err(|error| RendererError::GpuRetirementWorkerUnavailable {
                reason: error.to_string(),
            })?;
        Ok(Self {
            _keep_alive: keep_alive,
        })
    }
}

async fn request_adapter(preference: AdapterPreference) -> Result<wgpu::Adapter, RendererError> {
    let compiled_backends = wgpu::Instance::enabled_backend_features();
    ensure_backends_available(compiled_backends)?;
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
        backends: compiled_backends,
        ..wgpu::InstanceDescriptor::new_without_display_handle()
    });
    instance
        .request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: match preference {
                AdapterPreference::HighPerformance | AdapterPreference::Fallback => {
                    wgpu::PowerPreference::HighPerformance
                }
                AdapterPreference::LowPower => wgpu::PowerPreference::LowPower,
            },
            force_fallback_adapter: preference == AdapterPreference::Fallback,
            compatible_surface: None,
            apply_limit_buckets: false,
        })
        .await
        .map_err(|error| RendererError::AdapterUnavailable {
            preference,
            reason: error.to_string(),
        })
}

async fn request_device(
    adapter: &wgpu::Adapter,
    adapter_summary: &AdapterSummary,
    config: &RendererConfig,
    readback_layout: ReadbackLayout,
) -> Result<(wgpu::Device, wgpu::Queue), RendererError> {
    let required_limits = required_limits(adapter, config, readback_layout);
    let issues = capability_issues(adapter, &required_limits);
    if !issues.is_empty() {
        return Err(RendererError::UnsupportedCapabilities {
            adapter: adapter_summary.clone(),
            issues,
        });
    }

    adapter
        .request_device(&wgpu::DeviceDescriptor {
            label: Some("cogniform-headless-device"),
            required_features: wgpu::Features::empty(),
            required_limits,
            experimental_features: wgpu::ExperimentalFeatures::disabled(),
            memory_hints: wgpu::MemoryHints::MemoryUsage,
            trace: wgpu::Trace::Off,
        })
        .await
        .map_err(|error| RendererError::DeviceRequestFailed {
            adapter: adapter_summary.clone(),
            reason: error.to_string(),
        })
}

async fn create_reference_pipeline(
    device: &wgpu::Device,
) -> Result<(wgpu::RenderPipeline, wgpu::BindGroupLayout), RendererError> {
    let scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("cogniform-reference-scene-shader"),
        source: wgpu::ShaderSource::Wgsl(include_str!("reference_scene.wgsl").into()),
    });
    let draw_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("cogniform-draw-bind-group-layout"),
        entries: &[wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        }],
    });
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("cogniform-reference-scene-layout"),
        bind_group_layouts: &[Some(&draw_layout)],
        immediate_size: 0,
    });
    let targets = [
        Some(COLOR_FORMAT.into()),
        Some(ENTITY_ID_FORMAT.into()),
        Some(NORMAL_FORMAT.into()),
    ];
    let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("cogniform-reference-scene-pipeline"),
        layout: Some(&layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs_main"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            buffers: &[Some(wgpu::VertexBufferLayout {
                array_stride: 24,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &ASSET_VERTEX_ATTRIBUTES,
            })],
        },
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: Some(wgpu::DepthStencilState {
            format: DEPTH_FORMAT,
            depth_write_enabled: Some(true),
            depth_compare: Some(wgpu::CompareFunction::Less),
            stencil: wgpu::StencilState::default(),
            bias: wgpu::DepthBiasState::default(),
        }),
        multisample: wgpu::MultisampleState::default(),
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs_main"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            targets: &targets,
        }),
        multiview_mask: None,
        cache: None,
    });
    if let Some(error) = scope.pop().await {
        return Err(RendererError::PipelineCreationFailed {
            reason: error.to_string(),
        });
    }
    Ok((pipeline, draw_layout))
}

fn validate_config(config: &RendererConfig) -> Result<(), RendererError> {
    if config.width == 0 || config.height == 0 {
        return Err(RendererError::InvalidTarget {
            width: config.width,
            height: config.height,
            reason: "dimensions must be non-zero",
        });
    }
    if config.width > MAX_TARGET_DIMENSION || config.height > MAX_TARGET_DIMENSION {
        return Err(RendererError::InvalidTarget {
            width: config.width,
            height: config.height,
            reason: "a dimension exceeds MAX_TARGET_DIMENSION",
        });
    }
    let pixel_count = u64::from(config.width)
        .checked_mul(u64::from(config.height))
        .ok_or(RendererError::InvalidTarget {
            width: config.width,
            height: config.height,
            reason: "pixel count overflowed",
        })?;
    if pixel_count > MAX_TARGET_PIXELS {
        return Err(RendererError::InvalidTarget {
            width: config.width,
            height: config.height,
            reason: "pixel count exceeds MAX_TARGET_PIXELS",
        });
    }
    if config.readback_timeout.is_zero() || config.readback_timeout > MAX_READBACK_TIMEOUT {
        return Err(RendererError::InvalidReadbackTimeout);
    }
    if config.readback_capacity.get() > MAX_READBACK_CAPACITY {
        return Err(RendererError::InvalidReadbackCapacity);
    }
    if config.max_asset_mesh_bytes.get() > config.max_pending_asset_upload_bytes.get() {
        return Err(RendererError::InvalidAssetConfig {
            reason: "pending upload bytes must admit at least one maximum-size asset mesh",
        });
    }
    if config.max_asset_mesh_bytes.get() > config.max_resident_asset_bytes.get() {
        return Err(RendererError::InvalidAssetConfig {
            reason: "resident asset bytes must admit at least one maximum-size asset mesh",
        });
    }
    Ok(())
}

fn ensure_backends_available(backends: wgpu::Backends) -> Result<(), RendererError> {
    if backends.is_empty() {
        Err(RendererError::BackendUnavailable)
    } else {
        Ok(())
    }
}

fn required_limits(
    adapter: &wgpu::Adapter,
    config: &RendererConfig,
    layout: ReadbackLayout,
) -> wgpu::Limits {
    let supported = adapter.limits();
    let mut required = wgpu::Limits::downlevel_defaults().or_worse_values_from(&supported);
    required.max_texture_dimension_2d = required
        .max_texture_dimension_2d
        .max(config.width.max(config.height));
    required.max_color_attachments = required.max_color_attachments.max(3);
    required.max_color_attachment_bytes_per_sample =
        required.max_color_attachment_bytes_per_sample.max(12);
    required.max_buffer_size = required.max_buffer_size.max(layout.buffer_size);
    required.max_buffer_size = required
        .max_buffer_size
        .max(config.max_asset_mesh_bytes.get());
    required
}

fn capability_issues(
    adapter: &wgpu::Adapter,
    required_limits: &wgpu::Limits,
) -> Vec<CapabilityIssue> {
    let supported_limits = adapter.limits();
    let mut issues = Vec::new();
    required_limits.check_limits_with_fail_fn(
        &supported_limits,
        false,
        |name, required, supported| {
            issues.push(CapabilityIssue::Limit {
                name,
                required,
                supported,
            });
        },
    );
    check_texture_usage(adapter, COLOR_FORMAT, RenderTargetKind::Color, &mut issues);
    check_texture_usage(adapter, DEPTH_FORMAT, RenderTargetKind::Depth, &mut issues);
    check_texture_usage(
        adapter,
        NORMAL_FORMAT,
        RenderTargetKind::Normal,
        &mut issues,
    );
    check_texture_usage(
        adapter,
        ENTITY_ID_FORMAT,
        RenderTargetKind::EntityId,
        &mut issues,
    );
    issues
}

fn check_texture_usage(
    adapter: &wgpu::Adapter,
    format: wgpu::TextureFormat,
    target: RenderTargetKind,
    issues: &mut Vec<CapabilityIssue>,
) {
    let required = wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC;
    if !adapter
        .get_texture_format_features(format)
        .allowed_usages
        .contains(required)
    {
        issues.push(CapabilityIssue::TextureUsage {
            target,
            required: "RENDER_ATTACHMENT | COPY_SRC",
        });
    }
}

struct RenderTargets {
    color: wgpu::Texture,
    depth: wgpu::Texture,
    entity_ids: wgpu::Texture,
    normals: wgpu::Texture,
    color_view: wgpu::TextureView,
    depth_view: wgpu::TextureView,
    entity_id_view: wgpu::TextureView,
    normal_view: wgpu::TextureView,
}

impl RenderTargets {
    fn new(device: &wgpu::Device, size: wgpu::Extent3d) -> Self {
        let color = create_target_texture(device, "cogniform-color-target", size, COLOR_FORMAT);
        let depth = create_target_texture(device, "cogniform-depth-target", size, DEPTH_FORMAT);
        let entity_ids =
            create_target_texture(device, "cogniform-entity-id-target", size, ENTITY_ID_FORMAT);
        let normals = create_target_texture(device, "cogniform-normal-target", size, NORMAL_FORMAT);
        let color_view = color.create_view(&wgpu::TextureViewDescriptor::default());
        let depth_view = depth.create_view(&wgpu::TextureViewDescriptor::default());
        let entity_id_view = entity_ids.create_view(&wgpu::TextureViewDescriptor::default());
        let normal_view = normals.create_view(&wgpu::TextureViewDescriptor::default());
        Self {
            color,
            depth,
            entity_ids,
            normals,
            color_view,
            depth_view,
            entity_id_view,
            normal_view,
        }
    }
}

pub(crate) struct ReadbackBuffers {
    color: wgpu::Buffer,
    depth: wgpu::Buffer,
    entity_ids: wgpu::Buffer,
    normals: wgpu::Buffer,
}

impl ReadbackBuffers {
    fn new(device: &wgpu::Device, size: u64) -> Self {
        Self {
            color: create_readback_buffer(device, "cogniform-color-readback", size),
            depth: create_readback_buffer(device, "cogniform-depth-readback", size),
            entity_ids: create_readback_buffer(device, "cogniform-entity-id-readback", size),
            normals: create_readback_buffer(device, "cogniform-normal-readback", size),
        }
    }
}

#[derive(Clone)]
struct ReadbackPool {
    inner: Arc<ReadbackPoolInner>,
}

struct ReadbackPoolInner {
    available: Mutex<Vec<ReadbackBuffers>>,
    capacity: u32,
}

impl ReadbackPool {
    fn new(device: &wgpu::Device, size: u64, capacity: core::num::NonZeroU32) -> Self {
        let available = (0..capacity.get())
            .map(|_| ReadbackBuffers::new(device, size))
            .collect();
        Self {
            inner: Arc::new(ReadbackPoolInner {
                available: Mutex::new(available),
                capacity: capacity.get(),
            }),
        }
    }

    fn try_acquire(&self) -> Result<ReadbackLease, RendererError> {
        let mut available = match self.inner.available.try_lock() {
            Ok(available) => available,
            Err(TryLockError::Poisoned(error)) => error.into_inner(),
            Err(TryLockError::WouldBlock) => {
                return Err(RendererError::ReadbackPoolExhausted {
                    capacity: self.inner.capacity,
                });
            }
        };
        let buffers = available
            .pop()
            .ok_or(RendererError::ReadbackPoolExhausted {
                capacity: self.inner.capacity,
            })?;
        drop(available);
        Ok(ReadbackLease {
            pool: self.clone(),
            buffers: Some(buffers),
            mapping_started: false,
        })
    }
}

pub(crate) struct ReadbackLease {
    pool: ReadbackPool,
    buffers: Option<ReadbackBuffers>,
    mapping_started: bool,
}

impl ReadbackLease {
    pub(crate) fn color(&self) -> &wgpu::Buffer {
        &self.buffers.as_ref().expect("live readback lease").color
    }

    pub(crate) fn depth(&self) -> &wgpu::Buffer {
        &self.buffers.as_ref().expect("live readback lease").depth
    }

    pub(crate) fn entity_ids(&self) -> &wgpu::Buffer {
        &self
            .buffers
            .as_ref()
            .expect("live readback lease")
            .entity_ids
    }

    pub(crate) fn normals(&self) -> &wgpu::Buffer {
        &self.buffers.as_ref().expect("live readback lease").normals
    }

    pub(crate) fn begin_mapping(&mut self) {
        debug_assert!(!self.mapping_started);
        self.mapping_started = true;
    }

    pub(crate) fn finish_mapping(&mut self) {
        self.unmap_started();
    }

    fn unmap_started(&mut self) {
        if !self.mapping_started {
            return;
        }
        let buffers = self.buffers.as_ref().expect("live readback lease");
        buffers.color.unmap();
        buffers.depth.unmap();
        buffers.entity_ids.unmap();
        buffers.normals.unmap();
        self.mapping_started = false;
    }
}

impl Drop for ReadbackLease {
    fn drop(&mut self) {
        self.unmap_started();
        let Some(buffers) = self.buffers.take() else {
            return;
        };
        let mut available = self
            .pool
            .inner
            .available
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        available.push(buffers);
    }
}

struct ScenePassResources<'a> {
    device: &'a wgpu::Device,
    queue: &'a wgpu::Queue,
    pipeline: &'a wgpu::RenderPipeline,
    draw_layout: &'a wgpu::BindGroupLayout,
    cube_vertices: &'a wgpu::Buffer,
    plane_vertices: &'a wgpu::Buffer,
    assets: &'a RendererAssets,
    targets: &'a RenderTargets,
}

fn encode_scene_pass(
    encoder: &mut wgpu::CommandEncoder,
    resources: &ScenePassResources<'_>,
    draws: &[PreparedDraw],
) {
    let color_attachments = [
        Some(wgpu::RenderPassColorAttachment {
            view: &resources.targets.color_view,
            depth_slice: None,
            resolve_target: None,
            ops: wgpu::Operations {
                load: wgpu::LoadOp::Clear(wgpu::Color {
                    r: 0.02,
                    g: 0.03,
                    b: 0.05,
                    a: 1.0,
                }),
                store: wgpu::StoreOp::Store,
            },
        }),
        Some(wgpu::RenderPassColorAttachment {
            view: &resources.targets.entity_id_view,
            depth_slice: None,
            resolve_target: None,
            ops: wgpu::Operations {
                load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                store: wgpu::StoreOp::Store,
            },
        }),
        Some(wgpu::RenderPassColorAttachment {
            view: &resources.targets.normal_view,
            depth_slice: None,
            resolve_target: None,
            ops: wgpu::Operations {
                load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                store: wgpu::StoreOp::Store,
            },
        }),
    ];
    let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: Some("cogniform-reference-scene-pass"),
        color_attachments: &color_attachments,
        depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
            view: &resources.targets.depth_view,
            depth_ops: Some(wgpu::Operations {
                load: wgpu::LoadOp::Clear(1.0),
                store: wgpu::StoreOp::Store,
            }),
            stencil_ops: None,
        }),
        timestamp_writes: None,
        occlusion_query_set: None,
        multiview_mask: None,
    });
    render_pass.set_pipeline(resources.pipeline);
    for draw in draws {
        let bytes = encode_draw_uniform(draw);
        let buffer = resources.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("cogniform-draw-uniform"),
            size: u64::try_from(bytes.len()).expect("uniform length fits u64"),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        resources.queue.write_buffer(&buffer, 0, &bytes);
        let bind_group = resources
            .device
            .create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("cogniform-draw-bind-group"),
                layout: resources.draw_layout,
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: buffer.as_entire_binding(),
                }],
            });
        render_pass.set_bind_group(0, &bind_group, &[]);
        let (vertices, vertex_count) = match draw.geometry {
            PreparedGeometry::Cuboid => (resources.cube_vertices, CUBE_VERTEX_COUNT),
            PreparedGeometry::Plane => (resources.plane_vertices, PLANE_VERTEX_COUNT),
            PreparedGeometry::Asset(key) => {
                let mesh = resources
                    .assets
                    .mesh(key)
                    .expect("prepared asset geometry remains resident until renderer drop");
                (mesh.buffer(), mesh.vertex_count())
            }
        };
        render_pass.set_vertex_buffer(0, vertices.slice(..));
        render_pass.draw(0..vertex_count, 0..1);
    }
}

fn create_builtin_vertex_buffer(
    device: &wgpu::Device,
    label: &'static str,
    positions: &[[f32; 3]],
) -> wgpu::Buffer {
    let encoded = encode_winding_vertices(positions);
    let size = u64::try_from(encoded.len()).expect("fixed built-in bytes fit u64");
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some(label),
        size,
        usage: wgpu::BufferUsages::VERTEX,
        mapped_at_creation: true,
    });
    {
        let mut mapped = buffer
            .slice(..)
            .get_mapped_range_mut()
            .expect("newly created mapped buffer is writable");
        mapped.copy_from_slice(&encoded);
    }
    buffer.unmap();
    buffer
}

fn encode_winding_vertices(positions: &[[f32; 3]]) -> Vec<u8> {
    debug_assert!(!positions.is_empty() && positions.len().is_multiple_of(3));
    let mut encoded = Vec::with_capacity(positions.len() * 24);
    for triangle in positions.chunks_exact(3) {
        let edge_a = subtract(triangle[1], triangle[0]);
        let edge_b = subtract(triangle[2], triangle[0]);
        let normal = normalize([
            edge_a[1] * edge_b[2] - edge_a[2] * edge_b[1],
            edge_a[2] * edge_b[0] - edge_a[0] * edge_b[2],
            edge_a[0] * edge_b[1] - edge_a[1] * edge_b[0],
        ]);
        for position in triangle {
            for value in position.iter().chain(&normal) {
                encoded.extend_from_slice(&value.to_le_bytes());
            }
        }
    }
    debug_assert_eq!(encoded.len(), positions.len() * 24);
    encoded
}

fn subtract(left: [f32; 3], right: [f32; 3]) -> [f32; 3] {
    [left[0] - right[0], left[1] - right[1], left[2] - right[2]]
}

fn normalize(vector: [f32; 3]) -> [f32; 3] {
    let inverse_length = vector
        .iter()
        .map(|value| value * value)
        .sum::<f32>()
        .sqrt()
        .recip();
    vector.map(|value| value * inverse_length)
}

fn encode_draw_uniform(draw: &PreparedDraw) -> Vec<u8> {
    const UNIFORM_BYTES: usize = (16 + 16 + 4 + 4) * 4;
    let mut bytes = Vec::with_capacity(UNIFORM_BYTES);
    for value in draw.model {
        bytes.extend_from_slice(&value.to_le_bytes());
    }
    for value in draw.view_projection {
        bytes.extend_from_slice(&value.to_le_bytes());
    }
    for value in draw.color {
        bytes.extend_from_slice(&value.to_le_bytes());
    }
    bytes.extend_from_slice(&draw.compact_id.to_le_bytes());
    bytes.extend_from_slice(&0_u32.to_le_bytes());
    bytes.extend_from_slice(&0_u32.to_le_bytes());
    bytes.extend_from_slice(&0_u32.to_le_bytes());
    debug_assert_eq!(bytes.len(), UNIFORM_BYTES);
    bytes
}

fn create_target_texture(
    device: &wgpu::Device,
    label: &'static str,
    size: wgpu::Extent3d,
    format: wgpu::TextureFormat,
) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    })
}

fn create_readback_buffer(device: &wgpu::Device, label: &'static str, size: u64) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some(label),
        size,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    })
}

fn copy_target_to_buffer(
    encoder: &mut wgpu::CommandEncoder,
    texture: &wgpu::Texture,
    aspect: wgpu::TextureAspect,
    buffer: &wgpu::Buffer,
    layout: ReadbackLayout,
    size: wgpu::Extent3d,
) {
    encoder.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect,
        },
        wgpu::TexelCopyBufferInfo {
            buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(layout.padded_bytes_per_row),
                rows_per_image: Some(layout.height),
            },
        },
        size,
    );
}

const fn align_up(value: u32, alignment: u32) -> Option<u32> {
    let remainder = value % alignment;
    if remainder == 0 {
        Some(value)
    } else {
        value.checked_add(alignment - remainder)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn target_validation_rejects_unbounded_inputs() {
        assert!(matches!(
            validate_config(&RendererConfig::new(0, 64)),
            Err(RendererError::InvalidTarget { .. })
        ));
        assert!(matches!(
            validate_config(&RendererConfig::new(MAX_TARGET_DIMENSION + 1, 1)),
            Err(RendererError::InvalidTarget { .. })
        ));
        assert!(matches!(
            validate_config(&RendererConfig::new(4_096, 4_097)),
            Err(RendererError::InvalidTarget { .. })
        ));
        assert!(matches!(
            validate_config(&RendererConfig::new(4_096, 4_096)),
            Err(RendererError::InvalidTarget { .. })
        ));
        assert!(matches!(
            validate_config(&RendererConfig::new(64, 64).with_readback_timeout(Duration::ZERO)),
            Err(RendererError::InvalidReadbackTimeout)
        ));
        assert!(matches!(
            validate_config(&RendererConfig::new(64, 64).with_readback_capacity(
                core::num::NonZeroU32::new(MAX_READBACK_CAPACITY + 1).unwrap()
            )),
            Err(RendererError::InvalidReadbackCapacity)
        ));
        assert!(matches!(
            validate_config(
                &RendererConfig::new(64, 64)
                    .with_readback_timeout(MAX_READBACK_TIMEOUT + Duration::from_millis(1))
            ),
            Err(RendererError::InvalidReadbackTimeout)
        ));
    }

    #[test]
    fn readback_rows_are_aligned_and_bounded() {
        let layout = ReadbackLayout::new(65, 3).expect("layout should be valid");
        assert_eq!(layout.unpadded_bytes_per_row, 260);
        assert_eq!(layout.padded_bytes_per_row, 512);
        assert_eq!(layout.buffer_size, 1_536);
        assert_eq!(layout.pixel_count(), 195);
        assert_eq!(layout.unpadded_size(), 780);
    }

    #[test]
    fn cube_vertices_interleave_winding_normals() {
        let encoded = encode_winding_vertices(&CUBE_POSITIONS);
        assert_eq!(
            usize::try_from(CUBE_VERTEX_COUNT).unwrap(),
            CUBE_POSITIONS.len()
        );
        assert_eq!(encoded.len(), CUBE_POSITIONS.len() * 24);
        let values = encoded
            .chunks_exact(4)
            .map(|bytes| f32::from_le_bytes(bytes.try_into().unwrap()))
            .collect::<Vec<_>>();
        assert_eq!(&values[..3], &CUBE_POSITIONS[0]);
        assert_eq!(&values[3..6], &[0.0, 0.0, 1.0]);
        assert_eq!(&values[6..9], &CUBE_POSITIONS[1]);
        assert_eq!(&values[9..12], &[0.0, 0.0, 1.0]);
    }

    #[test]
    fn plane_vertices_are_centered_counter_clockwise_and_plus_z() {
        let encoded = encode_winding_vertices(&PLANE_POSITIONS);
        assert_eq!(
            usize::try_from(PLANE_VERTEX_COUNT).unwrap(),
            PLANE_POSITIONS.len()
        );
        assert_eq!(encoded.len(), PLANE_POSITIONS.len() * 24);
        let values = encoded
            .chunks_exact(4)
            .map(|bytes| f32::from_le_bytes(bytes.try_into().unwrap()))
            .collect::<Vec<_>>();
        for (vertex, position) in values.chunks_exact(6).zip(PLANE_POSITIONS) {
            assert_eq!(&vertex[..3], &position);
            assert_eq!(&vertex[3..], &[0.0, 0.0, 1.0]);
        }
    }

    #[test]
    fn unavailable_backend_is_structured() {
        assert!(matches!(
            ensure_backends_available(wgpu::Backends::empty()),
            Err(RendererError::BackendUnavailable)
        ));
    }

    #[test]
    fn normal_target_capability_is_structured() {
        let issue = CapabilityIssue::TextureUsage {
            target: RenderTargetKind::Normal,
            required: "RENDER_ATTACHMENT | COPY_SRC",
        };
        assert_eq!(
            issue.to_string(),
            "normal target requires RENDER_ATTACHMENT | COPY_SRC"
        );
    }
}
