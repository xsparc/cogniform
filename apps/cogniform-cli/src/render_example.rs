use std::{
    collections::BTreeMap,
    ffi::OsStr,
    fs::{self, OpenOptions},
    io::{self, Write},
    path::Path,
};

use cogniform_protocol::StableEntityId;
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

    let color = flatten_rgba(frame.color());
    let depth = visualize_depth(frame.depth());
    let normals = visualize_normals(frame.normals());
    let (identity, identities) = visualize_identities(frame.stable_entity_ids());

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
            encode_png(width, height, png::ColorType::Rgba, &color, true)?,
        ),
        Artifact::png(
            "depth.png",
            encode_png(width, height, png::ColorType::Grayscale, &depth, false)?,
        ),
        Artifact::png(
            "normals.png",
            encode_png(width, height, png::ColorType::Rgba, &normals, false)?,
        ),
        Artifact::png(
            "identity.png",
            encode_png(width, height, png::ColorType::Rgba, &identity, false)?,
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

pub(crate) fn flatten_rgba(pixels: &[[u8; 4]]) -> Vec<u8> {
    let mut encoded = Vec::with_capacity(pixels.len().saturating_mul(4));
    for pixel in pixels {
        encoded.extend_from_slice(pixel);
    }
    encoded
}

pub(crate) fn visualize_depth(values: &[f32]) -> Vec<u8> {
    values
        .iter()
        .map(|depth| unit_to_byte(1.0 - depth))
        .collect()
}

pub(crate) fn visualize_normals(values: &[Option<[f32; 3]>]) -> Vec<u8> {
    let mut pixels = Vec::with_capacity(values.len().saturating_mul(4));
    for value in values {
        match value {
            None => pixels.extend_from_slice(&[0, 0, 0, 0]),
            Some(normal) => pixels.extend_from_slice(&[
                signed_unit_to_byte(normal[0]),
                signed_unit_to_byte(normal[1]),
                signed_unit_to_byte(normal[2]),
                255,
            ]),
        }
    }
    pixels
}

pub(crate) fn visualize_identities(
    values: &[Option<StableEntityId>],
) -> (Vec<u8>, Vec<IdentityManifest>) {
    let mut pixels = Vec::with_capacity(values.len().saturating_mul(4));
    let mut palette = BTreeMap::new();
    for value in values {
        match value {
            None => pixels.extend_from_slice(&[0, 0, 0, 0]),
            Some(entity_id) => {
                let color = identity_color(*entity_id);
                pixels.extend_from_slice(&color);
                palette.entry(*entity_id).or_insert(color);
            }
        }
    }
    let identities = palette
        .into_iter()
        .map(|(entity_id, color)| IdentityManifest {
            entity_id: entity_id.to_string(),
            color: format!("#{:02x}{:02x}{:02x}", color[0], color[1], color[2]),
        })
        .collect();
    (pixels, identities)
}

fn identity_color(entity_id: StableEntityId) -> [u8; 4] {
    let [
        b0,
        b1,
        b2,
        b3,
        b4,
        b5,
        b6,
        b7,
        b8,
        b9,
        b10,
        b11,
        b12,
        b13,
        b14,
        b15,
    ] = entity_id.get().to_le_bytes();
    let low = u64::from_le_bytes([b0, b1, b2, b3, b4, b5, b6, b7]);
    let high = u64::from_le_bytes([b8, b9, b10, b11, b12, b13, b14, b15]);
    let mut mixed = low ^ high.rotate_left(29);
    mixed = mixed.wrapping_add(0x9e37_79b9_7f4a_7c15);
    mixed = (mixed ^ (mixed >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    mixed = (mixed ^ (mixed >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    mixed ^= mixed >> 31;
    let bytes = mixed.to_le_bytes();
    [
        visible_channel(bytes[0]),
        visible_channel(bytes[1]),
        visible_channel(bytes[2]),
        255,
    ]
}

fn visible_channel(value: u8) -> u8 {
    let scaled = (u16::from(value) * 191) / 255;
    64 + u8::try_from(scaled).expect("scaled color channel is at most 191")
}

fn signed_unit_to_byte(value: f32) -> u8 {
    unit_to_byte(value.mul_add(0.5, 0.5))
}

// The clamp and scale prove the rounded value is finite and within u8.
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn unit_to_byte(value: f32) -> u8 {
    (value.clamp(0.0, 1.0) * 255.0).round() as u8
}

pub(crate) fn encode_png(
    width: u32,
    height: u32,
    color_type: png::ColorType,
    pixels: &[u8],
    linear_gamma: bool,
) -> Result<Vec<u8>, png::EncodingError> {
    let mut png_bytes = Vec::new();
    {
        let mut png_encoder = png::Encoder::new(&mut png_bytes, width, height);
        png_encoder.set_color(color_type);
        png_encoder.set_depth(png::BitDepth::Eight);
        if linear_gamma {
            png_encoder.set_source_gamma(png::ScaledFloat::new(1.0));
        }
        let mut writer = png_encoder.write_header()?;
        writer.write_image_data(pixels)?;
    }
    Ok(png_bytes)
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
    use std::io::Cursor;

    use cogniform_protocol::StableEntityId;

    use super::{
        AdapterManifest, FileManifest, FrameManifest, IdentityManifest, Manifest, encode_manifest,
        encode_png, identity_color, signed_unit_to_byte, unit_to_byte, visualize_depth,
        visualize_identities, visualize_normals,
    };

    #[test]
    fn depth_visualization_maps_near_to_white_and_far_to_black() {
        assert_eq!(visualize_depth(&[0.0, 0.5, 1.0]), [255, 128, 0]);
        assert_eq!(unit_to_byte(-1.0), 0);
        assert_eq!(unit_to_byte(2.0), 255);
    }

    #[test]
    fn normal_visualization_preserves_axes_and_marks_background_transparent() {
        assert_eq!(signed_unit_to_byte(-1.0), 0);
        assert_eq!(signed_unit_to_byte(0.0), 128);
        assert_eq!(signed_unit_to_byte(1.0), 255);
        assert_eq!(
            visualize_normals(&[None, Some([-1.0, 0.0, 1.0])]),
            [0, 0, 0, 0, 0, 128, 255, 255]
        );
    }

    #[test]
    fn identity_visualization_is_repeatable_bright_and_manifest_linked() {
        let first = StableEntityId::new(7).unwrap();
        let second = StableEntityId::new(8).unwrap();
        let first_color = identity_color(first);
        assert_eq!(first_color, [225, 73, 101, 255]);
        assert_eq!(first_color, identity_color(first));
        assert_ne!(first_color, identity_color(second));
        assert!(first_color[..3].iter().all(|channel| *channel >= 64));

        let (pixels, identities) =
            visualize_identities(&[None, Some(first), Some(first), Some(second)]);
        assert_eq!(&pixels[..4], &[0, 0, 0, 0]);
        assert_eq!(&pixels[4..8], &first_color);
        assert_eq!(&pixels[8..12], &first_color);
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

    #[test]
    fn png_encoder_writes_exact_eight_bit_shape_and_pixels() {
        let source = [1, 2, 3, 4, 5, 6, 7, 8];
        let encoded = encode_png(2, 1, png::ColorType::Rgba, &source, true).unwrap();
        let decoder = png::Decoder::new(Cursor::new(encoded));
        let mut reader = decoder.read_info().unwrap();
        assert_eq!(reader.output_buffer_size(), Some(source.len()));
        let mut pixel_bytes = vec![0; source.len()];
        let info = reader.next_frame(&mut pixel_bytes).unwrap();
        assert_eq!(info.width, 2);
        assert_eq!(info.height, 1);
        assert_eq!(info.color_type, png::ColorType::Rgba);
        assert_eq!(info.bit_depth, png::BitDepth::Eight);
        assert_eq!(pixel_bytes, source);
        assert_eq!(reader.info().gamma(), Some(png::ScaledFloat::new(1.0)));
    }
}
