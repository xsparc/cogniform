//! Black-box coverage for the bounded MCP standard-stream adapter.

use std::{
    io::{BufRead as _, BufReader, Read as _, Write as _},
    process::{Child, ChildStderr, ChildStdin, ChildStdout, Command, Output, Stdio},
};

use serde_json::{Value, json};

#[test]
fn arguments_are_exact_before_protocol_mode() {
    for arguments in [
        &["serve-mcp-stdio", "unexpected"][..],
        &["serve-mcp-stdio", "--help"][..],
        &["serve-mcp-stdio", "--"][..],
    ] {
        let output = command().args(arguments).output().unwrap();
        assert!(!output.status.success());
        assert!(output.stdout.is_empty());
        assert_eq!(
            normalize(&output.stderr),
            "error: serve-mcp-stdio accepts no arguments\n"
        );
    }
}

#[test]
fn initialize_list_and_eof_keep_stdout_protocol_pure() {
    let output = run_batch(&[
        initialize(1),
        json!({"jsonrpc": "2.0", "method": "notifications/initialized"}),
        json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list", "params": {}}),
    ]);
    assert!(output.status.success(), "{}", normalize(&output.stderr));
    assert!(output.stderr.is_empty());
    let responses = json_lines(&output.stdout);
    assert_eq!(responses.len(), 2);
    assert_eq!(responses[0]["id"], 1);
    assert_eq!(responses[0]["result"]["protocolVersion"], "2025-11-25");
    assert_eq!(responses[1]["id"], 2);
    assert_eq!(
        responses[1]["result"]["tools"]
            .as_array()
            .unwrap()
            .iter()
            .map(|tool| tool["name"].as_str().unwrap())
            .collect::<Vec<_>>(),
        [
            "cogniform.query_scene",
            "cogniform.submit_imagination",
            "cogniform.apply_patch"
        ]
    );
}

#[test]
fn malformed_input_is_redacted_and_emits_no_stdout() {
    let mut child = command()
        .arg("serve-mcp-stdio")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(b"not-json\n")
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    assert_eq!(
        normalize(&output.stderr),
        "error: serve-mcp-stdio transport failed: invalid_message\n"
    );
}

#[test]
#[ignore = "requires an approved DX12 or Vulkan conformance adapter"]
fn controlled_child_applies_patch_and_imagination_replays_and_closes_cleanly() {
    let mut session = Session::start();
    let initial = session.call(
        2,
        "cogniform.query_scene",
        &json!({
            "schema_version": 1,
            "scene_revision": 0,
            "entity_ids": [],
            "component_kinds": [],
            "limit": 4
        }),
    );
    assert_eq!(initial["result"]["isError"], false);
    assert_eq!(
        initial["result"]["structuredContent"]["entities"],
        json!([])
    );

    let patch = camera_patch();
    let patch_applied = session.call(3, "cogniform.apply_patch", &patch);
    assert_eq!(
        patch_applied["result"]["structuredContent"]["admission"],
        "queued"
    );
    assert_eq!(
        patch_applied["result"]["structuredContent"]["receipt"]["new_revision"],
        1
    );
    let patch_replayed = session.call(4, "cogniform.apply_patch", &patch);
    assert_eq!(
        patch_replayed["result"]["structuredContent"]["admission"],
        "replayed"
    );
    assert_eq!(
        patch_replayed["result"]["structuredContent"]["receipt"]["status"],
        "idempotent_replay"
    );
    let mut conflicting_patch = patch.clone();
    conflicting_patch["transaction_id"] = json!("00000000000000000000000000000012");
    conflicting_patch["base_revision"] = json!(1);
    let conflicting = session.call(5, "cogniform.apply_patch", &conflicting_patch);
    assert_eq!(
        conflicting["result"]["structuredContent"]["error"],
        "patch_rejected"
    );

    let mut imagination: Value = serde_json::from_str(include_str!(
        "../../../crates/cogniform-protocol/tests/fixtures/imagination_v1.json"
    ))
    .unwrap();
    imagination["base_revision"] = json!(1);
    let applied = session.call(6, "cogniform.submit_imagination", &imagination);
    assert_eq!(
        applied["result"]["structuredContent"]["admission"],
        "queued"
    );
    assert_eq!(
        applied["result"]["structuredContent"]["receipt"]["new_revision"],
        2
    );
    let replayed = session.call(7, "cogniform.submit_imagination", &imagination);
    assert_eq!(
        replayed["result"]["structuredContent"]["admission"],
        "replayed"
    );
    assert_eq!(
        replayed["result"]["structuredContent"]["receipt"]["new_revision"],
        2
    );

    let mut stale_patch = patch;
    stale_patch["transaction_id"] = json!("00000000000000000000000000000013");
    stale_patch["idempotency_key"] = json!("00000000000000000000000000000023");
    let stale = session.call(8, "cogniform.apply_patch", &stale_patch);
    assert_eq!(
        stale["result"]["structuredContent"]["error"],
        "patch_rejected"
    );

    assert_camera_query(&mut session);
    session.finish();
}

fn assert_camera_query(session: &mut Session) {
    let query = session.call(
        9,
        "cogniform.query_scene",
        &json!({
            "schema_version": 1,
            "scene_revision": 2,
            "entity_ids": ["00000000000000000000000000000031"],
            "component_kinds": ["local_transform", "camera"],
            "limit": 1
        }),
    );
    assert_eq!(
        query["result"]["structuredContent"]["entities"],
        json!([{
            "entity_id": "00000000000000000000000000000031",
            "parent_id": null,
            "components": [
                {
                    "component": "local_transform",
                    "value": {
                        "translation": {"x": 0.0, "y": 0.0, "z": 3.0},
                        "rotation": {"x": 0.0, "y": 0.0, "z": 0.0, "w": 1.0},
                        "scale": {"x": 1.0, "y": 1.0, "z": 1.0}
                    }
                },
                {
                    "component": "camera",
                    "value": {
                        "vertical_fov_radians": 1.0,
                        "near": 0.100_000_001_490_116_12,
                        "far": 100.0
                    }
                }
            ]
        }])
    );
}

struct Session {
    child: Child,
    input: Option<ChildStdin>,
    output: BufReader<ChildStdout>,
    error: ChildStderr,
}

impl Session {
    fn start() -> Self {
        let mut child = command()
            .arg("serve-mcp-stdio")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let input = child.stdin.take().unwrap();
        let output = BufReader::new(child.stdout.take().unwrap());
        let error = child.stderr.take().unwrap();
        let mut session = Self {
            child,
            input: Some(input),
            output,
            error,
        };
        let response = session.send(&initialize(1));
        assert_eq!(response["result"]["protocolVersion"], "2025-11-25");
        session.notify(&json!({
            "jsonrpc": "2.0",
            "method": "notifications/initialized"
        }));
        session
    }

    fn call(&mut self, id: u64, name: &str, arguments: &Value) -> Value {
        self.send(&json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": "tools/call",
            "params": {"name": name, "arguments": arguments}
        }))
    }

    fn send(&mut self, message: &Value) -> Value {
        self.notify(message);
        let mut line = String::new();
        self.output.read_line(&mut line).unwrap();
        assert!(!line.is_empty(), "MCP child closed before responding");
        serde_json::from_str(&line).unwrap()
    }

    fn notify(&mut self, message: &Value) {
        let input = self.input.as_mut().unwrap();
        serde_json::to_writer(&mut *input, &message).unwrap();
        input.write_all(b"\n").unwrap();
        input.flush().unwrap();
    }

    fn finish(mut self) {
        drop(self.input.take());
        let status = self.child.wait().unwrap();
        let mut stdout_tail = Vec::new();
        self.output.read_to_end(&mut stdout_tail).unwrap();
        let mut stderr = Vec::new();
        self.error.read_to_end(&mut stderr).unwrap();
        assert!(status.success(), "{}", normalize(&stderr));
        assert!(stdout_tail.is_empty());
        assert!(stderr.is_empty());
    }
}

fn initialize(id: u64) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "method": "initialize",
        "params": {
            "protocolVersion": "2025-11-25",
            "capabilities": {},
            "clientInfo": {"name": "cogniform-cli-test", "version": "1"}
        }
    })
}

fn camera_patch() -> Value {
    json!({
        "schema_version": 1,
        "transaction_id": "00000000000000000000000000000011",
        "idempotency_key": "00000000000000000000000000000021",
        "base_revision": 0,
        "conflict_policy": "require_exact_base",
        "delivery": {"mode": "must_apply"},
        "declared_budget": {
            "max_operations": 8,
            "max_components": 16,
            "max_text_bytes": 128,
            "max_decoded_bytes": 2048
        },
        "operations": [{
            "operation": "create",
            "value": {
                "entity_id": "00000000000000000000000000000031",
                "components": [
                    {
                        "component": "local_transform",
                        "value": {
                            "translation": {"x": 0.0, "y": 0.0, "z": 3.0},
                            "rotation": {"x": 0.0, "y": 0.0, "z": 0.0, "w": 1.0},
                            "scale": {"x": 1.0, "y": 1.0, "z": 1.0}
                        }
                    },
                    {
                        "component": "camera",
                        "value": {
                            "vertical_fov_radians": 1.0,
                            "near": 0.1,
                            "far": 100.0
                        }
                    }
                ]
            }
        }]
    })
}

fn run_batch(messages: &[Value]) -> Output {
    let mut child = command()
        .arg("serve-mcp-stdio")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    {
        let mut input = child.stdin.take().unwrap();
        for message in messages {
            serde_json::to_writer(&mut input, message).unwrap();
            input.write_all(b"\n").unwrap();
        }
    }
    child.wait_with_output().unwrap()
}

fn json_lines(encoded: &[u8]) -> Vec<Value> {
    std::str::from_utf8(encoded)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

fn normalize(encoded: &[u8]) -> String {
    String::from_utf8_lossy(encoded).replace("\r\n", "\n")
}

fn command() -> Command {
    Command::new(env!("CARGO_BIN_EXE_cogniform-cli"))
}
