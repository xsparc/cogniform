use std::{
    ffi::OsStr,
    io,
    path::Path,
    thread,
    time::{Duration, Instant},
};

use cogniform_engine::{
    CanonicalScenarioConfig, CanonicalScenarioReport, LocalService, LocalServiceConfig,
    Observation, ObservationPayload, run_canonical_scenario,
};
use cogniform_protocol::{
    ObservationId, ObservationKind, ObservationQuality, ObservationRequest, SceneRevision,
    SchemaVersion, StableEntityId,
};
use cogniform_renderer::AdapterSummary;
use serde::Serialize;

use crate::{
    LOCAL_PROFILE_HEIGHT, LOCAL_PROFILE_WIDTH,
    render_example::{
        Artifact, IdentityManifest, encode_json_line, encode_png, flatten_rgba,
        validate_output_target, visualize_depth, visualize_identities, visualize_normals,
        write_artifacts,
    },
};

const COMMAND: &str = "render-scenario";
const SCHEMA_VERSION: u32 = 1;
const EXAMPLE: &str = "canonical-mvp-render-v1";
const OBSERVATION_TIMEOUT: Duration = Duration::from_secs(10);
const COLOR_OBSERVATION_ID: u128 = 0x4_001;
const DEPTH_OBSERVATION_ID: u128 = 0x4_002;
const NORMAL_OBSERVATION_ID: u128 = 0x4_003;
const ENTITY_ID_OBSERVATION_ID: u128 = 0x4_004;

pub(crate) fn run(output_directory: &OsStr) -> Result<(), Box<dyn std::error::Error>> {
    let output_directory = Path::new(output_directory);
    validate_output_target(output_directory, COMMAND)?;

    let mut service = pollster::block_on(LocalService::new(LocalServiceConfig::new(
        LOCAL_PROFILE_WIDTH,
        LOCAL_PROFILE_HEIGHT,
    )))?;
    let adapter = service.adapter().clone();
    let scenario = run_canonical_scenario(&mut service, CanonicalScenarioConfig::default())?;
    let revision = scenario.update_receipt.new_revision;
    let camera_id = scenario.camera_id;
    let observations = [
        request_observation(
            &mut service,
            COLOR_OBSERVATION_ID,
            revision,
            camera_id,
            ObservationKind::Color,
        )?,
        request_observation(
            &mut service,
            DEPTH_OBSERVATION_ID,
            revision,
            camera_id,
            ObservationKind::Depth,
        )?,
        request_observation(
            &mut service,
            NORMAL_OBSERVATION_ID,
            revision,
            camera_id,
            ObservationKind::Normal,
        )?,
        request_observation(
            &mut service,
            ENTITY_ID_OBSERVATION_ID,
            revision,
            camera_id,
            ObservationKind::EntityId,
        )?,
    ];
    let artifacts = build_artifacts(&adapter, &scenario, &observations)?;
    write_artifacts(output_directory, &artifacts, COMMAND)?;

    println!("Cogniform canonical scenario graphics created");
    println!("output files: color.png, depth.png, normals.png, identity.png, manifest.json");
    Ok(())
}

fn request_observation(
    service: &mut LocalService,
    id: u128,
    revision: SceneRevision,
    camera_id: StableEntityId,
    kind: ObservationKind,
) -> Result<Observation, Box<dyn std::error::Error>> {
    let request = ObservationRequest {
        schema_version: SchemaVersion::V1,
        observation_id: ObservationId::new(id).expect("scenario render identity is non-zero"),
        scene_revision: revision,
        camera_id,
        kind,
        quality: ObservationQuality::Low,
    };
    service.request_observation(request)?;
    let deadline = Instant::now() + OBSERVATION_TIMEOUT;
    loop {
        if let Some(observation) = service.try_receive_observation()? {
            let metadata = observation.metadata();
            if metadata.observation_id != request.observation_id
                || metadata.kind != request.kind
                || metadata.scene_revision != revision
                || metadata.camera_id != camera_id
                || metadata.quality != ObservationQuality::Low
                || metadata.staleness.latest_known_revision != revision
                || metadata.staleness.revisions_behind != 0
            {
                return Err(io_failure(
                    "render-scenario observation causality did not match its request",
                ));
            }
            return Ok(observation);
        }
        if Instant::now() >= deadline {
            return Err(io_failure("render-scenario observation timed out"));
        }
        thread::sleep(Duration::from_millis(1));
    }
}

fn build_artifacts(
    adapter: &AdapterSummary,
    scenario: &CanonicalScenarioReport,
    observations: &[Observation; 4],
) -> Result<Vec<Artifact>, Box<dyn std::error::Error>> {
    let (width, height, expected_pixels) = validate_observation_set(observations)?;

    let ObservationPayload::Color(color) = observations[0].payload() else {
        return Err(io_failure(
            "render-scenario color request returned a different payload",
        ));
    };
    let ObservationPayload::Depth(depth) = observations[1].payload() else {
        return Err(io_failure(
            "render-scenario depth request returned a different payload",
        ));
    };
    let ObservationPayload::Normal(normals) = observations[2].payload() else {
        return Err(io_failure(
            "render-scenario normal request returned a different payload",
        ));
    };
    let ObservationPayload::EntityId(entity_ids) = observations[3].payload() else {
        return Err(io_failure(
            "render-scenario entity-ID request returned a different payload",
        ));
    };
    if [color.len(), depth.len(), normals.len(), entity_ids.len()]
        .into_iter()
        .any(|length| length != expected_pixels)
    {
        return Err(io_failure(
            "render-scenario payload length does not match its dimensions",
        ));
    }

    let color = flatten_rgba(color);
    let depth = visualize_depth(depth);
    let normals = visualize_normals(normals);
    let (identity, identities) = visualize_identities(entity_ids);
    let files = [
        file_manifest(&observations[0], "color.png", "linear-rgba8-source-values"),
        file_manifest(
            &observations[1],
            "depth.png",
            "near-white-far-black-grayscale8",
        ),
        file_manifest(
            &observations[2],
            "normals.png",
            "world-xyz-remapped-to-rgb-transparent-background",
        ),
        file_manifest(
            &observations[3],
            "identity.png",
            "deterministic-palette-transparent-background",
        ),
    ];
    let manifest = Manifest {
        schema_version: SCHEMA_VERSION,
        example: EXAMPLE,
        width,
        height,
        adapter: AdapterManifest::from(adapter),
        scene: SceneManifest {
            scene_revision: scenario.update_receipt.new_revision.get(),
            room_id: scenario.room_id.to_string(),
            table_id: scenario.table_id.to_string(),
            light_id: scenario.light_id.to_string(),
            camera_id: scenario.camera_id.to_string(),
            logical_hash: scenario.logical_hash.to_string(),
            replayed_logical_hash: scenario.replayed_logical_hash.to_string(),
        },
        diagnostic_only: true,
        files,
        identities,
    };

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
        Artifact::new("manifest.json", encode_json_line(&manifest)?),
    ])
}

fn validate_observation_set(
    observations: &[Observation; 4],
) -> Result<(u32, u32, usize), Box<dyn std::error::Error>> {
    let dimensions = observations[0]
        .metadata()
        .dimensions
        .ok_or_else(|| io_failure("render-scenario color observation has no dimensions"))?;
    let expected_pixels = usize::try_from(dimensions.pixel_count())
        .map_err(|_| io_failure("render-scenario pixel count does not fit this platform"))?;
    for observation in observations {
        if observation.metadata().dimensions != Some(dimensions)
            || observation.payload().item_count() != dimensions.pixel_count()
        {
            return Err(io_failure(
                "render-scenario observation outputs disagree on dimensions",
            ));
        }
    }
    if !observations
        .windows(2)
        .all(|pair| pair[0].metadata().frame_id < pair[1].metadata().frame_id)
    {
        return Err(io_failure(
            "render-scenario observation frame identities are not increasing",
        ));
    }
    Ok((
        dimensions.width.get(),
        dimensions.height.get(),
        expected_pixels,
    ))
}

fn file_manifest(
    observation: &Observation,
    name: &'static str,
    visualization: &'static str,
) -> FileManifest {
    let metadata = observation.metadata();
    FileManifest {
        name,
        observation: match metadata.kind {
            ObservationKind::Color => "color",
            ObservationKind::Depth => "depth",
            ObservationKind::Normal => "normal",
            ObservationKind::EntityId => "entity_id",
            ObservationKind::Visibility => "visibility",
        },
        visualization,
        observation_id: metadata.observation_id.to_string(),
        frame_id: metadata.frame_id.get(),
        scene_revision: metadata.scene_revision.get(),
        camera_id: metadata.camera_id.to_string(),
    }
}

fn io_failure(message: impl Into<String>) -> Box<dyn std::error::Error> {
    Box::new(io::Error::other(message.into()))
}

#[derive(Serialize)]
struct Manifest {
    schema_version: u32,
    example: &'static str,
    width: u32,
    height: u32,
    adapter: AdapterManifest,
    scene: SceneManifest,
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

impl From<&AdapterSummary> for AdapterManifest {
    fn from(value: &AdapterSummary) -> Self {
        Self {
            name: value.name.clone(),
            backend: value.backend.clone(),
            device_type: value.device_type.clone(),
            webgpu_compliant: value.webgpu_compliant,
        }
    }
}

#[derive(Serialize)]
struct SceneManifest {
    scene_revision: u64,
    room_id: String,
    table_id: String,
    light_id: String,
    camera_id: String,
    logical_hash: String,
    replayed_logical_hash: String,
}

#[derive(Serialize)]
struct FileManifest {
    name: &'static str,
    observation: &'static str,
    visualization: &'static str,
    observation_id: String,
    frame_id: u64,
    scene_revision: u64,
    camera_id: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manifest_encoding_is_canonical_and_explicit_about_distinct_frames() {
        let manifest = Manifest {
            schema_version: 1,
            example: "canonical-mvp-render-v1",
            width: 64,
            height: 64,
            adapter: AdapterManifest {
                name: "fixture adapter".into(),
                backend: "fixture-backend".into(),
                device_type: "fixture-device".into(),
                webgpu_compliant: true,
            },
            scene: SceneManifest {
                scene_revision: 2,
                room_id: "00000000000000000000000000000100".into(),
                table_id: "00000000000000000000000000000200".into(),
                light_id: "00000000000000000000000000000300".into(),
                camera_id: "00000000000000000000000000000400".into(),
                logical_hash: "11".repeat(32),
                replayed_logical_hash: "11".repeat(32),
            },
            diagnostic_only: true,
            files: [
                file(
                    "color.png",
                    "color",
                    "linear-rgba8-source-values",
                    0x4_001,
                    4,
                ),
                file(
                    "depth.png",
                    "depth",
                    "near-white-far-black-grayscale8",
                    0x4_002,
                    5,
                ),
                file(
                    "normals.png",
                    "normal",
                    "world-xyz-remapped-to-rgb-transparent-background",
                    0x4_003,
                    6,
                ),
                file(
                    "identity.png",
                    "entity_id",
                    "deterministic-palette-transparent-background",
                    0x4_004,
                    7,
                ),
            ],
            identities: vec![IdentityManifest {
                entity_id: "00000000000000000000000000000200".into(),
                color: "#beef40".into(),
            }],
        };
        let encoded = String::from_utf8(encode_json_line(&manifest).unwrap()).unwrap();

        assert_eq!(
            encoded,
            concat!(
                "{\"schema_version\":1,\"example\":\"canonical-mvp-render-v1\",",
                "\"width\":64,\"height\":64,\"adapter\":{\"name\":\"fixture adapter\",",
                "\"backend\":\"fixture-backend\",\"device_type\":\"fixture-device\",",
                "\"webgpu_compliant\":true},\"scene\":{\"scene_revision\":2,",
                "\"room_id\":\"00000000000000000000000000000100\",",
                "\"table_id\":\"00000000000000000000000000000200\",",
                "\"light_id\":\"00000000000000000000000000000300\",",
                "\"camera_id\":\"00000000000000000000000000000400\",",
                "\"logical_hash\":\"1111111111111111111111111111111111111111111111111111111111111111\",",
                "\"replayed_logical_hash\":\"1111111111111111111111111111111111111111111111111111111111111111\"},",
                "\"diagnostic_only\":true,\"files\":[{\"name\":\"color.png\",",
                "\"observation\":\"color\",\"visualization\":\"linear-rgba8-source-values\",",
                "\"observation_id\":\"00000000000000000000000000004001\",",
                "\"frame_id\":4,\"scene_revision\":2,",
                "\"camera_id\":\"00000000000000000000000000000400\"},",
                "{\"name\":\"depth.png\",\"observation\":\"depth\",",
                "\"visualization\":\"near-white-far-black-grayscale8\",",
                "\"observation_id\":\"00000000000000000000000000004002\",",
                "\"frame_id\":5,\"scene_revision\":2,",
                "\"camera_id\":\"00000000000000000000000000000400\"},",
                "{\"name\":\"normals.png\",\"observation\":\"normal\",",
                "\"visualization\":\"world-xyz-remapped-to-rgb-transparent-background\",",
                "\"observation_id\":\"00000000000000000000000000004003\",",
                "\"frame_id\":6,\"scene_revision\":2,",
                "\"camera_id\":\"00000000000000000000000000000400\"},",
                "{\"name\":\"identity.png\",\"observation\":\"entity_id\",",
                "\"visualization\":\"deterministic-palette-transparent-background\",",
                "\"observation_id\":\"00000000000000000000000000004004\",",
                "\"frame_id\":7,\"scene_revision\":2,",
                "\"camera_id\":\"00000000000000000000000000000400\"}],",
                "\"identities\":[{\"entity_id\":\"00000000000000000000000000000200\",",
                "\"color\":\"#beef40\"}]}\n",
            )
        );
    }

    fn file(
        name: &'static str,
        observation: &'static str,
        visualization: &'static str,
        observation_id: u128,
        frame_id: u64,
    ) -> FileManifest {
        FileManifest {
            name,
            observation,
            visualization,
            observation_id: format!("{observation_id:032x}"),
            frame_id,
            scene_revision: 2,
            camera_id: "00000000000000000000000000000400".into(),
        }
    }
}
