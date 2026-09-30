use std::{
    collections::BTreeMap,
    ffi::OsStr,
    fs::{self, OpenOptions},
    io::{self, Write},
    num::NonZeroU32,
    path::Path,
};

use cogniform_observation::{
    DiagnosticPngLimits, DiagnosticPngSource, diagnostic_identity_color, encode_diagnostic_png,
};
use cogniform_protocol::{ImageDimensions, RuntimeLimits, StableEntityId};
use cogniform_renderer::{HeadlessRenderer, RenderedFrame, RendererConfig};
use serde::Serialize;

const SCHEMA_VERSION: u32 = 1;
const EXAMPLE: &str = "headless-reference-v1";
const EXAMPLE_WIDTH: u32 = 64;
const EXAMPLE_HEIGHT: u32 = 64;

pub(crate) fn run(output_directory: &OsStr) -> Result<(), Box<dyn std::error::Error>> {
    let output_directory = Path::new(output_directory);
    validate_output_target(output_directory, "render-example")?;

    let mut renderer = pollster::block_on(HeadlessRenderer::new(RendererConfig::new(
        EXAMPLE_WIDTH,
        EXAMPLE_HEIGHT,
    )))?;
    let frame = renderer.submit_reference_scene()?.read()?;
    let artifacts = build_artifacts(&frame)?;
    write_artifacts(output_directory, &artifacts, "render-example")?;

    println!("Cogniform rendered example created");
    println!("output files: color.png, depth.png, normals.png, identity.png, manifest.json");
    Ok(())
}

pub(crate) fn validate_output_target(
    path: &Path,
    command: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    if path.as_os_str().is_empty() {
        return Err(invalid_input(format!(
            "{command} output directory must not be empty"
        )));
    }

    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    match fs::metadata(parent) {
        Ok(metadata) if metadata.is_dir() => {}
        Ok(_) => Err(invalid_input(format!(
            "{command} output parent is not a directory"
        )))?,
        Err(error) if error.kind() == io::ErrorKind::NotFound => Err(invalid_input(format!(
            "{command} output parent directory does not exist"
        )))?,
        Err(_) => {
            return Err(io_failure(format!(
                "failed to inspect {command} output parent"
            )));
        }
    }

    match fs::symlink_metadata(path) {
        Ok(_) => Err(invalid_input(format!(
            "{command} output directory already exists"
        ))),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(_) => Err(io_failure(format!(
            "failed to inspect {command} output target"
        ))),
    }
}

fn build_artifacts(frame: &RenderedFrame) -> Result<Vec<Artifact>, Box<dyn std::error::Error>> {
    let width = frame.width();
    let height = frame.height();
    let expected_pixels = (width as usize)
        .checked_mul(height as usize)
        .ok_or_else(|| io_failure("render-example pixel count overflow"))?;
    if frame.color().len() != expected_pixels
        || frame.depth().len() != expected_pixels
        || frame.normals().len() != expected_pixels
        || frame.stable_entity_ids().len() != expected_pixels
    {
        return Err(io_failure(
            "render-example frame outputs disagree on dimensions",
        ));
    }

    let dimensions = diagnostic_dimensions(width, height, "render-example")?;
    let identities = identity_manifest(frame.stable_entity_ids());

    let metadata = frame.metadata();
    let manifest = Manifest {
        schema_version: SCHEMA_VERSION,
        example: EXAMPLE,
        width,
        height,
        adapter: AdapterManifest {
            name: frame.adapter().name.clone(),
            backend: frame.adapter().backend.clone(),
            device_type: frame.adapter().device_type.clone(),
            webgpu_compliant: frame.adapter().webgpu_compliant,
        },
        frame: FrameManifest {
            frame_id: metadata.frame_id.get(),
            scene_revision: metadata.scene_revision.get(),
            camera_id: metadata.camera_id.to_string(),
            extraction_generation: metadata.extraction_generation,
        },
        diagnostic_only: true,
        files: FILES,
        identities,
    };
    let manifest_json = encode_manifest(&manifest)?;

    Ok(vec![
        Artifact::png(
            "color.png",
            encode_diagnostic_image(
                dimensions,
                DiagnosticPngSource::Color(frame.color()),
                "render-example",
            )?,
        ),
        Artifact::png(
            "depth.png",
            encode_diagnostic_image(
                dimensions,
                DiagnosticPngSource::Depth(frame.depth()),
                "render-example",
            )?,
        ),
        Artifact::png(
            "normals.png",
            encode_diagnostic_image(
                dimensions,
                DiagnosticPngSource::Normal(frame.normals()),
                "render-example",
            )?,
        ),
        Artifact::png(
            "identity.png",
            encode_diagnostic_image(
                dimensions,
                DiagnosticPngSource::EntityId(frame.stable_entity_ids()),
                "render-example",
            )?,
        ),
        Artifact {
            name: "manifest.json",
            bytes: manifest_json,
        },
    ])
}

pub(crate) fn encode_json_line<T: Serialize>(value: &T) -> Result<Vec<u8>, serde_json::Error> {
    let mut encoded = Vec::new();
    serde_json::to_writer(&mut encoded, value)?;
    encoded.push(b'\n');
    Ok(encoded)
}

fn encode_manifest(manifest: &Manifest) -> Result<Vec<u8>, serde_json::Error> {
    encode_json_line(manifest)
}

pub(crate) fn write_artifacts(
    path: &Path,
    artifacts: &[Artifact],
    command: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    fs::create_dir(path).map_err(|error| match error.kind() {
        io::ErrorKind::AlreadyExists => {
            invalid_input(format!("{command} output directory already exists"))
        }
        io::ErrorKind::NotFound => {
            invalid_input(format!("{command} output parent directory does not exist"))
        }
        _ => io_failure(format!("failed to create {command} output directory")),
    })?;

    for artifact in artifacts {
        let mut target = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path.join(artifact.name))
            .map_err(|_| io_failure(format!("failed to create {}", artifact.name)))?;
        target
            .write_all(&artifact.bytes)
            .map_err(|_| io_failure(format!("failed to write {}", artifact.name)))?;
    }
    Ok(())
}

pub(crate) fn identity_manifest(values: &[Option<StableEntityId>]) -> Vec<IdentityManifest> {
    let mut palette = BTreeMap::new();
    for entity_id in values.iter().flatten() {
        if !palette.contains_key(entity_id) {
            palette.insert(*entity_id, diagnostic_identity_color(*entity_id));
        }
    }
    palette
        .into_iter()
        .map(|(entity_id, color)| IdentityManifest {
            entity_id: entity_id.to_string(),
            color: format!("#{:02x}{:02x}{:02x}", color[0], color[1], color[2]),
        })
        .collect()
}

pub(crate) fn diagnostic_dimensions(
    width: u32,
    height: u32,
    command: &str,
) -> Result<ImageDimensions, Box<dyn std::error::Error>> {
    Ok(ImageDimensions {
        width: NonZeroU32::new(width)
            .ok_or_else(|| io_failure(format!("{command} image width is zero")))?,
        height: NonZeroU32::new(height)
            .ok_or_else(|| io_failure(format!("{command} image height is zero")))?,
    })
}

pub(crate) fn encode_diagnostic_image(
    dimensions: ImageDimensions,
    source: DiagnosticPngSource<'_>,
    command: &str,
) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    encode_diagnostic_png(
        dimensions,
        source,
        &RuntimeLimits::default(),
        DiagnosticPngLimits::default(),
    )
    .map_err(|_| io_failure(format!("{command} diagnostic PNG encoding failed")))
}

fn invalid_input(message: impl Into<String>) -> Box<dyn std::error::Error> {
    Box::new(io::Error::new(io::ErrorKind::InvalidInput, message.into()))
}

fn io_failure(message: impl Into<String>) -> Box<dyn std::error::Error> {
    Box::new(io::Error::other(message.into()))
}

pub(crate) struct Artifact {
    name: &'static str,
    bytes: Vec<u8>,
}

impl Artifact {
    pub(crate) fn new(name: &'static str, bytes: Vec<u8>) -> Self {
        Self { name, bytes }
    }

    pub(crate) fn png(name: &'static str, bytes: Vec<u8>) -> Self {
        Self::new(name, bytes)
    }
}

#[derive(Serialize)]
struct Manifest {
    schema_version: u32,
    example: &'static str,
    width: u32,
    height: u32,
    adapter: AdapterManifest,
    frame: FrameManifest,
    diagnostic_only: bool,
    files: [FileManifest; 4],
    identities: Vec<IdentityManifest>,
}

#[derive(Serialize)]
struct AdapterManifest {
    name: String,
    backend: String,
    device_type: String,
    webgpu_compliant: bool,
}

#[derive(Serialize)]
struct FrameManifest {
    frame_id: u64,
    scene_revision: u64,
    camera_id: String,
    extraction_generation: u64,
}

#[derive(Clone, Copy, Serialize)]
struct FileManifest {
    name: &'static str,
    observation: &'static str,
    visualization: &'static str,
}

#[derive(Serialize)]
pub(crate) struct IdentityManifest {
    pub(crate) entity_id: String,
    pub(crate) color: String,
}

const FILES: [FileManifest; 4] = [
    FileManifest {
        name: "color.png",
        observation: "color",
        visualization: "linear-rgba8-source-values",
    },
    FileManifest {
        name: "depth.png",
        observation: "depth",
        visualization: "near-white-far-black-grayscale8",
    },
    FileManifest {
        name: "normals.png",
        observation: "normal",
        visualization: "world-xyz-remapped-to-rgb-transparent-background",
    },
    FileManifest {
        name: "identity.png",
        observation: "entity_id",
        visualization: "deterministic-palette-transparent-background",
    },
];

#[cfg(test)]
mod tests {
    use cogniform_protocol::StableEntityId;

    use super::{
        AdapterManifest, FileManifest, FrameManifest, IdentityManifest, Manifest, encode_manifest,
        identity_manifest,
    };

    #[test]
    fn identity_manifest_is_repeatable_and_linked_to_the_shared_palette() {
        let first = StableEntityId::new(7).unwrap();
        let second = StableEntityId::new(8).unwrap();
        let identities = identity_manifest(&[None, Some(first), Some(first), Some(second)]);
        assert_eq!(identities.len(), 2);
        assert_eq!(identities[0].entity_id, first.to_string());
        assert_eq!(identities[0].color, "#e14965");
    }

    #[test]
    fn manifest_encoding_is_canonical_and_newline_terminated() {
        let manifest = Manifest {
            schema_version: 1,
            example: "headless-reference-v1",
            width: 64,
            height: 64,
            adapter: AdapterManifest {
                name: "fixture adapter".into(),
                backend: "fixture-backend".into(),
                device_type: "fixture-device".into(),
                webgpu_compliant: true,
            },
            frame: FrameManifest {
                frame_id: 1,
                scene_revision: 0,
                camera_id: "00000000000000000000000000000001".into(),
                extraction_generation: 0,
            },
            diagnostic_only: true,
            files: [
                FileManifest {
                    name: "color.png",
                    observation: "color",
                    visualization: "linear-rgba8-source-values",
                },
                FileManifest {
                    name: "depth.png",
                    observation: "depth",
                    visualization: "near-white-far-black-grayscale8",
                },
                FileManifest {
                    name: "normals.png",
                    observation: "normal",
                    visualization: "world-xyz-remapped-to-rgb-transparent-background",
                },
                FileManifest {
                    name: "identity.png",
                    observation: "entity_id",
                    visualization: "deterministic-palette-transparent-background",
                },
            ],
            identities: vec![IdentityManifest {
                entity_id: "00000000000000000000000000000007".into(),
                color: "#e14965".into(),
            }],
        };

        assert_eq!(
            String::from_utf8(encode_manifest(&manifest).unwrap()).unwrap(),
            concat!(
                "{\"schema_version\":1,\"example\":\"headless-reference-v1\",",
                "\"width\":64,\"height\":64,\"adapter\":{\"name\":\"fixture adapter\",",
                "\"backend\":\"fixture-backend\",\"device_type\":\"fixture-device\",",
                "\"webgpu_compliant\":true},\"frame\":{\"frame_id\":1,",
                "\"scene_revision\":0,\"camera_id\":",
                "\"00000000000000000000000000000001\",\"extraction_generation\":0},",
                "\"diagnostic_only\":true,\"files\":[{\"name\":\"color.png\",",
                "\"observation\":\"color\",\"visualization\":\"linear-rgba8-source-values\"},",
                "{\"name\":\"depth.png\",\"observation\":\"depth\",",
                "\"visualization\":\"near-white-far-black-grayscale8\"},",
                "{\"name\":\"normals.png\",\"observation\":\"normal\",",
                "\"visualization\":\"world-xyz-remapped-to-rgb-transparent-background\"},",
                "{\"name\":\"identity.png\",\"observation\":\"entity_id\",",
                "\"visualization\":\"deterministic-palette-transparent-background\"}],",
                "\"identities\":[{\"entity_id\":",
                "\"00000000000000000000000000000007\",\"color\":\"#e14965\"}]}\n",
            )
        );
    }
}
