//! Black-box coverage for canonical-scenario rendered graphics.

use core::sync::atomic::{AtomicU64, Ordering};
use std::{
    collections::BTreeSet,
    fs, io,
    io::BufReader,
    path::{Path, PathBuf},
    process::{Command, Output},
};

use serde_json::Value;

static NEXT_DIRECTORY: AtomicU64 = AtomicU64::new(1);

#[test]
fn arguments_and_create_new_target_are_checked_before_gpu_selection() {
    let output = command().arg("render-scenario").arg("").output().unwrap();
    assert_failure(
        output,
        "error: render-scenario output directory must not be empty\n",
    );

    for arguments in [
        &["render-scenario"][..],
        &["render-scenario", "first", "second"][..],
        &["render-scenario", "--"][..],
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
            .arg("render-scenario")
            .arg(target)
            .output()
            .unwrap();
        assert_failure(
            output,
            "error: render-scenario output directory already exists\n",
        );
    }
    assert_eq!(fs::read(&existing_file).unwrap(), b"preserve me");

    let output = command()
        .arg("render-scenario")
        .arg(existing_file.join("example"))
        .output()
        .unwrap();
    assert_failure(
        output,
        "error: render-scenario output parent is not a directory\n",
    );

    let output = command()
        .arg("render-scenario")
        .arg(directory.path().join("missing-parent").join("example"))
        .output()
        .unwrap();
    assert_failure(
        output,
        "error: render-scenario output parent directory does not exist\n",
    );
}

#[test]
#[ignore = "requires a controlled headless GPU adapter"]
fn command_writes_canonical_scene_observations_with_distinct_causality() {
    let directory = TestDirectory::new("render");
    let output_directory = directory.path().join("scenario");
    let output = command()
        .arg("render-scenario")
        .arg(&output_directory)
        .output()
        .unwrap();
    assert_success(&output);
    assert_eq!(
        normalize(output.stdout.clone()),
        concat!(
            "Cogniform canonical scenario graphics created\n",
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

    let color = assert_png(
        &output_directory.join("color.png"),
        png::ColorType::Rgba,
        true,
    );
    let depth = assert_png(
        &output_directory.join("depth.png"),
        png::ColorType::Grayscale,
        false,
    );
    let normals = assert_png(
        &output_directory.join("normals.png"),
        png::ColorType::Rgba,
        false,
    );
    let identity = assert_png(
        &output_directory.join("identity.png"),
        png::ColorType::Rgba,
        false,
    );
    let center = (64 / 2 * 64 + 64 / 2) * 4;
    assert_eq!(&color[center..center + 4], &[55, 30, 11, 255]);
    assert!(depth.iter().any(|value| *value > 0));
    assert!(normals.chunks_exact(4).any(|pixel| pixel[3] == 255));
    assert!(identity.chunks_exact(4).any(|pixel| pixel[3] == 0));

    let manifest_bytes = assert_manifest(&output_directory, &identity);

    let before = manifest_bytes;
    let repeated = command()
        .arg("render-scenario")
        .arg(&output_directory)
        .output()
        .unwrap();
    assert_failure(
        repeated,
        "error: render-scenario output directory already exists\n",
    );
    assert_eq!(
        fs::read(output_directory.join("manifest.json")).unwrap(),
        before
    );
}

fn assert_manifest(output_directory: &Path, identity_pixels: &[u8]) -> Vec<u8> {
    let manifest_bytes = fs::read(output_directory.join("manifest.json")).unwrap();
    assert!(manifest_bytes.ends_with(b"\n"));
    let manifest: Value = serde_json::from_slice(&manifest_bytes).unwrap();
    assert_eq!(manifest["schema_version"], 1);
    assert_eq!(manifest["example"], "canonical-mvp-render-v1");
    assert_eq!(manifest["width"], 64);
    assert_eq!(manifest["height"], 64);
    assert_eq!(manifest["diagnostic_only"], true);
    assert_manifest_adapter(&manifest);
    assert_eq!(manifest["scene"]["scene_revision"], 2);
    assert_eq!(
        manifest["scene"]["room_id"],
        "00000000000000000000000000000100"
    );
    assert_eq!(
        manifest["scene"]["table_id"],
        "00000000000000000000000000000200"
    );
    assert_eq!(
        manifest["scene"]["light_id"],
        "00000000000000000000000000000300"
    );
    assert_eq!(
        manifest["scene"]["camera_id"],
        "00000000000000000000000000000400"
    );
    assert_eq!(
        manifest["scene"]["logical_hash"],
        manifest["scene"]["replayed_logical_hash"]
    );
    assert_lowercase_hash(manifest["scene"]["logical_hash"].as_str().unwrap());
    assert_file_causality(&manifest);
    assert_identity_palette(&manifest, identity_pixels);
    manifest_bytes
}

fn assert_manifest_adapter(manifest: &Value) {
    let adapter = manifest["adapter"].as_object().unwrap();
    assert!(!adapter["name"].as_str().unwrap().is_empty());
    assert!(!adapter["backend"].as_str().unwrap().is_empty());
    assert!(!adapter["device_type"].as_str().unwrap().is_empty());
    assert!(adapter["webgpu_compliant"].is_boolean());
}

fn assert_file_causality(manifest: &Value) {
    let files = manifest["files"].as_array().unwrap();
    assert_eq!(files.len(), 4);
    assert_eq!(
        files
            .iter()
            .map(|file| file["name"].as_str().unwrap())
            .collect::<Vec<_>>(),
        ["color.png", "depth.png", "normals.png", "identity.png"]
    );
    assert_eq!(
        files
            .iter()
            .map(|file| file["observation"].as_str().unwrap())
            .collect::<Vec<_>>(),
        ["color", "depth", "normal", "entity_id"]
    );
    assert_eq!(
        files
            .iter()
            .map(|file| file["frame_id"].as_u64().unwrap())
            .collect::<Vec<_>>(),
        [4, 5, 6, 7]
    );
    assert_eq!(
        files
            .iter()
            .map(|file| file["observation_id"].as_str().unwrap())
            .collect::<Vec<_>>(),
        [
            "00000000000000000000000000004001",
            "00000000000000000000000000004002",
            "00000000000000000000000000004003",
            "00000000000000000000000000004004",
        ]
    );
    assert_eq!(
        files
            .iter()
            .map(|file| file["visualization"].as_str().unwrap())
            .collect::<Vec<_>>(),
        [
            "linear-rgba8-source-values",
            "near-white-far-black-grayscale8",
            "world-xyz-remapped-to-rgb-transparent-background",
            "deterministic-palette-transparent-background",
        ]
    );
    assert!(files.iter().all(|file| file["scene_revision"] == 2));
    assert!(files.iter().all(|file| {
        file["camera_id"] == "00000000000000000000000000000400"
            && file["observation_id"].as_str().unwrap().len() == 32
    }));
}

fn assert_identity_palette(manifest: &Value, identity_pixels: &[u8]) {
    let manifest_colors = manifest["identities"]
        .as_array()
        .unwrap()
        .iter()
        .map(|identity| identity["color"].as_str().unwrap().to_owned())
        .collect::<BTreeSet<_>>();
    let pixel_colors = identity_pixels
        .chunks_exact(4)
        .filter(|pixel| pixel[3] == 255)
        .map(|pixel| format!("#{:02x}{:02x}{:02x}", pixel[0], pixel[1], pixel[2]))
        .collect::<BTreeSet<_>>();
    assert_eq!(manifest_colors, pixel_colors);
    assert!(manifest_colors.len() >= 2);
}

fn assert_png(path: &Path, expected_color: png::ColorType, linear_gamma: bool) -> Vec<u8> {
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
    assert_eq!(
        reader.info().gamma(),
        linear_gamma.then(|| png::ScaledFloat::new(1.0))
    );
    pixels
}

fn assert_lowercase_hash(value: &str) {
    assert_eq!(value.len(), 64);
    assert!(
        value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
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
                "cogniform-cli-render-scenario-{label}-{}-{}",
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
