use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    time::{Duration, Instant},
};

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
    /// Whether this processing step uploaded one or more source texture roles.
    pub texture_uploaded: bool,
    /// Exact aggregate texture bytes uploaded by this step, or zero when already resident or absent.
    pub texture_byte_len: u64,
}

/// Aggregate renderer asset occupancy without source bytes or backend handles.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RendererAssetStats {
    /// Jobs admitted but not yet processed.
    pub pending_uploads: u32,
    /// Monotonic elapsed microseconds for the oldest pending upload.
    pub oldest_pending_upload_age_micros: Option<u64>,
    /// Bytes reserved by pending upload jobs.
    pub pending_bytes: u64,
    /// Immutable GPU-resident meshes.
    pub resident_meshes: u32,
    /// Exact resident vertex-buffer bytes.
    pub resident_bytes: u64,
    /// Unique source-and-role textures reserved by pending upload jobs.
    pub pending_textures: u32,
    /// Exact bytes reserved by pending unique textures.
    pub pending_texture_bytes: u64,
    /// Unique immutable source-and-role textures resident on the GPU.
    pub resident_textures: u32,
    /// Exact resident RGBA8 texture bytes.
    pub resident_texture_bytes: u64,
}

/// Exact renderer-domain resources released for one content hash.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RendererAssetEviction {
    /// Immutable content identity selected for eviction.
    pub content_hash: ContentHash,
    /// Upload jobs removed while preserving unrelated FIFO order.
    pub removed_pending_uploads: u32,
    /// Exact pending vertex bytes released.
    pub released_pending_bytes: u64,
    /// GPU-resident meshes removed.
    pub removed_resident_meshes: u32,
    /// Exact resident vertex-buffer bytes released from renderer ownership.
    pub released_resident_bytes: u64,
    /// Unique pending role-texture reservations removed; currently zero to two.
    pub removed_pending_textures: u32,
    /// Exact pending RGBA8 texture bytes released.
    pub released_pending_texture_bytes: u64,
    /// Unique resident role textures removed; currently zero to two.
    pub removed_resident_textures: u32,
    /// Exact resident RGBA8 texture bytes released from renderer ownership.
    pub released_resident_texture_bytes: u64,
}

impl RendererAssetEviction {
    /// Returns whether the selected hash had no queued or resident renderer state.
    #[must_use]
    pub const fn is_already_absent(self) -> bool {
        self.removed_pending_uploads == 0
            && self.released_pending_bytes == 0
            && self.removed_resident_meshes == 0
            && self.released_resident_bytes == 0
            && self.removed_pending_textures == 0
            && self.released_pending_texture_bytes == 0
            && self.removed_resident_textures == 0
            && self.released_resident_texture_bytes == 0
    }
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum AssetTextureRole {
    BaseColor,
    Normal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct AssetTextureKey {
    content_hash: ContentHash,
    role: AssetTextureRole,
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

struct PendingAssetUpload {
    job: AssetUploadJob,
    queued_at: Instant,
}

impl PendingAssetUpload {
    fn key(&self) -> AssetMeshKey {
        self.job.key()
    }

    fn byte_len(&self) -> u64 {
        self.job.byte_len()
    }

    fn base_color_texture(&self) -> Option<&AssetTexture> {
        self.job.base_color_texture()
    }

    fn normal_texture(&self) -> Option<&AssetTexture> {
        self.job.normal_texture()
    }

    fn texture(&self, role: AssetTextureRole) -> Option<&AssetTexture> {
        match role {
            AssetTextureRole::BaseColor => self.base_color_texture(),
            AssetTextureRole::Normal => self.normal_texture(),
        }
    }
}

pub(crate) struct RendererAssets {
    pending: VecDeque<PendingAssetUpload>,
    pending_bytes: u64,
    pending_textures: BTreeSet<AssetTextureKey>,
    pending_texture_bytes: u64,
    resident: BTreeMap<AssetMeshKey, GpuAssetMesh>,
    resident_bytes: u64,
    resident_textures: BTreeMap<AssetTextureKey, GpuAssetTexture>,
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
        self.enqueue_with_time(job, config, None)
    }

    #[cfg(test)]
    fn enqueue_at(
        &mut self,
        job: AssetUploadJob,
        config: &RendererConfig,
        queued_at: Instant,
    ) -> Result<AssetUploadAdmission, RendererError> {
        self.enqueue_with_time(job, config, Some(queued_at))
    }

    fn enqueue_with_time(
        &mut self,
        job: AssetUploadJob,
        config: &RendererConfig,
        queued_at: Option<Instant>,
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
        if job.material().has_normal_texture() != job.normal_texture().is_some() {
            return Err(RendererError::InvalidAssetMesh {
                key,
                reason: "normal-textured material and immutable image must be present together",
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
        self.reserve_textures(&job, config)?;
        self.pending.push_back(PendingAssetUpload {
            job,
            queued_at: queued_at.unwrap_or_else(Instant::now),
        });
        self.pending_bytes = projected_pending;
        Ok(AssetUploadAdmission::Queued { key })
    }

    fn reserve_textures(
        &mut self,
        job: &AssetUploadJob,
        config: &RendererConfig,
    ) -> Result<(), RendererError> {
        let mesh_key = job.key();
        let mut reservations = Vec::with_capacity(2);
        for (role, texture) in [
            (AssetTextureRole::BaseColor, job.base_color_texture()),
            (AssetTextureRole::Normal, job.normal_texture()),
        ] {
            let Some(texture) = texture else {
                continue;
            };
            validate_texture(mesh_key, texture, config)?;
            let key = AssetTextureKey {
                content_hash: mesh_key.content_hash,
                role,
            };
            if !self.resident_textures.contains_key(&key) && !self.pending_textures.contains(&key) {
                reservations.push((key, texture.byte_len()));
            }
        }
        let new_bytes = reservations.iter().map(|(_, bytes)| bytes).sum::<u64>();
        let projected_pending = self.pending_texture_bytes.saturating_add(new_bytes);
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
            .saturating_add(reservations.len());
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
        for (key, _) in reservations {
            self.pending_textures.insert(key);
        }
        self.pending_texture_bytes = projected_pending;
        Ok(())
    }

    pub(crate) fn process_next(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) -> Option<AssetUploadOutcome> {
        let job = self.pending.pop_front()?.job;
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
        let mut texture_uploaded = false;
        let mut texture_byte_len = 0_u64;
        for (role, texture) in [
            (AssetTextureRole::BaseColor, job.base_color_texture()),
            (AssetTextureRole::Normal, job.normal_texture()),
        ] {
            let Some(texture) = texture else {
                continue;
            };
            let texture_key = AssetTextureKey {
                content_hash: key.content_hash,
                role,
            };
            if self.resident_textures.contains_key(&texture_key) {
                continue;
            }
            let gpu_texture = create_texture(device, queue, texture, role);
            let uploaded_bytes = gpu_texture.byte_len;
            let was_pending = self.pending_textures.remove(&texture_key);
            debug_assert!(was_pending, "unique role texture was reserved at admission");
            self.pending_texture_bytes = self
                .pending_texture_bytes
                .checked_sub(uploaded_bytes)
                .expect("admitted texture bytes remain reserved");
            let previous = self.resident_textures.insert(texture_key, gpu_texture);
            debug_assert!(previous.is_none(), "shared role texture uploads only once");
            self.resident_texture_bytes = self
                .resident_texture_bytes
                .checked_add(uploaded_bytes)
                .expect("admission reserved resident texture bytes");
            texture_uploaded = true;
            texture_byte_len = texture_byte_len
                .checked_add(uploaded_bytes)
                .expect("admission bounded aggregate texture upload bytes");
        }
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

    pub(crate) fn evict(&mut self, content_hash: ContentHash) -> RendererAssetEviction {
        let pending_texture_keys: Vec<_> = self
            .pending_textures
            .iter()
            .copied()
            .filter(|key| key.content_hash == content_hash)
            .collect();
        let removed_pending_textures =
            u32::try_from(pending_texture_keys.len()).expect("at most two roles are reserved");
        let pending_texture_bytes = pending_texture_keys
            .iter()
            .map(|key| {
                self.pending
                    .iter()
                    .filter(|job| job.key().content_hash == content_hash)
                    .find_map(|job| job.texture(key.role))
                    .expect("pending texture reservation retains an upload job")
                    .byte_len()
            })
            .sum();
        for key in pending_texture_keys {
            self.pending_textures.remove(&key);
        }

        let mut removed_pending_uploads = 0_u32;
        let mut released_pending_bytes = 0_u64;
        self.pending.retain(|job| {
            if job.key().content_hash == content_hash {
                removed_pending_uploads = removed_pending_uploads
                    .checked_add(1)
                    .expect("pending upload count remains exactly accounted");
                released_pending_bytes = released_pending_bytes
                    .checked_add(job.byte_len())
                    .expect("pending vertex bytes remain exactly accounted");
                false
            } else {
                true
            }
        });
        self.pending_bytes = self
            .pending_bytes
            .checked_sub(released_pending_bytes)
            .expect("pending vertex bytes remain exactly accounted");
        self.pending_texture_bytes = self
            .pending_texture_bytes
            .checked_sub(pending_texture_bytes)
            .expect("pending texture bytes remain exactly accounted");

        let mut removed_resident_meshes = 0_u32;
        let mut released_resident_bytes = 0_u64;
        self.resident.retain(|key, mesh| {
            if key.content_hash == content_hash {
                removed_resident_meshes = removed_resident_meshes
                    .checked_add(1)
                    .expect("resident mesh count remains exactly accounted");
                released_resident_bytes = released_resident_bytes
                    .checked_add(mesh.byte_len)
                    .expect("resident vertex bytes remain exactly accounted");
                false
            } else {
                true
            }
        });
        self.resident_bytes = self
            .resident_bytes
            .checked_sub(released_resident_bytes)
            .expect("resident vertex bytes remain exactly accounted");

        let resident_texture_keys: Vec<_> = self
            .resident_textures
            .keys()
            .copied()
            .filter(|key| key.content_hash == content_hash)
            .collect();
        let removed_resident_textures =
            u32::try_from(resident_texture_keys.len()).expect("at most two roles are resident");
        let resident_texture_bytes = resident_texture_keys
            .into_iter()
            .map(|key| {
                self.resident_textures
                    .remove(&key)
                    .expect("collected resident texture key remains present")
                    .byte_len
            })
            .sum();
        self.resident_texture_bytes = self
            .resident_texture_bytes
            .checked_sub(resident_texture_bytes)
            .expect("resident texture bytes remain exactly accounted");

        RendererAssetEviction {
            content_hash,
            removed_pending_uploads,
            released_pending_bytes,
            removed_resident_meshes,
            released_resident_bytes,
            removed_pending_textures,
            released_pending_texture_bytes: pending_texture_bytes,
            removed_resident_textures,
            released_resident_texture_bytes: resident_texture_bytes,
        }
    }

    pub(crate) fn mesh(&self, key: AssetMeshKey) -> Option<&GpuAssetMesh> {
        self.resident.get(&key)
    }

    pub(crate) fn texture_view(
        &self,
        content_hash: ContentHash,
        role: AssetTextureRole,
    ) -> Option<&wgpu::TextureView> {
        self.resident_textures
            .get(&AssetTextureKey { content_hash, role })
            .map(|texture| &texture.view)
    }

    pub(crate) fn stats(&self) -> RendererAssetStats {
        self.stats_at(Instant::now())
    }

    fn stats_at(&self, sampled_at: Instant) -> RendererAssetStats {
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
            oldest_pending_upload_age_micros: self
                .pending
                .iter()
                .map(|pending| pending.queued_at)
                .min()
                .map(|queued_at| elapsed_micros(sampled_at, queued_at)),
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

fn elapsed_micros(sampled_at: Instant, started_at: Instant) -> u64 {
    duration_micros(sampled_at.saturating_duration_since(started_at))
}

fn duration_micros(duration: Duration) -> u64 {
    u64::try_from(duration.as_micros()).unwrap_or(u64::MAX)
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
    role: AssetTextureRole,
) -> GpuAssetTexture {
    let size = wgpu::Extent3d {
        width: source.width(),
        height: source.height(),
        depth_or_array_layers: 1,
    };
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some(match role {
            AssetTextureRole::BaseColor => "cogniform-asset-base-color",
            AssetTextureRole::Normal => "cogniform-asset-normal",
        }),
        size,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: match role {
            AssetTextureRole::BaseColor => wgpu::TextureFormat::Rgba8UnormSrgb,
            AssetTextureRole::Normal => wgpu::TextureFormat::Rgba8Unorm,
        },
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
                .chain(&vertex.tangent)
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
    fn upload_age_tracks_only_retained_pending_jobs() {
        let started_at = Instant::now();
        let first = fixture_upload(false);
        let first_key = first.key();
        let second = fixture_upload(true);
        let second_key = second.key();
        let config =
            RendererConfig::new(64, 64).with_asset_upload_capacity(NonZeroU32::new(2).unwrap());
        let mut assets = RendererAssets::new();
        assert_eq!(
            assets.stats_at(started_at).oldest_pending_upload_age_micros,
            None
        );
        assets
            .enqueue_at(first.clone(), &config, started_at)
            .unwrap();
        assert!(matches!(
            assets
                .enqueue_at(first, &config, started_at + Duration::from_micros(8))
                .unwrap(),
            AssetUploadAdmission::AlreadyQueued { .. }
        ));
        assets
            .enqueue_at(second, &config, started_at + Duration::from_micros(3))
            .unwrap();
        let sampled_at = started_at + Duration::from_micros(13);
        assert_eq!(
            assets.stats_at(sampled_at).oldest_pending_upload_age_micros,
            Some(13)
        );
        assert!(matches!(
            assets.enqueue_at(
                textured_upload([1, 2, 3, 255]),
                &config,
                started_at + Duration::from_micros(9)
            ),
            Err(RendererError::AssetUploadCapacityExceeded { capacity: 2 })
        ));
        assert_eq!(
            assets.stats_at(sampled_at).oldest_pending_upload_age_micros,
            Some(13)
        );

        assert_eq!(
            assets.evict(first_key.content_hash).removed_pending_uploads,
            1
        );
        assert_eq!(assets.pending.front().unwrap().key(), second_key);
        assert_eq!(
            assets.stats_at(sampled_at).oldest_pending_upload_age_micros,
            Some(10)
        );
        assert_eq!(
            assets
                .evict(second_key.content_hash)
                .removed_pending_uploads,
            1
        );
        assert_eq!(
            assets.stats_at(sampled_at).oldest_pending_upload_age_micros,
            None
        );
        assert_eq!(duration_micros(Duration::MAX), u64::MAX);
    }

    #[test]
    fn upload_vertices_are_interleaved_position_normal_texcoord_then_tangent() {
        let finite = |value| FiniteF32::new(value).unwrap();
        let vertex = AssetVertex {
            position: [finite(1.0), finite(2.0), finite(3.0)],
            normal: [finite(0.0), finite(0.0), finite(1.0)],
            texcoord_0: [finite(-0.25), finite(1.25)],
            tangent: [finite(1.0), finite(0.0), finite(0.0), finite(-1.0)],
        };
        let encoded = encode_vertices(&[vertex]);
        assert_eq!(encoded.len(), 48);
        let values = encoded
            .chunks_exact(4)
            .map(|bytes| f32::from_le_bytes(bytes.try_into().unwrap()))
            .collect::<Vec<_>>();
        assert_eq!(
            values,
            [
                1.0, 2.0, 3.0, 0.0, 0.0, 1.0, -0.25, 1.25, 1.0, 0.0, 0.0, -1.0
            ]
        );
    }

    #[test]
    fn exact_interleaved_bytes_are_rejected_before_gpu_allocation() {
        let upload = fixture_upload(false);
        assert_eq!(upload.byte_len(), 144);
        let config =
            RendererConfig::new(64, 64).with_max_asset_mesh_bytes(NonZeroU64::new(143).unwrap());
        let mut assets = RendererAssets::new();
        assert!(matches!(
            assets.enqueue(upload, &config),
            Err(RendererError::AssetMeshBytesExceeded {
                actual: 144,
                limit: 143,
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
        assert_eq!(assets.stats().pending_bytes, 144);
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

    #[test]
    fn dual_role_texture_reservations_are_atomic_and_exact() {
        let upload = dual_textured_upload([128, 128, 255, 255]);
        assert!(upload.base_color_texture().is_some());
        assert!(upload.normal_texture().is_some());
        let cases = [
            RendererConfig::new(64, 64)
                .with_max_pending_asset_texture_bytes(NonZeroU64::new(7).unwrap()),
            RendererConfig::new(64, 64)
                .with_max_resident_asset_textures(NonZeroU32::new(1).unwrap()),
            RendererConfig::new(64, 64)
                .with_max_resident_asset_texture_bytes(NonZeroU64::new(7).unwrap()),
        ];
        for config in cases {
            let mut assets = RendererAssets::new();
            assert!(assets.enqueue(upload.clone(), &config).is_err());
            assert_eq!(assets.stats().pending_uploads, 0);
            assert_eq!(assets.stats().pending_bytes, 0);
            assert_eq!(assets.stats().pending_textures, 0);
            assert_eq!(assets.stats().pending_texture_bytes, 0);
        }

        let mut assets = RendererAssets::new();
        assets
            .enqueue(upload.clone(), &RendererConfig::new(64, 64))
            .unwrap();
        assert_eq!(assets.stats().pending_uploads, 1);
        assert_eq!(assets.stats().pending_textures, 2);
        assert_eq!(assets.stats().pending_texture_bytes, 8);
        let eviction = assets.evict(upload.key().content_hash);
        assert_eq!(eviction.removed_pending_uploads, 1);
        assert_eq!(eviction.removed_pending_textures, 2);
        assert_eq!(eviction.released_pending_texture_bytes, 8);
        assert!(assets.stats().pending_uploads == 0 && assets.stats().pending_textures == 0);
    }

    #[test]
    fn pending_eviction_releases_exact_reservations_and_preserves_other_fifo_work() {
        let selected = textured_upload([255, 0, 0, 255]);
        let selected_key = selected.key();
        let retained = fixture_upload(true);
        let retained_key = retained.key();
        let config = RendererConfig::new(64, 64);
        let mut assets = RendererAssets::new();
        assets.enqueue(selected.clone(), &config).unwrap();
        assets.enqueue(retained, &config).unwrap();

        let eviction = assets.evict(selected_key.content_hash);
        assert_eq!(eviction.content_hash, selected_key.content_hash);
        assert_eq!(eviction.removed_pending_uploads, 1);
        assert_eq!(eviction.released_pending_bytes, selected.byte_len());
        assert_eq!(eviction.removed_resident_meshes, 0);
        assert_eq!(eviction.released_resident_bytes, 0);
        assert_eq!(eviction.removed_pending_textures, 1);
        assert_eq!(eviction.released_pending_texture_bytes, 4);
        assert_eq!(eviction.removed_resident_textures, 0);
        assert_eq!(eviction.released_resident_texture_bytes, 0);
        assert_eq!(assets.stats().pending_uploads, 1);
        assert_eq!(assets.stats().pending_bytes, 144);
        assert_eq!(assets.stats().pending_textures, 0);
        assert_eq!(assets.pending.front().unwrap().key(), retained_key);

        assert!(assets.evict(selected_key.content_hash).is_already_absent());
        assert!(matches!(
            assets.enqueue(selected, &config),
            Ok(AssetUploadAdmission::Queued { key }) if key == selected_key
        ));
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

    fn dual_textured_upload(texel: [u8; 4]) -> AssetUploadJob {
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
        for normal in [[0.0_f32, 0.0, 1.0]; 3] {
            for value in normal {
                binary.extend_from_slice(&value.to_le_bytes());
            }
        }
        for tangent in [[1.0_f32, 0.0, 0.0, 1.0]; 3] {
            for value in tangent {
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
            r#"{{"asset":{{"version":"2.0"}},"buffers":[{{"byteLength":{binary_length}}}],"bufferViews":[{{"buffer":0,"byteOffset":0,"byteLength":36}},{{"buffer":0,"byteOffset":36,"byteLength":36}},{{"buffer":0,"byteOffset":72,"byteLength":48}},{{"buffer":0,"byteOffset":120,"byteLength":24}},{{"buffer":0,"byteOffset":{image_offset},"byteLength":{image_length}}}],"accessors":[{{"bufferView":0,"componentType":5126,"count":3,"type":"VEC3"}},{{"bufferView":1,"componentType":5126,"count":3,"type":"VEC3"}},{{"bufferView":2,"componentType":5126,"count":3,"type":"VEC4"}},{{"bufferView":3,"componentType":5126,"count":3,"type":"VEC2"}}],"materials":[{{"pbrMetallicRoughness":{{"baseColorTexture":{{"index":0}}}},"normalTexture":{{"index":0}}}}],"textures":[{{"source":0}}],"images":[{{"bufferView":4,"mimeType":"image/png"}}],"meshes":[{{"primitives":[{{"attributes":{{"POSITION":0,"NORMAL":1,"TANGENT":2,"TEXCOORD_0":3}},"material":0}}]}}]}}"#,
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
