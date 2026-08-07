//! Exact framing, stream behavior, bounds, corruption, and redaction coverage.

use core::num::{NonZeroU32, NonZeroU64};
use std::{
    fmt::Write as _,
    io::{self, Cursor, Read, Write},
};

use cogniform_local_transport::{
    LOCAL_FRAME_HEADER_BYTES, LOCAL_FRAME_VERSION, LocalFrame, LocalFrameConfig, LocalFrameError,
    LocalFrameIoOperation, LocalFrameLimits, LocalFrameSection, decode_frame, encode_frame,
    read_frame, write_frame,
};
use cogniform_observation::{ObservationEnvelopeError, ObservationPayload};
use cogniform_protocol::{
    FrameId, ImageDimensions, ObservationId, ObservationKind, ObservationMetadata,
    ObservationQuality, ObservationStaleness, SceneRevision, SchemaVersion, StableEntityId,
};
use sha2::{Digest, Sha256};

const PREFIX_BYTES: usize = 36;
const DIGEST_OFFSET: usize = 36;
const CONTROL_LENGTH_OFFSET: usize = 20;
const BULK_LENGTH_OFFSET: usize = 28;

#[test]
fn control_fixture_and_header_are_exact() {
    let frame = control(7, br#"{"schema_version":1}"#);
    let encoded = encode_frame(&frame, &LocalFrameConfig::default()).unwrap();

    assert_eq!(&encoded[..8], b"COGLOC01");
    assert_eq!(
        u16::from_be_bytes(copy_array(&encoded[8..10])),
        LOCAL_FRAME_VERSION
    );
    assert_eq!(encoded[10], 1);
    assert_eq!(encoded[11], 0);
    assert_eq!(u64::from_be_bytes(copy_array(&encoded[12..20])), 7);
    assert_eq!(u64::from_be_bytes(copy_array(&encoded[20..28])), 20);
    assert_eq!(u64::from_be_bytes(copy_array(&encoded[28..36])), 0);
    assert_eq!(
        decode_frame(&encoded, &LocalFrameConfig::default()).unwrap(),
        frame
    );
    assert_eq!(
        hexadecimal(&encoded),
        "434f474c4f43303100010100000000000000000700000000000000140000000000000000e9d32ddc8983e2a350db88be2756a362142d8510f4bcb29abe9753a61a4befdd7b22736368656d615f76657273696f6e223a317d"
    );
}

#[test]
fn control_and_observation_frames_round_trip() {
    let cases = [control(1, b"one"), observation(2)];
    for frame in cases {
        let encoded = encode_frame(&frame, &LocalFrameConfig::default()).unwrap();
        assert_eq!(
            encoded[10],
            match frame {
                LocalFrame::Control { .. } => 1,
                LocalFrame::Observation { .. } => 2,
            }
        );
        assert_eq!(
            decode_frame(&encoded, &LocalFrameConfig::default()).unwrap(),
            frame
        );
    }
}

#[test]
fn short_and_interrupted_streams_preserve_back_to_back_frames() {
    let first = control(1, b"first");
    let second = observation(2);
    let mut writer = ChunkedWriter::new(3, true);
    write_frame(&mut writer, &first, &LocalFrameConfig::default()).unwrap();
    write_frame(&mut writer, &second, &LocalFrameConfig::default()).unwrap();

    let mut reader = ChunkedReader::new(writer.bytes, 2, true);
    assert_eq!(
        read_frame(&mut reader, &LocalFrameConfig::default()).unwrap(),
        Some(first)
    );
    assert_eq!(
        read_frame(&mut reader, &LocalFrameConfig::default()).unwrap(),
        Some(second)
    );
    assert_eq!(
        read_frame(&mut reader, &LocalFrameConfig::default()).unwrap(),
        None
    );
}

#[test]
fn clean_end_of_stream_is_distinct_from_truncation_and_trailing_bytes() {
    assert_eq!(
        read_frame(&mut Cursor::new(Vec::new()), &LocalFrameConfig::default()).unwrap(),
        None
    );

    let encoded = encode_frame(&control(1, b"body"), &LocalFrameConfig::default()).unwrap();
    for length in 1..encoded.len() {
        assert!(decode_frame(&encoded[..length], &LocalFrameConfig::default()).is_err());
    }

    let mut short_header = Cursor::new(encoded[..LOCAL_FRAME_HEADER_BYTES - 1].to_vec());
    assert!(matches!(
        read_frame(&mut short_header, &LocalFrameConfig::default()),
        Err(LocalFrameError::Truncated {
            section: LocalFrameSection::Header,
            ..
        })
    ));

    let mut short_control = Cursor::new(encoded[..encoded.len() - 1].to_vec());
    assert_eq!(
        read_frame(&mut short_control, &LocalFrameConfig::default()).unwrap_err(),
        LocalFrameError::Truncated {
            section: LocalFrameSection::Control,
            expected: 4,
            actual: 3,
        }
    );

    let observation = encode_frame(&observation(1), &LocalFrameConfig::default()).unwrap();
    let mut short_bulk = Cursor::new(observation[..observation.len() - 1].to_vec());
    assert!(matches!(
        read_frame(&mut short_bulk, &LocalFrameConfig::default()),
        Err(LocalFrameError::Truncated {
            section: LocalFrameSection::Bulk,
            ..
        })
    ));

    let mut trailing = encoded;
    trailing.push(0);
    assert!(matches!(
        decode_frame(&trailing, &LocalFrameConfig::default()),
        Err(LocalFrameError::TrailingBytes { .. })
    ));
}

#[test]
fn malformed_header_values_fail_before_body_semantics() {
    let encoded = encode_frame(&control(1, b"body"), &LocalFrameConfig::default()).unwrap();

    let mut invalid_magic = encoded.clone();
    invalid_magic[0] = b'X';
    assert_eq!(
        decode_frame(&invalid_magic, &LocalFrameConfig::default()).unwrap_err(),
        LocalFrameError::InvalidMagic
    );

    let mut version = encoded.clone();
    version[8..10].copy_from_slice(&2_u16.to_be_bytes());
    assert_eq!(
        decode_frame(&version, &LocalFrameConfig::default()).unwrap_err(),
        LocalFrameError::UnsupportedVersion { found: 2 }
    );

    let mut kind = encoded.clone();
    kind[10] = 99;
    assert_eq!(
        decode_frame(&kind, &LocalFrameConfig::default()).unwrap_err(),
        LocalFrameError::UnsupportedKind { found: 99 }
    );

    let mut reserved = encoded.clone();
    reserved[11] = 1;
    assert_eq!(
        decode_frame(&reserved, &LocalFrameConfig::default()).unwrap_err(),
        LocalFrameError::NonCanonicalHeader
    );

    let mut correlation = encoded.clone();
    correlation[12..20].fill(0);
    assert_eq!(
        decode_frame(&correlation, &LocalFrameConfig::default()).unwrap_err(),
        LocalFrameError::InvalidCorrelationId
    );

    let mut empty_control = encoded.clone();
    empty_control[CONTROL_LENGTH_OFFSET..BULK_LENGTH_OFFSET].fill(0);
    assert_eq!(
        decode_frame(
            &empty_control[..LOCAL_FRAME_HEADER_BYTES],
            &LocalFrameConfig::default()
        )
        .unwrap_err(),
        LocalFrameError::EmptyControl
    );

    let mut unexpected_bulk = encoded;
    unexpected_bulk[BULK_LENGTH_OFFSET..DIGEST_OFFSET].copy_from_slice(&1_u64.to_be_bytes());
    assert_eq!(
        decode_frame(&unexpected_bulk, &LocalFrameConfig::default()).unwrap_err(),
        LocalFrameError::InvalidSectionLayout
    );
}

#[test]
fn declared_limits_reject_before_a_reader_touches_the_body() {
    let config = LocalFrameConfig::default();
    let mut header =
        encode_frame(&control(1, b"x"), &config).unwrap()[..LOCAL_FRAME_HEADER_BYTES].to_vec();
    header[CONTROL_LENGTH_OFFSET..BULK_LENGTH_OFFSET]
        .copy_from_slice(&(config.frame_limits.max_control_bytes.get() + 1).to_be_bytes());
    let mut reader = HeaderOnlyReader::new(header);
    assert!(matches!(
        read_frame(&mut reader, &config),
        Err(LocalFrameError::ControlLimitExceeded { .. })
    ));

    let mut observation_header =
        encode_frame(&observation(1), &config).unwrap()[..LOCAL_FRAME_HEADER_BYTES].to_vec();
    observation_header[BULK_LENGTH_OFFSET..DIGEST_OFFSET]
        .copy_from_slice(&(config.frame_limits.max_bulk_bytes.get() + 1).to_be_bytes());
    let mut reader = HeaderOnlyReader::new(observation_header);
    assert!(matches!(
        read_frame(&mut reader, &config),
        Err(LocalFrameError::BulkLimitExceeded { .. })
    ));

    let tiny_frame = LocalFrameConfig::new(
        LocalFrameLimits::new(
            NonZeroU64::new(u64::try_from(LOCAL_FRAME_HEADER_BYTES + 1).unwrap()).unwrap(),
            NonZeroU64::new(16).unwrap(),
            NonZeroU64::new(16).unwrap(),
        ),
        config.runtime_limits,
        config.payload_limits,
    );
    assert!(matches!(
        encode_frame(&control(1, b"xx"), &tiny_frame),
        Err(LocalFrameError::FrameLimitExceeded { .. })
    ));
    let mut reader = HeaderOnlyReader::new(
        encode_frame(&control(1, b"xx"), &config).unwrap()[..LOCAL_FRAME_HEADER_BYTES].to_vec(),
    );
    assert!(matches!(
        read_frame(&mut reader, &tiny_frame),
        Err(LocalFrameError::FrameLimitExceeded { .. })
    ));

    let tiny_bulk = LocalFrameConfig::new(
        LocalFrameLimits::new(
            NonZeroU64::new(2048).unwrap(),
            NonZeroU64::new(1024).unwrap(),
            NonZeroU64::new(1).unwrap(),
        ),
        config.runtime_limits,
        config.payload_limits,
    );
    assert!(matches!(
        encode_frame(&observation(1), &tiny_bulk),
        Err(LocalFrameError::BulkLimitExceeded { .. })
    ));

    let maximum = NonZeroU64::new(u64::MAX).unwrap();
    let unbounded_lengths = LocalFrameConfig::new(
        LocalFrameLimits::new(maximum, maximum, maximum),
        config.runtime_limits,
        config.payload_limits,
    );
    let mut overflow =
        encode_frame(&control(1, b"x"), &config).unwrap()[..LOCAL_FRAME_HEADER_BYTES].to_vec();
    overflow[CONTROL_LENGTH_OFFSET..BULK_LENGTH_OFFSET].copy_from_slice(&u64::MAX.to_be_bytes());
    assert_eq!(
        decode_frame(&overflow, &unbounded_lengths).unwrap_err(),
        LocalFrameError::SizeOverflow
    );
}

#[test]
fn outer_and_inner_integrity_bind_every_byte_and_causal_metadata() {
    let config = LocalFrameConfig::default();
    let frame = observation(1);
    let encoded = encode_frame(&frame, &config).unwrap();
    for index in 0..encoded.len() {
        let mut corrupted = encoded.clone();
        corrupted[index] ^= 1;
        assert!(
            decode_frame(&corrupted, &config).is_err(),
            "byte {index} was not protected"
        );
    }

    let control_bytes = usize::try_from(u64::from_be_bytes(copy_array(
        &encoded[CONTROL_LENGTH_OFFSET..BULK_LENGTH_OFFSET],
    )))
    .unwrap();
    let original_control =
        &encoded[LOCAL_FRAME_HEADER_BYTES..LOCAL_FRAME_HEADER_BYTES + control_bytes];
    let original_bulk = &encoded[LOCAL_FRAME_HEADER_BYTES + control_bytes..];

    let mut other_metadata = metadata();
    other_metadata.observation_id = ObservationId::new(2).unwrap();
    let other_control = other_metadata
        .to_canonical_json(&config.runtime_limits)
        .unwrap();
    let substituted = raw_frame(2, 1, &other_control, original_bulk);
    assert_eq!(
        decode_frame(&substituted, &config).unwrap_err(),
        LocalFrameError::ObservationEnvelope(ObservationEnvelopeError::IntegrityMismatch)
    );

    let mut noncanonical = original_control.to_vec();
    noncanonical.insert(1, b' ');
    let noncanonical = raw_frame(2, 1, &noncanonical, original_bulk);
    assert_eq!(
        decode_frame(&noncanonical, &config).unwrap_err(),
        LocalFrameError::InvalidObservationMetadata
    );

    let mut invalid_inner = original_bulk.to_vec();
    *invalid_inner.last_mut().unwrap() ^= 1;
    let invalid_inner = raw_frame(2, 1, original_control, &invalid_inner);
    assert_eq!(
        decode_frame(&invalid_inner, &config).unwrap_err(),
        LocalFrameError::ObservationEnvelope(ObservationEnvelopeError::IntegrityMismatch)
    );
}

#[test]
fn writer_failure_and_debug_output_are_payload_redacted() {
    let secret = b"do-not-print-this-control-value";
    let frame = control(1, secret);
    let debug = format!("{frame:?}");
    assert!(!debug.contains("do-not-print"));
    assert!(debug.contains("control_bytes"));

    let error = write_frame(&mut ZeroWriter, &frame, &LocalFrameConfig::default()).unwrap_err();
    assert_eq!(
        error,
        LocalFrameError::Io {
            operation: LocalFrameIoOperation::WriteFrame,
            kind: io::ErrorKind::WriteZero,
        }
    );
    assert!(!format!("{error:?}").contains("do-not-print"));

    let observation_debug = format!("{:?}", observation(1));
    assert!(!observation_debug.contains("[1, 2, 3, 4]"));
    assert!(observation_debug.contains("payload_items"));
}

#[test]
fn validation_precedes_writes_and_io_failures_keep_stable_operation_kinds() {
    let invalid = control(1, b"");
    let mut untouched = ChunkedWriter::new(8, false);
    assert_eq!(
        write_frame(&mut untouched, &invalid, &LocalFrameConfig::default()).unwrap_err(),
        LocalFrameError::EmptyControl
    );
    assert!(untouched.bytes.is_empty());

    let mut header_error = ErrorReader(io::ErrorKind::PermissionDenied);
    assert_eq!(
        read_frame(&mut header_error, &LocalFrameConfig::default()).unwrap_err(),
        LocalFrameError::Io {
            operation: LocalFrameIoOperation::ReadHeader,
            kind: io::ErrorKind::PermissionDenied,
        }
    );

    let encoded = encode_frame(&control(1, b"body"), &LocalFrameConfig::default()).unwrap();
    let mut body_error = PrefixThenError::new(
        encoded[..LOCAL_FRAME_HEADER_BYTES].to_vec(),
        io::ErrorKind::BrokenPipe,
    );
    assert_eq!(
        read_frame(&mut body_error, &LocalFrameConfig::default()).unwrap_err(),
        LocalFrameError::Io {
            operation: LocalFrameIoOperation::ReadControl,
            kind: io::ErrorKind::BrokenPipe,
        }
    );

    let observation = encode_frame(&observation(1), &LocalFrameConfig::default()).unwrap();
    let control_bytes = usize::try_from(u64::from_be_bytes(
        observation[CONTROL_LENGTH_OFFSET..BULK_LENGTH_OFFSET]
            .try_into()
            .unwrap(),
    ))
    .unwrap();
    let mut bulk_error = PrefixThenError::new(
        observation[..LOCAL_FRAME_HEADER_BYTES + control_bytes].to_vec(),
        io::ErrorKind::ConnectionReset,
    );
    assert_eq!(
        read_frame(&mut bulk_error, &LocalFrameConfig::default()).unwrap_err(),
        LocalFrameError::Io {
            operation: LocalFrameIoOperation::ReadBulk,
            kind: io::ErrorKind::ConnectionReset,
        }
    );
}

fn control(correlation_id: u64, bytes: &[u8]) -> LocalFrame {
    LocalFrame::Control {
        correlation_id: NonZeroU64::new(correlation_id).unwrap(),
        bytes: bytes.to_vec(),
    }
}

fn observation(correlation_id: u64) -> LocalFrame {
    LocalFrame::Observation {
        correlation_id: NonZeroU64::new(correlation_id).unwrap(),
        metadata: metadata(),
        payload: ObservationPayload::Color(vec![[1, 2, 3, 4], [5, 6, 7, 8]]),
    }
}

fn metadata() -> ObservationMetadata {
    ObservationMetadata {
        schema_version: SchemaVersion::V1,
        observation_id: ObservationId::new(1).unwrap(),
        scene_revision: SceneRevision::new(3),
        frame_id: FrameId::new(4).unwrap(),
        camera_id: StableEntityId::new(5).unwrap(),
        kind: ObservationKind::Color,
        dimensions: Some(ImageDimensions {
            width: NonZeroU32::new(2).unwrap(),
            height: NonZeroU32::new(1).unwrap(),
        }),
        quality: ObservationQuality::Low,
        observed_at_unix_micros: 6,
        production_latency_micros: 7,
        staleness: ObservationStaleness {
            latest_known_revision: SceneRevision::new(3),
            revisions_behind: 0,
        },
    }
}

fn raw_frame(kind: u8, correlation_id: u64, control: &[u8], bulk: &[u8]) -> Vec<u8> {
    let mut encoded = Vec::new();
    encoded.extend_from_slice(b"COGLOC01");
    encoded.extend_from_slice(&1_u16.to_be_bytes());
    encoded.push(kind);
    encoded.push(0);
    encoded.extend_from_slice(&correlation_id.to_be_bytes());
    encoded.extend_from_slice(&u64::try_from(control.len()).unwrap().to_be_bytes());
    encoded.extend_from_slice(&u64::try_from(bulk.len()).unwrap().to_be_bytes());
    encoded.extend_from_slice(&[0_u8; 32]);
    encoded.extend_from_slice(control);
    encoded.extend_from_slice(bulk);
    update_digest(&mut encoded);
    encoded
}

fn update_digest(encoded: &mut [u8]) {
    let control_bytes = usize::try_from(u64::from_be_bytes(copy_array(
        &encoded[CONTROL_LENGTH_OFFSET..BULK_LENGTH_OFFSET],
    )))
    .unwrap();
    let control_end = LOCAL_FRAME_HEADER_BYTES + control_bytes;
    let mut hasher = Sha256::new();
    hasher.update(&encoded[..PREFIX_BYTES]);
    hasher.update(&encoded[LOCAL_FRAME_HEADER_BYTES..control_end]);
    hasher.update(&encoded[control_end..]);
    encoded[DIGEST_OFFSET..LOCAL_FRAME_HEADER_BYTES].copy_from_slice(&hasher.finalize());
}

fn hexadecimal(bytes: &[u8]) -> String {
    let mut encoded = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        write!(&mut encoded, "{byte:02x}").unwrap();
    }
    encoded
}

fn copy_array<const N: usize>(encoded: &[u8]) -> [u8; N] {
    let mut value = [0_u8; N];
    value.copy_from_slice(encoded);
    value
}

struct ChunkedReader {
    inner: Cursor<Vec<u8>>,
    maximum: usize,
    interrupt_next: bool,
}

impl ChunkedReader {
    fn new(bytes: Vec<u8>, maximum: usize, interrupt_next: bool) -> Self {
        Self {
            inner: Cursor::new(bytes),
            maximum,
            interrupt_next,
        }
    }
}

impl Read for ChunkedReader {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        if self.interrupt_next {
            self.interrupt_next = false;
            return Err(io::Error::from(io::ErrorKind::Interrupted));
        }
        self.interrupt_next = true;
        let length = buffer.len().min(self.maximum);
        self.inner.read(&mut buffer[..length])
    }
}

struct ChunkedWriter {
    bytes: Vec<u8>,
    maximum: usize,
    interrupt_next: bool,
}

impl ChunkedWriter {
    fn new(maximum: usize, interrupt_next: bool) -> Self {
        Self {
            bytes: Vec::new(),
            maximum,
            interrupt_next,
        }
    }
}

impl Write for ChunkedWriter {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        if self.interrupt_next {
            self.interrupt_next = false;
            return Err(io::Error::from(io::ErrorKind::Interrupted));
        }
        self.interrupt_next = true;
        let length = buffer.len().min(self.maximum);
        self.bytes.extend_from_slice(&buffer[..length]);
        Ok(length)
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

struct HeaderOnlyReader {
    inner: Cursor<Vec<u8>>,
}

impl HeaderOnlyReader {
    fn new(header: Vec<u8>) -> Self {
        Self {
            inner: Cursor::new(header),
        }
    }
}

impl Read for HeaderOnlyReader {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        assert_ne!(
            usize::try_from(self.inner.position()).unwrap(),
            self.inner.get_ref().len(),
            "body read occurred before declared limits rejected the header"
        );
        self.inner.read(buffer)
    }
}

struct ZeroWriter;

impl Write for ZeroWriter {
    fn write(&mut self, _buffer: &[u8]) -> io::Result<usize> {
        Ok(0)
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

struct ErrorReader(io::ErrorKind);

impl Read for ErrorReader {
    fn read(&mut self, _buffer: &mut [u8]) -> io::Result<usize> {
        Err(io::Error::from(self.0))
    }
}

struct PrefixThenError {
    inner: Cursor<Vec<u8>>,
    kind: io::ErrorKind,
}

impl PrefixThenError {
    fn new(prefix: Vec<u8>, kind: io::ErrorKind) -> Self {
        Self {
            inner: Cursor::new(prefix),
            kind,
        }
    }
}

impl Read for PrefixThenError {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        if usize::try_from(self.inner.position()).unwrap() == self.inner.get_ref().len() {
            return Err(io::Error::from(self.kind));
        }
        self.inner.read(buffer)
    }
}
