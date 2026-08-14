use std::{
    io,
    num::{NonZeroU16, NonZeroU64},
    sync::{Arc, Mutex as StdMutex},
};

use rmcp::{
    model::{ClientJsonRpcMessage, ClientNotification, RequestId, ServerJsonRpcMessage},
    service::{RoleServer, RxJsonRpcMessage, TxJsonRpcMessage},
    transport::Transport,
};
use tokio::{
    io::{AsyncBufReadExt as _, AsyncRead, AsyncWrite, AsyncWriteExt as _, BufReader},
    sync::{Mutex, Notify, OwnedSemaphorePermit, Semaphore},
};

const DEFAULT_MAX_INPUT_BYTES: u64 = 1_114_112;
const DEFAULT_MAX_OUTPUT_BYTES: u64 = 8_388_608;
const DEFAULT_MAX_JSON_NESTING_DEPTH: u16 = 40;

/// Explicit resource limits for newline-delimited MCP JSON-RPC frames.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct McpTransportLimits {
    /// Maximum input bytes including the terminating LF.
    pub max_input_bytes: NonZeroU64,
    /// Maximum output bytes including the terminating LF.
    pub max_output_bytes: NonZeroU64,
    /// Maximum object/array nesting accepted or emitted.
    pub max_json_nesting_depth: NonZeroU16,
}

impl Default for McpTransportLimits {
    fn default() -> Self {
        Self {
            max_input_bytes: NonZeroU64::new(DEFAULT_MAX_INPUT_BYTES)
                .expect("constant is non-zero"),
            max_output_bytes: NonZeroU64::new(DEFAULT_MAX_OUTPUT_BYTES)
                .expect("constant is non-zero"),
            max_json_nesting_depth: NonZeroU16::new(DEFAULT_MAX_JSON_NESTING_DEPTH)
                .expect("constant is non-zero"),
        }
    }
}

/// Stable payload-redacted transport failure category.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransportFailureKind {
    /// An input frame exceeded the configured byte bound.
    InputSizeExceeded,
    /// An output frame exceeded the configured byte bound.
    OutputSizeExceeded,
    /// A frame exceeded the configured object/array nesting bound.
    NestingExceeded,
    /// A complete input line was not a valid MCP JSON-RPC message.
    InvalidMessage,
    /// End-of-file arrived in the middle of a frame.
    TruncatedMessage,
    /// The inherited input stream failed.
    InputFailed,
    /// The inherited output stream failed.
    OutputFailed,
}

impl std::fmt::Display for TransportFailureKind {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::InputSizeExceeded => "input_size_exceeded",
            Self::OutputSizeExceeded => "output_size_exceeded",
            Self::NestingExceeded => "nesting_exceeded",
            Self::InvalidMessage => "invalid_message",
            Self::TruncatedMessage => "truncated_message",
            Self::InputFailed => "input_failed",
            Self::OutputFailed => "output_failed",
        })
    }
}

/// Transport error surfaced to the SDK without input, output, or OS details.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct McpTransportError(TransportFailureKind);

impl std::fmt::Display for McpTransportError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(formatter)
    }
}

impl std::error::Error for McpTransportError {}

#[derive(Debug, Default)]
struct TransportStatusInner {
    failure: StdMutex<Option<TransportFailureKind>>,
    failed: Notify,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct TransportStatus(Arc<TransportStatusInner>);

impl TransportStatus {
    fn record(&self, kind: TransportFailureKind) {
        if let Ok(mut failure) = self.0.failure.lock()
            && failure.is_none()
        {
            *failure = Some(kind);
            self.0.failed.notify_one();
        }
    }

    pub(crate) fn failure(&self) -> Option<TransportFailureKind> {
        self.0.failure.lock().ok().and_then(|failure| *failure)
    }
}

pub(crate) struct BoundedTransport<R, W> {
    reader: BufReader<R>,
    writer: Arc<Mutex<W>>,
    input: Vec<u8>,
    limits: McpTransportLimits,
    status: TransportStatus,
    request_permits: Arc<Semaphore>,
    request_flow: Arc<Mutex<RequestFlow>>,
    pending_input: Option<ClientJsonRpcMessage>,
}

struct ActiveRequest {
    id: RequestId,
    permit: OwnedSemaphorePermit,
}

#[derive(Default)]
struct RequestFlow {
    active: Option<ActiveRequest>,
    cancelled: Option<RequestId>,
}

impl<R: AsyncRead + Unpin, W> BoundedTransport<R, W> {
    pub(crate) fn new(reader: R, writer: W, limits: McpTransportLimits) -> (Self, TransportStatus) {
        let status = TransportStatus::default();
        (
            Self {
                reader: BufReader::new(reader),
                writer: Arc::new(Mutex::new(writer)),
                input: Vec::new(),
                limits,
                status: status.clone(),
                request_permits: Arc::new(Semaphore::new(1)),
                request_flow: Arc::new(Mutex::new(RequestFlow::default())),
                pending_input: None,
            },
            status,
        )
    }

    async fn read_message(&mut self) -> Option<ClientJsonRpcMessage> {
        loop {
            if self.status.failure().is_some() {
                return None;
            }
            let available = tokio::select! {
                available = self.reader.fill_buf() => available,
                () = self.status.0.failed.notified() => return None,
            };
            let Ok(available) = available else {
                self.status.record(TransportFailureKind::InputFailed);
                return None;
            };
            if available.is_empty() {
                if !self.input.is_empty() {
                    self.status.record(TransportFailureKind::TruncatedMessage);
                }
                return None;
            }

            let newline = available.iter().position(|byte| *byte == b'\n');
            let consumed = newline.map_or(available.len(), |index| index + 1);
            if exceeds_limit(
                self.input.len(),
                consumed,
                self.limits.max_input_bytes.get(),
            ) {
                self.status.record(TransportFailureKind::InputSizeExceeded);
                return None;
            }
            let payload_len = newline.unwrap_or(available.len());
            self.input.extend_from_slice(&available[..payload_len]);
            self.reader.consume(consumed);
            if newline.is_some() {
                break;
            }
        }

        if json_nesting_depth(&self.input) > self.limits.max_json_nesting_depth.get() {
            self.status.record(TransportFailureKind::NestingExceeded);
            return None;
        }
        let mut message = serde_json::from_slice(&self.input).map_or_else(
            |_| {
                self.status.record(TransportFailureKind::InvalidMessage);
                None
            },
            Some,
        );
        self.input.clear();
        if let Some(ClientJsonRpcMessage::Notification(notification)) = message.as_mut()
            && let ClientNotification::CancelledNotification(cancelled) =
                &mut notification.notification
        {
            cancelled.params.reason = None;
        }
        if matches!(
            message,
            Some(ClientJsonRpcMessage::Response(_) | ClientJsonRpcMessage::Error(_))
        ) {
            self.status.record(TransportFailureKind::InvalidMessage);
            return None;
        }
        message
    }

    pub(crate) async fn read_opening_message(&mut self) -> Option<ClientJsonRpcMessage> {
        debug_assert!(self.pending_input.is_none());
        debug_assert!(self.request_flow.lock().await.active.is_none());
        self.read_message().await
    }

    pub(crate) fn retain_opening_message(&mut self, message: ClientJsonRpcMessage) {
        debug_assert!(self.pending_input.is_none());
        self.pending_input = Some(message);
    }

    async fn dispatch_pending(&mut self) -> Option<ClientJsonRpcMessage> {
        let Ok(permit) = Arc::clone(&self.request_permits).acquire_owned().await else {
            self.status.record(TransportFailureKind::InputFailed);
            return None;
        };
        if self.status.failure().is_some() {
            return None;
        }
        let mut flow = self.request_flow.lock().await;
        let message = self
            .pending_input
            .take()
            .expect("pending input remains owned until dispatch");
        if let ClientJsonRpcMessage::Request(request) = &message {
            let replaced = flow.active.replace(ActiveRequest {
                id: request.id.clone(),
                permit,
            });
            debug_assert!(replaced.is_none(), "one request is active at a time");
        } else {
            drop(permit);
        }
        Some(message)
    }
}

impl<R, W> Transport<RoleServer> for BoundedTransport<R, W>
where
    R: AsyncRead + Unpin + Send + 'static,
    W: AsyncWrite + Unpin + Send + 'static,
{
    type Error = McpTransportError;

    fn send(
        &mut self,
        item: TxJsonRpcMessage<RoleServer>,
    ) -> impl Future<Output = Result<(), Self::Error>> + Send + 'static {
        let writer = Arc::clone(&self.writer);
        let limits = self.limits;
        let status = self.status.clone();
        let request_flow = Arc::clone(&self.request_flow);
        let response_id = match &item {
            ServerJsonRpcMessage::Response(response) => Some(response.id.clone()),
            ServerJsonRpcMessage::Error(error) => error.id.clone(),
            ServerJsonRpcMessage::Request(_) | ServerJsonRpcMessage::Notification(_) => None,
        };
        async move {
            let (request_permit, suppress) = if let Some(response_id) = response_id.as_ref() {
                let mut flow = request_flow.lock().await;
                let suppress = flow.cancelled.as_ref() == Some(response_id);
                let completes_active = flow
                    .active
                    .as_ref()
                    .is_some_and(|active| active.id == *response_id);
                let permit = completes_active
                    .then(|| flow.active.take().map(|active| active.permit))
                    .flatten();
                (permit, suppress)
            } else {
                (None, false)
            };
            if suppress {
                drop(request_permit);
                return Ok(());
            }
            let encoded = encode_bounded(&item, limits).map_err(|kind| {
                status.record(kind);
                McpTransportError(kind)
            })?;
            let mut writer = writer.lock().await;
            writer.write_all(&encoded).await.map_err(|_| {
                status.record(TransportFailureKind::OutputFailed);
                McpTransportError(TransportFailureKind::OutputFailed)
            })?;
            writer.flush().await.map_err(|_| {
                status.record(TransportFailureKind::OutputFailed);
                McpTransportError(TransportFailureKind::OutputFailed)
            })?;
            drop(request_permit);
            Ok(())
        }
    }

    async fn receive(&mut self) -> Option<RxJsonRpcMessage<RoleServer>> {
        loop {
            if self.request_flow.lock().await.cancelled.is_some() {
                return None;
            }
            if self.pending_input.is_some() {
                return self.dispatch_pending().await;
            }

            let active_before_read = self.request_flow.lock().await.active.is_some();
            if !active_before_read
                && !wait_for_response_flush(Arc::clone(&self.request_permits), self.status.clone())
                    .await
            {
                return None;
            }
            let Some(message) = self.read_message().await else {
                if active_before_read && self.status.failure().is_none() {
                    let _ = wait_for_response_flush(
                        Arc::clone(&self.request_permits),
                        self.status.clone(),
                    )
                    .await;
                }
                return None;
            };
            self.pending_input = Some(message);
            let mut flow = self.request_flow.lock().await;
            if let Some(active) = flow.active.as_ref() {
                if matching_cancellation(
                    self.pending_input.as_ref().expect("message was retained"),
                    &active.id,
                ) {
                    flow.cancelled = Some(active.id.clone());
                    drop(flow);
                    return self.pending_input.take();
                }
                drop(flow);
                continue;
            }
            drop(flow);
            return self.dispatch_pending().await;
        }
    }

    async fn close(&mut self) -> Result<(), Self::Error> {
        self.writer.lock().await.flush().await.map_err(|_| {
            self.status.record(TransportFailureKind::OutputFailed);
            McpTransportError(TransportFailureKind::OutputFailed)
        })
    }
}

async fn wait_for_response_flush(request_permits: Arc<Semaphore>, status: TransportStatus) -> bool {
    let Ok(read_turn) = request_permits.acquire_owned().await else {
        status.record(TransportFailureKind::InputFailed);
        return false;
    };
    drop(read_turn);
    status.failure().is_none()
}

fn matching_cancellation(message: &ClientJsonRpcMessage, active_id: &RequestId) -> bool {
    let ClientJsonRpcMessage::Notification(notification) = message else {
        return false;
    };
    let ClientNotification::CancelledNotification(cancelled) = &notification.notification else {
        return false;
    };
    cancelled.params.request_id.as_ref() == Some(active_id)
}

fn exceeds_limit(current: usize, additional: usize, limit: u64) -> bool {
    u64::try_from(current)
        .unwrap_or(u64::MAX)
        .saturating_add(u64::try_from(additional).unwrap_or(u64::MAX))
        > limit
}

fn encode_bounded<T: serde::Serialize>(
    value: &T,
    limits: McpTransportLimits,
) -> Result<Vec<u8>, TransportFailureKind> {
    let payload_limit = limits.max_output_bytes.get().saturating_sub(1);
    let mut output = BoundedWriter::new(payload_limit);
    serde_json::to_writer(&mut output, value)
        .map_err(|_| TransportFailureKind::OutputSizeExceeded)?;
    if json_nesting_depth(&output.bytes) > limits.max_json_nesting_depth.get() {
        return Err(TransportFailureKind::NestingExceeded);
    }
    output.bytes.push(b'\n');
    Ok(output.bytes)
}

struct BoundedWriter {
    bytes: Vec<u8>,
    limit: u64,
}

impl BoundedWriter {
    const fn new(limit: u64) -> Self {
        Self {
            bytes: Vec::new(),
            limit,
        }
    }
}

impl io::Write for BoundedWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if exceeds_limit(self.bytes.len(), bytes.len(), self.limit) {
            return Err(io::Error::new(
                io::ErrorKind::FileTooLarge,
                "bounded output",
            ));
        }
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

fn json_nesting_depth(encoded: &[u8]) -> u16 {
    let mut depth = 0_u16;
    let mut maximum = 0_u16;
    let mut in_string = false;
    let mut escaped = false;
    for byte in encoded {
        if in_string {
            if escaped {
                escaped = false;
            } else if *byte == b'\\' {
                escaped = true;
            } else if *byte == b'"' {
                in_string = false;
            }
            continue;
        }
        match byte {
            b'"' => in_string = true,
            b'{' | b'[' => {
                depth = depth.saturating_add(1);
                maximum = maximum.max(depth);
            }
            b'}' | b']' => depth = depth.saturating_sub(1),
            _ => {}
        }
    }
    maximum
}

#[cfg(test)]
mod tests {
    use super::*;
    use cogniform_compilation::CompilationLimits;
    use cogniform_observation::ObservationPayloadLimits;
    use cogniform_protocol::RuntimeLimits;
    use std::{
        future::Future as _,
        io::Write as _,
        num::{NonZeroU16, NonZeroU64},
        pin::Pin,
        task::{Context, Poll, Waker},
    };
    use tokio::io::{AsyncReadExt as _, AsyncWrite, BufReader, duplex, sink};

    #[derive(Clone, Default)]
    struct RecordingWriter(Arc<StdMutex<Vec<u8>>>);

    impl AsyncWrite for RecordingWriter {
        fn poll_write(
            self: Pin<&mut Self>,
            _context: &mut Context<'_>,
            bytes: &[u8],
        ) -> Poll<Result<usize, io::Error>> {
            self.0.lock().unwrap().extend_from_slice(bytes);
            Poll::Ready(Ok(bytes.len()))
        }

        fn poll_flush(
            self: Pin<&mut Self>,
            _context: &mut Context<'_>,
        ) -> Poll<Result<(), io::Error>> {
            Poll::Ready(Ok(()))
        }

        fn poll_shutdown(
            self: Pin<&mut Self>,
            _context: &mut Context<'_>,
        ) -> Poll<Result<(), io::Error>> {
            Poll::Ready(Ok(()))
        }
    }

    #[test]
    fn nesting_ignores_delimiters_inside_strings() {
        assert_eq!(json_nesting_depth(br#"{"value":"[{}]","nested":[{}]}"#), 3);
    }

    #[test]
    fn bounded_writer_accepts_equality_and_rejects_one_more() {
        let mut writer = BoundedWriter::new(3);
        assert!(writer.write_all(b"abc").is_ok());
        assert!(writer.write_all(b"d").is_err());
    }

    #[test]
    fn output_size_accepts_equality_and_rejects_one_less() {
        let value = serde_json::json!({"result": [1, 2, 3]});
        let encoded_len = serde_json::to_vec(&value).unwrap().len() as u64 + 1;
        let limits = McpTransportLimits {
            max_output_bytes: NonZeroU64::new(encoded_len).unwrap(),
            ..McpTransportLimits::default()
        };
        assert_eq!(
            encode_bounded(&value, limits).unwrap().len() as u64,
            encoded_len
        );
        let limits = McpTransportLimits {
            max_output_bytes: NonZeroU64::new(encoded_len - 1).unwrap(),
            ..McpTransportLimits::default()
        };
        assert_eq!(
            encode_bounded(&value, limits),
            Err(TransportFailureKind::OutputSizeExceeded)
        );
    }

    #[test]
    fn output_nesting_accepts_equality_and_rejects_one_less() {
        let value = serde_json::json!({"result": [{}]});
        let limits = McpTransportLimits {
            max_json_nesting_depth: NonZeroU16::new(3).unwrap(),
            ..McpTransportLimits::default()
        };
        assert!(encode_bounded(&value, limits).is_ok());
        let limits = McpTransportLimits {
            max_json_nesting_depth: NonZeroU16::new(2).unwrap(),
            ..McpTransportLimits::default()
        };
        assert_eq!(
            encode_bounded(&value, limits),
            Err(TransportFailureKind::NestingExceeded)
        );
    }

    #[test]
    fn default_transport_capacity_contains_core_result_bounds() {
        let transport = McpTransportLimits::default();
        let runtime = RuntimeLimits::default();
        let compilation = CompilationLimits::for_runtime_limits(runtime);
        assert!(
            transport.max_input_bytes.get()
                >= runtime.max_encoded_bytes.get().saturating_add(65_536)
        );
        assert!(
            transport.max_output_bytes.get()
                >= compilation
                    .max_encoded_bytes
                    .get()
                    .saturating_add(runtime.max_encoded_bytes.get())
                    .saturating_add(1_048_576)
        );
        assert!(
            transport.max_json_nesting_depth.get()
                >= runtime.max_json_nesting_depth.get().saturating_add(2)
        );
        assert!(
            transport.max_json_nesting_depth.get()
                >= compilation.max_json_nesting_depth.get().saturating_add(3)
        );
        let envelope_bytes =
            usize::try_from(ObservationPayloadLimits::default().max_envelope_bytes.get()).unwrap();
        let blob_bytes = crate::server::base64_encoded_len(envelope_bytes).unwrap();
        assert!(
            transport.max_output_bytes.get()
                >= u64::try_from(blob_bytes)
                    .unwrap()
                    .saturating_add(transport.max_input_bytes.get())
                    .saturating_add(65_536)
        );
    }

    #[test]
    fn maximum_observation_resource_accepts_exact_output_bound() {
        let raw_bytes = ObservationPayloadLimits::default().max_envelope_bytes.get();
        let blob_bytes =
            crate::server::base64_encoded_len(usize::try_from(raw_bytes).unwrap()).unwrap();
        let request_id_bytes = usize::try_from(DEFAULT_MAX_INPUT_BYTES)
            .unwrap()
            .saturating_sub(128);
        let value = serde_json::json!({
            "jsonrpc": "2.0",
            "id": "i".repeat(request_id_bytes),
            "result": {
                "contents": [{
                    "uri": "cogniform://observations/00000000000000000000000000000001",
                    "mimeType": "application/vnd.cogniform.observation-envelope",
                    "blob": "A".repeat(blob_bytes)
                }]
            }
        });
        let encoded_len = u64::try_from(serde_json::to_vec(&value).unwrap().len()).unwrap() + 1;
        assert!(encoded_len <= DEFAULT_MAX_OUTPUT_BYTES);
        let exact = McpTransportLimits {
            max_output_bytes: NonZeroU64::new(encoded_len).unwrap(),
            ..McpTransportLimits::default()
        };
        assert_eq!(
            u64::try_from(encode_bounded(&value, exact).unwrap().len()).unwrap(),
            encoded_len
        );
        let one_less = McpTransportLimits {
            max_output_bytes: NonZeroU64::new(encoded_len - 1).unwrap(),
            ..McpTransportLimits::default()
        };
        assert_eq!(
            encode_bounded(&value, one_less),
            Err(TransportFailureKind::OutputSizeExceeded)
        );
    }

    #[tokio::test]
    async fn matching_cancellation_is_terminal_and_suppresses_output() {
        for (id, error_response) in [
            (serde_json::json!(7), false),
            (serde_json::json!(7), true),
            (serde_json::json!("request-7"), false),
            (serde_json::json!("request-7"), true),
        ] {
            let request = json_line(&serde_json::json!({
                "jsonrpc": "2.0",
                "id": id.clone(),
                "method": "tools/call",
                "params": {"name": "cogniform.observe_scene"}
            }));
            let cancellation = json_line(&serde_json::json!({
                "jsonrpc": "2.0",
                "method": "notifications/cancelled",
                "params": {"requestId": id.clone(), "reason": "caller stopped"}
            }));
            let later = json_line(&serde_json::json!({
                "jsonrpc": "2.0",
                "id": 8,
                "method": "ping"
            }));
            let (mut input, reader) = duplex(4096);
            let writer = RecordingWriter::default();
            let written = Arc::clone(&writer.0);
            let (mut transport, status) =
                BoundedTransport::new(reader, writer, McpTransportLimits::default());

            input.write_all(&request).await.unwrap();
            let Some(ClientJsonRpcMessage::Request(active)) = transport.receive().await else {
                panic!("expected active request");
            };
            input.write_all(&cancellation).await.unwrap();
            input.write_all(&later).await.unwrap();
            let Some(ClientJsonRpcMessage::Notification(notification)) = transport.receive().await
            else {
                panic!("expected matching cancellation");
            };
            let ClientNotification::CancelledNotification(cancelled) = notification.notification
            else {
                panic!("expected cancellation notification");
            };
            assert_eq!(cancelled.params.request_id.as_ref(), Some(&active.id));
            assert_eq!(cancelled.params.reason, None);
            assert!(transport.receive().await.is_none());

            let output = if error_response {
                serde_json::json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "error": {"code": -32603, "message": "suppressed"}
                })
            } else {
                serde_json::json!({"jsonrpc": "2.0", "id": id, "result": {}})
            };
            let response: ServerJsonRpcMessage = serde_json::from_value(output).unwrap();
            assert_eq!(transport.send(response).await, Ok(()));
            assert!(written.lock().unwrap().is_empty());
            assert_eq!(status.failure(), None);
        }
    }

    #[tokio::test]
    async fn nonmatching_cancellation_waits_for_flush_and_is_not_terminal() {
        for params in [serde_json::json!({}), serde_json::json!({"requestId": 2})] {
            let (mut input, reader) = duplex(4096);
            let writer = RecordingWriter::default();
            let (mut transport, status) =
                BoundedTransport::new(reader, writer, McpTransportLimits::default());
            input
                .write_all(&json_line(&serde_json::json!({
                    "jsonrpc": "2.0",
                    "id": 1,
                    "method": "ping"
                })))
                .await
                .unwrap();
            assert!(matches!(
                transport.receive().await,
                Some(ClientJsonRpcMessage::Request(_))
            ));
            input
                .write_all(&json_line(&serde_json::json!({
                    "jsonrpc": "2.0",
                    "method": "notifications/cancelled",
                    "params": params
                })))
                .await
                .unwrap();

            let response: ServerJsonRpcMessage = serde_json::from_value(serde_json::json!({
                "jsonrpc": "2.0",
                "id": 1,
                "result": {}
            }))
            .unwrap();
            let send = transport.send(response);
            let delivered = {
                let mut cancellation = std::pin::pin!(transport.receive());
                let mut context = Context::from_waker(Waker::noop());
                assert!(matches!(
                    cancellation.as_mut().poll(&mut context),
                    Poll::Pending
                ));
                assert_eq!(send.await, Ok(()));
                cancellation.await
            };
            assert!(matches!(
                delivered,
                Some(ClientJsonRpcMessage::Notification(_))
            ));

            input
                .write_all(&json_line(&serde_json::json!({
                    "jsonrpc": "2.0",
                    "id": 3,
                    "method": "ping"
                })))
                .await
                .unwrap();
            assert!(matches!(
                transport.receive().await,
                Some(ClientJsonRpcMessage::Request(_))
            ));
            assert_eq!(status.failure(), None);
        }
    }

    #[tokio::test]
    async fn cancellation_after_response_write_begins_is_late() {
        let (mut input, reader) = duplex(4096);
        let (writer, mut output) = duplex(1);
        let (mut transport, status) =
            BoundedTransport::new(reader, writer, McpTransportLimits::default());
        input
            .write_all(&json_line(&serde_json::json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "ping"
            })))
            .await
            .unwrap();
        assert!(matches!(
            transport.receive().await,
            Some(ClientJsonRpcMessage::Request(_))
        ));
        input
            .write_all(&json_line(&serde_json::json!({
                "jsonrpc": "2.0",
                "method": "notifications/cancelled",
                "params": {"requestId": 1}
            })))
            .await
            .unwrap();

        let response: ServerJsonRpcMessage = serde_json::from_value(serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "result": {"padding": "response is already being written"}
        }))
        .unwrap();
        let send = tokio::spawn(transport.send(response));
        let mut first = [0_u8; 1];
        output.read_exact(&mut first).await.unwrap();
        let delivered = {
            let mut cancellation = std::pin::pin!(transport.receive());
            let mut context = Context::from_waker(Waker::noop());
            assert!(matches!(
                cancellation.as_mut().poll(&mut context),
                Poll::Pending
            ));
            let drain = tokio::spawn(async move {
                let mut output = BufReader::new(output);
                let mut remainder = Vec::new();
                output.read_until(b'\n', &mut remainder).await.unwrap();
                remainder
            });
            assert_eq!(send.await.unwrap(), Ok(()));
            assert!(!drain.await.unwrap().is_empty());
            cancellation.await
        };
        assert!(matches!(
            delivered,
            Some(ClientJsonRpcMessage::Notification(_))
        ));

        input
            .write_all(&json_line(&serde_json::json!({
                "jsonrpc": "2.0",
                "id": 2,
                "method": "ping"
            })))
            .await
            .unwrap();
        assert!(matches!(
            transport.receive().await,
            Some(ClientJsonRpcMessage::Request(_))
        ));
        assert_eq!(status.failure(), None);
    }

    #[tokio::test]
    async fn pipeline_decodes_only_one_pending_request_before_each_flush() {
        let (mut input, reader) = duplex(4096);
        let (mut transport, status) = BoundedTransport::new(
            reader,
            RecordingWriter::default(),
            McpTransportLimits::default(),
        );
        for id in 1..=3 {
            input
                .write_all(&json_line(&serde_json::json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "method": "ping"
                })))
                .await
                .unwrap();
        }
        let Some(ClientJsonRpcMessage::Request(first)) = transport.receive().await else {
            panic!("expected first request");
        };
        assert_eq!(first.id, RequestId::Number(1));

        let response = |id| {
            serde_json::from_value::<ServerJsonRpcMessage>(serde_json::json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": {}
            }))
            .unwrap()
        };
        let send = transport.send(response(1));
        let second = {
            let mut pending = std::pin::pin!(transport.receive());
            let mut context = Context::from_waker(Waker::noop());
            assert!(matches!(pending.as_mut().poll(&mut context), Poll::Pending));
            assert_eq!(send.await, Ok(()));
            pending.await
        };
        let Some(ClientJsonRpcMessage::Request(second)) = second else {
            panic!("expected second request");
        };
        assert_eq!(second.id, RequestId::Number(2));

        let send = transport.send(response(2));
        let third = {
            let mut pending = std::pin::pin!(transport.receive());
            let mut context = Context::from_waker(Waker::noop());
            assert!(matches!(pending.as_mut().poll(&mut context), Poll::Pending));
            assert_eq!(send.await, Ok(()));
            pending.await
        };
        let Some(ClientJsonRpcMessage::Request(third)) = third else {
            panic!("expected third request");
        };
        assert_eq!(third.id, RequestId::Number(3));
        assert_eq!(status.failure(), None);
    }

    #[tokio::test]
    async fn pending_input_survives_receive_future_cancellation() {
        let (mut input, reader) = duplex(4096);
        let (mut transport, status) = BoundedTransport::new(
            reader,
            RecordingWriter::default(),
            McpTransportLimits::default(),
        );
        for id in 1..=2 {
            input
                .write_all(&json_line(&serde_json::json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "method": "ping"
                })))
                .await
                .unwrap();
        }
        assert!(matches!(
            transport.receive().await,
            Some(ClientJsonRpcMessage::Request(_))
        ));
        {
            let mut pending = std::pin::pin!(transport.receive());
            let mut context = Context::from_waker(Waker::noop());
            assert!(matches!(pending.as_mut().poll(&mut context), Poll::Pending));
        }

        let response: ServerJsonRpcMessage = serde_json::from_value(serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "result": {}
        }))
        .unwrap();
        assert_eq!(transport.send(response).await, Ok(()));
        let Some(ClientJsonRpcMessage::Request(second)) = transport.receive().await else {
            panic!("cancel-safe receive must retain the decoded pending request");
        };
        assert_eq!(second.id, RequestId::Number(2));
        assert_eq!(status.failure(), None);
    }

    #[tokio::test]
    async fn partial_line_survives_receive_future_cancellation() {
        let (mut input, reader) = duplex(4096);
        let (mut transport, status) = BoundedTransport::new(
            reader,
            RecordingWriter::default(),
            McpTransportLimits::default(),
        );
        input
            .write_all(&json_line(&serde_json::json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "ping"
            })))
            .await
            .unwrap();
        assert!(matches!(
            transport.receive().await,
            Some(ClientJsonRpcMessage::Request(_))
        ));
        let second = json_line(&serde_json::json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "ping"
        }));
        let midpoint = second.len() / 2;
        input.write_all(&second[..midpoint]).await.unwrap();
        {
            let mut partial = std::pin::pin!(transport.receive());
            let mut context = Context::from_waker(Waker::noop());
            assert!(matches!(partial.as_mut().poll(&mut context), Poll::Pending));
        }
        input.write_all(&second[midpoint..]).await.unwrap();

        let response: ServerJsonRpcMessage = serde_json::from_value(serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "result": {}
        }))
        .unwrap();
        let send = transport.send(response);
        let received = {
            let mut pending = std::pin::pin!(transport.receive());
            let mut context = Context::from_waker(Waker::noop());
            assert!(matches!(pending.as_mut().poll(&mut context), Poll::Pending));
            assert_eq!(send.await, Ok(()));
            pending.await
        };
        let Some(ClientJsonRpcMessage::Request(second)) = received else {
            panic!("cancel-safe receive must retain the partial line");
        };
        assert_eq!(second.id, RequestId::Number(2));
        assert_eq!(status.failure(), None);
    }

    #[tokio::test]
    async fn pipelined_resource_read_waits_for_the_prior_response_flush() {
        let uri = "cogniform://observations/00000000000000000000000000000001";
        let request = |id| {
            let mut encoded = serde_json::to_vec(&serde_json::json!({
                "jsonrpc": "2.0",
                "id": id,
                "method": "resources/read",
                "params": {"uri": uri}
            }))
            .unwrap();
            encoded.push(b'\n');
            encoded
        };
        let (mut input_client, input_server) = duplex(4096);
        let (output_server, output_client) = duplex(1);
        let (mut transport, _) =
            BoundedTransport::new(input_server, output_server, McpTransportLimits::default());
        input_client.write_all(&request(1)).await.unwrap();
        input_client.write_all(&request(2)).await.unwrap();
        assert!(matches!(
            transport.receive().await,
            Some(ClientJsonRpcMessage::Request(_))
        ));

        let response: ServerJsonRpcMessage = serde_json::from_value(serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "result": {
                "contents": [{
                    "uri": uri,
                    "mimeType": "application/vnd.cogniform.observation-envelope",
                    "blob": "AAAA"
                }]
            }
        }))
        .unwrap();
        let send = tokio::spawn(transport.send(response));
        let mut second = std::pin::pin!(transport.receive());
        let mut context = Context::from_waker(Waker::noop());
        assert!(matches!(second.as_mut().poll(&mut context), Poll::Pending));

        let drain = tokio::spawn(async move {
            let mut output = BufReader::new(output_client);
            let mut line = Vec::new();
            output.read_until(b'\n', &mut line).await.unwrap();
            line
        });
        assert_eq!(send.await.unwrap(), Ok(()));
        assert!(matches!(
            second.await,
            Some(ClientJsonRpcMessage::Request(_))
        ));
        assert!(!drain.await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn input_size_accepts_equality_and_rejects_one_less() {
        let frame = b"{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"ping\"}\n";
        let (mut input, reader) = duplex(frame.len() * 2);
        input.write_all(frame).await.unwrap();
        input.shutdown().await.unwrap();
        let limits = McpTransportLimits {
            max_input_bytes: NonZeroU64::new(frame.len() as u64).unwrap(),
            ..McpTransportLimits::default()
        };
        let (mut transport, status) = BoundedTransport::new(reader, sink(), limits);
        assert!(Transport::receive(&mut transport).await.is_some());
        assert_eq!(status.failure(), None);

        let (mut input, reader) = duplex(frame.len() * 2);
        input.write_all(frame).await.unwrap();
        input.shutdown().await.unwrap();
        let limits = McpTransportLimits {
            max_input_bytes: NonZeroU64::new(frame.len() as u64 - 1).unwrap(),
            ..McpTransportLimits::default()
        };
        let (mut transport, status) = BoundedTransport::new(reader, sink(), limits);
        assert!(Transport::receive(&mut transport).await.is_none());
        assert_eq!(
            status.failure(),
            Some(TransportFailureKind::InputSizeExceeded)
        );
    }

    #[tokio::test]
    async fn input_nesting_rejects_before_json_rpc_decode() {
        let frame =
            b"{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"ping\",\"params\":{\"nested\":[[]]}}\n";
        let (mut input, reader) = duplex(frame.len() * 2);
        input.write_all(frame).await.unwrap();
        input.shutdown().await.unwrap();
        let limits = McpTransportLimits {
            max_json_nesting_depth: NonZeroU16::new(3).unwrap(),
            ..McpTransportLimits::default()
        };
        let (mut transport, status) = BoundedTransport::new(reader, sink(), limits);
        assert!(Transport::receive(&mut transport).await.is_none());
        assert_eq!(
            status.failure(),
            Some(TransportFailureKind::NestingExceeded)
        );
    }

    #[tokio::test]
    async fn malformed_and_truncated_inputs_have_redacted_categories() {
        let (mut input, reader) = duplex(64);
        input.write_all(b"not-json\n").await.unwrap();
        let (mut transport, status) =
            BoundedTransport::new(reader, sink(), McpTransportLimits::default());
        assert!(Transport::receive(&mut transport).await.is_none());
        assert_eq!(status.failure(), Some(TransportFailureKind::InvalidMessage));

        let (mut input, reader) = duplex(64);
        input.write_all(b"{\"jsonrpc\":").await.unwrap();
        input.shutdown().await.unwrap();
        let (mut transport, status) =
            BoundedTransport::new(reader, sink(), McpTransportLimits::default());
        assert!(Transport::receive(&mut transport).await.is_none());
        assert_eq!(
            status.failure(),
            Some(TransportFailureKind::TruncatedMessage)
        );
    }

    fn json_line(value: &serde_json::Value) -> Vec<u8> {
        let mut encoded = serde_json::to_vec(&value).unwrap();
        encoded.push(b'\n');
        encoded
    }
}
