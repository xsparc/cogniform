//! Black-box coverage for read-only exact-hash asset-source inspection.

use core::sync::atomic::{AtomicU64, Ordering};
use std::{
    fs, io,
    path::{Path, PathBuf},
    process::{Command, Output},
};

use cogniform_engine::content_hash;

static NEXT_DIRECTORY: AtomicU64 = AtomicU64::new(1);

#[test]
fn valid_asset_is_verified_without_mutation_or_payload_output() {
    let directory = TestDirectory::new("valid");
    let path = directory.path().join("private-asset.glb");
    let source = b"private exact asset source";
    fs::write(&path, source).unwrap();
    let hash = content_hash(source);
    let before = fs::read(&path).unwrap();

    let output = command()
        .arg("inspect-asset")
        .arg(hash.to_string())
        .arg(&path)
        .output()
        .unwrap();
    assert_success(&output);
    assert_eq!(
        normalize(output.stdout),
        format!(
            "Cogniform asset source inspection passed\ncontent hash: {hash}\nsource bytes: {}\n",
            source.len()
        )
    );
    assert_eq!(fs::read(&path).unwrap(), before);
}

#[test]
fn option_like_path_is_an_ordinary_asset_path() {
    let directory = TestDirectory::new("option-path");
    let path = directory.path().join("--json");
    let source = b"option-like path fixture";
    fs::write(&path, source).unwrap();
    let hash = content_hash(source);

    let output = command()
        .current_dir(directory.path())
        .args(["inspect-asset", &hash.to_string(), "--json"])
        .output()
        .unwrap();
    assert_success(&output);
    assert_eq!(fs::read(path).unwrap(), source);
}

#[test]
fn mismatch_and_non_file_failures_are_path_and_payload_redacted() {
    let directory = TestDirectory::new("redaction");
    let marker = "private-asset-payload-marker";
    let path = directory.path().join("private-asset-path-marker.glb");
    fs::write(&path, marker).unwrap();
    let expected = content_hash(b"different expected source");

    let mismatch = command()
        .arg("inspect-asset")
        .arg(expected.to_string())
        .arg(&path)
        .output()
        .unwrap();
    assert_failure_redacted(mismatch, directory.marker(), marker);

    let non_file = command()
        .arg("inspect-asset")
        .arg(content_hash(b"").to_string())
        .arg(directory.path())
        .output()
        .unwrap();
    let stderr = assert_failure_redacted(non_file, directory.marker(), marker);
    assert_eq!(
        stderr,
        "error: asset file load target is not a regular file\n"
    );
}

#[test]
fn arguments_are_exact_and_reject_before_file_work() {
    for arguments in [&["inspect-asset"][..], &["inspect-asset", valid_hash()][..]] {
        let output = command().args(arguments).output().unwrap();
        assert!(!output.status.success());
        assert!(output.stdout.is_empty());
        assert_eq!(
            normalize(output.stderr),
            "error: inspect-asset requires one content hash and one path\n"
        );
    }

    for invalid_hash in ["invalid", &valid_hash().to_uppercase()] {
        let output = command()
            .args(["inspect-asset", invalid_hash, "missing-path-marker"])
            .output()
            .unwrap();
        assert!(!output.status.success());
        assert!(output.stdout.is_empty());
        assert_eq!(
            normalize(output.stderr),
            "error: inspect-asset content hash must be 64 lowercase hexadecimal characters\n"
        );
    }

    let extra = command()
        .args(["inspect-asset", valid_hash(), "first", "second"])
        .output()
        .unwrap();
    assert!(!extra.status.success());
    assert!(extra.stdout.is_empty());
    assert_eq!(
        normalize(extra.stderr),
        "error: inspect-asset accepts one content hash and one path\n"
    );
}

fn assert_success(output: &Output) {
    assert!(
        output.status.success(),
        "{}",
        normalize(output.stderr.clone())
    );
    assert!(output.stderr.is_empty());
}

fn assert_failure_redacted(output: Output, path_marker: &str, payload_marker: &str) -> String {
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    let stderr = normalize(output.stderr);
    assert!(!stderr.contains(path_marker));
    assert!(!stderr.contains(payload_marker));
    stderr
}

fn valid_hash() -> &'static str {
    "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
}

fn normalize(bytes: Vec<u8>) -> String {
    String::from_utf8(bytes).unwrap().replace("\r\n", "\n")
}

fn command() -> Command {
    Command::new(env!("CARGO_BIN_EXE_cogniform-cli"))
}

struct TestDirectory {
    path: PathBuf,
    marker: String,
}

impl TestDirectory {
    fn new(label: &str) -> Self {
        loop {
            let marker = format!(
                "cogniform-cli-asset-{label}-{}-{}",
                std::process::id(),
                NEXT_DIRECTORY.fetch_add(1, Ordering::Relaxed)
            );
            let path = std::env::temp_dir().join(&marker);
            match fs::create_dir(&path) {
                Ok(()) => return Self { path, marker },
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
                Err(error) => panic!("failed to create CLI test directory: {error:?}"),
            }
        }
    }

    fn path(&self) -> &Path {
        &self.path
    }

    fn marker(&self) -> &str {
        &self.marker
    }
}

impl Drop for TestDirectory {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.path).unwrap();
    }
}
