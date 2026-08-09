use std::{
    sync::Arc,
    thread,
    time::{Duration, Instant},
};

use cogniform_compilation::{CompilationLimits, CompilationResult};
use cogniform_engine::{
    GatewayAdmission, GatewayResponse, LocalService, LocalServiceConfig, LocalServiceError,
    ObservationDelivery,
};
use cogniform_observation::{
    ObservationEnvelopeError, ObservationPayload, ObservationPayloadLimits, encode_payload,
};
use cogniform_protocol::{
    ApplyReceipt, ApplyStatus, ImaginationEnvelope, ObservationId, ObservationKind,
    ObservationMetadata, ObservationRequest, RuntimeLimits, ScenePatch, SceneQuery,
};
use rmcp::{
    ErrorData as McpError, ServerHandler,
    model::{
        CallToolRequestParams, CallToolResult, ContentBlock, Implementation, ListResourcesResult,
        ListToolsResult, PaginatedRequestParams, ProtocolVersion, ReadResourceRequestParams,
        ReadResourceResult, Resource, ResourceContents, ServerCapabilities, ServerInfo, Tool,
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
/// Stable MCP name for the idempotent explicit patch tool.
pub const APPLY_PATCH_TOOL: &str = "cogniform.apply_patch";
/// Stable MCP name for the exact-revision observation tool.
pub const OBSERVE_SCENE_TOOL: &str = "cogniform.observe_scene";

pub(crate) const OBSERVATION_RESOURCE_MIME_TYPE: &str =
    "application/vnd.cogniform.observation-envelope";
const OBSERVATION_POLL_CADENCE: Duration = Duration::from_millis(2);
const OBSERVATION_POLL_DEADLINE: Duration = Duration::from_secs(15);
const BASE64_ALPHABET: &[u8; 64] =
    b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

#[derive(Clone)]
pub(crate) struct CogniformMcpServer {
    state: Arc<Mutex<LazyLocalService>>,
}

struct LazyLocalService {
    config: LocalServiceConfig,
    service: Option<LocalService>,
    service_failed: bool,
    retained_observation: Option<RetainedObservation>,
    observation_poll_policy: ObservationPollPolicy,
    #[cfg(test)]
    test_observation_backend: Option<Box<dyn ObservationBackend>>,
}

#[derive(Clone, Debug)]
struct RetainedObservation {
    resource: Resource,
    raw_size: u64,
    metadata: ObservationMetadata,
    blob: String,
}

pub(crate) enum AdapterObservationDelivery {
    Completed {
        metadata: ObservationMetadata,
        payload: ObservationPayload,
    },
    Failed {
        observation_id: ObservationId,
    },
}

pub(crate) trait ObservationBackend: Send {
    fn dimensions(&self) -> (u32, u32);

    fn request_observation(&mut self, request: ObservationRequest) -> Result<(), ()>;

    fn try_receive_observation(&mut self) -> Result<Option<AdapterObservationDelivery>, ()>;
}

impl ObservationBackend for LocalService {
    fn dimensions(&self) -> (u32, u32) {
        self.observation_dimensions()
    }

    fn request_observation(&mut self, request: ObservationRequest) -> Result<(), ()> {
        self.request_observation(request).map_err(|_| ())
    }

    fn try_receive_observation(&mut self) -> Result<Option<AdapterObservationDelivery>, ()> {
        self.try_receive_observation_delivery()
            .map_err(|_| ())
            .map(|delivery| {
                delivery.map(|delivery| match delivery {
                    ObservationDelivery::Completed(observation) => {
                        let (metadata, payload) = observation.into_parts();
                        AdapterObservationDelivery::Completed { metadata, payload }
                    }
                    ObservationDelivery::Failed { observation_id, .. } => {
                        AdapterObservationDelivery::Failed { observation_id }
                    }
                })
            })
    }
}

#[derive(Clone, Copy)]
struct ObservationPollPolicy {
    cadence: Duration,
    deadline: Duration,
}

impl Default for ObservationPollPolicy {
    fn default() -> Self {
        Self {
            cadence: OBSERVATION_POLL_CADENCE,
            deadline: OBSERVATION_POLL_DEADLINE,
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct ObservationToolFailure {
    code: &'static str,
    poison_service: bool,
}

impl ObservationToolFailure {
    const fn stable(code: &'static str) -> Self {
        Self {
            code,
            poison_service: false,
        }
    }

    const fn poison(code: &'static str) -> Self {
        Self {
            code,
            poison_service: true,
        }
    }
}

impl CogniformMcpServer {
    pub(crate) fn new(config: LocalServiceConfig) -> Self {
        Self {
            state: Arc::new(Mutex::new(LazyLocalService {
                config,
                service: None,
                service_failed: false,
                retained_observation: None,
                observation_poll_policy: ObservationPollPolicy::default(),
                #[cfg(test)]
                test_observation_backend: None,
            })),
        }
    }

    #[cfg(test)]
    pub(crate) fn new_with_observation_backend(
        config: LocalServiceConfig,
        backend: Box<dyn ObservationBackend>,
    ) -> Self {
        Self {
            state: Arc::new(Mutex::new(LazyLocalService {
                config,
                service: None,
                service_failed: false,
                retained_observation: None,
                observation_poll_policy: ObservationPollPolicy::default(),
                test_observation_backend: Some(backend),
            })),
        }
    }

    #[cfg(test)]
    pub(crate) fn new_with_observation_backend_and_poll_policy(
        config: LocalServiceConfig,
        backend: Box<dyn ObservationBackend>,
        cadence: Duration,
        deadline: Duration,
    ) -> Self {
        Self {
            state: Arc::new(Mutex::new(LazyLocalService {
                config,
                service: None,
                service_failed: false,
                retained_observation: None,
                observation_poll_policy: ObservationPollPolicy { cadence, deadline },
                test_observation_backend: Some(backend),
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

    async fn apply_patch(&self, arguments: Option<Map<String, Value>>) -> CallToolResult {
        let patch = match parse_arguments::<ScenePatch>(arguments) {
            Ok(patch) => patch,
            Err(result) => return result,
        };
        let mut state = self.state.lock().await;
        let runtime_limits = state.runtime_limits();
        if patch.to_canonical_json(&runtime_limits).is_err() {
            return tool_error("invalid_patch");
        }
        let service = match state.service().await {
            Ok(service) => service,
            Err(result) => return result,
        };
        if service.status().command_queue.depth != 0 {
            return tool_error("service_busy");
        }

        let Ok(admission) = service.submit_patch(patch.clone()) else {
            return tool_error("patch_rejected");
        };
        finish_patch_admission(&patch, &runtime_limits, admission, || {
            service.process_next()
        })
    }

    async fn observe_scene(&self, arguments: Option<Map<String, Value>>) -> CallToolResult {
        let request = match parse_arguments::<ObservationRequest>(arguments) {
            Ok(request) => request,
            Err(result) => return result,
        };
        let mut state = self.state.lock().await;
        let runtime_limits = state.runtime_limits();
        if request.to_canonical_json(&runtime_limits).is_err() {
            return tool_error("invalid_observation");
        }
        let payload_limits = ObservationPayloadLimits::default();
        if state.service_failed {
            return tool_error("service_failed");
        }
        let poll_policy = state.observation_poll_policy;

        #[cfg(test)]
        let outcome = if let Some(backend) = state.test_observation_backend.as_mut() {
            drive_observation(
                &request,
                backend.as_mut(),
                &runtime_limits,
                payload_limits,
                poll_policy,
            )
        } else {
            let service = match state.service().await {
                Ok(service) => service,
                Err(result) => return result,
            };
            drive_observation(
                &request,
                service,
                &runtime_limits,
                payload_limits,
                poll_policy,
            )
        };

        #[cfg(not(test))]
        let outcome = {
            let service = match state.service().await {
                Ok(service) => service,
                Err(result) => return result,
            };
            drive_observation(
                &request,
                service,
                &runtime_limits,
                payload_limits,
                poll_policy,
            )
        };

        match outcome {
            Ok(retained) => {
                let result = observation_result(&retained);
                if result.is_error == Some(false) {
                    state.retained_observation = Some(retained);
                }
                result
            }
            Err(failure) => {
                if failure.poison_service {
                    state.poison_service();
                }
                tool_error(failure.code)
            }
        }
    }
}

fn drive_observation(
    request: &ObservationRequest,
    backend: &mut dyn ObservationBackend,
    runtime_limits: &RuntimeLimits,
    payload_limits: ObservationPayloadLimits,
    policy: ObservationPollPolicy,
) -> Result<RetainedObservation, ObservationToolFailure> {
    backend
        .request_observation(*request)
        .map_err(|()| ObservationToolFailure::stable("observation_rejected"))?;
    let started = Instant::now();
    loop {
        if started.elapsed() >= policy.deadline {
            return Err(ObservationToolFailure::poison("observation_timeout"));
        }
        let delivery = backend.try_receive_observation();
        let elapsed = started.elapsed();
        if elapsed >= policy.deadline {
            return Err(ObservationToolFailure::poison("observation_timeout"));
        }
        match delivery {
            Ok(Some(AdapterObservationDelivery::Completed { metadata, payload })) => {
                return retain_observation(
                    request,
                    metadata,
                    &payload,
                    backend.dimensions(),
                    runtime_limits,
                    payload_limits,
                );
            }
            Ok(Some(AdapterObservationDelivery::Failed { observation_id })) => {
                return if observation_id == request.observation_id {
                    Err(ObservationToolFailure::stable("observation_failed"))
                } else {
                    Err(ObservationToolFailure::poison("invalid_service_output"))
                };
            }
            Ok(None) => {
                let remaining = policy.deadline.checked_sub(elapsed).unwrap_or_default();
                thread::sleep(policy.cadence.min(remaining));
            }
            Err(()) => return Err(ObservationToolFailure::poison("service_failed")),
        }
    }
}

fn retain_observation(
    request: &ObservationRequest,
    metadata: ObservationMetadata,
    payload: &ObservationPayload,
    dimensions: (u32, u32),
    runtime_limits: &RuntimeLimits,
    payload_limits: ObservationPayloadLimits,
) -> Result<RetainedObservation, ObservationToolFailure> {
    validate_observation_completion(request, &metadata, dimensions, runtime_limits)
        .map_err(|()| ObservationToolFailure::poison("invalid_service_output"))?;
    let envelope =
        encode_payload(&metadata, payload, runtime_limits, payload_limits).map_err(|error| {
            match error {
                ObservationEnvelopeError::EnvelopeLimitExceeded { .. }
                | ObservationEnvelopeError::VisibilityEntryLimitExceeded { .. }
                | ObservationEnvelopeError::VisibilityPixelLimitExceeded { .. } => {
                    ObservationToolFailure::stable("observation_too_large")
                }
                ObservationEnvelopeError::AllocationFailed => {
                    ObservationToolFailure::stable("output_unavailable")
                }
                _ => ObservationToolFailure::poison("invalid_service_output"),
            }
        })?;
    let raw_size = u64::try_from(envelope.len())
        .map_err(|_| ObservationToolFailure::stable("output_unavailable"))?;
    let blob = base64_encode(&envelope)
        .map_err(|()| ObservationToolFailure::stable("output_unavailable"))?;
    let uri = observation_resource_uri(request.observation_id);
    let resource = Resource::new(uri, format!("observation-{}", request.observation_id))
        .with_title("Cogniform observation payload")
        .with_description("Canonical revision-bound COGOBS01 observation payload envelope")
        .with_mime_type(OBSERVATION_RESOURCE_MIME_TYPE)
        .with_size(raw_size);
    Ok(RetainedObservation {
        resource,
        raw_size,
        metadata,
        blob,
    })
}

fn validate_observation_completion(
    request: &ObservationRequest,
    metadata: &ObservationMetadata,
    dimensions: (u32, u32),
    runtime_limits: &RuntimeLimits,
) -> Result<(), ()> {
    metadata
        .validate_with_limits(runtime_limits)
        .map_err(|_| ())?;
    if metadata.observation_id != request.observation_id
        || metadata.scene_revision != request.scene_revision
        || metadata.camera_id != request.camera_id
        || metadata.kind != request.kind
        || metadata.quality != request.quality
        || metadata.staleness.latest_known_revision != request.scene_revision
        || metadata.staleness.revisions_behind != 0
    {
        return Err(());
    }
    match (request.kind, metadata.dimensions) {
        (ObservationKind::Visibility, None) => Ok(()),
        (ObservationKind::Visibility, Some(_)) | (_, None) => Err(()),
        (_, Some(actual))
            if actual.width.get() == dimensions.0 && actual.height.get() == dimensions.1 =>
        {
            Ok(())
        }
        (_, Some(_)) => Err(()),
    }
}

fn observation_resource_uri(observation_id: ObservationId) -> String {
    format!("cogniform://observations/{observation_id}")
}

fn base64_encode(input: &[u8]) -> Result<String, ()> {
    let encoded_len = base64_encoded_len(input.len())?;
    let mut encoded = String::new();
    encoded.try_reserve_exact(encoded_len).map_err(|_| ())?;
    let mut chunks = input.chunks_exact(3);
    for chunk in &mut chunks {
        let bits = (u32::from(chunk[0]) << 16) | (u32::from(chunk[1]) << 8) | u32::from(chunk[2]);
        encoded.push(char::from(BASE64_ALPHABET[((bits >> 18) & 0x3f) as usize]));
        encoded.push(char::from(BASE64_ALPHABET[((bits >> 12) & 0x3f) as usize]));
        encoded.push(char::from(BASE64_ALPHABET[((bits >> 6) & 0x3f) as usize]));
        encoded.push(char::from(BASE64_ALPHABET[(bits & 0x3f) as usize]));
    }
    match chunks.remainder() {
        [] => {}
        [first] => {
            let bits = u32::from(*first) << 16;
            encoded.push(char::from(BASE64_ALPHABET[((bits >> 18) & 0x3f) as usize]));
            encoded.push(char::from(BASE64_ALPHABET[((bits >> 12) & 0x3f) as usize]));
            encoded.push('=');
            encoded.push('=');
        }
        [first, second] => {
            let bits = (u32::from(*first) << 16) | (u32::from(*second) << 8);
            encoded.push(char::from(BASE64_ALPHABET[((bits >> 18) & 0x3f) as usize]));
            encoded.push(char::from(BASE64_ALPHABET[((bits >> 12) & 0x3f) as usize]));
            encoded.push(char::from(BASE64_ALPHABET[((bits >> 6) & 0x3f) as usize]));
            encoded.push('=');
        }
        _ => unreachable!("chunks_exact remainder is shorter than three bytes"),
    }
    debug_assert_eq!(encoded.len(), encoded_len);
    Ok(encoded)
}

pub(crate) fn base64_encoded_len(input_len: usize) -> Result<usize, ()> {
    input_len
        .checked_add(2)
        .and_then(|value| value.checked_div(3))
        .and_then(|value| value.checked_mul(4))
        .ok_or(())
}

fn observation_result(retained: &RetainedObservation) -> CallToolResult {
    serde_json::to_value(ObservationToolOutput {
        schema_version: 1,
        resource_uri: &retained.resource.uri,
        resource_size: retained.raw_size,
        metadata: &retained.metadata,
    })
    .map_or_else(
        |_| tool_error("output_unavailable"),
        |value| {
            let mut result = CallToolResult::success(vec![
                ContentBlock::text("cogniform observation resource"),
                ContentBlock::ResourceLink(retained.resource.clone()),
            ]);
            result.structured_content = Some(value);
            result
        },
    )
}

fn finish_patch_admission(
    patch: &ScenePatch,
    runtime_limits: &RuntimeLimits,
    admission: GatewayAdmission,
    process_next: impl FnOnce() -> Result<Option<GatewayResponse>, LocalServiceError>,
) -> CallToolResult {
    let (admission_name, response) = match admission {
        GatewayAdmission::Queued { idempotency_key } => {
            if idempotency_key != patch.idempotency_key {
                return tool_error("invalid_service_output");
            }
            match process_next() {
                Ok(Some(response)) => ("queued", response),
                Ok(None) => return tool_error("service_failed"),
                Err(error) => return tool_error(patch_process_error(&error)),
            }
        }
        GatewayAdmission::Replayed { response } => ("replayed", *response),
        GatewayAdmission::AlreadyQueued { .. }
        | GatewayAdmission::Superseded { .. }
        | GatewayAdmission::Dropped { .. } => return tool_error("service_busy"),
    };
    let GatewayResponse::PatchApplied { receipt } = response else {
        return tool_error("invalid_service_output");
    };
    if validate_patch_completion(
        patch,
        &receipt,
        runtime_limits,
        admission_name == "replayed",
    )
    .is_err()
    {
        return tool_error("invalid_service_output");
    }
    structured_result(&PatchToolOutput {
        schema_version: 1,
        admission: admission_name,
        receipt: &receipt,
    })
}

impl LazyLocalService {
    fn runtime_limits(&self) -> RuntimeLimits {
        self.config.engine.world.runtime_limits
    }

    async fn service(&mut self) -> Result<&mut LocalService, CallToolResult> {
        if self.service_failed {
            return Err(tool_error("service_failed"));
        }
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

    fn poison_service(&mut self) {
        self.service = None;
        self.service_failed = true;
    }
}

impl ServerHandler for CogniformMcpServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(
            ServerCapabilities::builder()
                .enable_tools()
                .enable_resources()
                .build(),
        )
            .with_protocol_version(ProtocolVersion::V_2025_11_25)
            .with_server_info(
                Implementation::new("cogniform", env!("CARGO_PKG_VERSION"))
                    .with_title("Cogniform local scene service"),
            )
            .with_instructions(
                "Query or observe an exact scene revision, submit one bounded idempotent imagination, or apply one bounded idempotent patch.",
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
            patch_tool(),
            observation_tool(),
        ])))
    }

    fn get_tool(&self, name: &str) -> Option<Tool> {
        match name {
            QUERY_SCENE_TOOL => Some(query_tool()),
            SUBMIT_IMAGINATION_TOOL => Some(imagination_tool()),
            APPLY_PATCH_TOOL => Some(patch_tool()),
            OBSERVE_SCENE_TOOL => Some(observation_tool()),
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
            APPLY_PATCH_TOOL => self.apply_patch(request.arguments).await,
            OBSERVE_SCENE_TOOL => self.observe_scene(request.arguments).await,
            _ => {
                return Err(McpError::method_not_found::<
                    rmcp::model::CallToolRequestMethod,
                >());
            }
        })
    }

    async fn list_resources(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListResourcesResult, McpError> {
        let state = self.state.lock().await;
        Ok(ListResourcesResult::with_all_items(
            state
                .retained_observation
                .as_ref()
                .map(|retained| vec![retained.resource.clone()])
                .unwrap_or_default(),
        ))
    }

    async fn read_resource(
        &self,
        request: ReadResourceRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<ReadResourceResult, McpError> {
        let state = self.state.lock().await;
        let retained = state
            .retained_observation
            .as_ref()
            .filter(|retained| retained.resource.uri == request.uri)
            .ok_or_else(|| McpError::resource_not_found("resource not found", None))?;
        Ok(ReadResourceResult::new(vec![
            ResourceContents::blob(retained.blob.clone(), retained.resource.uri.clone())
                .with_mime_type(OBSERVATION_RESOURCE_MIME_TYPE),
        ]))
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

#[derive(Serialize)]
struct PatchToolOutput<'a> {
    schema_version: u16,
    admission: &'static str,
    receipt: &'a ApplyReceipt,
}

#[derive(Serialize)]
struct ObservationToolOutput<'a> {
    schema_version: u16,
    resource_uri: &'a str,
    resource_size: u64,
    metadata: &'a ObservationMetadata,
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

fn validate_patch_completion(
    patch: &ScenePatch,
    receipt: &ApplyReceipt,
    runtime_limits: &RuntimeLimits,
    replayed: bool,
) -> Result<(), ()> {
    if receipt.transaction_id != patch.transaction_id
        || receipt.idempotency_key != patch.idempotency_key
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
    receipt.validate_with_limits(runtime_limits).map_err(|_| ())
}

fn patch_process_error(error: &LocalServiceError) -> &'static str {
    if error.is_patch_rejected_without_mutation() {
        "patch_rejected"
    } else {
        "service_failed"
    }
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

fn patch_tool() -> Tool {
    Tool::new(
        APPLY_PATCH_TOOL,
        "Validate and atomically apply one bounded explicit scene patch with exact idempotent replay.",
        Arc::new(schema_object(json!({
            "type": "object",
            "additionalProperties": false,
            "required": ["schema_version", "transaction_id", "idempotency_key", "base_revision", "conflict_policy", "delivery", "declared_budget", "operations"],
            "properties": {
                "schema_version": {"const": 1},
                "transaction_id": {"type": "string"},
                "idempotency_key": {"type": "string"},
                "base_revision": {"type": "integer", "minimum": 0},
                "conflict_policy": {"const": "require_exact_base"},
                "delivery": {"type": "object"},
                "declared_budget": {"type": "object"},
                "operations": {"type": "array", "minItems": 1, "items": {"type": "object"}}
            }
        }))),
    )
    .with_raw_output_schema(Arc::new(schema_object(json!({
        "oneOf": [
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
        ]
    }))))
    .with_annotations(
        ToolAnnotations::with_title("Apply Cogniform scene patch")
            .read_only(false)
            .destructive(true)
            .idempotent(true)
            .open_world(false),
    )
}

fn observation_tool() -> Tool {
    Tool::new(
        OBSERVE_SCENE_TOOL,
        "Render one bounded exact-revision observation and retain its canonical payload as one MCP resource.",
        Arc::new(schema_object(json!({
            "type": "object",
            "additionalProperties": false,
            "required": ["schema_version", "observation_id", "scene_revision", "camera_id", "kind", "quality"],
            "properties": {
                "schema_version": {"const": 1},
                "observation_id": {"type": "string"},
                "scene_revision": {"type": "integer", "minimum": 0},
                "camera_id": {"type": "string"},
                "kind": {"enum": ["color", "depth", "normal", "entity_id", "visibility"]},
                "quality": {"enum": ["low", "medium", "high"]}
            }
        }))),
    )
    .with_raw_output_schema(Arc::new(schema_object(json!({
        "oneOf": [
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
        ]
    }))))
    .with_annotations(
        ToolAnnotations::with_title("Observe Cogniform scene")
            .read_only(false)
            .destructive(false)
            .idempotent(false)
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
    use cogniform_protocol::{
        FrameId, ImageDimensions, ObservationQuality, ObservationStaleness, SceneRevision,
        SchemaVersion, StableEntityId,
    };

    fn error_code(result: &CallToolResult) -> Option<&str> {
        result.structured_content.as_ref()?.get("error")?.as_str()
    }

    fn patch() -> ScenePatch {
        serde_json::from_str(include_str!(
            "../../cogniform-protocol/tests/fixtures/scene_patch_v1.json"
        ))
        .unwrap()
    }

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

    #[tokio::test]
    async fn invalid_patch_precedes_lazy_service_creation() {
        let server = CogniformMcpServer::new(LocalServiceConfig::new(64, 64));
        let mut patch = patch();
        patch.operations.clear();
        let Value::Object(arguments) = serde_json::to_value(patch).unwrap() else {
            unreachable!("test patch is an object");
        };
        let result = server.apply_patch(Some(arguments)).await;
        assert_eq!(result.is_error, Some(true));
        assert_eq!(
            result.structured_content,
            Some(json!({"schema_version": 1, "error": "invalid_patch"}))
        );
        assert!(server.state.lock().await.service.is_none());
    }

    #[tokio::test]
    async fn malformed_and_over_limit_patches_precede_service_creation() {
        let server = CogniformMcpServer::new(LocalServiceConfig::new(64, 64));
        assert_eq!(
            error_code(&server.apply_patch(None).await),
            Some("invalid_arguments")
        );
        assert!(server.state.lock().await.service.is_none());

        let mut config = LocalServiceConfig::new(64, 64);
        config.engine.world.runtime_limits.max_operations = std::num::NonZeroU32::new(2).unwrap();
        let server = CogniformMcpServer::new(config);
        let Value::Object(arguments) = serde_json::to_value(patch()).unwrap() else {
            unreachable!("test patch is an object");
        };
        assert_eq!(
            error_code(&server.apply_patch(Some(arguments)).await),
            Some("invalid_patch")
        );
        assert!(server.state.lock().await.service.is_none());
    }

    #[tokio::test]
    async fn invalid_observation_precedes_lazy_service_creation() {
        let server = CogniformMcpServer::new(LocalServiceConfig::new(64, 64));
        let Value::Object(arguments) = json!({
            "schema_version": 2,
            "observation_id": "00000000000000000000000000000041",
            "scene_revision": 0,
            "camera_id": "00000000000000000000000000000031",
            "kind": "visibility",
            "quality": "low"
        }) else {
            unreachable!("test arguments are an object");
        };
        let result = server.observe_scene(Some(arguments)).await;
        assert_eq!(error_code(&result), Some("invalid_observation"));
        assert!(server.state.lock().await.service.is_none());
    }

    #[test]
    fn base64_encoding_matches_rfc_4648_vectors() {
        for (input, expected) in [
            (&b""[..], ""),
            (&b"f"[..], "Zg=="),
            (&b"fo"[..], "Zm8="),
            (&b"foo"[..], "Zm9v"),
            (&b"foob"[..], "Zm9vYg=="),
            (&b"fooba"[..], "Zm9vYmE="),
            (&b"foobar"[..], "Zm9vYmFy"),
        ] {
            assert_eq!(base64_encode(input).unwrap(), expected);
        }
    }

    #[test]
    fn observation_completion_builds_the_exact_minimum_resource() {
        let request = observation_request(0x41);
        let metadata = visibility_metadata(&request);
        let limits = RuntimeLimits::default();
        let retained = retain_observation(
            &request,
            metadata.clone(),
            &ObservationPayload::Visibility(Vec::new()),
            (64, 64),
            &limits,
            ObservationPayloadLimits::default(),
        )
        .unwrap();
        assert_eq!(retained.resource.size, Some(60));
        assert_eq!(retained.blob.len(), 80);
    }

    #[test]
    fn observation_completion_rejects_every_mismatched_causal_role() {
        let request = observation_request(0x41);
        let metadata = visibility_metadata(&request);

        let mut mismatched = metadata.clone();
        mismatched.observation_id = ObservationId::new(0x42).unwrap();
        assert_invalid_completion(&request, &mismatched);

        let mut mismatched = metadata.clone();
        mismatched.scene_revision = SceneRevision::new(1);
        mismatched.staleness.latest_known_revision = SceneRevision::new(1);
        assert_invalid_completion(&request, &mismatched);

        let mut mismatched = metadata.clone();
        mismatched.camera_id = StableEntityId::new(0x32).unwrap();
        assert_invalid_completion(&request, &mismatched);

        let mut mismatched = metadata.clone();
        mismatched.kind = ObservationKind::Color;
        mismatched.dimensions = Some(image_dimensions(64, 64));
        assert_invalid_completion(&request, &mismatched);

        let mut mismatched = metadata.clone();
        mismatched.quality = ObservationQuality::High;
        assert_invalid_completion(&request, &mismatched);

        let mut mismatched = metadata.clone();
        mismatched.staleness.latest_known_revision = SceneRevision::new(1);
        mismatched.staleness.revisions_behind = 1;
        assert_invalid_completion(&request, &mismatched);

        let mut mismatched = metadata;
        mismatched.staleness.revisions_behind = 1;
        assert_invalid_completion(&request, &mismatched);
    }

    #[test]
    fn observation_completion_enforces_visibility_and_image_dimensions() {
        let visibility_request = observation_request(0x41);
        let mut visibility_result = visibility_metadata(&visibility_request);
        visibility_result.dimensions = Some(image_dimensions(64, 64));
        assert_invalid_completion(&visibility_request, &visibility_result);

        let mut image_request = observation_request(0x42);
        image_request.kind = ObservationKind::Color;
        let mut image_metadata = visibility_metadata(&image_request);
        assert_invalid_completion(&image_request, &image_metadata);

        image_metadata.dimensions = Some(image_dimensions(32, 64));
        assert_invalid_completion(&image_request, &image_metadata);

        image_metadata.dimensions = Some(image_dimensions(64, 64));
        assert!(
            validate_observation_completion(
                &image_request,
                &image_metadata,
                (64, 64),
                &RuntimeLimits::default(),
            )
            .is_ok()
        );
    }

    #[test]
    fn observation_resource_limits_map_to_a_stable_request_failure() {
        let request = observation_request(0x41);
        let metadata = visibility_metadata(&request);
        let limits = RuntimeLimits::default();

        let tiny = ObservationPayloadLimits::new(
            std::num::NonZeroU64::new(60).unwrap(),
            std::num::NonZeroU32::new(1).unwrap(),
        );
        assert_eq!(
            retain_observation(
                &request,
                metadata,
                &ObservationPayload::Visibility(vec![cogniform_observation::EntityVisibility {
                    entity_id: StableEntityId::new(0x51).unwrap(),
                    visible_pixels: 1,
                },]),
                (64, 64),
                &limits,
                tiny,
            )
            .unwrap_err()
            .code,
            "observation_too_large"
        );
    }

    fn assert_invalid_completion(request: &ObservationRequest, metadata: &ObservationMetadata) {
        assert!(
            validate_observation_completion(
                request,
                metadata,
                (64, 64),
                &RuntimeLimits::default(),
            )
            .is_err()
        );
    }

    fn image_dimensions(width: u32, height: u32) -> ImageDimensions {
        ImageDimensions {
            width: std::num::NonZeroU32::new(width).unwrap(),
            height: std::num::NonZeroU32::new(height).unwrap(),
        }
    }

    #[test]
    fn polling_timeout_is_terminal_and_poisoning() {
        struct NeverReady;

        impl ObservationBackend for NeverReady {
            fn dimensions(&self) -> (u32, u32) {
                (64, 64)
            }

            fn request_observation(&mut self, _request: ObservationRequest) -> Result<(), ()> {
                Ok(())
            }

            fn try_receive_observation(
                &mut self,
            ) -> Result<Option<AdapterObservationDelivery>, ()> {
                Ok(None)
            }
        }

        let failure = drive_observation(
            &observation_request(0x41),
            &mut NeverReady,
            &RuntimeLimits::default(),
            ObservationPayloadLimits::default(),
            ObservationPollPolicy {
                cadence: Duration::from_millis(1),
                deadline: Duration::ZERO,
            },
        )
        .unwrap_err();
        assert_eq!(failure.code, "observation_timeout");
        assert!(failure.poison_service);
    }

    #[test]
    fn completion_available_only_after_the_deadline_is_not_polled() {
        struct ReadyOnSecondPoll {
            request: Option<ObservationRequest>,
            polls: u8,
        }
        impl ObservationBackend for ReadyOnSecondPoll {
            fn dimensions(&self) -> (u32, u32) {
                (64, 64)
            }

            fn request_observation(&mut self, request: ObservationRequest) -> Result<(), ()> {
                self.request = Some(request);
                Ok(())
            }

            fn try_receive_observation(
                &mut self,
            ) -> Result<Option<AdapterObservationDelivery>, ()> {
                self.polls += 1;
                if self.polls == 1 {
                    return Ok(None);
                }
                let request = self.request.take().unwrap();
                Ok(Some(AdapterObservationDelivery::Completed {
                    metadata: visibility_metadata(&request),
                    payload: ObservationPayload::Visibility(Vec::new()),
                }))
            }
        }

        let mut backend = ReadyOnSecondPoll {
            request: None,
            polls: 0,
        };
        let failure = drive_observation(
            &observation_request(0x41),
            &mut backend,
            &RuntimeLimits::default(),
            ObservationPayloadLimits::default(),
            ObservationPollPolicy {
                cadence: Duration::from_millis(10),
                deadline: Duration::from_millis(10),
            },
        )
        .unwrap_err();
        assert_eq!(failure.code, "observation_timeout");
        assert!(failure.poison_service);
        assert_eq!(backend.polls, 1);
        assert!(backend.request.is_some());
    }

    #[test]
    fn polling_policy_and_service_failures_are_exact() {
        struct OnePoll(Result<Option<AdapterObservationDelivery>, ()>);
        impl ObservationBackend for OnePoll {
            fn dimensions(&self) -> (u32, u32) {
                (64, 64)
            }

            fn request_observation(&mut self, _request: ObservationRequest) -> Result<(), ()> {
                Ok(())
            }

            fn try_receive_observation(
                &mut self,
            ) -> Result<Option<AdapterObservationDelivery>, ()> {
                std::mem::replace(&mut self.0, Ok(None))
            }
        }

        assert_eq!(
            ObservationPollPolicy::default().cadence,
            Duration::from_millis(2)
        );
        assert_eq!(
            ObservationPollPolicy::default().deadline,
            Duration::from_secs(15)
        );

        let request = observation_request(0x41);
        let policy = ObservationPollPolicy::default();
        let poll_failure = drive_observation(
            &request,
            &mut OnePoll(Err(())),
            &RuntimeLimits::default(),
            ObservationPayloadLimits::default(),
            policy,
        )
        .unwrap_err();
        assert_eq!(poll_failure.code, "service_failed");
        assert!(poll_failure.poison_service);

        let wrong_id = drive_observation(
            &request,
            &mut OnePoll(Ok(Some(AdapterObservationDelivery::Failed {
                observation_id: ObservationId::new(0x42).unwrap(),
            }))),
            &RuntimeLimits::default(),
            ObservationPayloadLimits::default(),
            policy,
        )
        .unwrap_err();
        assert_eq!(wrong_id.code, "invalid_service_output");
        assert!(wrong_id.poison_service);
    }

    fn observation_request(observation_id: u128) -> ObservationRequest {
        ObservationRequest {
            schema_version: SchemaVersion::V1,
            observation_id: ObservationId::new(observation_id).unwrap(),
            scene_revision: SceneRevision::INITIAL,
            camera_id: StableEntityId::new(0x31).unwrap(),
            kind: ObservationKind::Visibility,
            quality: ObservationQuality::Low,
        }
    }

    fn visibility_metadata(request: &ObservationRequest) -> ObservationMetadata {
        ObservationMetadata {
            schema_version: SchemaVersion::V1,
            observation_id: request.observation_id,
            scene_revision: request.scene_revision,
            frame_id: FrameId::new(1).unwrap(),
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
        }
    }

    #[test]
    fn patch_completion_requires_exact_roles() {
        let limits = RuntimeLimits::default();
        let patch = patch();
        let mut receipt: ApplyReceipt = serde_json::from_str(include_str!(
            "../../cogniform-protocol/tests/fixtures/apply_receipt_v1.json"
        ))
        .unwrap();
        assert!(validate_patch_completion(&patch, &receipt, &limits, false).is_ok());

        receipt.status = ApplyStatus::IdempotentReplay;
        assert!(validate_patch_completion(&patch, &receipt, &limits, true).is_ok());
        assert!(validate_patch_completion(&patch, &receipt, &limits, false).is_err());

        let replay_receipt = receipt.clone();
        receipt.transaction_id = cogniform_protocol::TransactionId::new(99).unwrap();
        assert!(validate_patch_completion(&patch, &receipt, &limits, true).is_err());

        receipt = replay_receipt.clone();
        receipt.idempotency_key = cogniform_protocol::IdempotencyKey::new(99).unwrap();
        assert!(validate_patch_completion(&patch, &receipt, &limits, true).is_err());

        receipt = replay_receipt.clone();
        receipt.previous_revision = cogniform_protocol::SceneRevision::new(6);
        assert!(validate_patch_completion(&patch, &receipt, &limits, true).is_err());

        receipt = replay_receipt.clone();
        receipt.operation_count = std::num::NonZeroU32::new(2).unwrap();
        assert!(validate_patch_completion(&patch, &receipt, &limits, true).is_err());

        receipt = replay_receipt;
        receipt.new_revision = cogniform_protocol::SceneRevision::new(9);
        assert!(validate_patch_completion(&patch, &receipt, &limits, true).is_err());
    }

    #[test]
    fn patch_admission_outcomes_are_stable_and_bounded() {
        let limits = RuntimeLimits::default();
        let patch = patch();
        let receipt: ApplyReceipt = serde_json::from_str(include_str!(
            "../../cogniform-protocol/tests/fixtures/apply_receipt_v1.json"
        ))
        .unwrap();

        let queued = finish_patch_admission(
            &patch,
            &limits,
            GatewayAdmission::Queued {
                idempotency_key: patch.idempotency_key,
            },
            || {
                Ok(Some(GatewayResponse::PatchApplied {
                    receipt: receipt.clone(),
                }))
            },
        );
        assert_eq!(queued.is_error, Some(false));
        let queued = queued.structured_content.unwrap();
        assert_eq!(queued["admission"], "queued");
        assert_eq!(queued["receipt"]["status"], "applied");
        assert_eq!(queued["receipt"]["new_revision"], 8);

        let mut replay_receipt = receipt.clone();
        replay_receipt.status = ApplyStatus::IdempotentReplay;
        let replayed = finish_patch_admission(
            &patch,
            &limits,
            GatewayAdmission::Replayed {
                response: Box::new(GatewayResponse::PatchApplied {
                    receipt: replay_receipt,
                }),
            },
            || panic!("replay must not process another command"),
        );
        assert_eq!(replayed.is_error, Some(false));
        let replayed = replayed.structured_content.unwrap();
        assert_eq!(replayed["admission"], "replayed");
        assert_eq!(replayed["receipt"]["status"], "idempotent_replay");
        assert_eq!(replayed["receipt"]["new_revision"], 8);

        for admission in [
            GatewayAdmission::AlreadyQueued {
                idempotency_key: patch.idempotency_key,
            },
            GatewayAdmission::Superseded {
                idempotency_key: patch.idempotency_key,
                superseded_idempotency_key: cogniform_protocol::IdempotencyKey::new(99).unwrap(),
            },
            GatewayAdmission::Dropped {
                idempotency_key: patch.idempotency_key,
            },
        ] {
            let result = finish_patch_admission(&patch, &limits, admission, || Ok(None));
            assert_eq!(error_code(&result), Some("service_busy"));
        }
    }

    #[test]
    fn patch_admission_rejects_invalid_service_outputs() {
        let limits = RuntimeLimits::default();
        let patch = patch();
        let wrong_key = cogniform_protocol::IdempotencyKey::new(99).unwrap();
        let result = finish_patch_admission(
            &patch,
            &limits,
            GatewayAdmission::Queued {
                idempotency_key: wrong_key,
            },
            || panic!("a mismatched admission must not be processed"),
        );
        assert_eq!(error_code(&result), Some("invalid_service_output"));

        let result = finish_patch_admission(
            &patch,
            &limits,
            GatewayAdmission::Queued {
                idempotency_key: patch.idempotency_key,
            },
            || Ok(None),
        );
        assert_eq!(error_code(&result), Some("service_failed"));

        let result = finish_patch_admission(
            &patch,
            &limits,
            GatewayAdmission::Queued {
                idempotency_key: patch.idempotency_key,
            },
            || {
                Ok(Some(GatewayResponse::ImaginationProcessed {
                    compilation: Box::new(
                        serde_json::from_str(include_str!(
                            "../../cogniform-compilation/tests/fixtures/compiled_result_v1.json"
                        ))
                        .unwrap(),
                    ),
                    receipt: None,
                }))
            },
        );
        assert_eq!(error_code(&result), Some("invalid_service_output"));

        let result = finish_patch_admission(
            &patch,
            &limits,
            GatewayAdmission::Queued {
                idempotency_key: patch.idempotency_key,
            },
            || {
                Err(LocalServiceError::Gateway(Box::new(
                    cogniform_engine::GatewayError::InvalidConfig { reason: "test" },
                )))
            },
        );
        assert_eq!(error_code(&result), Some("service_failed"));
    }
}
