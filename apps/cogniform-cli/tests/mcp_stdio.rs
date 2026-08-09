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
        ["cogniform.query_scene", "cogniform.submit_imagination"]
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
fn controlled_child_queries_applies_replays_and_closes_cleanly() {
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

    let mut imagination: Value = serde_json::from_str(include_str!(
        "../../../crates/cogniform-protocol/tests/fixtures/imagination_v1.json"
    ))
    .unwrap();
    imagination["base_revision"] = json!(0);
    let applied = session.call(3, "cogniform.submit_imagination", &imagination);
    assert_eq!(
        applied["result"]["structuredContent"]["admission"],
        "queued"
    );
    assert_eq!(
        applied["result"]["structuredContent"]["receipt"]["new_revision"],
        1
    );
    let replayed = session.call(4, "cogniform.submit_imagination", &imagination);
    assert_eq!(
        replayed["result"]["structuredContent"]["admission"],
        "replayed"
    );
    assert_eq!(
        replayed["result"]["structuredContent"]["receipt"]["new_revision"],
        1
    );

    let final_query = session.call(
        5,
        "cogniform.query_scene",
        &json!({
            "schema_version": 1,
            "scene_revision": 1,
            "entity_ids": [],
            "component_kinds": [],
            "limit": 4
        }),
    );
    assert_eq!(
        final_query["result"]["structuredContent"]["entities"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    session.finish();
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
