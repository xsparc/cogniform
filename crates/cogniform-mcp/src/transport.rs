use std::{
    io,
    num::{NonZeroU16, NonZeroU64},
    sync::{Arc, Mutex as StdMutex},
};

use rmcp::{
    service::{RoleServer, RxJsonRpcMessage, TxJsonRpcMessage},
    transport::Transport,
};
use tokio::{
    io::{AsyncBufReadExt as _, AsyncRead, AsyncWrite, AsyncWriteExt as _, BufReader},
    sync::{Mutex, Notify},
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
}

impl<R: AsyncRead, W> BoundedTransport<R, W> {
    pub(crate) fn new(reader: R, writer: W, limits: McpTransportLimits) -> (Self, TransportStatus) {
        let status = TransportStatus::default();
        (
            Self {
                reader: BufReader::new(reader),
                writer: Arc::new(Mutex::new(writer)),
                input: Vec::new(),
                limits,
                status: status.clone(),
            },
            status,
        )
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
        async move {
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
            })
        }
    }

    async fn receive(&mut self) -> Option<RxJsonRpcMessage<RoleServer>> {
        self.input.clear();
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
        serde_json::from_slice(&self.input).map_or_else(
            |_| {
                self.status.record(TransportFailureKind::InvalidMessage);
                None
            },
            Some,
        )
    }

    async fn close(&mut self) -> Result<(), Self::Error> {
        self.writer.lock().await.flush().await.map_err(|_| {
            self.status.record(TransportFailureKind::OutputFailed);
            McpTransportError(TransportFailureKind::OutputFailed)
        })
    }
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
    use cogniform_protocol::RuntimeLimits;
    use std::io::Write as _;
    use std::num::{NonZeroU16, NonZeroU64};
    use tokio::io::{duplex, sink};

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
}
