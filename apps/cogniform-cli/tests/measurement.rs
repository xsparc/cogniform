//! Black-box coverage for the controlled CPU measurement command.

use std::process::{Command, Output};

use serde_json::Value;

const WARNING: &str = "warning: measure-world evidence should be collected with --release\n";
const DISTRIBUTIONS: [&str; 5] = [
    "apply_total",
    "validate_and_preflight",
    "atomic_commit",
    "render_extraction",
    "logical_hash",
];

#[test]
fn human_measurement_report_preserves_labels_order_and_warning() {
    let output = command().arg("measure-world").output().unwrap();
    assert!(output.status.success());
    assert_warning(&output);
    let stdout = normalized_stdout(output);
    let lines = stdout.lines().collect::<Vec<_>>();
    assert_eq!(lines.len(), 12);
    assert_eq!(lines[0], "Cogniform controlled world measurement");
    assert_eq!(lines[1], "fixture: world-create-empty-v1");
    assert_eq!(lines[2], format!("profile: {}", expected_profile()));
    assert_eq!(lines[3], "operations per sample: 1000");
    assert_eq!(lines[4], "warmup samples: 5");
    assert_eq!(lines[5], "measured samples: 30");
    assert_human_distribution(lines[6], "apply total");
    assert_human_distribution(lines[7], "validate and preflight");
    assert_human_distribution(lines[8], "atomic commit");
    assert_human_distribution(lines[9], "render extraction");
    assert_human_distribution(lines[10], "logical hash");
    assert_eq!(
        lines[11],
        "threshold: informational only; no release or merge gate"
    );
}

#[test]
fn json_measurement_report_is_versioned_ordered_typed_and_informational() {
    let output = command()
        .args(["measure-world", "--json"])
        .output()
        .unwrap();
    assert!(output.status.success());
    assert_warning(&output);
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.ends_with('\n'));
    assert!(!stdout.contains('\r'));
    assert_eq!(stdout.bytes().filter(|byte| *byte == b'\n').count(), 1);
    assert_key_order(
        &stdout,
        &[
            "schema_version",
            "fixture",
            "profile",
            "operations_per_sample",
            "warmup_samples",
            "measured_samples",
            "unit",
            "informational_only",
            "apply_total",
            "validate_and_preflight",
            "atomic_commit",
            "render_extraction",
            "logical_hash",
        ],
    );

    let report: Value = serde_json::from_str(&stdout).unwrap();
    let object = report.as_object().unwrap();
    assert_eq!(object.len(), 13);
    assert_eq!(object["schema_version"].as_u64(), Some(1));
    assert_eq!(object["fixture"], "world-create-empty-v1");
    assert_eq!(object["profile"], expected_profile());
    assert_eq!(object["operations_per_sample"].as_u64(), Some(1_000));
    assert_eq!(object["warmup_samples"].as_u64(), Some(5));
    assert_eq!(object["measured_samples"].as_u64(), Some(30));
    assert_eq!(object["unit"], "nanoseconds");
    assert_eq!(object["informational_only"].as_bool(), Some(true));
    for name in DISTRIBUTIONS {
        assert_nested_key_order(&stdout, name);
        assert_json_distribution(&object[name]);
    }
}

#[test]
fn measurement_arguments_are_exact_and_leave_stdout_empty() {
    for arguments in [
        &["measure-world", "unexpected"][..],
        &["measure-world", "--"][..],
        &["measure-world", "--json", "extra"][..],
    ] {
        let output = command().args(arguments).output().unwrap();
        assert!(!output.status.success());
        assert!(output.stdout.is_empty());
        assert_eq!(
            String::from_utf8(output.stderr)
                .unwrap()
                .replace("\r\n", "\n"),
            "error: measure-world accepts only optional --json\n"
        );
    }
}

fn assert_warning(output: &Output) {
    let stderr = String::from_utf8(output.stderr.clone())
        .unwrap()
        .replace("\r\n", "\n");
    if cfg!(debug_assertions) {
        assert_eq!(stderr, WARNING);
    } else {
        assert!(stderr.is_empty());
    }
}

fn assert_human_distribution(line: &str, label: &str) {
    let values = line
        .strip_prefix(&format!("{label} (microseconds): min="))
        .unwrap();
    let values = values
        .split(" median=")
        .flat_map(|part| part.split(" p95="))
        .flat_map(|part| part.split(" max="))
        .map(parse_micros)
        .collect::<Vec<_>>();
    assert_distribution_order(&values);
}

fn parse_micros(value: &str) -> u128 {
    let (whole, fraction) = value.split_once('.').unwrap();
    assert_eq!(fraction.len(), 3);
    whole.parse::<u128>().unwrap() * 1_000 + fraction.parse::<u128>().unwrap()
}

fn assert_json_distribution(value: &Value) {
    let object = value.as_object().unwrap();
    assert_eq!(object.len(), 4);
    let values = [
        object["min"].as_u64().unwrap(),
        object["median"].as_u64().unwrap(),
        object["p95"].as_u64().unwrap(),
        object["max"].as_u64().unwrap(),
    ];
    assert_distribution_order(&values);
}

fn assert_nested_key_order(json: &str, name: &str) {
    let marker = format!("\"{name}\":{{");
    let start = json.find(&marker).unwrap() + marker.len();
    let end = start + json[start..].find('}').unwrap();
    assert_key_order(&json[start..end], &["min", "median", "p95", "max"]);
}

fn assert_distribution_order<T: Ord + std::fmt::Debug>(values: &[T]) {
    assert_eq!(values.len(), 4);
    assert!(values[0] <= values[1]);
    assert!(values[1] <= values[2]);
    assert!(values[2] <= values[3]);
}

fn assert_key_order(json: &str, keys: &[&str]) {
    let mut offset = 0;
    for key in keys {
        let marker = format!("\"{key}\":");
        let found = json[offset..].find(&marker).unwrap();
        offset += found + marker.len();
    }
}

fn expected_profile() -> &'static str {
    if cfg!(debug_assertions) {
        "debug"
    } else {
        "release"
    }
}

fn normalized_stdout(output: Output) -> String {
    String::from_utf8(output.stdout)
        .unwrap()
        .replace("\r\n", "\n")
}

fn command() -> Command {
    Command::new(env!("CARGO_BIN_EXE_cogniform-cli"))
}
