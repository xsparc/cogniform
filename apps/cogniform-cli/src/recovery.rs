use std::{
    ffi::OsStr,
    io::{self, Write},
    path::Path,
};

use cogniform_engine::{EngineConfig, RecoveryInspection, inspect_recovery_point};
use cogniform_storage::RecoveryFileStore;
use serde::Serialize;

use crate::{LOCAL_PROFILE_HEIGHT, LOCAL_PROFILE_WIDTH};

const SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RecoveryOutput {
    Human,
    Json,
}

pub(crate) fn run(path: &OsStr, output: RecoveryOutput) -> Result<(), Box<dyn std::error::Error>> {
    let config = EngineConfig::new(LOCAL_PROFILE_WIDTH, LOCAL_PROFILE_HEIGHT);
    let store = RecoveryFileStore::new(config.replay)?;
    let recovery = store.load(Path::new(path))?;
    let inspection = inspect_recovery_point(&config, &recovery)?;
    let profile = format!("default-local-{LOCAL_PROFILE_WIDTH}x{LOCAL_PROFILE_HEIGHT}");

    match output {
        RecoveryOutput::Human => print_human(&profile, inspection),
        RecoveryOutput::Json => print_json(&profile, inspection)?,
    }
    Ok(())
}

fn print_human(profile: &str, inspection: RecoveryInspection) {
    println!("Cogniform recovery inspection passed");
    println!("profile: {profile}");
    println!("replay entries: {}", inspection.replay_entries());
    println!("replay bytes: {}", inspection.replay_bytes());
    println!("scene revision: {}", inspection.scene_revision().get());
    println!("next frame: {}", inspection.next_frame_id().get());
    println!("logical hash: {}", inspection.logical_hash());
    println!("final entry hash: {}", inspection.final_entry_hash());
}

fn print_json(
    profile: &str,
    inspection: RecoveryInspection,
) -> Result<(), Box<dyn std::error::Error>> {
    let report = RecoveryInspectionReport {
        schema_version: SCHEMA_VERSION,
        profile,
        replay_entries: inspection.replay_entries(),
        replay_bytes: inspection.replay_bytes(),
        scene_revision: inspection.scene_revision().get(),
        next_frame: inspection.next_frame_id().get(),
        logical_hash: inspection.logical_hash().to_string(),
        final_entry_hash: inspection.final_entry_hash().to_string(),
    };
    let mut stdout = io::stdout().lock();
    serde_json::to_writer(&mut stdout, &report)?;
    stdout.write_all(b"\n")?;
    Ok(())
}

#[derive(Serialize)]
struct RecoveryInspectionReport<'a> {
    schema_version: u32,
    profile: &'a str,
    replay_entries: u32,
    replay_bytes: u64,
    scene_revision: u64,
    next_frame: u64,
    logical_hash: String,
    final_entry_hash: String,
}
