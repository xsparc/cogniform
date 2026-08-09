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

pub use server::{APPLY_PATCH_TOOL, OBSERVE_SCENE_TOOL, QUERY_SCENE_TOOL, SUBMIT_IMAGINATION_TOOL};
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
    let handler = server::CogniformMcpServer::new(config.service);
    serve_io_with_handler(reader, writer, config.transport, handler).await
}

async fn serve_io_with_handler<R, W>(
    reader: R,
    writer: W,
    limits: McpTransportLimits,
    handler: server::CogniformMcpServer,
) -> Result<(), McpServeError>
where
    R: AsyncRead + Unpin + Send + 'static,
    W: AsyncWrite + Unpin + Send + 'static,
{
    let (mut transport, status) = transport::BoundedTransport::new(reader, writer, limits);
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
    use std::{collections::VecDeque, num::NonZeroU32, time::Duration};

    use cogniform_observation::{
        EntityVisibility, ObservationPayload, ObservationPayloadLimits, decode_payload,
    };
    use cogniform_protocol::{
        FrameId, ImageDimensions, ObservationId, ObservationKind, ObservationMetadata,
        ObservationRequest, ObservationStaleness, RuntimeLimits, SchemaVersion, StableEntityId,
    };
    use rmcp::{
        ServiceError, ServiceExt as _,
        model::{
            CallToolRequestParams, CallToolResult, ContentBlock, ErrorCode, ProtocolVersion,
            ReadResourceRequestParams, ResourceContents, Tool,
        },
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

    async fn client_and_server_with_observation_backend(
        backend: Box<dyn server::ObservationBackend>,
    ) -> (
        rmcp::service::RunningService<rmcp::service::RoleClient, ()>,
        tokio::task::JoinHandle<Result<(), McpServeError>>,
    ) {
        let (client_stream, server_stream) = duplex(16 * 1024 * 1024);
        let (client_read, client_write) = split(client_stream);
        let (server_read, server_write) = split(server_stream);
        let config = McpServerConfig::local_profile(64, 64);
        let handler =
            server::CogniformMcpServer::new_with_observation_backend(config.service, backend);
        let server = tokio::spawn(serve_io_with_handler(
            server_read,
            server_write,
            config.transport,
            handler,
        ));
        let client = ().serve((client_read, client_write)).await.unwrap();
        (client, server)
    }

    async fn client_and_server_with_observation_backend_and_policy(
        backend: Box<dyn server::ObservationBackend>,
        width: u32,
        height: u32,
        cadence: Duration,
        deadline: Duration,
    ) -> (
        rmcp::service::RunningService<rmcp::service::RoleClient, ()>,
        tokio::task::JoinHandle<Result<(), McpServeError>>,
    ) {
        let (client_stream, server_stream) = duplex(16 * 1024 * 1024);
        let (client_read, client_write) = split(client_stream);
        let (server_read, server_write) = split(server_stream);
        let config = McpServerConfig::local_profile(width, height);
        let handler = server::CogniformMcpServer::new_with_observation_backend_and_poll_policy(
            config.service,
            backend,
            cadence,
            deadline,
        );
        let server = tokio::spawn(serve_io_with_handler(
            server_read,
            server_write,
            config.transport,
            handler,
        ));
        let client = ().serve((client_read, client_write)).await.unwrap();
        (client, server)
    }

    enum FakeObservationOutcome {
        Completed(ObservationPayload),
        Failed,
        Rejected,
        Pending,
        PollFailed,
        MismatchedFailed,
        MismatchedCompletion,
    }

    struct FakeObservationBackend {
        outcomes: VecDeque<FakeObservationOutcome>,
        active: Option<ObservationRequest>,
        next_frame: u64,
        dimensions: (u32, u32),
    }

    impl FakeObservationBackend {
        fn new(outcomes: impl IntoIterator<Item = FakeObservationOutcome>) -> Self {
            Self {
                outcomes: outcomes.into_iter().collect(),
                active: None,
                next_frame: 1,
                dimensions: (64, 64),
            }
        }

        fn with_dimensions(
            outcomes: impl IntoIterator<Item = FakeObservationOutcome>,
            dimensions: (u32, u32),
        ) -> Self {
            Self {
                outcomes: outcomes.into_iter().collect(),
                active: None,
                next_frame: 1,
                dimensions,
            }
        }
    }

    impl server::ObservationBackend for FakeObservationBackend {
        fn dimensions(&self) -> (u32, u32) {
            self.dimensions
        }

        fn request_observation(&mut self, request: ObservationRequest) -> Result<(), ()> {
            if self.active.is_some() || self.outcomes.is_empty() {
                return Err(());
            }
            if matches!(
                self.outcomes.front(),
                Some(FakeObservationOutcome::Rejected)
            ) {
                self.outcomes.pop_front();
                return Err(());
            }
            self.active = Some(request);
            Ok(())
        }

        fn try_receive_observation(
            &mut self,
        ) -> Result<Option<server::AdapterObservationDelivery>, ()> {
            if matches!(self.outcomes.front(), Some(FakeObservationOutcome::Pending)) {
                return Ok(None);
            }
            let request = self.active.take().ok_or(())?;
            let outcome = self.outcomes.pop_front().ok_or(())?;
            let delivery = match outcome {
                FakeObservationOutcome::Completed(payload) => {
                    let frame_id = FrameId::new(self.next_frame).unwrap();
                    self.next_frame += 1;
                    server::AdapterObservationDelivery::Completed {
                        metadata: ObservationMetadata {
                            schema_version: SchemaVersion::V1,
                            observation_id: request.observation_id,
                            scene_revision: request.scene_revision,
                            frame_id,
                            camera_id: request.camera_id,
                            kind: request.kind,
                            dimensions: (request.kind != ObservationKind::Visibility).then(|| {
                                ImageDimensions {
                                    width: NonZeroU32::new(self.dimensions.0).unwrap(),
                                    height: NonZeroU32::new(self.dimensions.1).unwrap(),
                                }
                            }),
                            quality: request.quality,
                            observed_at_unix_micros: 1,
                            production_latency_micros: 2,
                            staleness: ObservationStaleness {
                                latest_known_revision: request.scene_revision,
                                revisions_behind: 0,
                            },
                        },
                        payload,
                    }
                }
                FakeObservationOutcome::Failed => server::AdapterObservationDelivery::Failed {
                    observation_id: request.observation_id,
                },
                FakeObservationOutcome::MismatchedFailed => {
                    server::AdapterObservationDelivery::Failed {
                        observation_id: ObservationId::new(
                            request.observation_id.get().checked_add(1).unwrap(),
                        )
                        .unwrap(),
                    }
                }
                FakeObservationOutcome::MismatchedCompletion => {
                    server::AdapterObservationDelivery::Completed {
                        metadata: ObservationMetadata {
                            schema_version: SchemaVersion::V1,
                            observation_id: ObservationId::new(
                                request.observation_id.get().checked_add(1).unwrap(),
                            )
                            .unwrap(),
                            scene_revision: request.scene_revision,
                            frame_id: FrameId::new(self.next_frame).unwrap(),
                            camera_id: request.camera_id,
                            kind: request.kind,
                            dimensions: None,
                            quality: request.quality,
                            observed_at_unix_micros: 1,
                            production_latency_micros: 2,
                            staleness: ObservationStaleness {
                                latest_known_revision: request.scene_revision,
                                revisions_behind: 0,
                            },
                        },
                        payload: ObservationPayload::Visibility(Vec::new()),
                    }
                }
                FakeObservationOutcome::PollFailed => return Err(()),
                FakeObservationOutcome::Rejected | FakeObservationOutcome::Pending => {
                    unreachable!("request or pending outcomes do not complete")
                }
            };
            Ok(Some(delivery))
        }
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
        let capabilities = &client.peer().peer_info().unwrap().capabilities;
        assert_eq!(capabilities.resources.as_ref().unwrap().subscribe, None);
        assert_eq!(capabilities.resources.as_ref().unwrap().list_changed, None);
        let tools = client.peer().list_all_tools().await.unwrap();
        assert_eq!(
            tools
                .iter()
                .map(|tool| tool.name.as_ref())
                .collect::<Vec<_>>(),
            [
                QUERY_SCENE_TOOL,
                SUBMIT_IMAGINATION_TOOL,
                APPLY_PATCH_TOOL,
                OBSERVE_SCENE_TOOL
            ]
        );
        assert_existing_tool_annotations(&tools);
        assert_patch_tool_contract(&tools[2]);
        assert_observation_tool_contract(&tools[3]);
        close(client, server).await;
    }

    fn assert_existing_tool_annotations(tools: &[Tool]) {
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
        let patch = tools[2].annotations.as_ref().unwrap();
        assert_eq!(patch.read_only_hint, Some(false));
        assert_eq!(patch.destructive_hint, Some(true));
        assert_eq!(patch.idempotent_hint, Some(true));
        assert_eq!(patch.open_world_hint, Some(false));
    }

    fn assert_patch_tool_contract(tool: &Tool) {
        assert_eq!(
            tool.input_schema.get("required"),
            Some(&json!([
                "schema_version",
                "transaction_id",
                "idempotency_key",
                "base_revision",
                "conflict_policy",
                "delivery",
                "declared_budget",
                "operations"
            ]))
        );
        assert_eq!(tool.input_schema["additionalProperties"], false);
        assert_eq!(
            tool.input_schema["properties"]["conflict_policy"],
            json!({"const": "require_exact_base"})
        );
        assert_eq!(
            tool.output_schema.as_ref().unwrap()["oneOf"],
            json!([
                {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["schema_version", "admission", "receipt"],
                    "properties": {
                        "schema_version": {"const": 1},
                        "admission": {"enum": ["queued", "replayed"]},
                        "receipt": {"type": "object"}
                    }
                },
                {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["schema_version", "error"],
                    "properties": {
                        "schema_version": {"const": 1},
                        "error": {"enum": [
                            "invalid_arguments",
                            "invalid_patch",
                            "patch_rejected",
                            "service_busy",
                            "service_unavailable",
                            "service_failed",
                            "invalid_service_output",
                            "output_unavailable"
                        ]}
                    }
                }
            ])
        );
        assert!(tool.execution.is_none());
    }

    fn assert_observation_tool_contract(tool: &Tool) {
        let observation = tool.annotations.as_ref().unwrap();
        assert_eq!(observation.read_only_hint, Some(false));
        assert_eq!(observation.destructive_hint, Some(false));
        assert_eq!(observation.idempotent_hint, Some(false));
        assert_eq!(observation.open_world_hint, Some(false));
        assert_eq!(
            tool.input_schema.get("required"),
            Some(&json!([
                "schema_version",
                "observation_id",
                "scene_revision",
                "camera_id",
                "kind",
                "quality"
            ]))
        );
        assert_eq!(tool.input_schema["additionalProperties"], false);
        assert_eq!(
            tool.input_schema["properties"]["kind"],
            json!({"enum": ["color", "depth", "normal", "entity_id", "visibility"]})
        );
        assert_eq!(
            tool.output_schema.as_ref().unwrap()["oneOf"],
            json!([
                {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["schema_version", "resource_uri", "resource_size", "metadata"],
                    "properties": {
                        "schema_version": {"const": 1},
                        "resource_uri": {"type": "string"},
                        "resource_size": {"type": "integer", "minimum": 60, "maximum": 4_194_304},
                        "metadata": {"type": "object"}
                    }
                },
                {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["schema_version", "error"],
                    "properties": {
                        "schema_version": {"const": 1},
                        "error": {"enum": [
                            "invalid_arguments",
                            "invalid_observation",
                            "observation_rejected",
                            "observation_failed",
                            "observation_timeout",
                            "observation_too_large",
                            "service_unavailable",
                            "service_failed",
                            "invalid_service_output",
                            "output_unavailable"
                        ]}
                    }
                }
            ])
        );
        assert!(tool.execution.is_none());
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
        for tool in [
            QUERY_SCENE_TOOL,
            SUBMIT_IMAGINATION_TOOL,
            APPLY_PATCH_TOOL,
            OBSERVE_SCENE_TOOL,
        ] {
            let result = client
                .peer()
                .call_tool(CallToolRequestParams::new(tool))
                .await
                .unwrap();
            assert_eq!(result.is_error, Some(true));
            assert_eq!(
                result.structured_content,
                Some(json!({"schema_version": 1, "error": "invalid_arguments"}))
            );
        }
        close(client, server).await;
    }

    #[tokio::test]
    async fn official_client_observes_lists_reads_replaces_and_preserves_on_failure() {
        let visible = StableEntityId::new(0x31).unwrap();
        let backend = FakeObservationBackend::new([
            FakeObservationOutcome::Completed(ObservationPayload::Visibility(Vec::new())),
            FakeObservationOutcome::Completed(ObservationPayload::Visibility(vec![
                EntityVisibility {
                    entity_id: visible,
                    visible_pixels: 7,
                },
            ])),
            FakeObservationOutcome::Failed,
        ]);
        let (client, server) = client_and_server_with_observation_backend(Box::new(backend)).await;
        assert!(client.peer().list_all_resources().await.unwrap().is_empty());
        let first_uri = assert_first_observation_resource(&client).await;
        let (second_uri, second_blob) =
            assert_observation_resource_replacement(&client, &first_uri).await;
        assert_failed_observation_preserves_resource(&client, &second_uri, &second_blob).await;
        close(client, server).await;
    }

    #[tokio::test]
    async fn rejected_and_over_limit_observations_preserve_the_exact_resource() {
        assert_scripted_failure_preserves_resource(
            FakeObservationBackend::new([
                FakeObservationOutcome::Completed(ObservationPayload::Visibility(Vec::new())),
                FakeObservationOutcome::Rejected,
            ]),
            (64, 64),
            Duration::from_secs(15),
            ObservationKind::Visibility,
            "observation_rejected",
            false,
        )
        .await;

        assert_scripted_failure_preserves_resource(
            FakeObservationBackend::with_dimensions(
                [
                    FakeObservationOutcome::Completed(ObservationPayload::Visibility(Vec::new())),
                    FakeObservationOutcome::Completed(ObservationPayload::Color(vec![
                        [0; 4];
                        1024 * 1024
                    ])),
                ],
                (1024, 1024),
            ),
            (1024, 1024),
            Duration::from_secs(15),
            ObservationKind::Color,
            "observation_too_large",
            false,
        )
        .await;
    }

    #[tokio::test]
    async fn poisoning_observation_failures_preserve_resource_and_block_service_calls() {
        for (outcome, expected, deadline) in [
            (
                FakeObservationOutcome::Pending,
                "observation_timeout",
                Duration::from_millis(10),
            ),
            (
                FakeObservationOutcome::MismatchedCompletion,
                "invalid_service_output",
                Duration::from_secs(15),
            ),
            (
                FakeObservationOutcome::PollFailed,
                "service_failed",
                Duration::from_secs(15),
            ),
            (
                FakeObservationOutcome::MismatchedFailed,
                "invalid_service_output",
                Duration::from_secs(15),
            ),
        ] {
            assert_scripted_failure_preserves_resource(
                FakeObservationBackend::new([
                    FakeObservationOutcome::Completed(ObservationPayload::Visibility(Vec::new())),
                    outcome,
                ]),
                (64, 64),
                deadline,
                ObservationKind::Visibility,
                expected,
                true,
            )
            .await;
        }
    }

    async fn assert_scripted_failure_preserves_resource(
        backend: FakeObservationBackend,
        dimensions: (u32, u32),
        deadline: Duration,
        failing_kind: ObservationKind,
        expected_error: &str,
        poisoned: bool,
    ) {
        let (client, server) = client_and_server_with_observation_backend_and_policy(
            Box::new(backend),
            dimensions.0,
            dimensions.1,
            Duration::from_millis(2),
            deadline,
        )
        .await;
        let seeded = call_observation(&client, 0x51, ObservationKind::Visibility).await;
        let uri = seeded.structured_content.as_ref().unwrap()["resource_uri"]
            .as_str()
            .unwrap()
            .to_owned();
        let before = client.peer().list_all_resources().await.unwrap();
        assert_eq!(before.len(), 1);
        let blob = resource_blob(
            &client
                .peer()
                .read_resource(ReadResourceRequestParams::new(&uri))
                .await
                .unwrap(),
        );

        let failed = call_observation(&client, 0x52, failing_kind).await;
        assert_eq!(failed.is_error, Some(true));
        assert_eq!(failed.structured_content.unwrap()["error"], expected_error);
        assert_eq!(client.peer().list_all_resources().await.unwrap(), before);
        assert_eq!(
            resource_blob(
                &client
                    .peer()
                    .read_resource(ReadResourceRequestParams::new(&uri))
                    .await
                    .unwrap(),
            ),
            blob
        );

        if poisoned {
            let blocked = call_observation(&client, 0x53, ObservationKind::Visibility).await;
            assert_eq!(
                blocked.structured_content.unwrap()["error"],
                "service_failed"
            );
            assert_eq!(client.peer().list_all_resources().await.unwrap(), before);
            assert_eq!(
                resource_blob(
                    &client
                        .peer()
                        .read_resource(ReadResourceRequestParams::new(&uri))
                        .await
                        .unwrap(),
                ),
                blob
            );
        }
        close(client, server).await;
    }

    async fn assert_first_observation_resource(client: &TestClient) -> String {
        let first = call_visibility_observation(client, 0x41).await;
        let first_uri = first.structured_content.as_ref().unwrap()["resource_uri"]
            .as_str()
            .unwrap()
            .to_owned();
        assert_eq!(
            first_uri,
            "cogniform://observations/00000000000000000000000000000041"
        );
        assert_eq!(
            first.structured_content.as_ref().unwrap()["resource_size"],
            60
        );
        let link = first
            .content
            .iter()
            .find_map(|content| match content {
                ContentBlock::ResourceLink(resource) => Some(resource),
                _ => None,
            })
            .unwrap();
        assert_eq!(link.uri, first_uri);
        assert_eq!(link.size, Some(60));
        assert_eq!(
            link.mime_type.as_deref(),
            Some(server::OBSERVATION_RESOURCE_MIME_TYPE)
        );

        let resources = client.peer().list_all_resources().await.unwrap();
        assert_eq!(resources, std::slice::from_ref(link));
        let first_blob = resource_blob(
            &client
                .peer()
                .read_resource(ReadResourceRequestParams::new(&first_uri))
                .await
                .unwrap(),
        );
        assert_eq!(first_blob.len(), 80);
        assert!(first_blob.starts_with("Q09HT0JTMDEAAQUA"));
        first_uri
    }

    async fn assert_observation_resource_replacement(
        client: &TestClient,
        first_uri: &str,
    ) -> (String, String) {
        let second = call_visibility_observation(client, 0x42).await;
        let second_uri = second.structured_content.as_ref().unwrap()["resource_uri"]
            .as_str()
            .unwrap()
            .to_owned();
        assert_eq!(
            second.structured_content.as_ref().unwrap()["resource_size"],
            84
        );
        let resources = client.peer().list_all_resources().await.unwrap();
        assert_eq!(resources.len(), 1);
        assert_eq!(resources[0].uri, second_uri);
        assert_eq!(resources[0].size, Some(84));
        let error = client
            .peer()
            .read_resource(ReadResourceRequestParams::new(first_uri))
            .await
            .unwrap_err();
        assert!(matches!(
            error,
            ServiceError::McpError(error) if error.code == ErrorCode::RESOURCE_NOT_FOUND
        ));
        let second_blob = resource_blob(
            &client
                .peer()
                .read_resource(ReadResourceRequestParams::new(&second_uri))
                .await
                .unwrap(),
        );
        assert_eq!(second_blob.len(), 112);
        (second_uri, second_blob)
    }

    async fn assert_failed_observation_preserves_resource(
        client: &TestClient,
        second_uri: &str,
        second_blob: &str,
    ) {
        let failed = call_visibility_observation(client, 0x43).await;
        assert_eq!(failed.is_error, Some(true));
        assert_eq!(
            failed.structured_content.unwrap()["error"],
            "observation_failed"
        );
        let resources = client.peer().list_all_resources().await.unwrap();
        assert_eq!(resources.len(), 1);
        assert_eq!(resources[0].uri, second_uri);
        assert_eq!(
            resource_blob(
                &client
                    .peer()
                    .read_resource(ReadResourceRequestParams::new(second_uri))
                    .await
                    .unwrap(),
            ),
            second_blob
        );
    }

    async fn call_visibility_observation(
        client: &TestClient,
        observation_id: u128,
    ) -> CallToolResult {
        call_observation(client, observation_id, ObservationKind::Visibility).await
    }

    async fn call_observation(
        client: &TestClient,
        observation_id: u128,
        kind: ObservationKind,
    ) -> CallToolResult {
        client
            .peer()
            .call_tool(
                CallToolRequestParams::new(OBSERVE_SCENE_TOOL).with_arguments(arguments(json!({
                    "schema_version": 1,
                    "observation_id": ObservationId::new(observation_id).unwrap().to_string(),
                    "scene_revision": 0,
                    "camera_id": StableEntityId::new(0x31).unwrap().to_string(),
                    "kind": kind,
                    "quality": "low"
                }))),
            )
            .await
            .unwrap()
    }

    fn resource_blob(result: &rmcp::model::ReadResourceResult) -> String {
        let [
            ResourceContents::BlobResourceContents {
                blob, mime_type, ..
            },
        ] = result.contents.as_slice()
        else {
            panic!("resource read must return exactly one blob");
        };
        assert_eq!(
            mime_type.as_deref(),
            Some(server::OBSERVATION_RESOURCE_MIME_TYPE)
        );
        blob.clone()
    }

    #[tokio::test]
    #[ignore = "requires an approved DX12 or Vulkan conformance adapter"]
    async fn query_patch_imagination_observation_and_replay_preserve_exact_effects() {
        let (client, server) = client_and_server().await;
        assert_initial_query(&client).await;
        let patch = camera_patch();
        assert_patch_apply_replay_and_conflict(&client, &patch).await;
        assert_imagination_apply_and_replay(&client).await;
        assert_stale_patch_is_rejected(&client, patch).await;
        assert_final_query(&client).await;
        assert_production_observation_resource(&client).await;
        close(client, server).await;
    }

    async fn assert_production_observation_resource(client: &TestClient) {
        let observation_id = ObservationId::new(0x44).unwrap();
        let result = client
            .peer()
            .call_tool(
                CallToolRequestParams::new(OBSERVE_SCENE_TOOL).with_arguments(arguments(json!({
                    "schema_version": 1,
                    "observation_id": observation_id.to_string(),
                    "scene_revision": 2,
                    "camera_id": StableEntityId::new(0x31).unwrap().to_string(),
                    "kind": "visibility",
                    "quality": "low"
                }))),
            )
            .await
            .unwrap();
        assert_eq!(result.is_error, Some(false));
        let output = result.structured_content.unwrap();
        assert_eq!(
            output["metadata"]["observation_id"],
            observation_id.to_string()
        );
        assert_eq!(output["metadata"]["scene_revision"], 2);
        assert_eq!(output["metadata"]["kind"], "visibility");
        let uri = output["resource_uri"].as_str().unwrap();
        let resources = client.peer().list_all_resources().await.unwrap();
        assert_eq!(resources.len(), 1);
        assert_eq!(resources[0].uri, uri);
        let read = client
            .peer()
            .read_resource(ReadResourceRequestParams::new(uri))
            .await
            .unwrap();
        let envelope = decode_base64(&resource_blob(&read)).unwrap();
        assert_eq!(envelope.len() as u64, resources[0].size.unwrap());
        let metadata: ObservationMetadata =
            serde_json::from_value(output["metadata"].clone()).unwrap();
        let payload = decode_payload(
            &metadata,
            &envelope,
            &RuntimeLimits::default(),
            ObservationPayloadLimits::default(),
        )
        .unwrap();
        assert!(matches!(payload, ObservationPayload::Visibility(_)));
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

    type TestClient = rmcp::service::RunningService<rmcp::service::RoleClient, ()>;

    async fn assert_initial_query(client: &TestClient) {
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
                            "value": {"vertical_fov_radians": 1.0, "near": 0.1, "far": 100.0}
                        }
                    ]
                }
            }]
        })
    }

    async fn assert_patch_apply_replay_and_conflict(client: &TestClient, patch: &Value) {
        let patch_request =
            CallToolRequestParams::new(APPLY_PATCH_TOOL).with_arguments(arguments(patch.clone()));
        let patch_applied = client
            .peer()
            .call_tool(patch_request.clone())
            .await
            .unwrap();
        assert_eq!(patch_applied.is_error, Some(false));
        let patch_applied = patch_applied.structured_content.unwrap();
        assert_eq!(patch_applied["admission"], "queued");
        assert_eq!(patch_applied["receipt"]["status"], "applied");
        assert_eq!(patch_applied["receipt"]["new_revision"], 1);

        let patch_replayed = client.peer().call_tool(patch_request).await.unwrap();
        assert_eq!(patch_replayed.is_error, Some(false));
        let patch_replayed = patch_replayed.structured_content.unwrap();
        assert_eq!(patch_replayed["admission"], "replayed");
        assert_eq!(patch_replayed["receipt"]["status"], "idempotent_replay");
        assert_eq!(patch_replayed["receipt"]["new_revision"], 1);

        let mut conflicting_patch = patch.clone();
        conflicting_patch["transaction_id"] = json!("00000000000000000000000000000012");
        conflicting_patch["base_revision"] = json!(1);
        let conflicting = client
            .peer()
            .call_tool(
                CallToolRequestParams::new(APPLY_PATCH_TOOL)
                    .with_arguments(arguments(conflicting_patch)),
            )
            .await
            .unwrap();
        assert_eq!(conflicting.is_error, Some(true));
        assert_eq!(
            conflicting.structured_content.unwrap()["error"],
            "patch_rejected"
        );
    }

    async fn assert_imagination_apply_and_replay(client: &TestClient) {
        let mut imagination: Value = serde_json::from_str(include_str!(
            "../../cogniform-protocol/tests/fixtures/imagination_v1.json"
        ))
        .unwrap();
        imagination["base_revision"] = json!(1);
        let request = CallToolRequestParams::new(SUBMIT_IMAGINATION_TOOL)
            .with_arguments(arguments(imagination));
        let applied = client.peer().call_tool(request.clone()).await.unwrap();
        assert_eq!(applied.is_error, Some(false));
        let applied = applied.structured_content.unwrap();
        assert_eq!(applied["admission"], "queued");
        assert_eq!(applied["receipt"]["status"], "applied");
        assert_eq!(applied["receipt"]["new_revision"], 2);

        let replayed = client.peer().call_tool(request).await.unwrap();
        assert_eq!(replayed.is_error, Some(false));
        let replayed = replayed.structured_content.unwrap();
        assert_eq!(replayed["admission"], "replayed");
        assert_eq!(replayed["receipt"]["status"], "idempotent_replay");
        assert_eq!(replayed["receipt"]["new_revision"], 2);
    }

    async fn assert_stale_patch_is_rejected(client: &TestClient, mut patch: Value) {
        patch["transaction_id"] = json!("00000000000000000000000000000013");
        patch["idempotency_key"] = json!("00000000000000000000000000000023");
        let stale = client
            .peer()
            .call_tool(
                CallToolRequestParams::new(APPLY_PATCH_TOOL).with_arguments(arguments(patch)),
            )
            .await
            .unwrap();
        assert_eq!(stale.is_error, Some(true));
        assert_eq!(stale.structured_content.unwrap()["error"], "patch_rejected");
    }

    async fn assert_final_query(client: &TestClient) {
        let final_query = json!({
            "schema_version": 1,
            "scene_revision": 2,
            "entity_ids": ["00000000000000000000000000000031"],
            "component_kinds": ["local_transform", "camera"],
            "limit": 1
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
            final_result.structured_content.unwrap()["entities"],
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
}
