use std::sync::Arc;

use cogniform_compilation::{CompilationLimits, CompilationResult};
use cogniform_engine::{GatewayAdmission, GatewayResponse, LocalService, LocalServiceConfig};
use cogniform_protocol::{
    ApplyReceipt, ApplyStatus, ImaginationEnvelope, RuntimeLimits, SceneQuery,
};
use rmcp::{
    ErrorData as McpError, ServerHandler,
    model::{
        CallToolRequestParams, CallToolResult, ContentBlock, Implementation, ListToolsResult,
        PaginatedRequestParams, ProtocolVersion, ServerCapabilities, ServerInfo, Tool,
        ToolAnnotations,
    },
    service::{RequestContext, RoleServer},
};
use serde::Serialize;
use serde_json::{Map, Value, json};
use tokio::sync::Mutex;

/// Stable MCP name for the exact-revision read-only query tool.
pub const QUERY_SCENE_TOOL: &str = "cogniform.query_scene";
/// Stable MCP name for the idempotent semantic mutation tool.
pub const SUBMIT_IMAGINATION_TOOL: &str = "cogniform.submit_imagination";

#[derive(Clone)]
pub(crate) struct CogniformMcpServer {
    state: Arc<Mutex<LazyLocalService>>,
}

struct LazyLocalService {
    config: LocalServiceConfig,
    service: Option<LocalService>,
}

impl CogniformMcpServer {
    pub(crate) fn new(config: LocalServiceConfig) -> Self {
        Self {
            state: Arc::new(Mutex::new(LazyLocalService {
                config,
                service: None,
            })),
        }
    }

    async fn query_scene(&self, arguments: Option<Map<String, Value>>) -> CallToolResult {
        let query = match parse_arguments::<SceneQuery>(arguments) {
            Ok(query) => query,
            Err(result) => return result,
        };
        let mut state = self.state.lock().await;
        let limits = state.runtime_limits();
        if query.validate_with_limits(&limits).is_err() {
            return tool_error("invalid_query");
        }
        let service = match state.service().await {
            Ok(service) => service,
            Err(result) => return result,
        };
        let Ok(result) = service.query(&query) else {
            return tool_error("query_rejected");
        };
        if result.validate_with_limits(&limits).is_err() {
            return tool_error("invalid_service_output");
        }
        structured_result(&result)
    }

    async fn submit_imagination(&self, arguments: Option<Map<String, Value>>) -> CallToolResult {
        let imagination = match parse_arguments::<ImaginationEnvelope>(arguments) {
            Ok(imagination) => imagination,
            Err(result) => return result,
        };
        let mut state = self.state.lock().await;
        let runtime_limits = state.runtime_limits();
        if imagination.validate_with_limits(&runtime_limits).is_err() {
            return tool_error("invalid_imagination");
        }
        let service = match state.service().await {
            Ok(service) => service,
            Err(result) => return result,
        };
        if service.status().command_queue.depth != 0 {
            return tool_error("service_busy");
        }

        let Ok(admission) = service.submit_imagination(imagination.clone()) else {
            return tool_error("imagination_rejected");
        };
        let (admission_name, response) = match admission {
            GatewayAdmission::Queued { .. } => match service.process_next() {
                Ok(Some(response)) => ("queued", response),
                Ok(None) | Err(_) => return tool_error("service_failed"),
            },
            GatewayAdmission::Replayed { response } => ("replayed", *response),
            GatewayAdmission::AlreadyQueued { .. }
            | GatewayAdmission::Superseded { .. }
            | GatewayAdmission::Dropped { .. } => return tool_error("service_busy"),
        };
        let GatewayResponse::ImaginationProcessed {
            compilation,
            receipt,
        } = response
        else {
            return tool_error("invalid_service_output");
        };
        if validate_completion(
            &imagination,
            &compilation,
            receipt.as_ref(),
            &runtime_limits,
            admission_name == "replayed",
        )
        .is_err()
        {
            return tool_error("invalid_service_output");
        }
        structured_result(&ImaginationToolOutput {
            schema_version: 1,
            admission: admission_name,
            compilation: &compilation,
            receipt: receipt.as_ref(),
        })
    }
}

impl LazyLocalService {
    fn runtime_limits(&self) -> RuntimeLimits {
        self.config.engine.world.runtime_limits
    }

    async fn service(&mut self) -> Result<&mut LocalService, CallToolResult> {
        if self.service.is_none() {
            self.service = Some(
                LocalService::new(self.config.clone())
                    .await
                    .map_err(|_| tool_error("service_unavailable"))?,
            );
        }
        self.service
            .as_mut()
            .ok_or_else(|| tool_error("service_unavailable"))
    }
}

impl ServerHandler for CogniformMcpServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build())
            .with_protocol_version(ProtocolVersion::V_2025_11_25)
            .with_server_info(
                Implementation::new("cogniform", env!("CARGO_PKG_VERSION"))
                    .with_title("Cogniform local scene service"),
            )
            .with_instructions(
                "Query an exact scene revision or submit one bounded idempotent imagination.",
            )
    }

    fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> impl Future<Output = Result<ListToolsResult, McpError>> + Send + '_ {
        std::future::ready(Ok(ListToolsResult::with_all_items(vec![
            query_tool(),
            imagination_tool(),
        ])))
    }

    fn get_tool(&self, name: &str) -> Option<Tool> {
        match name {
            QUERY_SCENE_TOOL => Some(query_tool()),
            SUBMIT_IMAGINATION_TOOL => Some(imagination_tool()),
            _ => None,
        }
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        Ok(match request.name.as_ref() {
            QUERY_SCENE_TOOL => self.query_scene(request.arguments).await,
            SUBMIT_IMAGINATION_TOOL => self.submit_imagination(request.arguments).await,
            _ => {
                return Err(McpError::method_not_found::<
                    rmcp::model::CallToolRequestMethod,
                >());
            }
        })
    }
}

fn parse_arguments<T: serde::de::DeserializeOwned>(
    arguments: Option<Map<String, Value>>,
) -> Result<T, CallToolResult> {
    serde_json::from_value(Value::Object(arguments.unwrap_or_default()))
        .map_err(|_| tool_error("invalid_arguments"))
}

fn structured_result<T: Serialize>(value: &T) -> CallToolResult {
    serde_json::to_value(value).map_or_else(
        |_| tool_error("output_unavailable"),
        |value| {
            let mut result = CallToolResult::success(vec![ContentBlock::text("cogniform result")]);
            result.structured_content = Some(value);
            result
        },
    )
}

fn tool_error(code: &'static str) -> CallToolResult {
    CallToolResult::structured_error(json!({
        "schema_version": 1,
        "error": code,
    }))
}

#[derive(Serialize)]
struct ImaginationToolOutput<'a> {
    schema_version: u16,
    admission: &'static str,
    compilation: &'a CompilationResult,
    receipt: Option<&'a ApplyReceipt>,
}

fn validate_completion(
    imagination: &ImaginationEnvelope,
    compilation: &CompilationResult,
    receipt: Option<&ApplyReceipt>,
    runtime_limits: &RuntimeLimits,
    replayed: bool,
) -> Result<(), ()> {
    let compilation_limits = CompilationLimits::for_runtime_limits(*runtime_limits);
    compilation
        .validate_with_limits(&compilation_limits)
        .map_err(|_| ())?;
    if compilation.imagination_id != imagination.imagination_id
        || compilation.scene_revision != imagination.base_revision
    {
        return Err(());
    }
    match (&compilation.patch, receipt) {
        (Some(patch), Some(receipt)) => {
            if patch.transaction_id != imagination.transaction_id
                || patch.idempotency_key != imagination.idempotency_key
                || patch.base_revision != imagination.base_revision
                || receipt.transaction_id != imagination.transaction_id
                || receipt.idempotency_key != imagination.idempotency_key
                || receipt.previous_revision != patch.base_revision
                || usize::try_from(receipt.operation_count.get()).unwrap_or(usize::MAX)
                    != patch.operations.len()
                || receipt.status
                    != if replayed {
                        ApplyStatus::IdempotentReplay
                    } else {
                        ApplyStatus::Applied
                    }
            {
                return Err(());
            }
            receipt
                .validate_with_limits(runtime_limits)
                .map_err(|_| ())?;
        }
        (None, None) if !compilation.unresolved.is_empty() => {}
        _ => return Err(()),
    }
    Ok(())
}

fn query_tool() -> Tool {
    Tool::new(
        QUERY_SCENE_TOOL,
        "Return canonical logical scene state for one exact revision without mutation.",
        Arc::new(schema_object(json!({
            "type": "object",
            "additionalProperties": false,
            "required": ["schema_version", "scene_revision", "entity_ids", "component_kinds", "limit"],
            "properties": {
                "schema_version": {"const": 1},
                "scene_revision": {"type": "integer", "minimum": 0},
                "entity_ids": {"type": "array", "items": {"type": "string"}},
                "component_kinds": {"type": "array", "items": {"type": "string"}},
                "limit": {"type": "integer", "minimum": 1}
            }
        }))),
    )
    .with_raw_output_schema(Arc::new(schema_object(json!({
        "type": "object",
        "required": ["schema_version", "scene_revision", "entities"]
    }))))
    .with_annotations(
        ToolAnnotations::with_title("Query Cogniform scene")
            .read_only(true)
            .destructive(false)
            .idempotent(true)
            .open_world(false),
    )
}

fn imagination_tool() -> Tool {
    Tool::new(
        SUBMIT_IMAGINATION_TOOL,
        "Compile and atomically apply one bounded semantic imagination with exact idempotent replay.",
        Arc::new(schema_object(json!({
            "type": "object",
            "additionalProperties": false,
            "required": ["schema_version", "imagination_id", "transaction_id", "idempotency_key", "base_revision", "delivery", "seed", "declared_budget", "entities", "relations", "constraints"],
            "properties": {
                "schema_version": {"const": 1},
                "imagination_id": {"type": "string"},
                "transaction_id": {"type": "string"},
                "idempotency_key": {"type": "string"},
                "base_revision": {"type": "integer", "minimum": 0},
                "delivery": {"type": "object"},
                "seed": {"type": "integer", "minimum": 0},
                "declared_budget": {"type": "object"},
                "entities": {"type": "array", "minItems": 1, "items": {"type": "object"}},
                "relations": {"type": "array", "items": {"type": "object"}},
                "constraints": {"type": "array", "items": {"type": "object"}}
            }
        }))),
    )
    .with_raw_output_schema(Arc::new(schema_object(json!({
        "type": "object",
        "required": ["schema_version", "admission", "compilation", "receipt"]
    }))))
    .with_annotations(
        ToolAnnotations::with_title("Submit Cogniform imagination")
            .read_only(false)
            .destructive(true)
            .idempotent(true)
            .open_world(false),
    )
}

fn schema_object(value: Value) -> Map<String, Value> {
    match value {
        Value::Object(object) => object,
        _ => Map::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn semantic_validation_precedes_lazy_service_creation() {
        let server = CogniformMcpServer::new(LocalServiceConfig::new(64, 64));
        let Value::Object(arguments) = json!({
            "schema_version": 1,
            "scene_revision": 0,
            "entity_ids": [],
            "component_kinds": ["local_transform", "local_transform"],
            "limit": 4
        }) else {
            unreachable!("test arguments are an object");
        };
        let result = server.query_scene(Some(arguments)).await;
        assert_eq!(result.is_error, Some(true));
        assert_eq!(
            result.structured_content,
            Some(json!({"schema_version": 1, "error": "invalid_query"}))
        );
        assert!(server.state.lock().await.service.is_none());
    }
}
