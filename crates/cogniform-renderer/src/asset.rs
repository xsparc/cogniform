use std::collections::{BTreeMap, BTreeSet, VecDeque};

use cogniform_assets::{AssetMaterial, AssetMeshKey, AssetTexture, AssetUploadJob};
use cogniform_protocol::ContentHash;

use crate::{RendererConfig, RendererError};

/// Result of admitting one immutable CPU mesh to the renderer upload queue.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssetUploadAdmission {
    /// A new upload job was admitted and capacity-reserved.
    Queued {
        /// Admitted immutable mesh identity.
        key: AssetMeshKey,
    },
    /// The same immutable mesh is already awaiting upload.
    AlreadyQueued {
        /// Existing queued immutable mesh identity.
        key: AssetMeshKey,
    },
    /// The same immutable mesh is already GPU-resident.
    AlreadyResident {
        /// Existing resident immutable mesh identity.
        key: AssetMeshKey,
    },
}

/// Result of processing one renderer-owned upload job.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AssetUploadOutcome {
    /// Immutable mesh identity that became resident.
    pub key: AssetMeshKey,
    /// Expanded triangle vertex count.
    pub vertex_count: u32,
    /// Exact allocated GPU vertex bytes.
    pub byte_len: u64,
    /// Whether this processing step uploaded the source's shared texture.
    pub texture_uploaded: bool,
    /// Exact texture bytes uploaded by this step, or zero when already resident or absent.
    pub texture_byte_len: u64,
}

/// Aggregate renderer asset occupancy without source bytes or backend handles.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RendererAssetStats {
    /// Jobs admitted but not yet processed.
    pub pending_uploads: u32,
    /// Bytes reserved by pending upload jobs.
    pub pending_bytes: u64,
    /// Immutable GPU-resident meshes.
    pub resident_meshes: u32,
    /// Exact resident vertex-buffer bytes.
    pub resident_bytes: u64,
    /// Unique source textures reserved by pending upload jobs.
    pub pending_textures: u32,
    /// Exact bytes reserved by pending unique textures.
    pub pending_texture_bytes: u64,
    /// Unique immutable source textures resident on the GPU.
    pub resident_textures: u32,
    /// Exact resident RGBA8 texture bytes.
    pub resident_texture_bytes: u64,
}

pub(crate) struct GpuAssetMesh {
    buffer: wgpu::Buffer,
    vertex_count: u32,
    material: AssetMaterial,
    byte_len: u64,
}

struct GpuAssetTexture {
    _texture: wgpu::Texture,
    view: wgpu::TextureView,
    byte_len: u64,
}

impl GpuAssetMesh {
    pub(crate) const fn buffer(&self) -> &wgpu::Buffer {
        &self.buffer
    }

    pub(crate) const fn vertex_count(&self) -> u32 {
        self.vertex_count
    }

    pub(crate) const fn material(&self) -> AssetMaterial {
        self.material
    }
}

pub(crate) struct RendererAssets {
    pending: VecDeque<AssetUploadJob>,
    pending_bytes: u64,
    pending_textures: BTreeSet<ContentHash>,
    pending_texture_bytes: u64,
    resident: BTreeMap<AssetMeshKey, GpuAssetMesh>,
    resident_bytes: u64,
    resident_textures: BTreeMap<ContentHash, GpuAssetTexture>,
    resident_texture_bytes: u64,
}

impl RendererAssets {
    pub(crate) fn new() -> Self {
        Self {
            pending: VecDeque::new(),
            pending_bytes: 0,
            pending_textures: BTreeSet::new(),
            pending_texture_bytes: 0,
            resident: BTreeMap::new(),
            resident_bytes: 0,
            resident_textures: BTreeMap::new(),
            resident_texture_bytes: 0,
        }
    }

    pub(crate) fn enqueue(
        &mut self,
        job: AssetUploadJob,
        config: &RendererConfig,
    ) -> Result<AssetUploadAdmission, RendererError> {
        let key = job.key();
        if self.resident.contains_key(&key) {
            return Ok(AssetUploadAdmission::AlreadyResident { key });
        }
        if self.pending.iter().any(|candidate| candidate.key() == key) {
            return Ok(AssetUploadAdmission::AlreadyQueued { key });
        }
        if job.material().has_base_color_texture() != job.base_color_texture().is_some() {
            return Err(RendererError::InvalidAssetMesh {
                key,
                reason: "textured material and immutable image must be present together",
            });
        }
        let vertex_count = u32::try_from(job.vertices().len()).unwrap_or(u32::MAX);
        if vertex_count == 0 || vertex_count % 3 != 0 {
            return Err(RendererError::InvalidAssetMesh {
                key,
                reason: "expanded triangle vertices must be non-empty and divisible by three",
            });
        }
        if vertex_count > config.max_asset_vertices.get() {
            return Err(RendererError::AssetVertexLimitExceeded {
                key,
                actual: vertex_count,
                limit: config.max_asset_vertices.get(),
            });
        }
        let byte_len = job.byte_len();
        if byte_len > config.max_asset_mesh_bytes.get() {
            return Err(RendererError::AssetMeshBytesExceeded {
                key,
                actual: byte_len,
                limit: config.max_asset_mesh_bytes.get(),
            });
        }
        if self.pending.len()
            >= usize::try_from(config.asset_upload_capacity.get())
                .expect("u32 upload capacity fits usize")
        {
            return Err(RendererError::AssetUploadCapacityExceeded {
                capacity: config.asset_upload_capacity.get(),
            });
        }
        let projected_pending = self.pending_bytes.saturating_add(byte_len);
        if projected_pending > config.max_pending_asset_upload_bytes.get() {
            return Err(RendererError::AssetUploadBytesExceeded {
                actual: projected_pending,
                limit: config.max_pending_asset_upload_bytes.get(),
            });
        }
        let projected_meshes = self
            .resident
            .len()
            .saturating_add(self.pending.len())
            .saturating_add(1);
        if projected_meshes
            > usize::try_from(config.max_resident_asset_meshes.get())
                .expect("u32 residency capacity fits usize")
        {
            return Err(RendererError::AssetResidencyCapacityExceeded {
                capacity: config.max_resident_asset_meshes.get(),
            });
        }
        let reserved_bytes = self.resident_bytes.saturating_add(projected_pending);
        if reserved_bytes > config.max_resident_asset_bytes.get() {
            return Err(RendererError::AssetResidencyBytesExceeded {
                actual: reserved_bytes,
                limit: config.max_resident_asset_bytes.get(),
            });
        }
        self.reserve_texture(key, job.base_color_texture(), config)?;
        self.pending.push_back(job);
        self.pending_bytes = projected_pending;
        Ok(AssetUploadAdmission::Queued { key })
    }

    fn reserve_texture(
        &mut self,
        key: AssetMeshKey,
        texture: Option<&AssetTexture>,
        config: &RendererConfig,
    ) -> Result<(), RendererError> {
        let Some(texture) = texture else {
            return Ok(());
        };
        validate_texture(key, texture, config)?;
        if self.resident_textures.contains_key(&key.content_hash)
            || self.pending_textures.contains(&key.content_hash)
        {
            return Ok(());
        }
        let texture_bytes = texture.byte_len();
        let projected_pending = self.pending_texture_bytes.saturating_add(texture_bytes);
        if projected_pending > config.max_pending_asset_texture_bytes.get() {
            return Err(RendererError::AssetTextureUploadBytesExceeded {
                actual: projected_pending,
                limit: config.max_pending_asset_texture_bytes.get(),
            });
        }
        let projected_textures = self
            .resident_textures
            .len()
            .saturating_add(self.pending_textures.len())
            .saturating_add(1);
        if projected_textures
            > usize::try_from(config.max_resident_asset_textures.get())
                .expect("u32 texture capacity fits usize")
        {
            return Err(RendererError::AssetTextureResidencyCapacityExceeded {
                capacity: config.max_resident_asset_textures.get(),
            });
        }
        let reserved_bytes = self
            .resident_texture_bytes
            .saturating_add(projected_pending);
        if reserved_bytes > config.max_resident_asset_texture_bytes.get() {
            return Err(RendererError::AssetTextureResidencyBytesExceeded {
                actual: reserved_bytes,
                limit: config.max_resident_asset_texture_bytes.get(),
            });
        }
        self.pending_textures.insert(key.content_hash);
        self.pending_texture_bytes = projected_pending;
        Ok(())
    }

    pub(crate) fn process_next(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) -> Option<AssetUploadOutcome> {
        let job = self.pending.pop_front()?;
        let key = job.key();
        let byte_len = job.byte_len();
        self.pending_bytes = self.pending_bytes.saturating_sub(byte_len);
        let vertex_count =
            u32::try_from(job.vertices().len()).expect("admitted vertex bound fits u32");
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("cogniform-asset-vertices"),
            size: byte_len,
            usage: wgpu::BufferUsages::VERTEX,
            mapped_at_creation: true,
        });
        {
            let mut mapped = buffer
                .slice(..)
                .get_mapped_range_mut()
                .expect("newly created mapped buffer is writable");
            let encoded = encode_vertices(job.vertices());
            debug_assert_eq!(u64::try_from(encoded.len()).ok(), Some(byte_len));
            mapped.copy_from_slice(&encoded);
        }
        buffer.unmap();
        let material = job.material();
        let (texture_uploaded, texture_byte_len) = if let Some(texture) = job.base_color_texture()
            && !self.resident_textures.contains_key(&key.content_hash)
        {
            let gpu_texture = create_texture(device, queue, texture);
            let texture_byte_len = gpu_texture.byte_len;
            let was_pending = self.pending_textures.remove(&key.content_hash);
            debug_assert!(was_pending, "unique texture was reserved at admission");
            self.pending_texture_bytes = self
                .pending_texture_bytes
                .checked_sub(texture_byte_len)
                .expect("admitted texture bytes remain reserved");
            let previous = self.resident_textures.insert(key.content_hash, gpu_texture);
            debug_assert!(previous.is_none(), "shared texture uploads only once");
            self.resident_texture_bytes = self
                .resident_texture_bytes
                .checked_add(texture_byte_len)
                .expect("admission reserved resident texture bytes");
            (true, texture_byte_len)
        } else {
            (false, 0)
        };
        let previous = self.resident.insert(
            key,
            GpuAssetMesh {
                buffer,
                vertex_count,
                material,
                byte_len,
            },
        );
        debug_assert!(
            previous.is_none(),
            "duplicate uploads are rejected at admission"
        );
        self.resident_bytes = self
            .resident_bytes
            .checked_add(byte_len)
            .expect("admission reserved resident bytes");
        Some(AssetUploadOutcome {
            key,
            vertex_count,
            byte_len,
            texture_uploaded,
            texture_byte_len,
        })
    }

    pub(crate) fn mesh(&self, key: AssetMeshKey) -> Option<&GpuAssetMesh> {
        self.resident.get(&key)
    }

    pub(crate) fn texture_view(&self, content_hash: ContentHash) -> Option<&wgpu::TextureView> {
        self.resident_textures
            .get(&content_hash)
            .map(|texture| &texture.view)
    }

    pub(crate) fn stats(&self) -> RendererAssetStats {
        debug_assert_eq!(
            self.resident_bytes,
            self.resident
                .values()
                .map(|mesh| mesh.byte_len)
                .sum::<u64>()
        );
        debug_assert_eq!(
            self.resident_texture_bytes,
            self.resident_textures
                .values()
                .map(|texture| texture.byte_len)
                .sum::<u64>()
        );
        RendererAssetStats {
            pending_uploads: u32::try_from(self.pending.len()).unwrap_or(u32::MAX),
            pending_bytes: self.pending_bytes,
            resident_meshes: u32::try_from(self.resident.len()).unwrap_or(u32::MAX),
            resident_bytes: self.resident_bytes,
            pending_textures: u32::try_from(self.pending_textures.len()).unwrap_or(u32::MAX),
            pending_texture_bytes: self.pending_texture_bytes,
            resident_textures: u32::try_from(self.resident_textures.len()).unwrap_or(u32::MAX),
            resident_texture_bytes: self.resident_texture_bytes,
        }
    }
}

fn validate_texture(
    key: AssetMeshKey,
    texture: &AssetTexture,
    config: &RendererConfig,
) -> Result<(), RendererError> {
    if texture.width() > config.max_asset_texture_dimension_2d.get()
        || texture.height() > config.max_asset_texture_dimension_2d.get()
    {
        return Err(RendererError::AssetTextureLimitExceeded {
            key,
            reason: "configured dimension limit",
        });
    }
    let expected = u64::from(texture.width())
        .checked_mul(u64::from(texture.height()))
        .and_then(|pixels| pixels.checked_mul(4));
    if expected != Some(texture.byte_len()) {
        return Err(RendererError::InvalidAssetMesh {
            key,
            reason: "texture must contain exact tightly packed RGBA8 rows",
        });
    }
    if texture.byte_len() > config.max_asset_texture_bytes.get() {
        return Err(RendererError::AssetTextureLimitExceeded {
            key,
            reason: "configured byte limit",
        });
    }
    Ok(())
}

fn create_texture(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    source: &AssetTexture,
) -> GpuAssetTexture {
    let size = wgpu::Extent3d {
        width: source.width(),
        height: source.height(),
        depth_or_array_layers: 1,
    };
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("cogniform-asset-base-color"),
        size,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8UnormSrgb,
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
        source.rgba8(),
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(source.width() * 4),
            rows_per_image: Some(source.height()),
        },
        size,
    );
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    GpuAssetTexture {
        _texture: texture,
        view,
        byte_len: source.byte_len(),
    }
}

fn encode_vertices(vertices: &[cogniform_assets::AssetVertex]) -> Vec<u8> {
    vertices
        .iter()
        .flat_map(|vertex| {
            vertex
                .position
                .iter()
                .chain(&vertex.normal)
                .chain(&vertex.texcoord_0)
                .flat_map(|value| value.get().to_le_bytes())
        })
        .collect()
}

impl std::fmt::Debug for RendererAssets {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("RendererAssets")
            .field("stats", &self.stats())
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use core::num::{NonZeroU32, NonZeroU64};

    use cogniform_assets::{AssetMeshKey, AssetStore, AssetVertex, content_hash};
    use cogniform_protocol::FiniteF32;

    use super::*;

    #[test]
    fn upload_count_is_reserved_before_gpu_allocation() {
        let first = fixture_upload(false);
        let second = fixture_upload(true);
        assert_ne!(first.key(), second.key());
        let config =
            RendererConfig::new(64, 64).with_asset_upload_capacity(NonZeroU32::new(1).unwrap());
        let mut assets = RendererAssets::new();
        assert!(matches!(
            assets.enqueue(first, &config),
            Ok(AssetUploadAdmission::Queued { .. })
        ));
        assert!(matches!(
            assets.enqueue(second, &config),
            Err(RendererError::AssetUploadCapacityExceeded { capacity: 1 })
        ));
        assert_eq!(assets.stats().pending_uploads, 1);
        assert_eq!(assets.stats().resident_meshes, 0);
    }

    #[test]
    fn upload_vertices_are_interleaved_position_normal_then_texcoord() {
        let finite = |value| FiniteF32::new(value).unwrap();
        let vertex = AssetVertex {
            position: [finite(1.0), finite(2.0), finite(3.0)],
            normal: [finite(0.0), finite(0.0), finite(1.0)],
            texcoord_0: [finite(-0.25), finite(1.25)],
        };
        let encoded = encode_vertices(&[vertex]);
        assert_eq!(encoded.len(), 32);
        let values = encoded
            .chunks_exact(4)
            .map(|bytes| f32::from_le_bytes(bytes.try_into().unwrap()))
            .collect::<Vec<_>>();
        assert_eq!(values, [1.0, 2.0, 3.0, 0.0, 0.0, 1.0, -0.25, 1.25]);
    }

    #[test]
    fn exact_interleaved_bytes_are_rejected_before_gpu_allocation() {
        let upload = fixture_upload(false);
        assert_eq!(upload.byte_len(), 96);
        let config =
            RendererConfig::new(64, 64).with_max_asset_mesh_bytes(NonZeroU64::new(95).unwrap());
        let mut assets = RendererAssets::new();
        assert!(matches!(
            assets.enqueue(upload, &config),
            Err(RendererError::AssetMeshBytesExceeded {
                actual: 96,
                limit: 95,
                ..
            })
        ));
        assert_eq!(assets.stats().pending_uploads, 0);
        assert_eq!(assets.stats().resident_meshes, 0);
    }

    #[test]
    fn texture_bytes_are_validated_and_reserved_before_gpu_allocation() {
        let upload = textured_upload([255, 0, 0, 255]);
        assert_eq!(upload.base_color_texture().unwrap().byte_len(), 4);
        let config =
            RendererConfig::new(64, 64).with_max_asset_texture_bytes(NonZeroU64::new(3).unwrap());
        let mut assets = RendererAssets::new();
        assert!(matches!(
            assets.enqueue(upload.clone(), &config),
            Err(RendererError::AssetTextureLimitExceeded {
                reason: "configured byte limit",
                ..
            })
        ));
        assert_eq!(assets.stats().pending_uploads, 0);
        assert_eq!(assets.stats().pending_textures, 0);

        assets
            .enqueue(upload, &RendererConfig::new(64, 64))
            .unwrap();
        assert_eq!(assets.stats().pending_uploads, 1);
        assert_eq!(assets.stats().pending_bytes, 96);
        assert_eq!(assets.stats().pending_textures, 1);
        assert_eq!(assets.stats().pending_texture_bytes, 4);
        assert_eq!(assets.stats().resident_textures, 0);
    }

    #[test]
    fn unique_texture_reservations_enforce_each_aggregate_cap_without_mutation() {
        let first = textured_upload([255, 0, 0, 255]);
        let second = textured_upload([0, 255, 0, 255]);
        let cases = [
            (
                RendererConfig::new(64, 64)
                    .with_max_asset_texture_bytes(NonZeroU64::new(4).unwrap())
                    .with_max_pending_asset_texture_bytes(NonZeroU64::new(4).unwrap())
                    .with_max_resident_asset_textures(NonZeroU32::new(2).unwrap())
                    .with_max_resident_asset_texture_bytes(NonZeroU64::new(8).unwrap()),
                "pending",
            ),
            (
                RendererConfig::new(64, 64)
                    .with_max_asset_texture_bytes(NonZeroU64::new(4).unwrap())
                    .with_max_pending_asset_texture_bytes(NonZeroU64::new(8).unwrap())
                    .with_max_resident_asset_textures(NonZeroU32::new(1).unwrap())
                    .with_max_resident_asset_texture_bytes(NonZeroU64::new(8).unwrap()),
                "count",
            ),
            (
                RendererConfig::new(64, 64)
                    .with_max_asset_texture_bytes(NonZeroU64::new(4).unwrap())
                    .with_max_pending_asset_texture_bytes(NonZeroU64::new(8).unwrap())
                    .with_max_resident_asset_textures(NonZeroU32::new(2).unwrap())
                    .with_max_resident_asset_texture_bytes(NonZeroU64::new(4).unwrap()),
                "resident_bytes",
            ),
        ];
        for (config, expected) in cases {
            let mut assets = RendererAssets::new();
            assets.enqueue(first.clone(), &config).unwrap();
            let error = assets.enqueue(second.clone(), &config).unwrap_err();
            assert!(
                matches!(
                    (expected, error),
                    (
                        "pending",
                        RendererError::AssetTextureUploadBytesExceeded { .. }
                    ) | (
                        "count",
                        RendererError::AssetTextureResidencyCapacityExceeded { .. }
                    ) | (
                        "resident_bytes",
                        RendererError::AssetTextureResidencyBytesExceeded { .. }
                    )
                ),
                "unexpected texture reservation error"
            );
            assert_eq!(assets.stats().pending_uploads, 1);
            assert_eq!(assets.stats().pending_textures, 1);
            assert_eq!(assets.stats().pending_texture_bytes, 4);
        }
    }

    fn fixture_upload(change_color: bool) -> AssetUploadJob {
        let mut bytes = decode_hex(include_str!("../../../tests/assets/triangle.glb.hex"));
        if change_color {
            let position = bytes
                .windows(3)
                .position(|window| window == b"0.2")
                .expect("fixture contains its base color");
            bytes[position + 2] = b'3';
        }
        let hash = content_hash(&bytes);
        let mut store = AssetStore::default();
        store.enqueue(hash, bytes).unwrap();
        store.process_next().unwrap();
        store
            .upload_job(AssetMeshKey {
                content_hash: hash,
                mesh_index: 0,
            })
            .unwrap()
    }

    fn textured_upload(texel: [u8; 4]) -> AssetUploadJob {
        let mut binary = Vec::new();
        for position in [
            [-0.75_f32, -0.75, 0.0],
            [0.75, -0.75, 0.0],
            [0.0, 0.75, 0.0],
        ] {
            for value in position {
                binary.extend_from_slice(&value.to_le_bytes());
            }
        }
        for texcoord in [[0.5_f32, 0.5]; 3] {
            for value in texcoord {
                binary.extend_from_slice(&value.to_le_bytes());
            }
        }
        let mut png_bytes = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut png_bytes, 1, 1);
            encoder.set_color(png::ColorType::Rgba);
            encoder.set_depth(png::BitDepth::Eight);
            let mut writer = encoder.write_header().unwrap();
            writer.write_image_data(&texel).unwrap();
        }
        let image_offset = binary.len();
        binary.extend_from_slice(&png_bytes);
        let json = format!(
            r#"{{"asset":{{"version":"2.0"}},"buffers":[{{"byteLength":{binary_length}}}],"bufferViews":[{{"buffer":0,"byteOffset":0,"byteLength":36}},{{"buffer":0,"byteOffset":36,"byteLength":24}},{{"buffer":0,"byteOffset":{image_offset},"byteLength":{image_length}}}],"accessors":[{{"bufferView":0,"componentType":5126,"count":3,"type":"VEC3"}},{{"bufferView":1,"componentType":5126,"count":3,"type":"VEC2"}}],"materials":[{{"pbrMetallicRoughness":{{"baseColorTexture":{{"index":0}}}}}}],"textures":[{{"source":0}}],"images":[{{"bufferView":2,"mimeType":"image/png"}}],"meshes":[{{"primitives":[{{"attributes":{{"POSITION":0,"TEXCOORD_0":1}},"material":0}}]}}]}}"#,
            binary_length = binary.len(),
            image_length = png_bytes.len(),
        );
        let bytes = glb_with_json(&json, &binary);
        let hash = content_hash(&bytes);
        let mut store = AssetStore::default();
        store.enqueue(hash, bytes).unwrap();
        store.process_next().unwrap();
        store
            .upload_job(AssetMeshKey {
                content_hash: hash,
                mesh_index: 0,
            })
            .unwrap()
    }

    fn glb_with_json(json: &str, binary: &[u8]) -> Vec<u8> {
        let mut json = json.as_bytes().to_vec();
        json.resize(json.len().next_multiple_of(4), b' ');
        let mut binary = binary.to_vec();
        binary.resize(binary.len().next_multiple_of(4), 0);
        let length = 12 + 8 + json.len() + 8 + binary.len();
        let mut output = Vec::with_capacity(length);
        output.extend_from_slice(b"glTF");
        output.extend_from_slice(&2_u32.to_le_bytes());
        output.extend_from_slice(&u32::try_from(length).unwrap().to_le_bytes());
        output.extend_from_slice(&u32::try_from(json.len()).unwrap().to_le_bytes());
        output.extend_from_slice(&0x4e4f_534a_u32.to_le_bytes());
        output.extend_from_slice(&json);
        output.extend_from_slice(&u32::try_from(binary.len()).unwrap().to_le_bytes());
        output.extend_from_slice(&0x004e_4942_u32.to_le_bytes());
        output.extend_from_slice(&binary);
        output
    }

    fn decode_hex(value: &str) -> Vec<u8> {
        value
            .trim()
            .as_bytes()
            .chunks_exact(2)
            .map(|pair| u8::from_str_radix(core::str::from_utf8(pair).unwrap(), 16).unwrap())
            .collect()
    }
}
