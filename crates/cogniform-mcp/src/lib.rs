//! Bounded Model Context Protocol adapter for one local Cogniform service.
//!
//! This crate owns only protocol translation and stdio framing. Authoritative
//! scene semantics remain in `cogniform-engine`, and the adapter creates no
//! listener, socket, credential store, or ambient network authority.

mod server;
mod transport;

use std::io::IsTerminal as _;

use cogniform_engine::LocalServiceConfig;
use rmcp::{
    ServerHandler as _,
    model::{
        ClientJsonRpcMessage, ClientRequest, EmptyResult, ErrorData, ProtocolVersion,
        ServerJsonRpcMessage, ServerResult,
    },
    service::{RoleServer, serve_directly},
    transport::Transport as _,
};
use tokio::io::{AsyncRead, AsyncWrite};

pub use server::{QUERY_SCENE_TOOL, SUBMIT_IMAGINATION_TOOL};
pub use transport::{McpTransportLimits, TransportFailureKind};

/// Stable MCP revision implemented by this adapter.
pub const MCP_PROTOCOL_VERSION: &str = "2025-11-25";

/// Complete configuration for one inherited-stdio MCP session.
#[derive(Debug, Clone)]
pub struct McpServerConfig {
    /// Bounded local service created lazily on the first tool call.
    pub service: LocalServiceConfig,
    /// Newline framing and JSON resource limits.
    pub transport: McpTransportLimits,
}

impl McpServerConfig {
    /// Creates the fixed local profile used by the command-line adapter.
    #[must_use]
    pub fn local_profile(width: u32, height: u32) -> Self {
        Self {
            service: LocalServiceConfig::new(width, height),
            transport: McpTransportLimits::default(),
        }
    }
}

/// Payload-redacted failure at the MCP composition boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum McpServeError {
    /// MCP stdio requires redirected input and output.
    StdioNotRedirected,
    /// A current-thread async runtime could not be created.
    RuntimeUnavailable,
    /// The MCP initialization exchange was rejected.
    InitializationRejected,
    /// The bounded transport rejected or failed a frame.
    Transport(TransportFailureKind),
    /// The SDK service task did not terminate normally.
    ServiceTaskFailed,
}

impl std::fmt::Display for McpServeError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::StdioNotRedirected => {
                formatter.write_str("serve-mcp-stdio requires redirected standard input and output")
            }
            Self::RuntimeUnavailable => {
                formatter.write_str("serve-mcp-stdio async runtime unavailable")
            }
            Self::InitializationRejected => {
                formatter.write_str("serve-mcp-stdio initialization rejected")
            }
            Self::Transport(kind) => write!(formatter, "serve-mcp-stdio transport failed: {kind}"),
            Self::ServiceTaskFailed => formatter.write_str("serve-mcp-stdio service task failed"),
        }
    }
}

impl std::error::Error for McpServeError {}

/// Runs one MCP session over inherited redirected standard input and output.
pub fn run_stdio(config: McpServerConfig) -> Result<(), McpServeError> {
    if std::io::stdin().is_terminal() || std::io::stdout().is_terminal() {
        return Err(McpServeError::StdioNotRedirected);
    }
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|_| McpServeError::RuntimeUnavailable)?;
    runtime.block_on(serve_io(tokio::io::stdin(), tokio::io::stdout(), config))
}

/// Serves one bounded MCP session over caller-owned async byte streams.
///
/// This entry point exists for controlled conformance tests and local
/// composition. Callers remain responsible for ensuring that the streams do
/// not introduce network or multi-tenant authority.
pub async fn serve_io<R, W>(
    reader: R,
    writer: W,
    config: McpServerConfig,
) -> Result<(), McpServeError>
where
    R: AsyncRead + Unpin + Send + 'static,
    W: AsyncWrite + Unpin + Send + 'static,
{
    let (mut transport, status) =
        transport::BoundedTransport::new(reader, writer, config.transport);
    let handler = server::CogniformMcpServer::new(config.service);
    let client_info = loop {
        let message = transport.receive().await.ok_or_else(|| {
            status.failure().map_or(
                McpServeError::InitializationRejected,
                McpServeError::Transport,
            )
        })?;
        let ClientJsonRpcMessage::Request(request) = message else {
            return Err(McpServeError::InitializationRejected);
        };
        match request.request {
            ClientRequest::PingRequest(_) => {
                transport
                    .send(ServerJsonRpcMessage::response(
                        ServerResult::EmptyResult(EmptyResult {}),
                        request.id,
                    ))
                    .await
                    .map_err(|_| transport_failure(&status))?;
            }
            ClientRequest::InitializeRequest(initialize) => {
                if initialize.params.protocol_version != ProtocolVersion::V_2025_11_25 {
                    transport
                        .send(ServerJsonRpcMessage::error(
                            ErrorData::invalid_params("unsupported protocol version", None),
                            Some(request.id),
                        ))
                        .await
                        .map_err(|_| transport_failure(&status))?;
                    return Err(McpServeError::InitializationRejected);
                }
                transport
                    .send(ServerJsonRpcMessage::response(
                        ServerResult::InitializeResult(handler.get_info()),
                        request.id,
                    ))
                    .await
                    .map_err(|_| transport_failure(&status))?;
                break initialize.params;
            }
            _ => {
                transport
                    .send(ServerJsonRpcMessage::error(
                        ErrorData::invalid_request("initialize required", None),
                        Some(request.id),
                    ))
                    .await
                    .map_err(|_| transport_failure(&status))?;
                return Err(McpServeError::InitializationRejected);
            }
        }
    };
    let running = serve_directly::<RoleServer, _, _, _, _>(handler, transport, Some(client_info));
    running
        .waiting()
        .await
        .map_err(|_| McpServeError::ServiceTaskFailed)?;
    match status.failure() {
        Some(kind) => Err(McpServeError::Transport(kind)),
        None => Ok(()),
    }
}

fn transport_failure(status: &transport::TransportStatus) -> McpServeError {
    status.failure().map_or(
        McpServeError::Transport(TransportFailureKind::OutputFailed),
        McpServeError::Transport,
    )
}

#[cfg(test)]
mod tests {
    use rmcp::{
        ServiceExt as _,
        model::{CallToolRequestParams, ProtocolVersion},
    };
    use serde_json::{Map, Value, json};
    use tokio::io::{duplex, split};

    use super::*;

    fn arguments(value: Value) -> Map<String, Value> {
        match value {
            Value::Object(arguments) => arguments,
            _ => panic!("test arguments must be an object"),
        }
    }

    async fn client_and_server() -> (
        rmcp::service::RunningService<rmcp::service::RoleClient, ()>,
        tokio::task::JoinHandle<Result<(), McpServeError>>,
    ) {
        let (client_stream, server_stream) = duplex(16 * 1024 * 1024);
        let (client_read, client_write) = split(client_stream);
        let (server_read, server_write) = split(server_stream);
        let server = tokio::spawn(serve_io(
            server_read,
            server_write,
            McpServerConfig::local_profile(64, 64),
        ));
        let client = ().serve((client_read, client_write)).await.unwrap();
        (client, server)
    }

    async fn close(
        client: rmcp::service::RunningService<rmcp::service::RoleClient, ()>,
        server: tokio::task::JoinHandle<Result<(), McpServeError>>,
    ) {
        client.cancel().await.unwrap();
        assert_eq!(server.await.unwrap(), Ok(()));
    }

    #[tokio::test]
    async fn official_client_negotiates_and_lists_exact_tools() {
        let (client, server) = client_and_server().await;
        assert_eq!(
            client.peer().peer_info().unwrap().protocol_version,
            ProtocolVersion::V_2025_11_25
        );
        let tools = client.peer().list_all_tools().await.unwrap();
        assert_eq!(
            tools
                .iter()
                .map(|tool| tool.name.as_ref())
                .collect::<Vec<_>>(),
            [QUERY_SCENE_TOOL, SUBMIT_IMAGINATION_TOOL]
        );
        let query = tools[0].annotations.as_ref().unwrap();
        assert_eq!(query.read_only_hint, Some(true));
        assert_eq!(query.destructive_hint, Some(false));
        assert_eq!(query.idempotent_hint, Some(true));
        assert_eq!(query.open_world_hint, Some(false));
        let imagination = tools[1].annotations.as_ref().unwrap();
        assert_eq!(imagination.read_only_hint, Some(false));
        assert_eq!(imagination.destructive_hint, Some(true));
        assert_eq!(imagination.idempotent_hint, Some(true));
        assert_eq!(imagination.open_world_hint, Some(false));
        close(client, server).await;
    }

    #[tokio::test]
    async fn unsupported_protocol_version_is_rejected_before_service_start() {
        let (client_stream, server_stream) = duplex(64 * 1024);
        let (client_read, client_write) = split(client_stream);
        let (server_read, server_write) = split(server_stream);
        let server = tokio::spawn(serve_io(
            server_read,
            server_write,
            McpServerConfig::local_profile(64, 64),
        ));
        let mut client_info = rmcp::model::ClientInfo::default();
        client_info.protocol_version = ProtocolVersion::V_2026_07_28;
        assert!(
            client_info
                .serve((client_read, client_write))
                .await
                .is_err()
        );
        assert_eq!(
            server.await.unwrap(),
            Err(McpServeError::InitializationRejected)
        );
    }

    #[tokio::test]
    async fn invalid_arguments_are_stable_without_initializing_the_service() {
        let (client, server) = client_and_server().await;
        let result = client
            .peer()
            .call_tool(CallToolRequestParams::new(QUERY_SCENE_TOOL))
            .await
            .unwrap();
        assert_eq!(result.is_error, Some(true));
        assert_eq!(
            result.structured_content,
            Some(json!({"schema_version": 1, "error": "invalid_arguments"}))
        );
        close(client, server).await;
    }

    #[tokio::test]
    #[ignore = "requires an approved DX12 or Vulkan conformance adapter"]
    async fn query_submission_and_replay_preserve_exact_effects() {
        let (client, server) = client_and_server().await;
        let initial_query = json!({
            "schema_version": 1,
            "scene_revision": 0,
            "entity_ids": [],
            "component_kinds": [],
            "limit": 4
        });
        let query_result = client
            .peer()
            .call_tool(
                CallToolRequestParams::new(QUERY_SCENE_TOOL)
                    .with_arguments(arguments(initial_query)),
            )
            .await
            .unwrap();
        assert_eq!(query_result.is_error, Some(false));
        assert_eq!(
            query_result.structured_content.unwrap()["entities"],
            json!([])
        );

        let mut imagination: Value = serde_json::from_str(include_str!(
            "../../cogniform-protocol/tests/fixtures/imagination_v1.json"
        ))
        .unwrap();
        imagination["base_revision"] = json!(0);
        let request = CallToolRequestParams::new(SUBMIT_IMAGINATION_TOOL)
            .with_arguments(arguments(imagination));
        let applied = client.peer().call_tool(request.clone()).await.unwrap();
        assert_eq!(applied.is_error, Some(false));
        let applied = applied.structured_content.unwrap();
        assert_eq!(applied["admission"], "queued");
        assert_eq!(applied["receipt"]["status"], "applied");
        assert_eq!(applied["receipt"]["new_revision"], 1);

        let replayed = client.peer().call_tool(request).await.unwrap();
        assert_eq!(replayed.is_error, Some(false));
        let replayed = replayed.structured_content.unwrap();
        assert_eq!(replayed["admission"], "replayed");
        assert_eq!(replayed["receipt"]["status"], "idempotent_replay");
        assert_eq!(replayed["receipt"]["new_revision"], 1);

        let final_query = json!({
            "schema_version": 1,
            "scene_revision": 1,
            "entity_ids": [],
            "component_kinds": [],
            "limit": 4
        });
        let final_result = client
            .peer()
            .call_tool(
                CallToolRequestParams::new(QUERY_SCENE_TOOL).with_arguments(arguments(final_query)),
            )
            .await
            .unwrap();
        assert_eq!(final_result.is_error, Some(false));
        assert_eq!(
            final_result.structured_content.unwrap()["entities"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
        close(client, server).await;
    }
}
