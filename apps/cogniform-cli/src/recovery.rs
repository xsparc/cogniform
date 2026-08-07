use std::{ffi::OsStr, path::Path};

use cogniform_engine::{EngineConfig, inspect_recovery_point};
use cogniform_storage::RecoveryFileStore;

use crate::{LOCAL_PROFILE_HEIGHT, LOCAL_PROFILE_WIDTH};

pub(crate) fn run(path: &OsStr) -> Result<(), Box<dyn std::error::Error>> {
    let config = EngineConfig::new(LOCAL_PROFILE_WIDTH, LOCAL_PROFILE_HEIGHT);
    let store = RecoveryFileStore::new(config.replay)?;
    let recovery = store.load(Path::new(path))?;
    let inspection = inspect_recovery_point(&config, &recovery)?;

    println!("Cogniform recovery inspection passed");
    println!("profile: default-local-{LOCAL_PROFILE_WIDTH}x{LOCAL_PROFILE_HEIGHT}");
    println!("replay entries: {}", inspection.replay_entries());
    println!("replay bytes: {}", inspection.replay_bytes());
    println!("scene revision: {}", inspection.scene_revision().get());
    println!("next frame: {}", inspection.next_frame_id().get());
    println!("logical hash: {}", inspection.logical_hash());
    println!("final entry hash: {}", inspection.final_entry_hash());
    Ok(())
}
