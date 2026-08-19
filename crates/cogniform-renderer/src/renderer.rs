use std::{
    sync::{Arc, Mutex, TryLockError, mpsc},
    thread,
};

use cogniform_assets::{
    AssetMaterial, AssetSampler, AssetSamplerFilter, AssetSamplerWrap, AssetUploadJob,
};
use cogniform_protocol::{ContentHash, FrameId, RenderExtraction, SceneRevision, StableEntityId};

use crate::{
    AdapterPreference, AdapterSummary, AssetUploadAdmission, AssetUploadOutcome, CapabilityIssue,
    FrameMetadata, MAX_READBACK_CAPACITY, MAX_READBACK_TIMEOUT, MAX_TARGET_DIMENSION,
    MAX_TARGET_PIXELS, PendingFrame, REFERENCE_COLOR, REFERENCE_ENTITY_ID, RenderTargetKind,
    RendererAssetStats, RendererConfig, RendererError, SceneUpdateError, SceneUpdateSummary,
    asset::{AssetTextureRole, GpuAssetMesh, RendererAssets},
    scene::{
        ImportedAlphaCoverage, ImportedFacePolicy, ImportedShadingModel, ImportedTextureRoles,
        MAX_DIRECTIONAL_LIGHTS, MAX_POINT_LIGHTS, PreparedDirectionalLight, PreparedDraw,
        PreparedGeometry, PreparedPointLight, PreparedScene, RenderScene,
    },
};

const COLOR_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;
const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;
const ENTITY_ID_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::R32Uint;
const NORMAL_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;
const ASSET_BASE_COLOR_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8UnormSrgb;
const ASSET_NORMAL_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;
const ASSET_SAMPLER_COUNT: usize = 36;
type AssetSamplerTable = [wgpu::Sampler; ASSET_SAMPLER_COUNT];
const BYTES_PER_PIXEL: u32 = 4;
const COPY_ROW_ALIGNMENT: u32 = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
// The shared ABI constant is fixed at 48 and fits every supported pointer width.
#[allow(clippy::cast_possible_truncation)]
const VERTEX_BYTES: usize = cogniform_assets::ASSET_VERTEX_BYTES as usize;
const CUBE_VERTEX_COUNT: u32 = 36;
const PLANE_VERTEX_COUNT: u32 = 6;
const SPHERE_LONGITUDE_SECTORS: u16 = 16;
const SPHERE_LATITUDE_BANDS: u16 = 8;
const SPHERE_VERTEX_COUNT: u32 = 672;
const SPHERE_RADIUS: f32 = 0.5;
const ASSET_VERTEX_ATTRIBUTES: [wgpu::VertexAttribute; 4] =
    wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3, 2 => Float32x2, 3 => Float32x4];
const CUBE_POSITIONS: [[f32; 3]; 36] = [
    [-0.5, -0.5, -0.5],
    [0.5, 0.5, -0.5],
    [0.5, -0.5, -0.5],
    [-0.5, -0.5, -0.5],
    [-0.5, 0.5, -0.5],
    [0.5, 0.5, -0.5],
    [0.5, -0.5, 0.5],
    [-0.5, 0.5, 0.5],
    [-0.5, -0.5, 0.5],
    [0.5, -0.5, 0.5],
    [0.5, 0.5, 0.5],
    [-0.5, 0.5, 0.5],
    [-0.5, -0.5, 0.5],
    [-0.5, 0.5, -0.5],
    [-0.5, -0.5, -0.5],
    [-0.5, -0.5, 0.5],
    [-0.5, 0.5, 0.5],
    [-0.5, 0.5, -0.5],
    [0.5, -0.5, -0.5],
    [0.5, 0.5, 0.5],
    [0.5, -0.5, 0.5],
    [0.5, -0.5, -0.5],
    [0.5, 0.5, -0.5],
    [0.5, 0.5, 0.5],
    [-0.5, 0.5, -0.5],
    [0.5, 0.5, 0.5],
    [0.5, 0.5, -0.5],
    [-0.5, 0.5, -0.5],
    [-0.5, 0.5, 0.5],
    [0.5, 0.5, 0.5],
    [-0.5, -0.5, 0.5],
    [0.5, -0.5, -0.5],
    [0.5, -0.5, 0.5],
    [-0.5, -0.5, 0.5],
    [-0.5, -0.5, -0.5],
    [0.5, -0.5, -0.5],
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
    unculled_pipeline: wgpu::RenderPipeline,
    back_cull_pipeline: wgpu::RenderPipeline,
    draw_layout: wgpu::BindGroupLayout,
    _white_base_color_texture: wgpu::Texture,
    white_base_color_view: wgpu::TextureView,
    _neutral_normal_texture: wgpu::Texture,
    neutral_normal_view: wgpu::TextureView,
    _neutral_metallic_roughness_texture: wgpu::Texture,
    neutral_metallic_roughness_view: wgpu::TextureView,
    asset_samplers: AssetSamplerTable,
    cube_vertices: wgpu::Buffer,
    plane_vertices: wgpu::Buffer,
    sphere_vertices: wgpu::Buffer,
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
        let (unculled_pipeline, back_cull_pipeline, draw_layout) =
            create_reference_pipelines(&device).await?;
        let (
            white_base_color_texture,
            white_base_color_view,
            neutral_normal_texture,
            neutral_normal_view,
            neutral_metallic_roughness_texture,
            neutral_metallic_roughness_view,
            asset_samplers,
        ) = create_asset_texture_resources(&device, &queue);
        let cube_vertices =
            create_builtin_vertex_buffer(&device, "cogniform-cube-vertices", &CUBE_POSITIONS);
        let plane_vertices =
            create_builtin_vertex_buffer(&device, "cogniform-plane-vertices", &PLANE_POSITIONS);
        let sphere_vertices = create_sphere_vertex_buffer(&device);
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
            unculled_pipeline,
            back_cull_pipeline,
            draw_layout,
            _white_base_color_texture: white_base_color_texture,
            white_base_color_view,
            _neutral_normal_texture: neutral_normal_texture,
            neutral_normal_view,
            _neutral_metallic_roughness_texture: neutral_metallic_roughness_texture,
            neutral_metallic_roughness_view,
            asset_samplers,
            cube_vertices,
            plane_vertices,
            sphere_vertices,
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
        self.assets.process_next(&self.device, &self.queue)
    }

    /// Explicitly releases all queued and resident renderer state for one source.
    ///
    /// Unrelated upload jobs preserve FIFO order. GPU backends may keep dropped
    /// resources alive until already-submitted work no longer references them.
    pub fn evict_asset(&mut self, content_hash: ContentHash) -> crate::RendererAssetEviction {
        self.assets.evict(content_hash)
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
                camera_position: [0.0, 0.0, 3.0],
                metallic: 0.0,
                roughness: 0.8,
                emissive: [0.0; 3],
                normal_scale: 1.0,
                imported_texture_roles: ImportedTextureRoles::NONE,
                imported_alpha_coverage: ImportedAlphaCoverage::Disabled,
                imported_face_policy: ImportedFacePolicy::Disabled,
                imported_shading_model: ImportedShadingModel::MetallicRoughness,
                compact_id: REFERENCE_ENTITY_ID,
            }],
            directional_lights: Vec::new(),
            point_lights: Vec::new(),
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
                    .map(super::asset::GpuAssetMesh::material)
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
                unculled_pipeline: &self.unculled_pipeline,
                back_cull_pipeline: &self.back_cull_pipeline,
                draw_layout: &self.draw_layout,
                white_base_color_view: &self.white_base_color_view,
                neutral_normal_view: &self.neutral_normal_view,
                neutral_metallic_roughness_view: &self.neutral_metallic_roughness_view,
                asset_samplers: &self.asset_samplers,
                cube_vertices: &self.cube_vertices,
                plane_vertices: &self.plane_vertices,
                sphere_vertices: &self.sphere_vertices,
                assets: &self.assets,
                targets: &targets,
            },
            &prepared.draws,
            &prepared.directional_lights,
            &prepared.point_lights,
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

async fn create_reference_pipelines(
    device: &wgpu::Device,
) -> Result<
    (
        wgpu::RenderPipeline,
        wgpu::RenderPipeline,
        wgpu::BindGroupLayout,
    ),
    RendererError,
> {
    let scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("cogniform-reference-scene-shader"),
        source: wgpu::ShaderSource::Wgsl(include_str!("reference_scene.wgsl").into()),
    });
    let draw_layout = create_draw_bind_group_layout(device);
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
    let create_pipeline = |label, cull_mode| {
        device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some(label),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: cogniform_assets::ASSET_VERTEX_BYTES,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &ASSET_VERTEX_ATTRIBUTES,
                })],
            },
            primitive: wgpu::PrimitiveState {
                cull_mode,
                ..wgpu::PrimitiveState::default()
            },
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
        })
    };
    let unculled_pipeline = create_pipeline("cogniform-reference-scene-unculled", None);
    let back_cull_pipeline = create_pipeline(
        "cogniform-reference-scene-back-cull",
        Some(wgpu::Face::Back),
    );
    if let Some(error) = scope.pop().await {
        return Err(RendererError::PipelineCreationFailed {
            reason: error.to_string(),
        });
    }
    Ok((unculled_pipeline, back_cull_pipeline, draw_layout))
}

fn create_draw_bind_group_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("cogniform-draw-bind-group-layout"),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 3,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 2,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 4,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 5,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 6,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 7,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 8,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            },
        ],
    })
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
    if config.max_asset_texture_bytes.get() > config.max_pending_asset_texture_bytes.get() {
        return Err(RendererError::InvalidAssetConfig {
            reason: "pending texture bytes must admit at least one maximum-size texture",
        });
    }
    if config.max_asset_texture_bytes.get() > config.max_resident_asset_texture_bytes.get() {
        return Err(RendererError::InvalidAssetConfig {
            reason: "resident texture bytes must admit at least one maximum-size texture",
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
    required.max_texture_dimension_2d = required.max_texture_dimension_2d.max(
        config
            .width
            .max(config.height)
            .max(config.max_asset_texture_dimension_2d.get()),
    );
    required.max_color_attachments = required.max_color_attachments.max(3);
    required.max_color_attachment_bytes_per_sample =
        required.max_color_attachment_bytes_per_sample.max(12);
    required.max_bindings_per_bind_group = required.max_bindings_per_bind_group.max(9);
    required.max_sampled_textures_per_shader_stage =
        required.max_sampled_textures_per_shader_stage.max(4);
    required.max_samplers_per_shader_stage = required.max_samplers_per_shader_stage.max(4);
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
    check_sampled_texture_usage(adapter, &mut issues);
    issues
}

fn check_sampled_texture_usage(adapter: &wgpu::Adapter, issues: &mut Vec<CapabilityIssue>) {
    let required = wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST;
    for (format, target) in [
        (ASSET_BASE_COLOR_FORMAT, RenderTargetKind::AssetBaseColor),
        (ASSET_NORMAL_FORMAT, RenderTargetKind::AssetNormal),
    ] {
        let features = adapter.get_texture_format_features(format);
        if !features.allowed_usages.contains(required)
            || !features
                .flags
                .contains(wgpu::TextureFormatFeatureFlags::FILTERABLE)
        {
            issues.push(CapabilityIssue::TextureUsage {
                target,
                required: "TEXTURE_BINDING | COPY_DST with linear filtering",
            });
        }
    }
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
    unculled_pipeline: &'a wgpu::RenderPipeline,
    back_cull_pipeline: &'a wgpu::RenderPipeline,
    draw_layout: &'a wgpu::BindGroupLayout,
    white_base_color_view: &'a wgpu::TextureView,
    neutral_normal_view: &'a wgpu::TextureView,
    neutral_metallic_roughness_view: &'a wgpu::TextureView,
    asset_samplers: &'a AssetSamplerTable,
    cube_vertices: &'a wgpu::Buffer,
    plane_vertices: &'a wgpu::Buffer,
    sphere_vertices: &'a wgpu::Buffer,
    assets: &'a RendererAssets,
    targets: &'a RenderTargets,
}

fn encode_scene_pass(
    encoder: &mut wgpu::CommandEncoder,
    resources: &ScenePassResources<'_>,
    draws: &[PreparedDraw],
    directional_lights: &[PreparedDirectionalLight],
    point_lights: &[PreparedPointLight],
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
    for draw in draws {
        let pipeline = if draw.imported_face_policy.culls_back_faces() {
            resources.back_cull_pipeline
        } else {
            resources.unculled_pipeline
        };
        render_pass.set_pipeline(pipeline);
        let (
            vertices,
            vertex_count,
            base_color_view,
            normal_view,
            metallic_roughness_view,
            emissive_view,
        ) = draw_resources(draw, resources);
        let bytes = encode_draw_uniform(draw, directional_lights, point_lights);
        let buffer = resources.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("cogniform-draw-uniform"),
            size: u64::try_from(bytes.len()).expect("uniform length fits u64"),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        resources.queue.write_buffer(&buffer, 0, &bytes);
        let bind_group = create_draw_bind_group(
            resources,
            draw,
            &buffer,
            base_color_view,
            normal_view,
            metallic_roughness_view,
            emissive_view,
        );
        render_pass.set_bind_group(0, &bind_group, &[]);
        render_pass.set_vertex_buffer(0, vertices.slice(..));
        render_pass.draw(0..vertex_count, 0..1);
    }
}

fn create_draw_bind_group(
    resources: &ScenePassResources<'_>,
    draw: &PreparedDraw,
    buffer: &wgpu::Buffer,
    base_color_view: &wgpu::TextureView,
    normal_view: &wgpu::TextureView,
    metallic_roughness_view: &wgpu::TextureView,
    emissive_view: &wgpu::TextureView,
) -> wgpu::BindGroup {
    let [
        base_color_sampler,
        normal_sampler,
        metallic_roughness_sampler,
        emissive_sampler,
    ] = draw_samplers(draw, resources);
    resources
        .device
        .create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("cogniform-draw-bind-group"),
            layout: resources.draw_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(base_color_view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(base_color_sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(normal_view),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::TextureView(metallic_roughness_view),
                },
                wgpu::BindGroupEntry {
                    binding: 5,
                    resource: wgpu::BindingResource::TextureView(emissive_view),
                },
                wgpu::BindGroupEntry {
                    binding: 6,
                    resource: wgpu::BindingResource::Sampler(normal_sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 7,
                    resource: wgpu::BindingResource::Sampler(metallic_roughness_sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 8,
                    resource: wgpu::BindingResource::Sampler(emissive_sampler),
                },
            ],
        })
}

fn draw_samplers<'a>(
    draw: &PreparedDraw,
    resources: &'a ScenePassResources<'_>,
) -> [&'a wgpu::Sampler; 4] {
    let material = match draw.geometry {
        PreparedGeometry::Asset(key) => resources.assets.mesh(key).map(GpuAssetMesh::material),
        PreparedGeometry::Cuboid | PreparedGeometry::Plane | PreparedGeometry::Sphere => None,
    };
    let policy = |enabled: bool, sampler: Option<AssetSampler>| {
        let sampler = if enabled {
            sampler.unwrap_or_default()
        } else {
            AssetSampler::LINEAR_REPEAT
        };
        &resources.asset_samplers[asset_sampler_index(sampler)]
    };
    [
        policy(
            draw.imported_texture_roles.base_color(),
            material.and_then(AssetMaterial::base_color_sampler),
        ),
        policy(
            draw.imported_texture_roles.normal(),
            material.and_then(AssetMaterial::normal_sampler),
        ),
        policy(
            draw.imported_texture_roles.metallic_roughness(),
            material.and_then(AssetMaterial::metallic_roughness_sampler),
        ),
        policy(
            draw.imported_texture_roles.emissive(),
            material.and_then(AssetMaterial::emissive_sampler),
        ),
    ]
}

fn draw_resources<'a>(
    draw: &PreparedDraw,
    resources: &'a ScenePassResources<'_>,
) -> (
    &'a wgpu::Buffer,
    u32,
    &'a wgpu::TextureView,
    &'a wgpu::TextureView,
    &'a wgpu::TextureView,
    &'a wgpu::TextureView,
) {
    match draw.geometry {
        PreparedGeometry::Cuboid => (
            resources.cube_vertices,
            CUBE_VERTEX_COUNT,
            resources.white_base_color_view,
            resources.neutral_normal_view,
            resources.neutral_metallic_roughness_view,
            resources.white_base_color_view,
        ),
        PreparedGeometry::Plane => (
            resources.plane_vertices,
            PLANE_VERTEX_COUNT,
            resources.white_base_color_view,
            resources.neutral_normal_view,
            resources.neutral_metallic_roughness_view,
            resources.white_base_color_view,
        ),
        PreparedGeometry::Sphere => (
            resources.sphere_vertices,
            SPHERE_VERTEX_COUNT,
            resources.white_base_color_view,
            resources.neutral_normal_view,
            resources.neutral_metallic_roughness_view,
            resources.white_base_color_view,
        ),
        PreparedGeometry::Asset(key) => {
            let mesh = resources
                .assets
                .mesh(key)
                .expect("prepared asset geometry remains resident until renderer drop");
            let base_color_view = if draw.imported_texture_roles.base_color() {
                resources
                    .assets
                    .texture_view(key.content_hash, AssetTextureRole::BaseColor)
                    .expect("textured resident mesh retains its shared GPU texture")
            } else {
                resources.white_base_color_view
            };
            let normal_view = if draw.imported_texture_roles.normal() {
                resources
                    .assets
                    .texture_view(key.content_hash, AssetTextureRole::Normal)
                    .expect("normal-textured resident mesh retains its shared GPU texture")
            } else {
                resources.neutral_normal_view
            };
            let metallic_roughness_view = if draw.imported_texture_roles.metallic_roughness() {
                resources
                    .assets
                    .texture_view(key.content_hash, AssetTextureRole::MetallicRoughness)
                    .expect(
                        "metallic-roughness-textured resident mesh retains its shared GPU texture",
                    )
            } else {
                resources.neutral_metallic_roughness_view
            };
            let emissive_view = if draw.imported_texture_roles.emissive() {
                resources
                    .assets
                    .texture_view(key.content_hash, AssetTextureRole::Emissive)
                    .expect("emissive-textured resident mesh retains its shared GPU texture")
            } else {
                resources.white_base_color_view
            };
            (
                mesh.buffer(),
                mesh.vertex_count(),
                base_color_view,
                normal_view,
                metallic_roughness_view,
                emissive_view,
            )
        }
    }
}

fn create_asset_texture_resources(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
) -> (
    wgpu::Texture,
    wgpu::TextureView,
    wgpu::Texture,
    wgpu::TextureView,
    wgpu::Texture,
    wgpu::TextureView,
    AssetSamplerTable,
) {
    let (base_color_texture, base_color_view) = create_solid_asset_texture(
        device,
        queue,
        "cogniform-white-base-color",
        ASSET_BASE_COLOR_FORMAT,
        [255; 4],
    );
    let (normal_texture, normal_view) = create_solid_asset_texture(
        device,
        queue,
        "cogniform-neutral-normal",
        ASSET_NORMAL_FORMAT,
        [128, 128, 255, 255],
    );
    let (metallic_roughness_texture, metallic_roughness_view) = create_solid_asset_texture(
        device,
        queue,
        "cogniform-neutral-metallic-roughness",
        ASSET_NORMAL_FORMAT,
        [255; 4],
    );
    let samplers = create_asset_sampler_table(device);
    (
        base_color_texture,
        base_color_view,
        normal_texture,
        normal_view,
        metallic_roughness_texture,
        metallic_roughness_view,
        samplers,
    )
}

fn create_asset_sampler_table(device: &wgpu::Device) -> AssetSamplerTable {
    std::array::from_fn(|index| {
        let (mag_filter, min_filter, wrap_s, wrap_t) = asset_sampler_from_index(index);
        device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("cogniform-asset-sampler"),
            address_mode_u: sampler_address_mode(wrap_s),
            address_mode_v: sampler_address_mode(wrap_t),
            address_mode_w: wgpu::AddressMode::Repeat,
            mag_filter: sampler_filter_mode(mag_filter),
            min_filter: sampler_filter_mode(min_filter),
            mipmap_filter: wgpu::MipmapFilterMode::Nearest,
            ..wgpu::SamplerDescriptor::default()
        })
    })
}

fn asset_sampler_index(sampler: AssetSampler) -> usize {
    effective_asset_sampler_index(
        sampler.mag_filter(),
        sampler.effective_min_filter(),
        sampler.wrap_s(),
        sampler.wrap_t(),
    )
}

fn effective_asset_sampler_index(
    mag_filter: AssetSamplerFilter,
    min_filter: AssetSamplerFilter,
    wrap_s: AssetSamplerWrap,
    wrap_t: AssetSamplerWrap,
) -> usize {
    let wrap_s = sampler_wrap_index(wrap_s);
    let wrap_t = sampler_wrap_index(wrap_t);
    let mag = sampler_filter_index(mag_filter);
    let min = sampler_filter_index(min_filter);
    (((wrap_s * 3) + wrap_t) * 2 + mag) * 2 + min
}

fn asset_sampler_from_index(
    index: usize,
) -> (
    AssetSamplerFilter,
    AssetSamplerFilter,
    AssetSamplerWrap,
    AssetSamplerWrap,
) {
    debug_assert!(index < ASSET_SAMPLER_COUNT);
    let min = sampler_filter_from_index(index % 2);
    let mag = sampler_filter_from_index((index / 2) % 2);
    let wrap_t = sampler_wrap_from_index((index / 4) % 3);
    let wrap_s = sampler_wrap_from_index((index / 12) % 3);
    (mag, min, wrap_s, wrap_t)
}

fn sampler_filter_index(filter: AssetSamplerFilter) -> usize {
    match filter {
        AssetSamplerFilter::Nearest => 0,
        AssetSamplerFilter::Linear => 1,
        _ => unreachable!("assets exposes only bounded core sampler filters"),
    }
}

fn sampler_filter_from_index(index: usize) -> AssetSamplerFilter {
    match index {
        0 => AssetSamplerFilter::Nearest,
        1 => AssetSamplerFilter::Linear,
        _ => unreachable!("sampler filter index is modulo two"),
    }
}

fn sampler_wrap_index(wrap: AssetSamplerWrap) -> usize {
    match wrap {
        AssetSamplerWrap::ClampToEdge => 0,
        AssetSamplerWrap::MirroredRepeat => 1,
        AssetSamplerWrap::Repeat => 2,
        _ => unreachable!("assets exposes only bounded core sampler wrapping"),
    }
}

fn sampler_wrap_from_index(index: usize) -> AssetSamplerWrap {
    match index {
        0 => AssetSamplerWrap::ClampToEdge,
        1 => AssetSamplerWrap::MirroredRepeat,
        2 => AssetSamplerWrap::Repeat,
        _ => unreachable!("sampler wrap index is modulo three"),
    }
}

fn sampler_filter_mode(filter: AssetSamplerFilter) -> wgpu::FilterMode {
    match filter {
        AssetSamplerFilter::Nearest => wgpu::FilterMode::Nearest,
        AssetSamplerFilter::Linear => wgpu::FilterMode::Linear,
        _ => unreachable!("assets exposes only bounded core sampler filters"),
    }
}

fn sampler_address_mode(wrap: AssetSamplerWrap) -> wgpu::AddressMode {
    match wrap {
        AssetSamplerWrap::ClampToEdge => wgpu::AddressMode::ClampToEdge,
        AssetSamplerWrap::MirroredRepeat => wgpu::AddressMode::MirrorRepeat,
        AssetSamplerWrap::Repeat => wgpu::AddressMode::Repeat,
        _ => unreachable!("assets exposes only bounded core sampler wrapping"),
    }
}

fn create_solid_asset_texture(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    label: &'static str,
    format: wgpu::TextureFormat,
    texel: [u8; 4],
) -> (wgpu::Texture, wgpu::TextureView) {
    let size = wgpu::Extent3d {
        width: 1,
        height: 1,
        depth_or_array_layers: 1,
    };
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture: &texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        &texel,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(4),
            rows_per_image: Some(1),
        },
        size,
    );
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    (texture, view)
}

fn create_builtin_vertex_buffer(
    device: &wgpu::Device,
    label: &'static str,
    positions: &[[f32; 3]],
) -> wgpu::Buffer {
    let encoded = encode_winding_vertices(positions);
    create_vertex_buffer(device, label, &encoded)
}

fn create_sphere_vertex_buffer(device: &wgpu::Device) -> wgpu::Buffer {
    let encoded = encode_sphere_vertices();
    create_vertex_buffer(device, "cogniform-sphere-vertices", &encoded)
}

fn create_vertex_buffer(
    device: &wgpu::Device,
    label: &'static str,
    encoded: &[u8],
) -> wgpu::Buffer {
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
        mapped.copy_from_slice(encoded);
    }
    buffer.unmap();
    buffer
}

fn encode_winding_vertices(positions: &[[f32; 3]]) -> Vec<u8> {
    debug_assert!(!positions.is_empty() && positions.len().is_multiple_of(3));
    let mut encoded = Vec::with_capacity(positions.len() * VERTEX_BYTES);
    for triangle in positions.chunks_exact(3) {
        let edge_a = subtract(triangle[1], triangle[0]);
        let edge_b = subtract(triangle[2], triangle[0]);
        let normal = normalize([
            edge_a[1] * edge_b[2] - edge_a[2] * edge_b[1],
            edge_a[2] * edge_b[0] - edge_a[0] * edge_b[2],
            edge_a[0] * edge_b[1] - edge_a[1] * edge_b[0],
        ]);
        for position in triangle {
            encode_vertex(&mut encoded, *position, normal);
        }
    }
    debug_assert_eq!(encoded.len(), positions.len() * VERTEX_BYTES);
    encoded
}

fn encode_sphere_vertices() -> Vec<u8> {
    let expected_vertices = usize::try_from(SPHERE_VERTEX_COUNT).expect("fixed count fits usize");
    let mut encoded = Vec::with_capacity(expected_vertices * VERTEX_BYTES);
    let bottom = [0.0, 0.0, -1.0];
    let top = [0.0, 0.0, 1.0];

    for longitude in 0..SPHERE_LONGITUDE_SECTORS {
        let current = sphere_direction(1, longitude);
        let next = sphere_direction(1, longitude + 1);
        encode_sphere_triangle(&mut encoded, [bottom, next, current]);
    }

    for lower_band in 1..(SPHERE_LATITUDE_BANDS - 1) {
        for longitude in 0..SPHERE_LONGITUDE_SECTORS {
            let lower_current = sphere_direction(lower_band, longitude);
            let lower_next = sphere_direction(lower_band, longitude + 1);
            let upper_current = sphere_direction(lower_band + 1, longitude);
            let upper_next = sphere_direction(lower_band + 1, longitude + 1);
            encode_sphere_triangle(&mut encoded, [lower_current, lower_next, upper_current]);
            encode_sphere_triangle(&mut encoded, [lower_next, upper_next, upper_current]);
        }
    }

    let top_band = SPHERE_LATITUDE_BANDS - 1;
    for longitude in 0..SPHERE_LONGITUDE_SECTORS {
        let current = sphere_direction(top_band, longitude);
        let next = sphere_direction(top_band, longitude + 1);
        encode_sphere_triangle(&mut encoded, [current, next, top]);
    }

    debug_assert_eq!(encoded.len(), expected_vertices * VERTEX_BYTES);
    encoded
}

fn sphere_direction(latitude_band: u16, longitude_sector: u16) -> [f32; 3] {
    debug_assert!(latitude_band > 0 && latitude_band < SPHERE_LATITUDE_BANDS);
    let latitude = -core::f32::consts::FRAC_PI_2
        + core::f32::consts::PI * f32::from(latitude_band) / f32::from(SPHERE_LATITUDE_BANDS);
    let wrapped_longitude = longitude_sector % SPHERE_LONGITUDE_SECTORS;
    let longitude =
        core::f32::consts::TAU * f32::from(wrapped_longitude) / f32::from(SPHERE_LONGITUDE_SECTORS);
    let (latitude_sine, latitude_cosine) = latitude.sin_cos();
    let (longitude_sine, longitude_cosine) = longitude.sin_cos();
    [
        latitude_cosine * longitude_cosine,
        latitude_cosine * longitude_sine,
        latitude_sine,
    ]
}

fn encode_sphere_triangle(encoded: &mut Vec<u8>, normals: [[f32; 3]; 3]) {
    for normal in normals {
        encode_vertex(encoded, normal.map(|value| value * SPHERE_RADIUS), normal);
    }
}

fn encode_vertex(encoded: &mut Vec<u8>, position: [f32; 3], normal: [f32; 3]) {
    for value in position
        .iter()
        .chain(&normal)
        .chain(&[0.0, 0.0])
        .chain(&[1.0, 0.0, 0.0, 1.0])
    {
        encoded.extend_from_slice(&value.to_le_bytes());
    }
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

fn encode_draw_uniform(
    draw: &PreparedDraw,
    directional_lights: &[PreparedDirectionalLight],
    point_lights: &[PreparedPointLight],
) -> Vec<u8> {
    const BASE_FLOATS: usize = 16 + 16 + 4 + 4 + 4;
    const FLOATS_PER_DIRECTIONAL_LIGHT: usize = 8;
    const POINT_COUNT_FLOATS: usize = 4;
    const FLOATS_PER_POINT_LIGHT: usize = 8;
    const MATERIAL_VIEW_EMISSIVE_FLOATS: usize = 12;
    const UNIFORM_BYTES: usize = (BASE_FLOATS
        + MAX_DIRECTIONAL_LIGHTS * FLOATS_PER_DIRECTIONAL_LIGHT
        + POINT_COUNT_FLOATS
        + MAX_POINT_LIGHTS * FLOATS_PER_POINT_LIGHT
        + MATERIAL_VIEW_EMISSIVE_FLOATS)
        * 4;
    debug_assert!(directional_lights.len() <= MAX_DIRECTIONAL_LIGHTS);
    debug_assert!(point_lights.len() <= MAX_POINT_LIGHTS);
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
    bytes.extend_from_slice(
        &u32::try_from(directional_lights.len())
            .expect("fixed directional-light count fits u32")
            .to_le_bytes(),
    );
    bytes.extend_from_slice(&0_u32.to_le_bytes());
    bytes.extend_from_slice(&0_u32.to_le_bytes());
    bytes.extend_from_slice(&0_u32.to_le_bytes());
    for index in 0..MAX_DIRECTIONAL_LIGHTS {
        let light = directional_lights
            .get(index)
            .copied()
            .unwrap_or(PreparedDirectionalLight {
                surface_to_light: [0.0; 3],
                color: [0.0; 3],
                intensity: 0.0,
            });
        for value in light.surface_to_light {
            bytes.extend_from_slice(&value.to_le_bytes());
        }
        bytes.extend_from_slice(&0.0_f32.to_le_bytes());
        for value in light.color {
            bytes.extend_from_slice(&value.to_le_bytes());
        }
        bytes.extend_from_slice(&light.intensity.to_le_bytes());
    }
    bytes.extend_from_slice(
        &u32::try_from(point_lights.len())
            .expect("fixed point-light count fits u32")
            .to_le_bytes(),
    );
    bytes.extend_from_slice(&0_u32.to_le_bytes());
    bytes.extend_from_slice(&0_u32.to_le_bytes());
    bytes.extend_from_slice(&0_u32.to_le_bytes());
    for index in 0..MAX_POINT_LIGHTS {
        let light = point_lights
            .get(index)
            .copied()
            .unwrap_or(PreparedPointLight {
                position: [0.0; 3],
                color: [0.0; 3],
                intensity: 0.0,
            });
        for value in light.position {
            bytes.extend_from_slice(&value.to_le_bytes());
        }
        bytes.extend_from_slice(&0.0_f32.to_le_bytes());
        for value in light.color {
            bytes.extend_from_slice(&value.to_le_bytes());
        }
        bytes.extend_from_slice(&light.intensity.to_le_bytes());
    }
    for value in draw.camera_position {
        bytes.extend_from_slice(&value.to_le_bytes());
    }
    bytes.extend_from_slice(&0.0_f32.to_le_bytes());
    bytes.extend_from_slice(&draw.metallic.to_le_bytes());
    bytes.extend_from_slice(&draw.roughness.to_le_bytes());
    bytes.extend_from_slice(&draw.normal_scale.to_le_bytes());
    let normal_flag = u8::from(draw.imported_texture_roles.normal());
    let material_flags = normal_flag
        | draw.imported_alpha_coverage.flags()
        | draw.imported_face_policy.flags()
        | draw.imported_shading_model.flags();
    bytes.extend_from_slice(&f32::from(material_flags).to_le_bytes());
    for value in draw.emissive {
        bytes.extend_from_slice(&value.to_le_bytes());
    }
    bytes.extend_from_slice(&draw.imported_alpha_coverage.cutoff().to_le_bytes());
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
    fn fixed_asset_sampler_table_is_complete_unique_and_reversible() {
        let mut keys = Vec::new();
        for index in 0..ASSET_SAMPLER_COUNT {
            let key = asset_sampler_from_index(index);
            assert!(
                !keys.contains(&key),
                "duplicate sampler key at index {index}"
            );
            keys.push(key);
            assert_eq!(
                effective_asset_sampler_index(key.0, key.1, key.2, key.3),
                index
            );
        }
        assert_eq!(keys.len(), 36);
        assert_eq!(ASSET_SAMPLER_COUNT, 36);
        assert_eq!(asset_sampler_index(AssetSampler::LINEAR_REPEAT), 35);
    }

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
        assert!(matches!(
            validate_config(
                &RendererConfig::new(64, 64)
                    .with_max_pending_asset_texture_bytes(core::num::NonZeroU64::new(1).unwrap())
            ),
            Err(RendererError::InvalidAssetConfig { .. })
        ));
        assert!(matches!(
            validate_config(
                &RendererConfig::new(64, 64)
                    .with_max_resident_asset_texture_bytes(core::num::NonZeroU64::new(1).unwrap())
            ),
            Err(RendererError::InvalidAssetConfig { .. })
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
    fn cube_vertices_are_centered_fixed_layout_and_outward() {
        let encoded = encode_winding_vertices(&CUBE_POSITIONS);
        assert_eq!(
            usize::try_from(CUBE_VERTEX_COUNT).unwrap(),
            CUBE_POSITIONS.len()
        );
        assert_eq!(CUBE_POSITIONS.len() / 3, 12);
        assert_eq!(encoded.len(), 1_728);
        assert_eq!(encoded.len(), CUBE_POSITIONS.len() * VERTEX_BYTES);
        let values = encoded
            .chunks_exact(4)
            .map(|bytes| f32::from_le_bytes(bytes.try_into().unwrap()))
            .collect::<Vec<_>>();
        let vertices = values
            .chunks_exact(12)
            .map(|vertex| <[f32; 12]>::try_from(vertex).unwrap())
            .collect::<Vec<_>>();
        let mut face_triangle_counts = [0_u8; 6];

        for vertex in &vertices {
            assert!(
                vertex[..3]
                    .iter()
                    .all(|value| value.to_bits() & 0x7fff_ffff == 0.5_f32.to_bits())
            );
            assert_eq!(&vertex[6..], &[0.0, 0.0, 1.0, 0.0, 0.0, 1.0]);
        }

        for triangle in vertices.chunks_exact(3) {
            let first = [triangle[0][0], triangle[0][1], triangle[0][2]];
            let second = [triangle[1][0], triangle[1][1], triangle[1][2]];
            let third = [triangle[2][0], triangle[2][1], triangle[2][2]];
            let normal = [triangle[0][3], triangle[0][4], triangle[0][5]];
            assert_eq!(&triangle[1][3..6], normal.as_slice());
            assert_eq!(&triangle[2][3..6], normal.as_slice());

            let edge_a = subtract(second, first);
            let edge_b = subtract(third, first);
            let winding_normal = [
                edge_a[1] * edge_b[2] - edge_a[2] * edge_b[1],
                edge_a[2] * edge_b[0] - edge_a[0] * edge_b[2],
                edge_a[0] * edge_b[1] - edge_a[1] * edge_b[0],
            ];
            let winding_length_squared = winding_normal
                .iter()
                .map(|value| value * value)
                .sum::<f32>();
            assert!(
                winding_length_squared > 0.0,
                "cuboid triangle is degenerate"
            );
            let canonical_bits = |value: f32| {
                let bits = value.to_bits();
                if bits.trailing_zeros() >= 31 { 0 } else { bits }
            };
            let normal_bits = normal.map(canonical_bits);
            assert_eq!(normalize(winding_normal).map(canonical_bits), normal_bits);

            let centroid = [
                (first[0] + second[0] + third[0]) / 3.0,
                (first[1] + second[1] + third[1]) / 3.0,
                (first[2] + second[2] + third[2]) / 3.0,
            ];
            let outward = winding_normal
                .iter()
                .zip(centroid)
                .map(|(direction, center)| direction * center)
                .sum::<f32>();
            assert!(outward > 0.0, "cuboid triangle winding must face outward");

            let negative_one = (-1.0_f32).to_bits();
            let positive_one = 1.0_f32.to_bits();
            let negative_half = (-0.5_f32).to_bits();
            let positive_half = 0.5_f32.to_bits();
            let (face_index, axis, coordinate_bits) = if normal_bits == [negative_one, 0, 0] {
                (0, 0, negative_half)
            } else if normal_bits == [positive_one, 0, 0] {
                (1, 0, positive_half)
            } else if normal_bits == [0, negative_one, 0] {
                (2, 1, negative_half)
            } else if normal_bits == [0, positive_one, 0] {
                (3, 1, positive_half)
            } else if normal_bits == [0, 0, negative_one] {
                (4, 2, negative_half)
            } else if normal_bits == [0, 0, positive_one] {
                (5, 2, positive_half)
            } else {
                panic!("cuboid normal is not axis-aligned: {normal:?}");
            };
            assert!(
                triangle
                    .iter()
                    .all(|vertex| vertex[axis].to_bits() == coordinate_bits)
            );
            face_triangle_counts[face_index] += 1;
        }

        assert_eq!(face_triangle_counts, [2; 6]);
    }

    #[test]
    fn plane_vertices_are_centered_counter_clockwise_and_plus_z() {
        let encoded = encode_winding_vertices(&PLANE_POSITIONS);
        assert_eq!(
            usize::try_from(PLANE_VERTEX_COUNT).unwrap(),
            PLANE_POSITIONS.len()
        );
        assert_eq!(encoded.len(), 288);
        assert_eq!(encoded.len(), PLANE_POSITIONS.len() * VERTEX_BYTES);
        let values = encoded
            .chunks_exact(4)
            .map(|bytes| f32::from_le_bytes(bytes.try_into().unwrap()))
            .collect::<Vec<_>>();
        for (vertex, position) in values.chunks_exact(12).zip(PLANE_POSITIONS) {
            assert_eq!(&vertex[..3], &position);
            assert_eq!(&vertex[3..6], &[0.0, 0.0, 1.0]);
            assert_eq!(&vertex[6..], &[0.0, 0.0, 1.0, 0.0, 0.0, 1.0]);
        }
    }

    #[test]
    fn draw_uniform_has_exact_fixed_light_layout_and_zero_padding() {
        let draw = PreparedDraw {
            geometry: PreparedGeometry::Plane,
            model: [1.0; 16],
            view_projection: [2.0; 16],
            color: [0.25, 0.5, 0.75, 1.0],
            camera_position: [6.0, 7.0, 8.0],
            metallic: 0.9,
            roughness: 0.2,
            emissive: [0.1, 0.3, 0.7],
            normal_scale: 1.0,
            imported_texture_roles: ImportedTextureRoles::NONE,
            imported_alpha_coverage: ImportedAlphaCoverage::Disabled,
            imported_face_policy: ImportedFacePolicy::Disabled,
            imported_shading_model: ImportedShadingModel::MetallicRoughness,
            compact_id: 42,
        };
        let lights = [
            PreparedDirectionalLight {
                surface_to_light: [0.0, 0.0, 1.0],
                color: [1.0, 0.5, 0.25],
                intensity: 0.75,
            },
            PreparedDirectionalLight {
                surface_to_light: [0.6, 0.8, 0.0],
                color: [0.1, 0.2, 0.3],
                intensity: 0.4,
            },
        ];
        let point_lights = [PreparedPointLight {
            position: [3.0, 4.0, 5.0],
            color: [0.9, 0.8, 0.7],
            intensity: 0.6,
        }];

        let bytes = encode_draw_uniform(&draw, &lights, &point_lights);
        assert_eq!(bytes.len(), 496);
        let words = bytes
            .chunks_exact(4)
            .map(|word| <[u8; 4]>::try_from(word).unwrap())
            .collect::<Vec<_>>();
        let float = |index: usize| f32::from_le_bytes(words[index]);
        let unsigned = |index: usize| u32::from_le_bytes(words[index]);

        assert_eq!(float(0).to_bits(), 1.0_f32.to_bits());
        assert_eq!(float(16).to_bits(), 2.0_f32.to_bits());
        assert_eq!(
            (32..36).map(float).collect::<Vec<_>>(),
            vec![0.25, 0.5, 0.75, 1.0]
        );
        assert_eq!(unsigned(36), 42);
        assert_eq!((37..40).map(unsigned).collect::<Vec<_>>(), vec![0; 3]);
        assert_eq!(unsigned(40), 2);
        assert_eq!((41..44).map(unsigned).collect::<Vec<_>>(), vec![0; 3]);
        assert_eq!(
            (44..52).map(float).collect::<Vec<_>>(),
            vec![0.0, 0.0, 1.0, 0.0, 1.0, 0.5, 0.25, 0.75]
        );
        assert_eq!(
            (52..60).map(float).collect::<Vec<_>>(),
            vec![0.6, 0.8, 0.0, 0.0, 0.1, 0.2, 0.3, 0.4]
        );
        assert!(words[60..76].iter().all(|word| *word == [0; 4]));
        assert_eq!(unsigned(76), 1);
        assert_eq!((77..80).map(unsigned).collect::<Vec<_>>(), vec![0; 3]);
        assert_eq!(
            (80..88).map(float).collect::<Vec<_>>(),
            vec![3.0, 4.0, 5.0, 0.0, 0.9, 0.8, 0.7, 0.6]
        );
        assert!(words[88..112].iter().all(|word| *word == [0; 4]));
        assert_eq!(
            (112..116).map(float).collect::<Vec<_>>(),
            vec![6.0, 7.0, 8.0, 0.0]
        );
        assert_eq!(
            (116..120).map(float).collect::<Vec<_>>(),
            vec![0.9, 0.2, 1.0, 0.0]
        );
        assert_eq!(
            (120..124).map(float).collect::<Vec<_>>(),
            vec![0.1, 0.3, 0.7, 0.0]
        );
    }

    #[test]
    fn draw_uniform_retains_mask_flags_and_cutoff_without_layout_growth() {
        let draw = PreparedDraw {
            geometry: PreparedGeometry::Plane,
            model: [1.0; 16],
            view_projection: [1.0; 16],
            color: [1.0; 4],
            camera_position: [0.0; 3],
            metallic: 0.0,
            roughness: 1.0,
            emissive: [0.0; 3],
            normal_scale: 1.0,
            imported_texture_roles: ImportedTextureRoles::NORMAL_ONLY,
            imported_alpha_coverage: ImportedAlphaCoverage::Mask { cutoff: 1.25 },
            imported_face_policy: ImportedFacePolicy::DoubleSided,
            imported_shading_model: ImportedShadingModel::Unlit,
            compact_id: 1,
        };

        let bytes = encode_draw_uniform(&draw, &[], &[]);
        assert_eq!(bytes.len(), 496);
        let float_at =
            |index: usize| f32::from_le_bytes(bytes[index * 4..index * 4 + 4].try_into().unwrap());
        assert_eq!(float_at(119).to_bits(), 31.0_f32.to_bits());
        assert_eq!(float_at(123).to_bits(), 1.25_f32.to_bits());
    }

    #[test]
    fn sphere_vertices_are_fixed_unit_diameter_smooth_and_outward() {
        assert_eq!(SPHERE_LONGITUDE_SECTORS, 16);
        assert_eq!(SPHERE_LATITUDE_BANDS, 8);
        assert_eq!(SPHERE_VERTEX_COUNT, 672);
        assert_eq!(SPHERE_RADIUS.to_bits(), 0.5_f32.to_bits());
        let encoded = encode_sphere_vertices();
        let expected_vertices = usize::try_from(SPHERE_VERTEX_COUNT).unwrap();
        let expected_triangles =
            2 * usize::from(SPHERE_LONGITUDE_SECTORS) * usize::from(SPHERE_LATITUDE_BANDS - 1);
        assert_eq!(expected_triangles, 224);
        assert_eq!(encoded.len(), 32_256);
        assert_eq!(encoded.len(), expected_vertices * VERTEX_BYTES);
        let values = encoded
            .chunks_exact(4)
            .map(|bytes| f32::from_le_bytes(bytes.try_into().unwrap()))
            .collect::<Vec<_>>();
        let vertices = values
            .chunks_exact(12)
            .map(|vertex| <[f32; 12]>::try_from(vertex).unwrap())
            .collect::<Vec<_>>();
        assert_eq!(vertices.len(), expected_vertices);
        assert_eq!(vertices.len() / 3, expected_triangles);
        assert_eq!(
            [
                vertices[0][3].to_bits(),
                vertices[0][4].to_bits(),
                vertices[0][5].to_bits(),
            ],
            [0.0_f32.to_bits(), 0.0_f32.to_bits(), (-1.0_f32).to_bits()]
        );
        let last = vertices.last().unwrap();
        assert_eq!(
            [last[3].to_bits(), last[4].to_bits(), last[5].to_bits()],
            [0.0_f32.to_bits(), 0.0_f32.to_bits(), 1.0_f32.to_bits()]
        );

        for vertex in &vertices {
            let position = [vertex[0], vertex[1], vertex[2]];
            let normal = [vertex[3], vertex[4], vertex[5]];
            let position_length = position
                .iter()
                .map(|value| value * value)
                .sum::<f32>()
                .sqrt();
            let normal_length = normal.iter().map(|value| value * value).sum::<f32>().sqrt();
            let radial_alignment = position
                .iter()
                .zip(normal)
                .map(|(position, normal)| position * normal)
                .sum::<f32>();
            assert_eq!(&vertex[6..], &[0.0, 0.0, 1.0, 0.0, 0.0, 1.0]);
            assert!((position_length - SPHERE_RADIUS).abs() <= 1.0e-5);
            assert!((normal_length - 1.0).abs() <= 1.0e-5);
            assert!((radial_alignment - SPHERE_RADIUS).abs() <= 1.0e-5);
        }

        for triangle in vertices.chunks_exact(3) {
            let first = [triangle[0][0], triangle[0][1], triangle[0][2]];
            let second = [triangle[1][0], triangle[1][1], triangle[1][2]];
            let third = [triangle[2][0], triangle[2][1], triangle[2][2]];
            let edge_a = subtract(second, first);
            let edge_b = subtract(third, first);
            let winding_normal = [
                edge_a[1] * edge_b[2] - edge_a[2] * edge_b[1],
                edge_a[2] * edge_b[0] - edge_a[0] * edge_b[2],
                edge_a[0] * edge_b[1] - edge_a[1] * edge_b[0],
            ];
            let centroid = [
                (first[0] + second[0] + third[0]) / 3.0,
                (first[1] + second[1] + third[1]) / 3.0,
                (first[2] + second[2] + third[2]) / 3.0,
            ];
            let outward = winding_normal
                .iter()
                .zip(centroid)
                .map(|(normal, center)| normal * center)
                .sum::<f32>();
            assert!(outward > 1.0e-6, "triangle winding must face outward");
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
