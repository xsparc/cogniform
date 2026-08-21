//! Local command-line composition root for Cogniform.

use std::{env, ffi::OsStr, io, process::ExitCode};

const LOCAL_PROFILE_WIDTH: u32 = 64;
const LOCAL_PROFILE_HEIGHT: u32 = 64;

mod asset;
mod measure;
mod profile;
mod recovery;
mod scenario;
mod serve_stdio;

use profile::LocalProfile;

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
            let output = match arguments.next() {
                None => scenario::ScenarioOutput::Human,
                Some(argument) if argument == OsStr::new("--json") => {
                    scenario::ScenarioOutput::Json
                }
                Some(_) => return Err(invalid_input("scenario accepts only optional --json")),
            };
            if arguments.next().is_some() {
                return Err(invalid_input("scenario accepts only optional --json"));
            }
            scenario::run(output)
        }
        Some(command) if command == OsStr::new("measure-world") => {
            let output = match arguments.next() {
                None => measure::MeasureOutput::Human,
                Some(argument) if argument == OsStr::new("--json") => measure::MeasureOutput::Json,
                Some(_) => {
                    return Err(invalid_input("measure-world accepts only optional --json"));
                }
            };
            if arguments.next().is_some() {
                return Err(invalid_input("measure-world accepts only optional --json"));
            }
            measure::run(output)
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
        Some(command) if command == OsStr::new("inspect-asset") => {
            let first = arguments.next().ok_or_else(|| {
                invalid_input("inspect-asset requires one content hash and one path")
            })?;
            let (output, encoded_hash) = if first == OsStr::new("--json") {
                (
                    asset::AssetOutput::Json,
                    arguments.next().ok_or_else(|| {
                        invalid_input("inspect-asset requires one content hash and one path")
                    })?,
                )
            } else {
                (asset::AssetOutput::Human, first)
            };
            let path = arguments.next().ok_or_else(|| {
                invalid_input("inspect-asset requires one content hash and one path")
            })?;
            if arguments.next().is_some() {
                return Err(invalid_input(
                    "inspect-asset accepts optional --json, one content hash, and one path",
                ));
            }
            let expected_hash = asset::parse_content_hash(&encoded_hash)?;
            asset::run(expected_hash, &path, output)
        }
        Some(command) if command == OsStr::new("serve-stdio") => run_binary_stdio(&mut arguments),
        Some(command) if command == OsStr::new("serve-mcp-stdio") => run_mcp_stdio(&mut arguments),
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

fn run_binary_stdio(arguments: &mut env::ArgsOs) -> Result<(), Box<dyn std::error::Error>> {
    let profile = LocalProfile::parse(arguments, "serve-stdio")?;
    serve_stdio::run(profile)?;
    Ok(())
}

fn run_mcp_stdio(arguments: &mut env::ArgsOs) -> Result<(), Box<dyn std::error::Error>> {
    let profile = LocalProfile::parse(arguments, "serve-mcp-stdio")?;
    let (width, height) = profile.dimensions();
    cogniform_mcp::run_stdio(cogniform_mcp::McpServerConfig::local_profile(width, height))?;
    Ok(())
}

fn print_usage() {
    println!("Cogniform local headless engine");
    println!();
    println!("Usage:");
    println!("  cogniform-cli scenario [--json]  Run the canonical unattended MVP scenario");
    println!("  cogniform-cli measure-world [--json]  Measure the controlled CPU world fixture");
    println!("  cogniform-cli inspect-recovery [--json] <path>  Verify an immutable recovery file");
    println!(
        "  cogniform-cli inspect-asset [--json] <content-hash> <path>  Verify an immutable asset source file"
    );
    println!(
        "  cogniform-cli serve-stdio [--profile <name>]  Run one bounded binary session over redirected stdio"
    );
    println!(
        "  cogniform-cli serve-mcp-stdio [--profile <name>]  Run one bounded MCP session over redirected stdio"
    );
    println!("  cogniform-cli --help    Show this help");
    println!();
    println!("Profiles:");
    for profile in LocalProfile::ALL {
        let default = if profile == LocalProfile::DEFAULT {
            " (default)"
        } else {
            ""
        };
        println!("  {}{default}", profile.name());
    }
}

fn invalid_input(message: impl Into<String>) -> Box<dyn std::error::Error> {
    Box::new(io::Error::new(io::ErrorKind::InvalidInput, message.into()))
}
