use std::{
    fmt::Write as FmtWrite,
    io::{self, Write as IoWrite},
};

use cogniform_engine::{
    AdapterSummary, CanonicalScenarioConfig, CanonicalScenarioReport, LocalService,
    LocalServiceConfig, run_canonical_scenario,
};
use serde::Serialize;

use crate::{LOCAL_PROFILE_HEIGHT, LOCAL_PROFILE_WIDTH};

const SCHEMA_VERSION: u32 = 1;
const SCENARIO: &str = "canonical-mvp-v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ScenarioOutput {
    Human,
    Json,
}

pub(crate) fn run(output: ScenarioOutput) -> Result<(), Box<dyn std::error::Error>> {
    let mut service = pollster::block_on(LocalService::new(LocalServiceConfig::new(
        LOCAL_PROFILE_WIDTH,
        LOCAL_PROFILE_HEIGHT,
    )))?;
    let adapter = service.adapter().clone();
    let result = run_canonical_scenario(&mut service, CanonicalScenarioConfig::default())?;
    let report = ScenarioReport::from_result(&adapter, &result);
    let encoded = match output {
        ScenarioOutput::Human => encode_human(&report).into_bytes(),
        ScenarioOutput::Json => encode_json(&report)?,
    };

    io::stdout().lock().write_all(&encoded)?;
    Ok(())
}

fn encode_human(report: &ScenarioReport) -> String {
    let mut encoded = String::new();
    writeln!(&mut encoded, "Cogniform canonical scenario passed")
        .expect("writing to a String cannot fail");
    writeln!(&mut encoded, "adapter: {}", report.adapter.name)
        .expect("writing to a String cannot fail");
    writeln!(&mut encoded, "backend: {}", report.adapter.backend)
        .expect("writing to a String cannot fail");
    writeln!(&mut encoded, "device type: {}", report.adapter.device_type)
        .expect("writing to a String cannot fail");
    writeln!(
        &mut encoded,
        "WebGPU compliant: {}",
        report.adapter.webgpu_compliant
    )
    .expect("writing to a String cannot fail");
    writeln!(&mut encoded, "revision: {}", report.scene_revision)
        .expect("writing to a String cannot fail");
    writeln!(&mut encoded, "entities: {}", report.queried_entities)
        .expect("writing to a String cannot fail");
    writeln!(&mut encoded, "table: {}", report.table_id).expect("writing to a String cannot fail");
    writeln!(&mut encoded, "camera: {}", report.camera_id)
        .expect("writing to a String cannot fail");
    writeln!(&mut encoded, "color frame: {}", report.color_frame)
        .expect("writing to a String cannot fail");
    writeln!(&mut encoded, "entity-ID frame: {}", report.entity_id_frame)
        .expect("writing to a String cannot fail");
    writeln!(
        &mut encoded,
        "visibility frame: {}",
        report.visibility_frame
    )
    .expect("writing to a String cannot fail");
    writeln!(&mut encoded, "center color: {}", report.center_color)
        .expect("writing to a String cannot fail");
    writeln!(&mut encoded, "center entity: {}", report.center_entity_id)
        .expect("writing to a String cannot fail");
    writeln!(
        &mut encoded,
        "table visible pixels: {}",
        report.table_visible_pixels
    )
    .expect("writing to a String cannot fail");
    writeln!(&mut encoded, "logical hash: {}", report.logical_hash)
        .expect("writing to a String cannot fail");
    writeln!(
        &mut encoded,
        "replayed logical hash: {}",
        report.replayed_logical_hash
    )
    .expect("writing to a String cannot fail");
    writeln!(&mut encoded, "replay entries: {}", report.replay_entries)
        .expect("writing to a String cannot fail");
    writeln!(&mut encoded, "replay bytes: {}", report.replay_bytes)
        .expect("writing to a String cannot fail");
    encoded
}

fn encode_json(report: &ScenarioReport) -> Result<Vec<u8>, serde_json::Error> {
    let mut encoded = Vec::new();
    serde_json::to_writer(&mut encoded, report)?;
    encoded.push(b'\n');
    Ok(encoded)
}

#[derive(Debug, Serialize)]
struct ScenarioReport {
    schema_version: u32,
    scenario: &'static str,
    profile: String,
    passed: bool,
    observation_width: u32,
    observation_height: u32,
    adapter: AdapterReport,
    scene_revision: u64,
    queried_entities: u32,
    table_id: String,
    camera_id: String,
    color_frame: u64,
    entity_id_frame: u64,
    visibility_frame: u64,
    center_color: String,
    center_entity_id: String,
    table_visible_pixels: u64,
    logical_hash: String,
    replayed_logical_hash: String,
    replay_entries: u32,
    replay_bytes: u64,
}

impl ScenarioReport {
    fn from_result(adapter: &AdapterSummary, report: &CanonicalScenarioReport) -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            scenario: SCENARIO,
            profile: format!("default-local-{LOCAL_PROFILE_WIDTH}x{LOCAL_PROFILE_HEIGHT}"),
            passed: true,
            observation_width: LOCAL_PROFILE_WIDTH,
            observation_height: LOCAL_PROFILE_HEIGHT,
            adapter: AdapterReport {
                name: adapter.name.clone(),
                backend: adapter.backend.clone(),
                device_type: adapter.device_type.clone(),
                webgpu_compliant: adapter.webgpu_compliant,
            },
            scene_revision: report.update_receipt.new_revision.get(),
            queried_entities: report.queried_entities,
            table_id: report.table_id.to_string(),
            camera_id: report.camera_id.to_string(),
            color_frame: report.color.frame_id.get(),
            entity_id_frame: report.entity_id.frame_id.get(),
            visibility_frame: report.visibility.frame_id.get(),
            center_color: format!(
                "#{:02x}{:02x}{:02x}{:02x}",
                report.center_color[0],
                report.center_color[1],
                report.center_color[2],
                report.center_color[3]
            ),
            center_entity_id: report.center_entity_id.to_string(),
            table_visible_pixels: report.table_visible_pixels,
            logical_hash: report.logical_hash.to_string(),
            replayed_logical_hash: report.replayed_logical_hash.to_string(),
            replay_entries: report.replay.entry_count(),
            replay_bytes: report.replay_bytes,
        }
    }
}

#[derive(Debug, Serialize)]
struct AdapterReport {
    name: String,
    backend: String,
    device_type: String,
    webgpu_compliant: bool,
}

#[cfg(test)]
mod tests {
    use super::{AdapterReport, ScenarioReport, encode_human, encode_json};

    fn fixture() -> ScenarioReport {
        ScenarioReport {
            schema_version: 1,
            scenario: "canonical-mvp-v1",
            profile: "default-local-64x64".to_owned(),
            passed: true,
            observation_width: 64,
            observation_height: 64,
            adapter: AdapterReport {
                name: "Test Adapter".to_owned(),
                backend: "Vulkan".to_owned(),
                device_type: "cpu".to_owned(),
                webgpu_compliant: true,
            },
            scene_revision: 2,
            queried_entities: 4,
            table_id: "00000000000000000000000000000200".to_owned(),
            camera_id: "00000000000000000000000000000400".to_owned(),
            color_frame: 1,
            entity_id_frame: 2,
            visibility_frame: 3,
            center_color: "#371e0bff".to_owned(),
            center_entity_id: "00000000000000000000000000000200".to_owned(),
            table_visible_pixels: 72,
            logical_hash: "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
                .to_owned(),
            replayed_logical_hash:
                "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef".to_owned(),
            replay_entries: 2,
            replay_bytes: 704,
        }
    }

    #[test]
    fn human_report_preserves_the_existing_contract() {
        assert_eq!(
            encode_human(&fixture()),
            concat!(
                "Cogniform canonical scenario passed\n",
                "adapter: Test Adapter\n",
                "backend: Vulkan\n",
                "device type: cpu\n",
                "WebGPU compliant: true\n",
                "revision: 2\n",
                "entities: 4\n",
                "table: 00000000000000000000000000000200\n",
                "camera: 00000000000000000000000000000400\n",
                "color frame: 1\n",
                "entity-ID frame: 2\n",
                "visibility frame: 3\n",
                "center color: #371e0bff\n",
                "center entity: 00000000000000000000000000000200\n",
                "table visible pixels: 72\n",
                "logical hash: 0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef\n",
                "replayed logical hash: 0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef\n",
                "replay entries: 2\n",
                "replay bytes: 704\n",
            )
        );
    }

    #[test]
    fn json_report_is_compact_versioned_and_ordered() {
        let encoded = encode_json(&fixture()).unwrap();
        assert_eq!(
            String::from_utf8(encoded).unwrap(),
            concat!(
                "{\"schema_version\":1,\"scenario\":\"canonical-mvp-v1\",",
                "\"profile\":\"default-local-64x64\",\"passed\":true,",
                "\"observation_width\":64,\"observation_height\":64,",
                "\"adapter\":{\"name\":\"Test Adapter\",\"backend\":\"Vulkan\",",
                "\"device_type\":\"cpu\",\"webgpu_compliant\":true},",
                "\"scene_revision\":2,\"queried_entities\":4,",
                "\"table_id\":\"00000000000000000000000000000200\",",
                "\"camera_id\":\"00000000000000000000000000000400\",",
                "\"color_frame\":1,\"entity_id_frame\":2,\"visibility_frame\":3,",
                "\"center_color\":\"#371e0bff\",",
                "\"center_entity_id\":\"00000000000000000000000000000200\",",
                "\"table_visible_pixels\":72,",
                "\"logical_hash\":\"0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef\",",
                "\"replayed_logical_hash\":\"0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef\",",
                "\"replay_entries\":2,\"replay_bytes\":704}\n",
            )
        );
    }
}
