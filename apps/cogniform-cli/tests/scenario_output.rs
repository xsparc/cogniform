//! Black-box coverage for canonical scenario output modes and argument parsing.

use std::process::{Command, Output};

use serde_json::Value;

const TOP_LEVEL_KEYS: [&str; 21] = [
    "schema_version",
    "scenario",
    "profile",
    "passed",
    "observation_width",
    "observation_height",
    "adapter",
    "scene_revision",
    "queried_entities",
    "table_id",
    "camera_id",
    "color_frame",
    "entity_id_frame",
    "visibility_frame",
    "center_color",
    "center_entity_id",
    "table_visible_pixels",
    "logical_hash",
    "replayed_logical_hash",
    "replay_entries",
    "replay_bytes",
];

#[test]
fn scenario_arguments_are_exact_and_leave_stdout_empty() {
    for arguments in [
        &["scenario", "unexpected"][..],
        &["scenario", "--"][..],
        &["scenario", "--json", "extra"][..],
        &["scenario", "--json", "--json"][..],
    ] {
        let output = command().args(arguments).output().unwrap();
        assert!(!output.status.success());
        assert!(output.stdout.is_empty());
        assert_eq!(
            normalize(output.stderr),
            "error: scenario accepts only optional --json\n"
        );
    }
}

#[test]
#[ignore = "requires a controlled headless GPU adapter"]
fn human_and_json_modes_prove_the_same_canonical_scenario() {
    let human = successful(command().arg("scenario").output().unwrap());
    let json = successful(command().args(["scenario", "--json"]).output().unwrap());

    let human = String::from_utf8(human.stdout).unwrap();
    let json = String::from_utf8(json.stdout).unwrap();
    assert!(human.ends_with('\n'));
    assert!(!human.contains('\r'));
    assert_eq!(human.bytes().filter(|byte| *byte == b'\n').count(), 19);
    assert!(json.ends_with('\n'));
    assert!(!json.contains('\r'));
    assert_eq!(json.bytes().filter(|byte| *byte == b'\n').count(), 1);
    assert_key_order(&json, &TOP_LEVEL_KEYS);
    assert_nested_key_order(
        &json,
        "adapter",
        &["name", "backend", "device_type", "webgpu_compliant"],
    );

    let report: Value = serde_json::from_str(&json).unwrap();
    let object = report.as_object().unwrap();
    assert_eq!(object.len(), TOP_LEVEL_KEYS.len());
    assert_eq!(object["schema_version"].as_u64(), Some(1));
    assert_eq!(object["scenario"], "canonical-mvp-v1");
    assert_eq!(object["profile"], "default-local-64x64");
    assert_eq!(object["passed"].as_bool(), Some(true));
    assert_eq!(object["observation_width"].as_u64(), Some(64));
    assert_eq!(object["observation_height"].as_u64(), Some(64));
    assert_eq!(object["scene_revision"].as_u64(), Some(2));
    assert_eq!(object["queried_entities"].as_u64(), Some(4));
    assert_eq!(object["replay_entries"].as_u64(), Some(2));
    assert!(object["replay_bytes"].as_u64().unwrap() > 0);

    let adapter = object["adapter"].as_object().unwrap();
    assert_eq!(adapter.len(), 4);
    assert!(!adapter["name"].as_str().unwrap().is_empty());
    assert!(!adapter["backend"].as_str().unwrap().is_empty());
    assert!(!adapter["device_type"].as_str().unwrap().is_empty());
    assert!(adapter["webgpu_compliant"].is_boolean());

    let table_id = object["table_id"].as_str().unwrap();
    let camera_id = object["camera_id"].as_str().unwrap();
    let center_entity_id = object["center_entity_id"].as_str().unwrap();
    assert_lower_hex(table_id, 32, false);
    assert_lower_hex(camera_id, 32, false);
    assert_eq!(center_entity_id, table_id);
    assert_lower_hex(object["center_color"].as_str().unwrap(), 8, true);
    assert_lower_hex(object["logical_hash"].as_str().unwrap(), 64, false);
    assert_eq!(object["logical_hash"], object["replayed_logical_hash"]);
    let color_frame = object["color_frame"].as_u64().unwrap();
    let entity_id_frame = object["entity_id_frame"].as_u64().unwrap();
    let visibility_frame = object["visibility_frame"].as_u64().unwrap();
    assert!(color_frame < entity_id_frame);
    assert!(entity_id_frame < visibility_frame);
    assert!(object["table_visible_pixels"].as_u64().unwrap() > 0);

    assert_eq!(human_value(&human, "adapter"), adapter["name"]);
    assert_eq!(human_value(&human, "backend"), adapter["backend"]);
    assert_eq!(human_value(&human, "device type"), adapter["device_type"]);
    assert_eq!(
        human_value(&human, "WebGPU compliant"),
        adapter["webgpu_compliant"].as_bool().unwrap().to_string()
    );
    assert_eq!(human_value(&human, "revision"), "2");
    assert_eq!(human_value(&human, "entities"), "4");
    assert_eq!(human_value(&human, "table"), table_id);
    assert_eq!(human_value(&human, "camera"), camera_id);
    assert_eq!(human_value(&human, "color frame"), color_frame.to_string());
    assert_eq!(
        human_value(&human, "entity-ID frame"),
        entity_id_frame.to_string()
    );
    assert_eq!(
        human_value(&human, "visibility frame"),
        visibility_frame.to_string()
    );
    assert_eq!(human_value(&human, "center entity"), table_id);
    assert_eq!(human_value(&human, "center color"), object["center_color"]);
    assert_eq!(
        human_value(&human, "table visible pixels"),
        object["table_visible_pixels"].as_u64().unwrap().to_string()
    );
    assert_eq!(human_value(&human, "logical hash"), object["logical_hash"]);
    assert_eq!(
        human_value(&human, "replayed logical hash"),
        object["replayed_logical_hash"]
    );
    assert_eq!(
        human_value(&human, "replay entries"),
        object["replay_entries"].as_u64().unwrap().to_string()
    );
    assert_eq!(
        human_value(&human, "replay bytes"),
        object["replay_bytes"].as_u64().unwrap().to_string()
    );
    assert!(human.starts_with("Cogniform canonical scenario passed\n"));
    assert_eq!(human.lines().count(), 19);
}

fn successful(output: Output) -> Output {
    assert!(
        output.status.success(),
        "{}",
        normalize(output.stderr.clone())
    );
    assert!(output.stderr.is_empty());
    output
}

fn human_value<'a>(human: &'a str, label: &str) -> &'a str {
    let prefix = format!("{label}: ");
    human
        .lines()
        .find_map(|line| line.strip_prefix(&prefix))
        .unwrap()
}

fn assert_lower_hex(value: &str, digit_count: usize, prefixed: bool) {
    let digits = if prefixed {
        value.strip_prefix('#').unwrap()
    } else {
        value
    };
    assert_eq!(digits.len(), digit_count);
    assert!(
        digits
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    );
}

fn assert_nested_key_order(json: &str, name: &str, keys: &[&str]) {
    let marker = format!("\"{name}\":{{");
    let start = json.find(&marker).unwrap() + marker.len();
    let end = start + json[start..].find('}').unwrap();
    assert_key_order(&json[start..end], keys);
}

fn assert_key_order(json: &str, keys: &[&str]) {
    let mut offset = 0;
    for key in keys {
        let marker = format!("\"{key}\":");
        let found = json[offset..].find(&marker).unwrap();
        offset += found + marker.len();
    }
}

fn normalize(bytes: Vec<u8>) -> String {
    String::from_utf8(bytes).unwrap().replace("\r\n", "\n")
}

fn command() -> Command {
    Command::new(env!("CARGO_BIN_EXE_cogniform-cli"))
}
