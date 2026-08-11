//! Black-box coverage for the bounded MCP standard-stream adapter.

use std::{
    io::{BufRead as _, BufReader, Read as _, Write as _},
    process::{Child, ChildStderr, ChildStdin, ChildStdout, Command, Output, Stdio},
};

use cogniform_observation::{ObservationPayload, ObservationPayloadLimits, decode_payload};
use cogniform_protocol::{ObservationMetadata, RuntimeLimits};
use serde_json::{Value, json};

const MCP_SERVER_INSTRUCTIONS: &str = "Fresh child: call query_scene with scene_revision 0. Thereafter use exact revisions from receipts or metadata. Use submit_imagination for semantic changes or apply_patch for direct changes; reuse transaction_id and idempotency_key only for an exact retry. Add a Camera before observe_scene, then read its cogniform:// resource. Calls are serialized. Discard the child after service_failed, invalid_service_output, observation_timeout, or mutating output_unavailable; never infer or retry an uncertain effect.";

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
        json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "server/discover",
            "params": {
                "_meta": {
                    "io.modelcontextprotocol/protocolVersion": "2025-11-25",
                    "io.modelcontextprotocol/clientInfo": {
                        "name": "cogniform-cli-test",
                        "version": "1"
                    },
                    "io.modelcontextprotocol/clientCapabilities": {}
                }
            }
        }),
        json!({
            "jsonrpc": "2.0",
            "id": 3,
            "method": "tasks/get",
            "params": {"taskId": "not-admitted"}
        }),
        json!({
            "jsonrpc": "2.0",
            "id": 4,
            "method": "tools/list",
            "params": {
                "_meta": {
                    "io.modelcontextprotocol/protocolVersion": "2026-07-28"
                }
            }
        }),
        json!({"jsonrpc": "2.0", "id": 5, "method": "tools/list", "params": {}}),
        json!({
            "jsonrpc": "2.0",
            "id": 6,
            "method": "tools/call",
            "params": {"name": "cogniform.query_scene", "arguments": {}}
        }),
        json!({"jsonrpc": "2.0", "id": 7, "method": "resources/list", "params": {}}),
    ]);
    assert!(output.status.success(), "{}", normalize(&output.stderr));
    assert!(output.stderr.is_empty());
    let responses = json_lines(&output.stdout);
    assert_eq!(responses.len(), 7);
    assert_eq!(responses[0]["id"], 1);
    assert_eq!(responses[0]["result"]["protocolVersion"], "2025-11-25");
    assert_eq!(
        responses[0]["result"]["instructions"],
        MCP_SERVER_INSTRUCTIONS
    );
    assert_eq!(MCP_SERVER_INSTRUCTIONS.len(), 508);
    assert!(responses[0]["result"].get("resultType").is_none());
    assert!(
        responses[0]["result"]["capabilities"]
            .get("extensions")
            .is_none()
    );
    assert_eq!(responses[1]["id"], 2);
    assert_eq!(responses[1]["error"]["code"], -32601);
    assert_eq!(responses[2]["id"], 3);
    assert_eq!(responses[2]["error"]["code"], -32601);
    assert_eq!(responses[3]["id"], 4);
    assert_eq!(responses[3]["error"]["code"], -32022);
    assert_eq!(
        responses[3]["error"]["data"],
        json!({"requested": "2026-07-28", "supported": ["2025-11-25"]})
    );
    assert_eq!(responses[4]["id"], 5);
    assert_eq!(
        responses[4]["result"]["tools"]
            .as_array()
            .unwrap()
            .iter()
            .map(|tool| tool["name"].as_str().unwrap())
            .collect::<Vec<_>>(),
        [
            "cogniform.query_scene",
            "cogniform.submit_imagination",
            "cogniform.apply_patch",
            "cogniform.observe_scene"
        ]
    );
    assert!(responses[4]["result"].get("resultType").is_none());
    for tool in responses[4]["result"]["tools"].as_array().unwrap() {
        assert!(tool.get("execution").is_none());
        assert!(tool.get("taskSupport").is_none());
    }
    assert_eq!(responses[5]["id"], 6);
    assert!(responses[5]["result"].get("resultType").is_none());
    assert_eq!(
        responses[5]["result"]["structuredContent"],
        json!({"schema_version": 1, "error": "invalid_arguments"})
    );
    assert_eq!(responses[6]["id"], 7);
    assert!(responses[6]["result"].get("resultType").is_none());
    assert_eq!(responses[6]["result"]["resources"], json!([]));
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
    assert_missing_camera_observation_preserves_empty_resources(&mut session);

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
    assert_observation_resource(&mut session);
    session.finish();
}

fn assert_missing_camera_observation_preserves_empty_resources(session: &mut Session) {
    let rejected = session.call(20, "cogniform.observe_scene", &observation_request(0x40, 0));
    assert_eq!(
        rejected["result"]["structuredContent"]["error"],
        "observation_rejected"
    );
    let resources = session.send(&json!({
        "jsonrpc": "2.0",
        "id": 21,
        "method": "resources/list",
        "params": {}
    }));
    assert_eq!(resources["result"]["resources"], json!([]));
}

fn assert_observation_resource(session: &mut Session) {
    let observed = session.call(10, "cogniform.observe_scene", &observation_request(0x44, 2));
    assert_eq!(observed["result"]["isError"], false);
    let output = &observed["result"]["structuredContent"];
    assert_eq!(output["metadata"]["scene_revision"], 2);
    assert_eq!(output["metadata"]["kind"], "visibility");
    let uri = output["resource_uri"].as_str().unwrap();
    let resources = session.send(&json!({
        "jsonrpc": "2.0",
        "id": 11,
        "method": "resources/list",
        "params": {}
    }));
    assert_eq!(
        resources["result"]["resources"].as_array().unwrap().len(),
        1
    );
    assert_eq!(resources["result"]["resources"][0]["uri"], uri);
    let read = session.send(&json!({
        "jsonrpc": "2.0",
        "id": 12,
        "method": "resources/read",
        "params": {"uri": uri}
    }));
    assert!(read["result"].get("resultType").is_none());
    let blob = read["result"]["contents"][0]["blob"].as_str().unwrap();
    let envelope = decode_base64(blob).unwrap();
    assert_eq!(
        u64::try_from(envelope.len()).unwrap(),
        resources["result"]["resources"][0]["size"]
            .as_u64()
            .unwrap()
    );
    let metadata: ObservationMetadata = serde_json::from_value(output["metadata"].clone()).unwrap();
    let payload = decode_payload(
        &metadata,
        &envelope,
        &RuntimeLimits::default(),
        ObservationPayloadLimits::default(),
    )
    .unwrap();
    assert!(matches!(payload, ObservationPayload::Visibility(_)));
}

fn observation_request(observation_id: u128, scene_revision: u64) -> Value {
    json!({
        "schema_version": 1,
        "observation_id": format!("{observation_id:032x}"),
        "scene_revision": scene_revision,
        "camera_id": "00000000000000000000000000000031",
        "kind": "visibility",
        "quality": "low"
    })
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

fn decode_base64(encoded: &str) -> Result<Vec<u8>, ()> {
    if !encoded.len().is_multiple_of(4) {
        return Err(());
    }
    let mut output = Vec::with_capacity(encoded.len() / 4 * 3);
    for chunk in encoded.as_bytes().chunks_exact(4) {
        let first = base64_value(chunk[0])?;
        let second = base64_value(chunk[1])?;
        let third = if chunk[2] == b'=' {
            0
        } else {
            base64_value(chunk[2])?
        };
        let fourth = if chunk[3] == b'=' {
            0
        } else {
            base64_value(chunk[3])?
        };
        let bits = (u32::from(first) << 18)
            | (u32::from(second) << 12)
            | (u32::from(third) << 6)
            | u32::from(fourth);
        output.push(u8::try_from(bits >> 16).unwrap());
        if chunk[2] != b'=' {
            output.push(u8::try_from((bits >> 8) & 0xff).unwrap());
        }
        if chunk[3] != b'=' {
            output.push(u8::try_from(bits & 0xff).unwrap());
        }
    }
    Ok(output)
}

fn base64_value(byte: u8) -> Result<u8, ()> {
    match byte {
        b'A'..=b'Z' => Ok(byte - b'A'),
        b'a'..=b'z' => Ok(byte - b'a' + 26),
        b'0'..=b'9' => Ok(byte - b'0' + 52),
        b'+' => Ok(62),
        b'/' => Ok(63),
        _ => Err(()),
    }
}

fn normalize(encoded: &[u8]) -> String {
    String::from_utf8_lossy(encoded).replace("\r\n", "\n")
}

fn command() -> Command {
    Command::new(env!("CARGO_BIN_EXE_cogniform-cli"))
}
