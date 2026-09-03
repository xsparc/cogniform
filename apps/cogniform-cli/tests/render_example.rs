//! Black-box coverage for the local rendered-observation example.

use core::sync::atomic::{AtomicU64, Ordering};
use std::{
    fs, io,
    io::BufReader,
    path::{Path, PathBuf},
    process::{Command, Output},
};

use serde_json::{Value, json};

static NEXT_DIRECTORY: AtomicU64 = AtomicU64::new(1);

#[test]
fn arguments_and_create_new_target_are_checked_before_gpu_selection() {
    let output = command().arg("render-example").arg("").output().unwrap();
    assert_failure(
        output,
        "error: render-example output directory must not be empty\n",
    );

    for arguments in [
        &["render-example"][..],
        &["render-example", "first", "second"][..],
        &["render-example", "--"][..],
    ] {
        let output = command().args(arguments).output().unwrap();
        assert!(!output.status.success());
        assert!(output.stdout.is_empty());
    }

    let directory = TestDirectory::new("preflight");
    let existing_directory = directory.path().join("existing-directory");
    fs::create_dir(&existing_directory).unwrap();
    let existing_file = directory.path().join("existing-file");
    fs::write(&existing_file, b"preserve me").unwrap();

    for target in [&existing_directory, &existing_file] {
        let output = command()
            .arg("render-example")
            .arg(target)
            .output()
            .unwrap();
        assert_failure(
            output,
            "error: render-example output directory already exists\n",
        );
    }
    assert_eq!(fs::read(&existing_file).unwrap(), b"preserve me");

    let output = command()
        .arg("render-example")
        .arg(existing_file.join("example"))
        .output()
        .unwrap();
    assert_failure(
        output,
        "error: render-example output parent is not a directory\n",
    );

    assert_symlink_targets_are_rejected(&directory, &existing_directory);

    let option_like = directory.path().join("--existing");
    fs::create_dir(&option_like).unwrap();
    let output = command()
        .current_dir(directory.path())
        .args(["render-example", "--", "--existing"])
        .output()
        .unwrap();
    assert_failure(
        output,
        "error: render-example output directory already exists\n",
    );

    let output = command()
        .arg("render-example")
        .arg(directory.path().join("missing-parent").join("example"))
        .output()
        .unwrap();
    assert_failure(
        output,
        "error: render-example output parent directory does not exist\n",
    );
}

#[test]
#[ignore = "requires a controlled headless GPU adapter"]
fn command_writes_one_complete_same_frame_example_set_without_overwrite() {
    let directory = TestDirectory::new("render");
    let output_directory = directory.path().join("example");
    let output = command()
        .arg("render-example")
        .arg(&output_directory)
        .output()
        .unwrap();
    assert_success(&output);
    assert_eq!(
        normalize(output.stdout.clone()),
        concat!(
            "Cogniform rendered example created\n",
            "output files: color.png, depth.png, normals.png, identity.png, manifest.json\n",
        )
    );

    let mut names = fs::read_dir(&output_directory)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().into_string().unwrap())
        .collect::<Vec<_>>();
    names.sort();
    assert_eq!(
        names,
        [
            "color.png",
            "depth.png",
            "identity.png",
            "manifest.json",
            "normals.png",
        ]
    );

    assert_pngs(&output_directory);
    assert_manifest(&output_directory);

    let before = fs::read(output_directory.join("manifest.json")).unwrap();
    let repeated = command()
        .arg("render-example")
        .arg(&output_directory)
        .output()
        .unwrap();
    assert_failure(
        repeated,
        "error: render-example output directory already exists\n",
    );
    assert_eq!(
        fs::read(output_directory.join("manifest.json")).unwrap(),
        before
    );
}

fn assert_pngs(output_directory: &Path) {
    let _ = assert_png(&output_directory.join("color.png"), png::ColorType::Rgba);
    let _ = assert_png(
        &output_directory.join("depth.png"),
        png::ColorType::Grayscale,
    );
    let _ = assert_png(&output_directory.join("normals.png"), png::ColorType::Rgba);
    let identity_pixels = assert_png(&output_directory.join("identity.png"), png::ColorType::Rgba);
    let mut opaque_colors = identity_pixels
        .chunks_exact(4)
        .filter(|pixel| pixel[3] == 255)
        .map(|pixel| [pixel[0], pixel[1], pixel[2], pixel[3]])
        .collect::<Vec<_>>();
    opaque_colors.sort_unstable();
    opaque_colors.dedup();
    assert_eq!(opaque_colors, [[225, 73, 101, 255]]);
    assert!(
        identity_pixels
            .chunks_exact(4)
            .any(|pixel| pixel == [0, 0, 0, 0])
    );
}

fn assert_manifest(output_directory: &Path) {
    let manifest_bytes = fs::read(output_directory.join("manifest.json")).unwrap();
    assert!(manifest_bytes.ends_with(b"\n"));
    assert!(!manifest_bytes[..manifest_bytes.len() - 1].contains(&b'\n'));
    let manifest: Value = serde_json::from_slice(&manifest_bytes).unwrap();
    assert_eq!(manifest["schema_version"], 1);
    assert_eq!(manifest["example"], "headless-reference-v1");
    assert_eq!(manifest["width"], 64);
    assert_eq!(manifest["height"], 64);
    assert_eq!(manifest["diagnostic_only"], true);
    assert_eq!(
        manifest["files"],
        json!([
            {
                "name": "color.png",
                "observation": "color",
                "visualization": "linear-rgba8-source-values"
            },
            {
                "name": "depth.png",
                "observation": "depth",
                "visualization": "near-white-far-black-grayscale8"
            },
            {
                "name": "normals.png",
                "observation": "normal",
                "visualization": "world-xyz-remapped-to-rgb-transparent-background"
            },
            {
                "name": "identity.png",
                "observation": "entity_id",
                "visualization": "deterministic-palette-transparent-background"
            }
        ])
    );
    assert_eq!(
        manifest["identities"],
        json!([{
            "entity_id": "00000000000000000000000000000007",
            "color": "#e14965"
        }])
    );
    assert!(manifest["adapter"]["name"].is_string());
    assert!(manifest["adapter"]["backend"].is_string());
    assert!(manifest["adapter"]["device_type"].is_string());
    assert!(manifest["adapter"]["webgpu_compliant"].is_boolean());
    assert_eq!(manifest["frame"]["frame_id"], 1);
    assert_eq!(manifest["frame"]["scene_revision"], 0);
    assert_eq!(
        manifest["frame"]["camera_id"],
        "00000000000000000000000000000001"
    );
    assert_eq!(manifest["frame"]["extraction_generation"], 0);
}

fn assert_png(path: &Path, expected_color: png::ColorType) -> Vec<u8> {
    let decoder = png::Decoder::new(BufReader::new(fs::File::open(path).unwrap()));
    let mut reader = decoder.read_info().unwrap();
    let output_size = reader.output_buffer_size().unwrap();
    let mut pixels = vec![0; output_size];
    let info = reader.next_frame(&mut pixels).unwrap();
    assert_eq!(info.width, 64);
    assert_eq!(info.height, 64);
    assert_eq!(info.color_type, expected_color);
    assert_eq!(info.bit_depth, png::BitDepth::Eight);
    assert_eq!(info.buffer_size(), output_size);
    pixels
}

fn assert_symlink_targets_are_rejected(directory: &TestDirectory, live_target: &Path) {
    let live_link = directory.path().join("live-target-link");
    if create_directory_symlink(live_target, &live_link).is_ok() {
        let output = command()
            .arg("render-example")
            .arg(&live_link)
            .output()
            .unwrap();
        assert_failure(
            output,
            "error: render-example output directory already exists\n",
        );
        remove_directory_symlink(&live_link).unwrap();
    }

    let broken_link = directory.path().join("broken-target-link");
    let missing_target = directory.path().join("missing-link-target");
    if create_directory_symlink(&missing_target, &broken_link).is_ok() {
        let output = command()
            .arg("render-example")
            .arg(&broken_link)
            .output()
            .unwrap();
        assert_failure(
            output,
            "error: render-example output directory already exists\n",
        );
        remove_directory_symlink(&broken_link).unwrap();
    }
}

#[cfg(unix)]
fn create_directory_symlink(target: &Path, link: &Path) -> io::Result<()> {
    std::os::unix::fs::symlink(target, link)
}

#[cfg(windows)]
fn create_directory_symlink(target: &Path, link: &Path) -> io::Result<()> {
    std::os::windows::fs::symlink_dir(target, link)
}

#[cfg(unix)]
fn remove_directory_symlink(link: &Path) -> io::Result<()> {
    fs::remove_file(link)
}

#[cfg(windows)]
fn remove_directory_symlink(link: &Path) -> io::Result<()> {
    fs::remove_dir(link)
}

fn assert_success(output: &Output) {
    assert!(
        output.status.success(),
        "{}",
        normalize(output.stderr.clone())
    );
    assert!(output.stderr.is_empty());
}

fn assert_failure(output: Output, expected_stderr: &str) {
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    assert_eq!(normalize(output.stderr), expected_stderr);
}

fn normalize(bytes: Vec<u8>) -> String {
    String::from_utf8(bytes).unwrap().replace("\r\n", "\n")
}

fn command() -> Command {
    Command::new(env!("CARGO_BIN_EXE_cogniform-cli"))
}

struct TestDirectory(PathBuf);

impl TestDirectory {
    fn new(label: &str) -> Self {
        loop {
            let path = std::env::temp_dir().join(format!(
                "cogniform-cli-render-example-{label}-{}-{}",
                std::process::id(),
                NEXT_DIRECTORY.fetch_add(1, Ordering::Relaxed)
            ));
            match fs::create_dir(&path) {
                Ok(()) => return Self(path),
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
                Err(error) => panic!("failed to create CLI test directory: {error:?}"),
            }
        }
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TestDirectory {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}
