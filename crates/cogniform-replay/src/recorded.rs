use cogniform_protocol::{ApplyReceipt, FrameId, RenderExtraction, ScenePatch};
use cogniform_world::{AuthoritativeWorld, WorldConfig, WorldExtractionError};

use crate::{
    RecordedApplyError, ReplayConfig, ReplayConfigError, ReplayEntry, ReplayError, ReplayLog,
    log::{ENTRY_ENVELOPE_BYTES, ReplayEntryMetadata},
};

/// Authoritative world wrapper that records every newly accepted patch.
///
/// The wrapped world is intentionally exposed read-only so accepted state and
/// append-only evidence cannot diverge through an unrecorded mutation.
pub struct RecordedWorld {
    world: AuthoritativeWorld,
    world_config: WorldConfig,
    log: ReplayLog,
}

impl RecordedWorld {
    /// Creates an empty recorded world with explicit world and replay bounds.
    pub fn new(
        world_config: WorldConfig,
        replay_config: ReplayConfig,
    ) -> Result<Self, ReplayConfigError> {
        replay_config.validate()?;
        Ok(Self {
            world: AuthoritativeWorld::new(world_config),
            world_config,
            log: ReplayLog::new(replay_config),
        })
    }

    /// Reconstructs authoritative state from one complete verified replay log.
    ///
    /// The supplied log remains the append-only evidence for later accepted
    /// patches. Callers that loaded bytes must reject any unverified tail before
    /// invoking this method.
    pub fn restore(world_config: WorldConfig, log: ReplayLog) -> Result<Self, ReplayError> {
        let world = log.replay(world_config)?;
        Ok(Self {
            world,
            world_config,
            log,
        })
    }

    /// Returns the authoritative world through a read-only reference.
    #[must_use]
    pub const fn world(&self) -> &AuthoritativeWorld {
        &self.world
    }

    /// Returns immutable append-only replay evidence.
    #[must_use]
    pub const fn log(&self) -> &ReplayLog {
        &self.log
    }

    /// Drains the next compact renderer extraction without exposing mutable world state.
    pub fn take_render_extraction(&mut self) -> Result<RenderExtraction, WorldExtractionError> {
        self.world.take_render_extraction()
    }

    /// Replays every accepted entry into a fresh world with the original bounds.
    pub fn replay(&self) -> Result<AuthoritativeWorld, ReplayError> {
        self.log.replay(self.world_config)
    }

    /// Applies a patch atomically and records it exactly once when newly accepted.
    ///
    /// Canonical encoding and all log capacity checks happen before mutation.
    /// Repeating an accepted idempotency key returns the world's recorded
    /// receipt and does not append another replay entry.
    pub fn apply_patch(
        &mut self,
        patch: &ScenePatch,
        estimated_visible_frame: FrameId,
    ) -> Result<ApplyReceipt, RecordedApplyError> {
        if self
            .world
            .recorded_transaction(patch.idempotency_key)
            .is_some()
        {
            return self
                .world
                .apply_patch(patch, estimated_visible_frame)
                .map_err(RecordedApplyError::World);
        }

        let patch_bytes = patch
            .to_canonical_json(&self.world_config.runtime_limits)
            .map_err(RecordedApplyError::Codec)?;
        let config = self.log.config();
        if self.log.len() >= usize::try_from(config.max_entries.get()).expect("u32 fits usize") {
            return Err(RecordedApplyError::EntryCapacityExceeded {
                limit: config.max_entries.get(),
            });
        }
        let entry_bytes = ENTRY_ENVELOPE_BYTES.saturating_add(patch_bytes.len() as u64);
        if entry_bytes > u64::from(config.max_entry_bytes.get()) {
            return Err(RecordedApplyError::EntrySizeExceeded {
                actual: entry_bytes,
                limit: config.max_entry_bytes.get(),
            });
        }
        let new_log_bytes = self.log.encoded_len().saturating_add(entry_bytes);
        if new_log_bytes > u64::from(config.max_log_bytes.get()) {
            return Err(RecordedApplyError::LogSizeExceeded {
                actual: new_log_bytes,
                limit: config.max_log_bytes.get(),
            });
        }

        let previous_revision = self.world.revision();
        let previous_scene_hash = self
            .world
            .logical_hash()
            .map_err(RecordedApplyError::Invariant)?;
        let receipt = self
            .world
            .apply_patch(patch, estimated_visible_frame)
            .map_err(RecordedApplyError::World)?;
        let new_scene_hash = self
            .world
            .logical_hash()
            .map_err(RecordedApplyError::Invariant)?;
        let sequence =
            u64::try_from(self.log.len()).expect("configured replay entry count fits u64") + 1;
        let entry = ReplayEntry::new(
            patch.clone(),
            patch_bytes,
            ReplayEntryMetadata {
                sequence,
                estimated_visible_frame,
                previous_revision,
                new_revision: receipt.new_revision,
                previous_scene_hash,
                new_scene_hash,
                previous_entry_hash: self.log.last_entry_hash(),
            },
        );
        debug_assert_eq!(entry.encoded_bytes(), entry_bytes);
        self.log.append(entry);
        Ok(receipt)
    }

    /// Splits the authoritative world from its append-only replay log.
    #[must_use]
    pub fn into_parts(self) -> (AuthoritativeWorld, ReplayLog) {
        (self.world, self.log)
    }
}

impl Default for RecordedWorld {
    fn default() -> Self {
        Self::new(WorldConfig::default(), ReplayConfig::default())
            .expect("default replay configuration is valid")
    }
}
