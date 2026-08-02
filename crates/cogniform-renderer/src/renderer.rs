use crate::{
    AdapterPreference, AdapterSummary, CapabilityIssue, MAX_READBACK_TIMEOUT, MAX_TARGET_DIMENSION,
    MAX_TARGET_PIXELS, PendingFrame, RenderTargetKind, RendererConfig, RendererError,
};

const COLOR_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;
const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;
const ENTITY_ID_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::R32Uint;
const BYTES_PER_PIXEL: u32 = 4;
const COPY_ROW_ALIGNMENT: u32 = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
const REFERENCE_VERTEX_COUNT: u32 = 36;

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
    readback_layout: ReadbackLayout,
}

impl std::fmt::Debug for HeadlessRenderer {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("HeadlessRenderer")
            .field("config", &self.config)
            .field("adapter", &self.adapter)
            .finish_non_exhaustive()
    }
}

impl HeadlessRenderer {
    /// Negotiates an adapter and device without creating a window or surface.
    pub async fn new(config: RendererConfig) -> Result<Self, RendererError> {
        validate_config(&config)?;
        let readback_layout = ReadbackLayout::new(config.width, config.height)?;
        let adapter = request_adapter(config.adapter_preference).await?;
        let adapter_summary = AdapterSummary::from_adapter(&adapter);
        let (device, queue) =
            request_device(&adapter, &adapter_summary, &config, readback_layout).await?;
        let pipeline = create_reference_pipeline(&device).await?;

        Ok(Self {
            config,
            adapter: adapter_summary,
            device,
            queue,
            pipeline,
            readback_layout,
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

    /// Encodes and submits the built-in cube reference scene without waiting
    /// for GPU completion or CPU readback.
    #[must_use]
    pub fn submit_reference_scene(&self) -> PendingFrame {
        let size = wgpu::Extent3d {
            width: self.config.width,
            height: self.config.height,
            depth_or_array_layers: 1,
        };
        let targets = RenderTargets::new(&self.device, size);
        let readbacks = ReadbackBuffers::new(&self.device, self.readback_layout.buffer_size);
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("cogniform-reference-scene-encoder"),
            });
        encode_reference_pass(&mut encoder, &self.pipeline, &targets);

        copy_target_to_buffer(
            &mut encoder,
            &targets.color,
            wgpu::TextureAspect::All,
            &readbacks.color,
            self.readback_layout,
            size,
        );
        copy_target_to_buffer(
            &mut encoder,
            &targets.depth,
            wgpu::TextureAspect::DepthOnly,
            &readbacks.depth,
            self.readback_layout,
            size,
        );
        copy_target_to_buffer(
            &mut encoder,
            &targets.entity_ids,
            wgpu::TextureAspect::All,
            &readbacks.entity_ids,
            self.readback_layout,
            size,
        );

        let submission = self.queue.submit([encoder.finish()]);
        PendingFrame {
            device: self.device.clone(),
            submission,
            color: readbacks.color,
            depth: readbacks.depth,
            entity_ids: readbacks.entity_ids,
            layout: self.readback_layout,
            adapter: self.adapter.clone(),
            timeout: self.config.readback_timeout,
        }
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
) -> Result<wgpu::RenderPipeline, RendererError> {
    let scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("cogniform-reference-scene-shader"),
        source: wgpu::ShaderSource::Wgsl(include_str!("reference_scene.wgsl").into()),
    });
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("cogniform-reference-scene-layout"),
        bind_group_layouts: &[],
        immediate_size: 0,
    });
    let targets = [Some(COLOR_FORMAT.into()), Some(ENTITY_ID_FORMAT.into())];
    let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("cogniform-reference-scene-pipeline"),
        layout: Some(&layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs_main"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            buffers: &[],
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
    Ok(pipeline)
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
    required.max_color_attachments = required.max_color_attachments.max(2);
    required.max_color_attachment_bytes_per_sample =
        required.max_color_attachment_bytes_per_sample.max(8);
    required.max_buffer_size = required.max_buffer_size.max(layout.buffer_size);
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
    color_view: wgpu::TextureView,
    depth_view: wgpu::TextureView,
    entity_id_view: wgpu::TextureView,
}

impl RenderTargets {
    fn new(device: &wgpu::Device, size: wgpu::Extent3d) -> Self {
        let color = create_target_texture(device, "cogniform-color-target", size, COLOR_FORMAT);
        let depth = create_target_texture(device, "cogniform-depth-target", size, DEPTH_FORMAT);
        let entity_ids =
            create_target_texture(device, "cogniform-entity-id-target", size, ENTITY_ID_FORMAT);
        let color_view = color.create_view(&wgpu::TextureViewDescriptor::default());
        let depth_view = depth.create_view(&wgpu::TextureViewDescriptor::default());
        let entity_id_view = entity_ids.create_view(&wgpu::TextureViewDescriptor::default());
        Self {
            color,
            depth,
            entity_ids,
            color_view,
            depth_view,
            entity_id_view,
        }
    }
}

struct ReadbackBuffers {
    color: wgpu::Buffer,
    depth: wgpu::Buffer,
    entity_ids: wgpu::Buffer,
}

impl ReadbackBuffers {
    fn new(device: &wgpu::Device, size: u64) -> Self {
        Self {
            color: create_readback_buffer(device, "cogniform-color-readback", size),
            depth: create_readback_buffer(device, "cogniform-depth-readback", size),
            entity_ids: create_readback_buffer(device, "cogniform-entity-id-readback", size),
        }
    }
}

fn encode_reference_pass(
    encoder: &mut wgpu::CommandEncoder,
    pipeline: &wgpu::RenderPipeline,
    targets: &RenderTargets,
) {
    let color_attachments = [
        Some(wgpu::RenderPassColorAttachment {
            view: &targets.color_view,
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
            view: &targets.entity_id_view,
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
            view: &targets.depth_view,
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
    render_pass.set_pipeline(pipeline);
    render_pass.draw(0..REFERENCE_VERTEX_COUNT, 0..1);
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
    fn unavailable_backend_is_structured() {
        assert!(matches!(
            ensure_backends_available(wgpu::Backends::empty()),
            Err(RendererError::BackendUnavailable)
        ));
    }
}
