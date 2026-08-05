use std::collections::{BTreeMap, VecDeque};

use cogniform_assets::{AssetMeshKey, AssetUploadJob};

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
}

pub(crate) struct GpuAssetMesh {
    buffer: wgpu::Buffer,
    vertex_count: u32,
    base_color: [f32; 4],
    byte_len: u64,
}

impl GpuAssetMesh {
    pub(crate) const fn buffer(&self) -> &wgpu::Buffer {
        &self.buffer
    }

    pub(crate) const fn vertex_count(&self) -> u32 {
        self.vertex_count
    }

    pub(crate) const fn base_color(&self) -> [f32; 4] {
        self.base_color
    }
}

pub(crate) struct RendererAssets {
    pending: VecDeque<AssetUploadJob>,
    pending_bytes: u64,
    resident: BTreeMap<AssetMeshKey, GpuAssetMesh>,
    resident_bytes: u64,
}

impl RendererAssets {
    pub(crate) fn new() -> Self {
        Self {
            pending: VecDeque::new(),
            pending_bytes: 0,
            resident: BTreeMap::new(),
            resident_bytes: 0,
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
        self.pending.push_back(job);
        self.pending_bytes = projected_pending;
        Ok(AssetUploadAdmission::Queued { key })
    }

    pub(crate) fn process_next(&mut self, device: &wgpu::Device) -> Option<AssetUploadOutcome> {
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
        let base_color = job.base_color().map(cogniform_protocol::UnitF32::get);
        let previous = self.resident.insert(
            key,
            GpuAssetMesh {
                buffer,
                vertex_count,
                base_color,
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
        })
    }

    pub(crate) fn mesh(&self, key: AssetMeshKey) -> Option<&GpuAssetMesh> {
        self.resident.get(&key)
    }

    pub(crate) fn stats(&self) -> RendererAssetStats {
        debug_assert_eq!(
            self.resident_bytes,
            self.resident
                .values()
                .map(|mesh| mesh.byte_len)
                .sum::<u64>()
        );
        RendererAssetStats {
            pending_uploads: u32::try_from(self.pending.len()).unwrap_or(u32::MAX),
            pending_bytes: self.pending_bytes,
            resident_meshes: u32::try_from(self.resident.len()).unwrap_or(u32::MAX),
            resident_bytes: self.resident_bytes,
        }
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

    use cogniform_assets::{AssetMeshKey, AssetStore, content_hash};

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
    fn upload_vertices_are_interleaved_position_then_normal() {
        let upload = fixture_upload(false);
        let encoded = encode_vertices(upload.vertices());
        assert_eq!(u64::try_from(encoded.len()).unwrap(), upload.byte_len());
        let values = encoded
            .chunks_exact(4)
            .map(|bytes| f32::from_le_bytes(bytes.try_into().unwrap()))
            .collect::<Vec<_>>();
        let first = upload.vertices()[0];
        let expected = first
            .position
            .iter()
            .chain(&first.normal)
            .map(|value| value.get())
            .collect::<Vec<_>>();
        assert_eq!(&values[..6], expected);
    }

    #[test]
    fn exact_interleaved_bytes_are_rejected_before_gpu_allocation() {
        let upload = fixture_upload(false);
        assert_eq!(upload.byte_len(), 72);
        let config =
            RendererConfig::new(64, 64).with_max_asset_mesh_bytes(NonZeroU64::new(71).unwrap());
        let mut assets = RendererAssets::new();
        assert!(matches!(
            assets.enqueue(upload, &config),
            Err(RendererError::AssetMeshBytesExceeded {
                actual: 72,
                limit: 71,
                ..
            })
        ));
        assert_eq!(assets.stats().pending_uploads, 0);
        assert_eq!(assets.stats().resident_meshes, 0);
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

    fn decode_hex(value: &str) -> Vec<u8> {
        value
            .trim()
            .as_bytes()
            .chunks_exact(2)
            .map(|pair| u8::from_str_radix(core::str::from_utf8(pair).unwrap(), 16).unwrap())
            .collect()
    }
}
