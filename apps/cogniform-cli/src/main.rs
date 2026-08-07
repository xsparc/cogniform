//! Local command-line composition root for Cogniform.

use std::{env, ffi::OsStr, io, process::ExitCode};

use cogniform_engine::{
    CanonicalScenarioConfig, LocalService, LocalServiceConfig, run_canonical_scenario,
};

const LOCAL_PROFILE_WIDTH: u32 = 64;
const LOCAL_PROFILE_HEIGHT: u32 = 64;

mod measure;
mod recovery;

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let mut arguments = env::args_os();
    let _program = arguments.next();
    match arguments.next() {
        None => {
            print_usage();
            Ok(())
        }
        Some(command) if command == OsStr::new("scenario") => {
            if arguments.next().is_some() {
                return Err(invalid_input("scenario accepts no arguments"));
            }
            run_scenario()
        }
        Some(command) if command == OsStr::new("measure-world") => {
            if arguments.next().is_some() {
                return Err(invalid_input("measure-world accepts no arguments"));
            }
            measure::run()
        }
        Some(command) if command == OsStr::new("inspect-recovery") => {
            let first = arguments
                .next()
                .ok_or_else(|| invalid_input("inspect-recovery requires one path"))?;
            let (output, candidate) = if first == OsStr::new("--json") {
                (
                    recovery::RecoveryOutput::Json,
                    arguments
                        .next()
                        .ok_or_else(|| invalid_input("inspect-recovery requires one path"))?,
                )
            } else {
                (recovery::RecoveryOutput::Human, first)
            };
            let path = if candidate == OsStr::new("--") {
                arguments
                    .next()
                    .ok_or_else(|| invalid_input("inspect-recovery requires one path"))?
            } else {
                candidate
            };
            if arguments.next().is_some() {
                return Err(invalid_input("inspect-recovery accepts exactly one path"));
            }
            recovery::run(&path, output)
        }
        Some(command) if command == OsStr::new("help") || command == OsStr::new("--help") => {
            if arguments.next().is_some() {
                return Err(invalid_input("help accepts no arguments"));
            }
            print_usage();
            Ok(())
        }
        Some(_) => Err(invalid_input("unknown command; run with --help for usage")),
    }
}

fn run_scenario() -> Result<(), Box<dyn std::error::Error>> {
    let mut service = pollster::block_on(LocalService::new(LocalServiceConfig::new(
        LOCAL_PROFILE_WIDTH,
        LOCAL_PROFILE_HEIGHT,
    )))?;
    let adapter = service.adapter().clone();
    let report = run_canonical_scenario(&mut service, CanonicalScenarioConfig::default())?;

    println!("Cogniform canonical scenario passed");
    println!("adapter: {}", adapter.name);
    println!("backend: {}", adapter.backend);
    println!("device type: {}", adapter.device_type);
    println!("WebGPU compliant: {}", adapter.webgpu_compliant);
    println!("revision: {}", report.update_receipt.new_revision.get());
    println!("entities: {}", report.queried_entities);
    println!("table: {}", report.table_id);
    println!("camera: {}", report.camera_id);
    println!("color frame: {}", report.color.frame_id.get());
    println!("entity-ID frame: {}", report.entity_id.frame_id.get());
    println!("visibility frame: {}", report.visibility.frame_id.get());
    println!(
        "center color: #{:02x}{:02x}{:02x}{:02x}",
        report.center_color[0],
        report.center_color[1],
        report.center_color[2],
        report.center_color[3]
    );
    println!("center entity: {}", report.center_entity_id);
    println!("table visible pixels: {}", report.table_visible_pixels);
    println!("logical hash: {}", report.logical_hash);
    println!("replayed logical hash: {}", report.replayed_logical_hash);
    println!("replay entries: {}", report.replay.entry_count());
    println!("replay bytes: {}", report.replay_bytes);
    Ok(())
}

fn print_usage() {
    println!("Cogniform local headless engine");
    println!();
    println!("Usage:");
    println!("  cogniform-cli scenario  Run the canonical unattended MVP scenario");
    println!("  cogniform-cli measure-world  Measure the controlled CPU world fixture");
    println!("  cogniform-cli inspect-recovery [--json] <path>  Verify an immutable recovery file");
    println!("  cogniform-cli --help    Show this help");
}

fn invalid_input(message: impl Into<String>) -> Box<dyn std::error::Error> {
    Box::new(io::Error::new(io::ErrorKind::InvalidInput, message.into()))
}
