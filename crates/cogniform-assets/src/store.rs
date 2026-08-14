use std::{
    collections::{BTreeMap, VecDeque},
    time::{Duration, Instant},
};

use cogniform_protocol::ContentHash;

use crate::{
    AssetAdmission, AssetDiagnostic, AssetDiagnosticCode, AssetError, AssetMeshKey,
    AssetProcessOutcome, AssetRecord, AssetState, AssetStoreConfig, AssetStoreStats,
    AssetUploadJob, content_hash,
    glb::{decode_glb, proxy_asset},
    types::DecodedAsset,
};

struct PendingImport {
    content_hash: ContentHash,
    source: Box<[u8]>,
    queued_at: Instant,
}

struct StoredAsset {
    record: AssetRecord,
    decoded: Option<DecodedAsset>,
}

/// Caller-driven bounded store for verified immutable GLB source and decoded meshes.
///
/// Admission only verifies the exact content hash and queues bytes. Decoding is
/// performed solely when [`AssetStore::process_next`] is called, allowing the
/// service domain to schedule that work away from world and render critical paths.
pub struct AssetStore {
    config: AssetStoreConfig,
    pending: VecDeque<PendingImport>,
    records: BTreeMap<ContentHash, StoredAsset>,
    pending_source_bytes: u64,
    resident_cpu_bytes: u64,
}

impl std::fmt::Debug for AssetStore {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("AssetStore")
            .field("config", &self.config)
            .field("stats", &self.stats())
            .finish_non_exhaustive()
    }
}

impl AssetStore {
    /// Creates an empty store with explicit source, queue, and decoded-memory bounds.
    #[must_use]
    pub fn new(config: AssetStoreConfig) -> Self {
        Self {
            config,
            pending: VecDeque::new(),
            records: BTreeMap::new(),
            pending_source_bytes: 0,
            resident_cpu_bytes: 0,
        }
    }

    /// Verifies exact bytes and queues a new immutable import without decoding it.
    pub fn enqueue(
        &mut self,
        expected_hash: ContentHash,
        source: Vec<u8>,
    ) -> Result<AssetAdmission, AssetError> {
        self.enqueue_with_time(expected_hash, source, None)
    }

    #[cfg(test)]
    fn enqueue_at(
        &mut self,
        expected_hash: ContentHash,
        source: Vec<u8>,
        queued_at: Instant,
    ) -> Result<AssetAdmission, AssetError> {
        self.enqueue_with_time(expected_hash, source, Some(queued_at))
    }

    fn enqueue_with_time(
        &mut self,
        expected_hash: ContentHash,
        source: Vec<u8>,
        queued_at: Option<Instant>,
    ) -> Result<AssetAdmission, AssetError> {
        let source_bytes = u64::try_from(source.len()).unwrap_or(u64::MAX);
        if source_bytes > self.config.limits.max_source_bytes.get() {
            return Err(AssetError::SourceSizeExceeded {
                actual: source_bytes,
                limit: self.config.limits.max_source_bytes.get(),
            });
        }
        let actual_hash = content_hash(&source);
        if actual_hash != expected_hash {
            return Err(AssetError::ContentHashMismatch {
                expected: expected_hash,
                actual: actual_hash,
            });
        }
        if let Some(stored) = self.records.get(&expected_hash) {
            return Ok(AssetAdmission::AlreadyKnown {
                content_hash: expected_hash,
                state: stored.record.state,
            });
        }
        if self.records.len()
            >= usize::try_from(self.config.limits.max_assets.get())
                .expect("u32 asset capacity fits usize")
        {
            return Err(AssetError::RecordCapacityExceeded {
                capacity: self.config.limits.max_assets.get(),
            });
        }
        if self.pending.len()
            >= usize::try_from(self.config.limits.max_pending_imports.get())
                .expect("u32 import capacity fits usize")
        {
            return Err(AssetError::ImportCapacityExceeded {
                capacity: self.config.limits.max_pending_imports.get(),
            });
        }
        let projected_pending = self.pending_source_bytes.saturating_add(source_bytes);
        if projected_pending > self.config.limits.max_pending_source_bytes.get() {
            return Err(AssetError::PendingSourceBytesExceeded {
                actual: projected_pending,
                limit: self.config.limits.max_pending_source_bytes.get(),
            });
        }

        self.records.insert(
            expected_hash,
            StoredAsset {
                record: AssetRecord {
                    content_hash: expected_hash,
                    state: AssetState::Queued,
                    source_bytes,
                    decoded_bytes: 0,
                    mesh_count: 0,
                    diagnostics: Vec::new(),
                },
                decoded: None,
            },
        );
        self.pending.push_back(PendingImport {
            content_hash: expected_hash,
            source: source.into_boxed_slice(),
            queued_at: queued_at.unwrap_or_else(Instant::now),
        });
        self.pending_source_bytes = projected_pending;
        Ok(AssetAdmission::Queued {
            content_hash: expected_hash,
        })
    }

    /// Decodes at most one queued source and releases its original source bytes.
    ///
    /// Import failures become retained typed records rather than escaping as
    /// parser strings. A configured proxy is used only for explicitly
    /// unsupported features, never for malformed or over-limit input.
    pub fn process_next(&mut self) -> Option<AssetProcessOutcome> {
        let pending = self.pending.pop_front()?;
        let source_bytes = u64::try_from(pending.source.len()).unwrap_or(u64::MAX);
        self.pending_source_bytes = self.pending_source_bytes.saturating_sub(source_bytes);

        let decoded = match decode_glb(&pending.source, self.config.limits) {
            Ok(decoded) => Ok((AssetState::Ready, decoded, None)),
            Err(diagnostic)
                if diagnostic.code.permits_proxy()
                    && self.config.unsupported_policy
                        == crate::UnsupportedAssetPolicy::ProxyCuboid =>
            {
                Ok((AssetState::ProxyReady, proxy_asset(), Some(diagnostic)))
            }
            Err(diagnostic) => Err(diagnostic),
        };

        let stored = self
            .records
            .get_mut(&pending.content_hash)
            .expect("queued import retains its record");
        match decoded {
            Ok((state, decoded, diagnostic)) => {
                let projected = self.resident_cpu_bytes.saturating_add(decoded.byte_len);
                if projected > self.config.limits.max_resident_cpu_bytes.get() {
                    reject_record(
                        stored,
                        AssetDiagnostic::new(
                            AssetDiagnosticCode::ByteLimitExceeded,
                            "asset_store.resident_cpu_bytes",
                            None,
                        ),
                    );
                } else {
                    stored.record.state = state;
                    stored.record.decoded_bytes = decoded.byte_len;
                    stored.record.mesh_count =
                        u32::try_from(decoded.meshes.len()).unwrap_or(u32::MAX);
                    stored.record.diagnostics = diagnostic.into_iter().collect();
                    stored.decoded = Some(decoded);
                    self.resident_cpu_bytes = projected;
                }
            }
            Err(diagnostic) => reject_record(stored, diagnostic),
        }

        Some(AssetProcessOutcome {
            content_hash: pending.content_hash,
            state: stored.record.state,
            mesh_count: stored.record.mesh_count,
        })
    }

    /// Returns a retained immutable lifecycle record.
    #[must_use]
    pub fn record(&self, content_hash: ContentHash) -> Option<&AssetRecord> {
        self.records.get(&content_hash).map(|stored| &stored.record)
    }

    /// Produces an immutable renderer upload job for one ready decoded mesh.
    pub fn upload_job(&self, key: AssetMeshKey) -> Result<AssetUploadJob, AssetError> {
        let stored = self
            .records
            .get(&key.content_hash)
            .ok_or(AssetError::AssetNotFound {
                content_hash: key.content_hash,
            })?;
        let decoded = stored.decoded.as_ref().ok_or(AssetError::AssetNotReady {
            content_hash: key.content_hash,
        })?;
        let mesh = decoded
            .meshes
            .get(usize::try_from(key.mesh_index).unwrap_or(usize::MAX))
            .ok_or(AssetError::MeshNotFound {
                content_hash: key.content_hash,
                mesh_index: key.mesh_index,
            })?;
        Ok(AssetUploadJob::new(
            key,
            mesh.vertices.clone(),
            mesh.material,
            mesh.material
                .has_base_color_texture()
                .then(|| decoded.base_color_texture.clone())
                .flatten(),
            mesh.material
                .has_metallic_roughness_texture()
                .then(|| decoded.metallic_roughness_texture.clone())
                .flatten(),
            mesh.material
                .has_normal_texture()
                .then(|| decoded.normal_texture.clone())
                .flatten(),
        ))
    }

    /// Explicitly removes all retained CPU-side state for one content hash.
    ///
    /// Eviction is content-hash-wide and caller-driven. It cancels a queued
    /// import or releases a decoded/rejected record without decoding, external
    /// I/O, or mutation of any logical world reference.
    pub fn evict(&mut self, content_hash: ContentHash) -> crate::AssetStoreEviction {
        let Some(stored) = self.records.remove(&content_hash) else {
            debug_assert!(
                !self
                    .pending
                    .iter()
                    .any(|pending| pending.content_hash == content_hash),
                "pending imports always retain a lifecycle record"
            );
            return crate::AssetStoreEviction {
                content_hash,
                previous_state: None,
                removed_pending_imports: 0,
                released_pending_source_bytes: 0,
                released_resident_cpu_bytes: 0,
                removed_meshes: 0,
                removed_textures: 0,
            };
        };

        let mut removed_pending_imports = 0_u32;
        let mut released_pending_source_bytes = 0_u64;
        self.pending.retain(|pending| {
            if pending.content_hash == content_hash {
                removed_pending_imports = removed_pending_imports
                    .checked_add(1)
                    .expect("pending import count remains exactly accounted");
                released_pending_source_bytes = released_pending_source_bytes
                    .checked_add(
                        u64::try_from(pending.source.len())
                            .expect("supported targets represent source lengths as u64"),
                    )
                    .expect("pending source bytes remain exactly accounted");
                false
            } else {
                true
            }
        });
        debug_assert!(removed_pending_imports <= 1);
        self.pending_source_bytes = self
            .pending_source_bytes
            .checked_sub(released_pending_source_bytes)
            .expect("queued source bytes remain exactly accounted");
        self.resident_cpu_bytes = self
            .resident_cpu_bytes
            .checked_sub(stored.record.decoded_bytes)
            .expect("decoded CPU bytes remain exactly accounted");

        let removed_textures = stored
            .decoded
            .as_ref()
            .map_or(0, crate::types::DecodedAsset::texture_count);
        crate::AssetStoreEviction {
            content_hash,
            previous_state: Some(stored.record.state),
            removed_pending_imports,
            released_pending_source_bytes,
            released_resident_cpu_bytes: stored.record.decoded_bytes,
            removed_meshes: stored.record.mesh_count,
            removed_textures,
        }
    }

    /// Returns aggregate occupancy without exposing admitted source content.
    #[must_use]
    pub fn stats(&self) -> AssetStoreStats {
        self.stats_at(Instant::now())
    }

    fn stats_at(&self, sampled_at: Instant) -> AssetStoreStats {
        AssetStoreStats {
            records: u32::try_from(self.records.len()).unwrap_or(u32::MAX),
            pending_imports: u32::try_from(self.pending.len()).unwrap_or(u32::MAX),
            oldest_pending_import_age_micros: self
                .pending
                .iter()
                .map(|pending| pending.queued_at)
                .min()
                .map(|queued_at| elapsed_micros(sampled_at, queued_at)),
            pending_source_bytes: self.pending_source_bytes,
            resident_cpu_bytes: self.resident_cpu_bytes,
        }
    }
}

impl Default for AssetStore {
    fn default() -> Self {
        Self::new(AssetStoreConfig::default())
    }
}

fn reject_record(stored: &mut StoredAsset, diagnostic: AssetDiagnostic) {
    stored.record.state = AssetState::Rejected;
    stored.record.decoded_bytes = 0;
    stored.record.mesh_count = 0;
    stored.record.diagnostics = vec![diagnostic];
    stored.decoded = None;
}

fn elapsed_micros(sampled_at: Instant, started_at: Instant) -> u64 {
    duration_micros(sampled_at.saturating_duration_since(started_at))
}

fn duration_micros(duration: Duration) -> u64 {
    u64::try_from(duration.as_micros()).unwrap_or(u64::MAX)
}

#[cfg(test)]
mod tests {
    use core::num::NonZeroU32;

    use super::*;

    #[test]
    fn import_age_tracks_only_retained_pending_sources() {
        let started_at = Instant::now();
        let mut store = AssetStore::default();
        assert_eq!(
            store.stats_at(started_at).oldest_pending_import_age_micros,
            None
        );

        let first = b"first pending source".to_vec();
        let first_hash = content_hash(&first);
        store
            .enqueue_at(first_hash, first.clone(), started_at)
            .unwrap();
        assert!(matches!(
            store
                .enqueue_at(first_hash, first, started_at + Duration::from_micros(7))
                .unwrap(),
            AssetAdmission::AlreadyKnown { .. }
        ));

        let second = b"second pending source".to_vec();
        let second_hash = content_hash(&second);
        store
            .enqueue_at(second_hash, second, started_at + Duration::from_micros(3))
            .unwrap();
        let sampled_at = started_at + Duration::from_micros(13);
        assert_eq!(
            store.stats_at(sampled_at).oldest_pending_import_age_micros,
            Some(13)
        );

        assert_eq!(store.evict(first_hash).removed_pending_imports, 1);
        assert_eq!(
            store.stats_at(sampled_at).oldest_pending_import_age_micros,
            Some(10)
        );
        store.process_next().unwrap();
        assert_eq!(
            store.stats_at(sampled_at).oldest_pending_import_age_micros,
            None
        );
        assert_eq!(duration_micros(Duration::MAX), u64::MAX);
    }

    #[test]
    fn rejected_import_does_not_change_oldest_age() {
        let mut config = AssetStoreConfig::default();
        config.limits.max_pending_imports = NonZeroU32::new(1).unwrap();
        let started_at = Instant::now();
        let mut store = AssetStore::new(config);
        let first = b"only pending source".to_vec();
        store
            .enqueue_at(content_hash(&first), first, started_at)
            .unwrap();

        let rejected = b"rejected pending source".to_vec();
        assert!(matches!(
            store.enqueue_at(
                content_hash(&rejected),
                rejected,
                started_at + Duration::from_micros(5)
            ),
            Err(AssetError::ImportCapacityExceeded { capacity: 1 })
        ));
        assert_eq!(
            store
                .stats_at(started_at + Duration::from_micros(9))
                .oldest_pending_import_age_micros,
            Some(9)
        );
    }
}
