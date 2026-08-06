use std::collections::{BTreeMap, VecDeque};

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
        ))
    }

    /// Returns aggregate occupancy without exposing admitted source content.
    #[must_use]
    pub fn stats(&self) -> AssetStoreStats {
        AssetStoreStats {
            records: u32::try_from(self.records.len()).unwrap_or(u32::MAX),
            pending_imports: u32::try_from(self.pending.len()).unwrap_or(u32::MAX),
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
