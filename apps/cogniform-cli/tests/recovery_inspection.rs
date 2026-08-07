//! Black-box coverage for the read-only recovery inspection command.

use core::sync::atomic::{AtomicU64, Ordering};
use std::{
    fs, io,
    path::{Path, PathBuf},
    process::Command,
};

use cogniform_engine::{EngineConfig, EngineRecoveryPoint, inspect_recovery_point};
use cogniform_protocol::FrameId;
use cogniform_storage::RecoveryFileStore;

static NEXT_DIRECTORY: AtomicU64 = AtomicU64::new(1);

#[test]
fn valid_recovery_is_verified_without_gpu_or_path_output() {
    let directory = TestDirectory::new("valid");
    let path = directory.path().join("private-recovery.cnf");
    let config = EngineConfig::new(64, 64);
    let recovery = empty_recovery(7);
    RecoveryFileStore::new(config.replay)
        .unwrap()
        .create_new(&path, &recovery)
        .unwrap();
    let inspection = inspect_recovery_point(&config, &recovery).unwrap();
    let before = fs::read(&path).unwrap();

    let output = command()
        .arg("inspect-recovery")
        .arg(&path)
        .output()
        .unwrap();
    assert!(output.status.success());
    assert!(output.stderr.is_empty());
    let stdout = String::from_utf8(output.stdout).unwrap();
    let expected = format!(
        "Cogniform recovery inspection passed\n\
         profile: default-local-64x64\n\
         replay entries: 0\n\
         replay bytes: {}\n\
         scene revision: 0\n\
         next frame: 7\n\
         logical hash: {}\n\
         final entry hash: {}\n",
        inspection.replay_bytes(),
        inspection.logical_hash(),
        inspection.final_entry_hash(),
    );
    assert_eq!(stdout.replace("\r\n", "\n"), expected);
    assert!(!stdout.contains("private-recovery"));
    assert_eq!(fs::read(&path).unwrap(), before);
}

#[test]
fn semantic_replay_failure_is_nonzero_and_path_redacted() {
    let directory = TestDirectory::new("semantic");
    let path = directory.path().join("secret-semantic-marker.cnf");
    let config = EngineConfig::new(64, 64);
    let recovery = EngineRecoveryPoint::from_parts(
        b"integrity-valid-but-not-replay".to_vec(),
        FrameId::new(1).unwrap(),
    );
    RecoveryFileStore::new(config.replay)
        .unwrap()
        .create_new(&path, &recovery)
        .unwrap();

    let output = command()
        .arg("inspect-recovery")
        .arg(&path)
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("accepted-event replay failed"));
    assert!(!stderr.contains("secret-semantic-marker"));
    assert!(!stderr.contains("integrity-valid-but-not-replay"));
}

#[test]
fn envelope_corruption_and_non_file_targets_are_path_redacted() {
    let directory = TestDirectory::new("storage");
    let path = directory.path().join("secret-corruption-marker.cnf");
    let config = EngineConfig::new(64, 64);
    RecoveryFileStore::new(config.replay)
        .unwrap()
        .create_new(&path, &empty_recovery(1))
        .unwrap();
    let mut bytes = fs::read(&path).unwrap();
    let last = bytes.len() - 1;
    bytes[last] ^= 1;
    fs::write(&path, bytes).unwrap();

    let corrupted = command()
        .arg("inspect-recovery")
        .arg(&path)
        .output()
        .unwrap();
    assert!(!corrupted.status.success());
    assert!(corrupted.stdout.is_empty());
    let stderr = String::from_utf8(corrupted.stderr).unwrap();
    assert!(stderr.contains("integrity check failed"));
    assert!(!stderr.contains("secret-corruption-marker"));

    let truncated_path = directory.path().join("secret-truncated-marker.cnf");
    let corrupted_bytes = fs::read(&path).unwrap();
    fs::write(&truncated_path, &corrupted_bytes[..16]).unwrap();
    let truncated = command()
        .arg("inspect-recovery")
        .arg(&truncated_path)
        .output()
        .unwrap();
    assert!(!truncated.status.success());
    assert!(truncated.stdout.is_empty());
    assert!(
        !String::from_utf8(truncated.stderr)
            .unwrap()
            .contains("secret-truncated-marker")
    );

    let non_file = command()
        .arg("inspect-recovery")
        .arg(directory.path())
        .output()
        .unwrap();
    assert!(!non_file.status.success());
    assert!(non_file.stdout.is_empty());
    let stderr = String::from_utf8(non_file.stderr).unwrap();
    assert!(stderr.contains("not a regular file"));
    assert!(!stderr.contains(directory.path().to_string_lossy().as_ref()));
}

#[test]
fn inspection_arguments_and_help_are_exact() {
    let missing = command().arg("inspect-recovery").output().unwrap();
    assert!(!missing.status.success());
    assert!(missing.stdout.is_empty());
    assert_eq!(
        String::from_utf8(missing.stderr)
            .unwrap()
            .replace("\r\n", "\n"),
        "error: inspect-recovery requires one path\n"
    );

    let extra = command()
        .args(["inspect-recovery", "first", "second"])
        .output()
        .unwrap();
    assert!(!extra.status.success());
    assert!(extra.stdout.is_empty());
    assert_eq!(
        String::from_utf8(extra.stderr)
            .unwrap()
            .replace("\r\n", "\n"),
        "error: inspect-recovery accepts exactly one path\n"
    );

    let help = command().arg("--help").output().unwrap();
    assert!(help.status.success());
    assert!(help.stderr.is_empty());
    assert_eq!(
        String::from_utf8(help.stdout)
            .unwrap()
            .replace("\r\n", "\n"),
        concat!(
            "Cogniform local headless engine\n\n",
            "Usage:\n",
            "  cogniform-cli scenario  Run the canonical unattended MVP scenario\n",
            "  cogniform-cli measure-world  Measure the controlled CPU world fixture\n",
            "  cogniform-cli inspect-recovery <path>  Verify an immutable recovery file\n",
            "  cogniform-cli --help    Show this help\n",
        )
    );
}

fn command() -> Command {
    Command::new(env!("CARGO_BIN_EXE_cogniform-cli"))
}

fn empty_recovery(next_frame: u64) -> EngineRecoveryPoint {
    EngineRecoveryPoint::from_parts(b"CNFRPL1\n".to_vec(), FrameId::new(next_frame).unwrap())
}

struct TestDirectory {
    path: PathBuf,
}

impl TestDirectory {
    fn new(label: &str) -> Self {
        loop {
            let unique = NEXT_DIRECTORY.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "cogniform-cli-{label}-{}-{unique}",
                std::process::id()
            ));
            match fs::create_dir(&path) {
                Ok(()) => return Self { path },
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
                Err(error) => panic!("failed to create CLI test directory: {error:?}"),
            }
        }
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TestDirectory {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.path).unwrap();
    }
}
