//! Fixed-profile binary session composition over inherited standard streams.

use std::{
    fmt,
    io::{self, IsTerminal, Read, Write},
    thread,
    time::{Duration, Instant},
};

use cogniform_engine::{LocalService, LocalServiceConfig};
use cogniform_local_executor::{
    LocalExecutorConfig, LocalExecutorPhase, LocalExecutorStatus, LocalSessionExecutor,
};
use cogniform_local_session::{
    LocalSessionServerKind, SessionFailureCode, decode_server_control_frame,
    decode_server_control_frame_with_limits,
};
use cogniform_local_transport::{LocalFrame, LocalFrameConfig, read_frame, write_frame};

use crate::profile::LocalProfile;

const POLL_INTERVAL: Duration = Duration::from_millis(2);
const OPERATION_TIMEOUT: Duration = Duration::from_secs(15);

/// Runs one binary half-duplex session over inherited standard input and output.
pub(crate) fn run(profile: LocalProfile) -> Result<(), ServeStdioError> {
    let stdin = io::stdin();
    let stdout = io::stdout();
    validate_standard_streams(stdin.is_terminal(), stdout.is_terminal())?;

    let mut reader = stdin.lock();
    let mut writer = stdout.lock();
    let mut clock = SystemClock;
    serve_streams_with_policy(
        &mut reader,
        &mut writer,
        &mut clock,
        SessionPolicy::FIXED,
        move || build_executor(profile),
    )
}

fn build_executor(profile: LocalProfile) -> Result<LocalSessionExecutor, ServeStdioError> {
    let (width, height) = profile.dimensions();
    let service = pollster::block_on(LocalService::new(LocalServiceConfig::new(width, height)))
        .map_err(|_| ServeStdioError::ServiceUnavailable)?;
    LocalSessionExecutor::new(service, LocalExecutorConfig::default())
        .map_err(|_| ServeStdioError::ExecutorFailed)
}

fn validate_standard_streams(
    stdin_is_terminal: bool,
    stdout_is_terminal: bool,
) -> Result<(), ServeStdioError> {
    if stdin_is_terminal || stdout_is_terminal {
        Err(ServeStdioError::InteractiveStandardStream)
    } else {
        Ok(())
    }
}

#[derive(Debug, Clone, Copy)]
struct SessionPolicy {
    poll_interval: Duration,
    operation_timeout: Duration,
}

impl SessionPolicy {
    const FIXED: Self = Self {
        poll_interval: POLL_INTERVAL,
        operation_timeout: OPERATION_TIMEOUT,
    };
}

fn serve_streams_with_policy<R, W, C, F, D>(
    reader: &mut R,
    writer: &mut W,
    clock: &mut C,
    policy: SessionPolicy,
    create_driver: F,
) -> Result<(), ServeStdioError>
where
    R: Read + ?Sized,
    W: Write + ?Sized,
    C: SessionClock,
    F: FnOnce() -> Result<D, ServeStdioError>,
    D: SessionDriver,
{
    let mut frame_config = LocalFrameConfig::default();
    let Some(first_frame) = read_input_frame(reader, &frame_config)? else {
        return Ok(());
    };

    let mut driver = create_driver()?;
    let mut next_frame = Some(first_frame);
    loop {
        let frame = next_frame
            .take()
            .expect("the session loop always owns one decoded input frame");
        let output = driver.handle_frame(&frame)?;
        adopt_negotiated_config(&driver, &mut frame_config)?;
        let fatal_service_failure = driver.output_requires_exit(&output, &frame_config)?;
        write_output_frames(writer, &output, &frame_config)?;
        if fatal_service_failure {
            return Err(ServeStdioError::ServiceFailed);
        }

        let mut status = driver.status();
        validate_status(status)?;
        if status.phase == LocalExecutorPhase::Closed {
            return closed_result(status);
        }
        if status.live_correlations != 0 {
            drive_to_terminal(&mut driver, writer, clock, policy, &mut frame_config)?;
            status = driver.status();
            validate_status(status)?;
            if status.phase == LocalExecutorPhase::Closed {
                return closed_result(status);
            }
        }

        next_frame = read_input_frame(reader, &frame_config)?;
        if next_frame.is_none() {
            return match driver.status().phase {
                LocalExecutorPhase::AwaitingHello => Err(ServeStdioError::SessionEndedBeforeHello),
                LocalExecutorPhase::Active => Err(ServeStdioError::SessionEndedBeforeClose),
                LocalExecutorPhase::Closed => closed_result(driver.status()),
            };
        }
    }
}

fn drive_to_terminal<D, W, C>(
    driver: &mut D,
    writer: &mut W,
    clock: &mut C,
    policy: SessionPolicy,
    frame_config: &mut LocalFrameConfig,
) -> Result<(), ServeStdioError>
where
    D: SessionDriver,
    W: Write + ?Sized,
    C: SessionClock,
{
    let deadline = clock
        .deadline_after(policy.operation_timeout)
        .ok_or(ServeStdioError::OperationTimedOut)?;
    loop {
        if clock.deadline_reached(deadline) {
            return Err(ServeStdioError::OperationTimedOut);
        }
        let output = driver.advance()?;
        if clock.deadline_reached(deadline) {
            return Err(ServeStdioError::OperationTimedOut);
        }
        adopt_negotiated_config(driver, frame_config)?;
        let fatal_service_failure = driver.output_requires_exit(&output, frame_config)?;
        write_output_frames(writer, &output, frame_config)?;
        if fatal_service_failure {
            return Err(ServeStdioError::ServiceFailed);
        }
        let status = driver.status();
        validate_status(status)?;
        if status.live_correlations == 0 {
            return Ok(());
        }
        let remaining = clock.remaining(deadline);
        if remaining.is_zero() {
            return Err(ServeStdioError::OperationTimedOut);
        }
        clock.sleep(policy.poll_interval.min(remaining));
    }
}

fn closed_result(status: LocalExecutorStatus) -> Result<(), ServeStdioError> {
    if status.live_correlations == 0
        && status.pending_patches == 0
        && status.pending_imaginations == 0
        && status.pending_observations == 0
    {
        Ok(())
    } else {
        Err(ServeStdioError::ExecutorFailed)
    }
}

fn validate_status(status: LocalExecutorStatus) -> Result<(), ServeStdioError> {
    let pending = status
        .pending_patches
        .checked_add(status.pending_imaginations)
        .ok_or(ServeStdioError::ExecutorFailed)?
        .checked_add(status.pending_observations)
        .ok_or(ServeStdioError::ExecutorFailed)?;
    if pending > status.live_correlations
        || (status.live_correlations != 0 && status.phase != LocalExecutorPhase::Active)
    {
        Err(ServeStdioError::ExecutorFailed)
    } else {
        Ok(())
    }
}

fn read_input_frame<R: Read + ?Sized>(
    reader: &mut R,
    frame_config: &LocalFrameConfig,
) -> Result<Option<LocalFrame>, ServeStdioError> {
    read_frame(reader, frame_config).map_err(|_| ServeStdioError::InputFrameRejected)
}

fn adopt_negotiated_config<D: SessionDriver>(
    driver: &D,
    frame_config: &mut LocalFrameConfig,
) -> Result<(), ServeStdioError> {
    if let Some(negotiated) = driver.negotiated_frame_config()? {
        *frame_config = negotiated;
    }
    Ok(())
}

fn write_output_frames<W: Write + ?Sized>(
    writer: &mut W,
    frames: &[LocalFrame],
    frame_config: &LocalFrameConfig,
) -> Result<(), ServeStdioError> {
    for frame in frames {
        write_frame(writer, frame, frame_config).map_err(|_| ServeStdioError::OutputFrameFailed)?;
        writer
            .flush()
            .map_err(|_| ServeStdioError::OutputFlushFailed)?;
    }
    Ok(())
}

trait SessionDriver {
    fn handle_frame(&mut self, frame: &LocalFrame) -> Result<Vec<LocalFrame>, ServeStdioError>;
    fn advance(&mut self) -> Result<Vec<LocalFrame>, ServeStdioError>;
    fn status(&self) -> LocalExecutorStatus;
    fn negotiated_frame_config(&self) -> Result<Option<LocalFrameConfig>, ServeStdioError>;
    fn output_requires_exit(
        &self,
        frames: &[LocalFrame],
        frame_config: &LocalFrameConfig,
    ) -> Result<bool, ServeStdioError>;
}

impl SessionDriver for LocalSessionExecutor {
    fn handle_frame(&mut self, frame: &LocalFrame) -> Result<Vec<LocalFrame>, ServeStdioError> {
        LocalSessionExecutor::handle_frame(self, frame).map_err(|_| ServeStdioError::ExecutorFailed)
    }

    fn advance(&mut self) -> Result<Vec<LocalFrame>, ServeStdioError> {
        LocalSessionExecutor::advance(self).map_err(|_| ServeStdioError::ExecutorFailed)
    }

    fn status(&self) -> LocalExecutorStatus {
        LocalSessionExecutor::status(self)
    }

    fn negotiated_frame_config(&self) -> Result<Option<LocalFrameConfig>, ServeStdioError> {
        self.negotiated_limits()
            .map(|limits| {
                limits
                    .to_frame_config()
                    .map_err(|_| ServeStdioError::ExecutorFailed)
            })
            .transpose()
    }

    fn output_requires_exit(
        &self,
        frames: &[LocalFrame],
        frame_config: &LocalFrameConfig,
    ) -> Result<bool, ServeStdioError> {
        if let Some(limits) = self.negotiated_compilation_limits() {
            output_contains_fatal_service_failure_with(frames, frame_config, |frame, config| {
                decode_server_control_frame_with_limits(frame, config, &limits)
                    .map(|(_, message)| message)
            })
        } else {
            output_contains_fatal_service_failure_with(frames, frame_config, |frame, config| {
                decode_server_control_frame(frame, config).map(|(_, message)| message)
            })
        }
    }
}

#[cfg(test)]
fn output_contains_fatal_service_failure(
    frames: &[LocalFrame],
    frame_config: &LocalFrameConfig,
) -> Result<bool, ServeStdioError> {
    output_contains_fatal_service_failure_with(frames, frame_config, |frame, config| {
        decode_server_control_frame(frame, config).map(|(_, message)| message)
    })
}

fn output_contains_fatal_service_failure_with<F>(
    frames: &[LocalFrame],
    frame_config: &LocalFrameConfig,
    mut decode: F,
) -> Result<bool, ServeStdioError>
where
    F: FnMut(
        &LocalFrame,
        &LocalFrameConfig,
    ) -> Result<
        cogniform_local_session::LocalSessionServerMessage,
        cogniform_local_session::LocalSessionError,
    >,
{
    for frame in frames {
        let LocalFrame::Control { .. } = frame else {
            continue;
        };
        let message = decode(frame, frame_config).map_err(|_| ServeStdioError::ExecutorFailed)?;
        if message_is_fatal_service_failure(&message) {
            return Ok(true);
        }
    }
    Ok(false)
}

fn message_is_fatal_service_failure(
    message: &cogniform_local_session::LocalSessionServerMessage,
) -> bool {
    matches!(
        &message.message,
        LocalSessionServerKind::Failure(failure)
            if matches!(
                failure.code,
                SessionFailureCode::ServiceUnavailable | SessionFailureCode::Internal
            )
    )
}

trait SessionClock {
    type Deadline: Copy;

    fn deadline_after(&self, duration: Duration) -> Option<Self::Deadline>;
    fn deadline_reached(&self, deadline: Self::Deadline) -> bool;
    fn remaining(&self, deadline: Self::Deadline) -> Duration;
    fn sleep(&mut self, duration: Duration);
}

struct SystemClock;

impl SessionClock for SystemClock {
    type Deadline = Instant;

    fn deadline_after(&self, duration: Duration) -> Option<Self::Deadline> {
        Instant::now().checked_add(duration)
    }

    fn deadline_reached(&self, deadline: Self::Deadline) -> bool {
        Instant::now() >= deadline
    }

    fn remaining(&self, deadline: Self::Deadline) -> Duration {
        deadline.saturating_duration_since(Instant::now())
    }

    fn sleep(&mut self, duration: Duration) {
        thread::sleep(duration);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ServeStdioError {
    InteractiveStandardStream,
    InputFrameRejected,
    ServiceUnavailable,
    ServiceFailed,
    ExecutorFailed,
    OutputFrameFailed,
    OutputFlushFailed,
    OperationTimedOut,
    SessionEndedBeforeHello,
    SessionEndedBeforeClose,
}

impl fmt::Display for ServeStdioError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InteractiveStandardStream => {
                "serve-stdio requires redirected standard input and output"
            }
            Self::InputFrameRejected => "serve-stdio input frame rejected",
            Self::ServiceUnavailable => "serve-stdio local service unavailable",
            Self::ServiceFailed => "serve-stdio local service failed",
            Self::ExecutorFailed => "serve-stdio session executor failed",
            Self::OutputFrameFailed => "serve-stdio output frame failed",
            Self::OutputFlushFailed => "serve-stdio output flush failed",
            Self::OperationTimedOut => "serve-stdio operation timed out",
            Self::SessionEndedBeforeHello => "serve-stdio session ended before hello",
            Self::SessionEndedBeforeClose => "serve-stdio session ended before close",
        })
    }
}

impl std::error::Error for ServeStdioError {}

#[cfg(test)]
mod tests {
    use std::{cell::RefCell, collections::VecDeque, io, num::NonZeroU64, rc::Rc};

    use cogniform_compilation::CompilationLimits;
    use cogniform_local_session::{
        LOCAL_SESSION_SCHEMA_VERSION, LOCAL_SESSION_SCHEMA_VERSION_V2, LocalSessionLimits,
        LocalSessionServerMessage, ServerHello, SessionFailure, server_control_frame,
        server_control_frame_with_limits,
    };
    use cogniform_local_transport::{LocalFrameLimits, encode_frame};

    use super::*;

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum Event {
        ReadFrame(usize),
        Handle,
        Advance,
        Write,
        Flush,
        Sleep(Duration),
    }

    struct DriverStep {
        output: Vec<LocalFrame>,
        phase: LocalExecutorPhase,
        live_correlations: u32,
        negotiated: Option<LocalFrameConfig>,
        error: Option<ServeStdioError>,
        fatal_after_output: bool,
    }

    impl DriverStep {
        fn success(
            output: Vec<LocalFrame>,
            phase: LocalExecutorPhase,
            live_correlations: u32,
        ) -> Self {
            Self {
                output,
                phase,
                live_correlations,
                negotiated: None,
                error: None,
                fatal_after_output: false,
            }
        }
    }

    struct FakeDriver {
        handles: VecDeque<DriverStep>,
        advances: VecDeque<DriverStep>,
        phase: LocalExecutorPhase,
        live_correlations: u32,
        negotiated: Option<LocalFrameConfig>,
        fatal_after_output: bool,
        events: Rc<RefCell<Vec<Event>>>,
    }

    impl FakeDriver {
        fn new(
            handles: Vec<DriverStep>,
            advances: Vec<DriverStep>,
            events: Rc<RefCell<Vec<Event>>>,
        ) -> Self {
            Self {
                handles: handles.into(),
                advances: advances.into(),
                phase: LocalExecutorPhase::AwaitingHello,
                live_correlations: 0,
                negotiated: None,
                fatal_after_output: false,
                events,
            }
        }

        fn apply(&mut self, step: DriverStep) -> Result<Vec<LocalFrame>, ServeStdioError> {
            self.phase = step.phase;
            self.live_correlations = step.live_correlations;
            if step.negotiated.is_some() {
                self.negotiated = step.negotiated;
            }
            self.fatal_after_output = step.fatal_after_output;
            if let Some(error) = step.error {
                Err(error)
            } else {
                Ok(step.output)
            }
        }
    }

    impl SessionDriver for FakeDriver {
        fn handle_frame(
            &mut self,
            _frame: &LocalFrame,
        ) -> Result<Vec<LocalFrame>, ServeStdioError> {
            self.events.borrow_mut().push(Event::Handle);
            let step = self.handles.pop_front().expect("scripted handle step");
            self.apply(step)
        }

        fn advance(&mut self) -> Result<Vec<LocalFrame>, ServeStdioError> {
            self.events.borrow_mut().push(Event::Advance);
            let step = self.advances.pop_front().unwrap_or_else(|| {
                DriverStep::success(Vec::new(), self.phase, self.live_correlations)
            });
            self.apply(step)
        }

        fn status(&self) -> LocalExecutorStatus {
            LocalExecutorStatus {
                phase: self.phase,
                live_correlations: self.live_correlations,
                live_correlation_capacity: 8,
                max_output_frames_per_call: 2,
                pending_patches: 0,
                pending_imaginations: 0,
                pending_observations: self.live_correlations,
            }
        }

        fn negotiated_frame_config(&self) -> Result<Option<LocalFrameConfig>, ServeStdioError> {
            Ok(self.negotiated.clone())
        }

        fn output_requires_exit(
            &self,
            _frames: &[LocalFrame],
            _frame_config: &LocalFrameConfig,
        ) -> Result<bool, ServeStdioError> {
            Ok(self.fatal_after_output)
        }
    }

    struct FrameReader {
        bytes: Vec<u8>,
        starts: Vec<usize>,
        ends: Vec<usize>,
        cursor: usize,
        fail_at_eof: bool,
        events: Rc<RefCell<Vec<Event>>>,
    }

    impl FrameReader {
        fn new(
            frames: &[LocalFrame],
            config: &LocalFrameConfig,
            events: Rc<RefCell<Vec<Event>>>,
        ) -> Self {
            let mut bytes = Vec::new();
            let mut starts = Vec::new();
            let mut ends = Vec::new();
            for frame in frames {
                starts.push(bytes.len());
                bytes.extend_from_slice(&encode_frame(frame, config).unwrap());
                ends.push(bytes.len());
            }
            Self {
                bytes,
                starts,
                ends,
                cursor: 0,
                fail_at_eof: false,
                events,
            }
        }

        fn with_eof_failure(mut self) -> Self {
            self.fail_at_eof = true;
            self
        }
    }

    impl Read for FrameReader {
        fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
            if self.cursor == self.bytes.len() {
                return if self.fail_at_eof {
                    Err(io::Error::new(io::ErrorKind::ConnectionReset, "test read"))
                } else {
                    Ok(0)
                };
            }
            let frame_index = self
                .ends
                .iter()
                .position(|end| self.cursor < *end)
                .expect("cursor remains inside scripted input");
            if self.cursor == self.starts[frame_index] {
                self.events
                    .borrow_mut()
                    .push(Event::ReadFrame(frame_index + 1));
            }
            let available = self.ends[frame_index] - self.cursor;
            let length = available.min(buffer.len());
            buffer[..length].copy_from_slice(&self.bytes[self.cursor..self.cursor + length]);
            self.cursor += length;
            Ok(length)
        }
    }

    struct RecordingWriter {
        bytes: Vec<u8>,
        max_chunk: usize,
        write_zero: bool,
        fail_flush: bool,
        fail_after_bytes: Option<usize>,
        events: Rc<RefCell<Vec<Event>>>,
    }

    impl RecordingWriter {
        fn new(events: Rc<RefCell<Vec<Event>>>) -> Self {
            Self {
                bytes: Vec::new(),
                max_chunk: usize::MAX,
                write_zero: false,
                fail_flush: false,
                fail_after_bytes: None,
                events,
            }
        }
    }

    impl Write for RecordingWriter {
        fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
            self.events.borrow_mut().push(Event::Write);
            if self.write_zero {
                return Ok(0);
            }
            if self
                .fail_after_bytes
                .is_some_and(|limit| self.bytes.len() >= limit)
            {
                return Err(io::Error::new(io::ErrorKind::BrokenPipe, "test write"));
            }
            let length = buffer.len().min(self.max_chunk);
            self.bytes.extend_from_slice(&buffer[..length]);
            Ok(length)
        }

        fn flush(&mut self) -> io::Result<()> {
            self.events.borrow_mut().push(Event::Flush);
            if self.fail_flush {
                Err(io::Error::new(io::ErrorKind::BrokenPipe, "test flush"))
            } else {
                Ok(())
            }
        }
    }

    struct FakeClock {
        now: Duration,
        events: Rc<RefCell<Vec<Event>>>,
    }

    impl FakeClock {
        fn new(events: Rc<RefCell<Vec<Event>>>) -> Self {
            Self {
                now: Duration::ZERO,
                events,
            }
        }
    }

    impl SessionClock for FakeClock {
        type Deadline = Duration;

        fn deadline_after(&self, duration: Duration) -> Option<Self::Deadline> {
            self.now.checked_add(duration)
        }

        fn deadline_reached(&self, deadline: Self::Deadline) -> bool {
            self.now >= deadline
        }

        fn remaining(&self, deadline: Self::Deadline) -> Duration {
            deadline.saturating_sub(self.now)
        }

        fn sleep(&mut self, duration: Duration) {
            self.events.borrow_mut().push(Event::Sleep(duration));
            self.now = self.now.saturating_add(duration);
        }
    }

    #[test]
    fn clean_prehello_eof_returns_without_creating_driver() {
        let events = Rc::new(RefCell::new(Vec::new()));
        let mut reader = io::empty();
        let mut writer = RecordingWriter::new(Rc::clone(&events));
        let mut clock = FakeClock::new(Rc::clone(&events));
        let created = Rc::new(RefCell::new(false));
        let created_for_factory = Rc::clone(&created);

        let result = serve_streams_with_policy(
            &mut reader,
            &mut writer,
            &mut clock,
            SessionPolicy::FIXED,
            move || {
                *created_for_factory.borrow_mut() = true;
                Ok(FakeDriver::new(Vec::new(), Vec::new(), Rc::clone(&events)))
            },
        );

        assert_eq!(result, Ok(()));
        assert!(!*created.borrow());
        assert!(writer.bytes.is_empty());
    }

    #[test]
    fn half_duplex_waits_for_terminal_flush_before_reading_next_request() {
        let events = Rc::new(RefCell::new(Vec::new()));
        let config = LocalFrameConfig::default();
        let input = [
            control(1, b"hello"),
            control(2, b"observe"),
            control(3, b"close"),
        ];
        let mut reader = FrameReader::new(&input, &config, Rc::clone(&events));
        let mut writer = RecordingWriter::new(Rc::clone(&events));
        writer.max_chunk = 7;
        let mut clock = FakeClock::new(Rc::clone(&events));
        let mut hello = DriverStep::success(
            vec![control(1, b"server-hello")],
            LocalExecutorPhase::Active,
            0,
        );
        hello.negotiated = Some(config.clone());
        let driver = FakeDriver::new(
            vec![
                hello,
                DriverStep::success(vec![control(2, b"accepted")], LocalExecutorPhase::Active, 1),
                DriverStep::success(vec![control(3, b"closed")], LocalExecutorPhase::Closed, 0),
            ],
            vec![
                DriverStep::success(vec![control(2, b"pending")], LocalExecutorPhase::Active, 1),
                DriverStep::success(
                    vec![control(2, b"observation")],
                    LocalExecutorPhase::Active,
                    0,
                ),
            ],
            Rc::clone(&events),
        );

        let result = serve_streams_with_policy(
            &mut reader,
            &mut writer,
            &mut clock,
            SessionPolicy::FIXED,
            || Ok(driver),
        );

        assert_eq!(result, Ok(()));
        let recorded = events.borrow();
        let third_read = position(&recorded, Event::ReadFrame(3));
        let terminal_advance = recorded
            .iter()
            .enumerate()
            .filter(|(_, event)| **event == Event::Advance)
            .nth(1)
            .unwrap()
            .0;
        let terminal_flush = recorded
            .iter()
            .enumerate()
            .find(|(index, event)| *index > terminal_advance && **event == Event::Flush)
            .unwrap()
            .0;
        assert!(third_read > terminal_flush);
        assert_eq!(
            recorded
                .iter()
                .filter(|event| **event == Event::Flush)
                .count(),
            5
        );
        assert!(recorded.contains(&Event::Sleep(POLL_INTERVAL)));

        let mut output = writer.bytes.as_slice();
        for expected_correlation in [1_u64, 2, 2, 2, 3] {
            let frame = read_frame(&mut output, &config).unwrap().unwrap();
            assert_eq!(frame.correlation_id().get(), expected_correlation);
        }
        assert!(read_frame(&mut output, &config).unwrap().is_none());
    }

    #[test]
    fn negotiated_policy_applies_before_server_hello_is_written() {
        let events = Rc::new(RefCell::new(Vec::new()));
        let config = LocalFrameConfig::default();
        let mut reader = FrameReader::new(&[control(1, b"hello")], &config, Rc::clone(&events));
        let mut writer = RecordingWriter::new(Rc::clone(&events));
        let mut clock = FakeClock::new(Rc::clone(&events));
        let mut narrow = config.clone();
        narrow.frame_limits = LocalFrameLimits::new(
            narrow.frame_limits.max_frame_bytes,
            NonZeroU64::new(1).unwrap(),
            narrow.frame_limits.max_bulk_bytes,
        );
        let mut hello =
            DriverStep::success(vec![control(1, b"larger")], LocalExecutorPhase::Active, 0);
        hello.negotiated = Some(narrow);
        let driver = FakeDriver::new(vec![hello], Vec::new(), Rc::clone(&events));

        let result = serve_streams_with_policy(
            &mut reader,
            &mut writer,
            &mut clock,
            SessionPolicy::FIXED,
            || Ok(driver),
        );

        assert_eq!(result, Err(ServeStdioError::OutputFrameFailed));
        assert!(writer.bytes.is_empty());
    }

    #[test]
    fn live_operation_timeout_sleeps_at_the_fixed_capped_interval() {
        let events = Rc::new(RefCell::new(Vec::new()));
        let config = LocalFrameConfig::default();
        let input = [control(1, b"hello"), control(2, b"observe")];
        let mut reader = FrameReader::new(&input, &config, Rc::clone(&events));
        let mut writer = RecordingWriter::new(Rc::clone(&events));
        let mut clock = FakeClock::new(Rc::clone(&events));
        let mut hello = DriverStep::success(
            vec![control(1, b"server-hello")],
            LocalExecutorPhase::Active,
            0,
        );
        hello.negotiated = Some(config);
        let driver = FakeDriver::new(
            vec![
                hello,
                DriverStep::success(vec![control(2, b"accepted")], LocalExecutorPhase::Active, 1),
            ],
            Vec::new(),
            Rc::clone(&events),
        );
        let policy = SessionPolicy {
            poll_interval: Duration::from_millis(2),
            operation_timeout: Duration::from_millis(5),
        };

        let result =
            serve_streams_with_policy(&mut reader, &mut writer, &mut clock, policy, || Ok(driver));

        assert_eq!(result, Err(ServeStdioError::OperationTimedOut));
        let sleeps = events
            .borrow()
            .iter()
            .filter(|event| matches!(event, Event::Sleep(_)))
            .copied()
            .collect::<Vec<_>>();
        assert_eq!(
            sleeps,
            vec![
                Event::Sleep(policy.poll_interval),
                Event::Sleep(policy.poll_interval),
                Event::Sleep(Duration::from_millis(1)),
            ]
        );
        assert!(!events.borrow().contains(&Event::ReadFrame(3)));
    }

    #[test]
    fn active_eof_requires_negotiated_close_after_last_complete_frame() {
        let events = Rc::new(RefCell::new(Vec::new()));
        let config = LocalFrameConfig::default();
        let mut reader = FrameReader::new(&[control(1, b"hello")], &config, Rc::clone(&events));
        let mut writer = RecordingWriter::new(Rc::clone(&events));
        let mut clock = FakeClock::new(Rc::clone(&events));
        let mut hello = DriverStep::success(
            vec![control(1, b"server-hello")],
            LocalExecutorPhase::Active,
            0,
        );
        hello.negotiated = Some(config.clone());
        let driver = FakeDriver::new(vec![hello], Vec::new(), Rc::clone(&events));

        let result = serve_streams_with_policy(
            &mut reader,
            &mut writer,
            &mut clock,
            SessionPolicy::FIXED,
            || Ok(driver),
        );

        assert_eq!(result, Err(ServeStdioError::SessionEndedBeforeClose));
        let mut output = writer.bytes.as_slice();
        assert!(read_frame(&mut output, &config).unwrap().is_some());
        assert!(read_frame(&mut output, &config).unwrap().is_none());
    }

    #[test]
    fn input_io_failure_after_hello_preserves_the_last_complete_output() {
        let events = Rc::new(RefCell::new(Vec::new()));
        let config = LocalFrameConfig::default();
        let mut reader = FrameReader::new(&[control(1, b"hello")], &config, Rc::clone(&events))
            .with_eof_failure();
        let mut writer = RecordingWriter::new(Rc::clone(&events));
        let mut clock = FakeClock::new(Rc::clone(&events));
        let mut hello = DriverStep::success(
            vec![control(1, b"server-hello")],
            LocalExecutorPhase::Active,
            0,
        );
        hello.negotiated = Some(config.clone());
        let driver = FakeDriver::new(vec![hello], Vec::new(), Rc::clone(&events));

        let result = serve_streams_with_policy(
            &mut reader,
            &mut writer,
            &mut clock,
            SessionPolicy::FIXED,
            || Ok(driver),
        );

        assert_eq!(result, Err(ServeStdioError::InputFrameRejected));
        assert_eq!(
            events
                .borrow()
                .iter()
                .filter(|event| **event == Event::Flush)
                .count(),
            1
        );
        let mut output = writer.bytes.as_slice();
        assert!(read_frame(&mut output, &config).unwrap().is_some());
        assert!(read_frame(&mut output, &config).unwrap().is_none());
    }

    #[test]
    fn fatal_service_failure_is_flushed_then_exits_before_another_read() {
        let events = Rc::new(RefCell::new(Vec::new()));
        let config = LocalFrameConfig::default();
        let input = [
            control(1, b"hello"),
            control(2, b"observe"),
            control(3, b"must-not-be-read"),
        ];
        let mut reader = FrameReader::new(&input, &config, Rc::clone(&events));
        let mut writer = RecordingWriter::new(Rc::clone(&events));
        let mut clock = FakeClock::new(Rc::clone(&events));
        let mut hello = DriverStep::success(
            vec![control(1, b"server-hello")],
            LocalExecutorPhase::Active,
            0,
        );
        hello.negotiated = Some(config);
        let mut service_failure = DriverStep::success(
            vec![control(2, b"service-unavailable")],
            LocalExecutorPhase::Active,
            0,
        );
        service_failure.fatal_after_output = true;
        let driver = FakeDriver::new(vec![hello, service_failure], Vec::new(), Rc::clone(&events));

        let result = serve_streams_with_policy(
            &mut reader,
            &mut writer,
            &mut clock,
            SessionPolicy::FIXED,
            || Ok(driver),
        );

        assert_eq!(result, Err(ServeStdioError::ServiceFailed));
        assert!(!events.borrow().contains(&Event::ReadFrame(3)));
        assert_eq!(
            events
                .borrow()
                .iter()
                .filter(|event| **event == Event::Flush)
                .count(),
            2
        );
    }

    #[test]
    fn production_fatal_classifier_distinguishes_service_from_request_failures() {
        let config = LocalFrameConfig::default();
        for (code, expected) in [
            (SessionFailureCode::ServiceUnavailable, true),
            (SessionFailureCode::Internal, true),
            (SessionFailureCode::ObservationRejected, false),
            (SessionFailureCode::ProtocolState, false),
        ] {
            let frame = server_control_frame(
                NonZeroU64::new(1).unwrap(),
                &LocalSessionServerMessage {
                    schema_version: LOCAL_SESSION_SCHEMA_VERSION,
                    message: LocalSessionServerKind::Failure(SessionFailure { code }),
                },
                &config,
            )
            .unwrap();
            assert_eq!(
                output_contains_fatal_service_failure(&[frame], &config),
                Ok(expected)
            );
        }
    }

    #[test]
    fn production_fatal_classifier_uses_negotiated_v2_compilation_limits() {
        let config = LocalFrameConfig::default();
        let compilation_limits = CompilationLimits {
            max_decisions: core::num::NonZeroU32::new(1).unwrap(),
            ..CompilationLimits::default()
        };
        let frame = server_control_frame_with_limits(
            NonZeroU64::new(1).unwrap(),
            &LocalSessionServerMessage {
                schema_version: LOCAL_SESSION_SCHEMA_VERSION_V2,
                message: LocalSessionServerKind::Hello(ServerHello {
                    effective_limits: LocalSessionLimits::from_config(&config).unwrap(),
                    effective_compilation_limits: Some(compilation_limits),
                }),
            },
            &config,
            &compilation_limits,
        )
        .unwrap();

        assert_eq!(
            output_contains_fatal_service_failure_with(
                std::slice::from_ref(&frame),
                &config,
                |frame, config| {
                    decode_server_control_frame_with_limits(frame, config, &compilation_limits)
                        .map(|(_, message)| message)
                }
            ),
            Ok(false)
        );
        assert_eq!(
            output_contains_fatal_service_failure(&[frame], &config),
            Err(ServeStdioError::ExecutorFailed)
        );
    }

    #[test]
    fn service_factory_and_executor_errors_stop_without_output_or_another_read() {
        for factory_failure in [true, false] {
            let events = Rc::new(RefCell::new(Vec::new()));
            let config = LocalFrameConfig::default();
            let input = [control(1, b"hello"), control(2, b"must-not-be-read")];
            let mut reader = FrameReader::new(&input, &config, Rc::clone(&events));
            let mut writer = RecordingWriter::new(Rc::clone(&events));
            let mut clock = FakeClock::new(Rc::clone(&events));
            let mut failure = DriverStep::success(Vec::new(), LocalExecutorPhase::AwaitingHello, 0);
            failure.error = Some(ServeStdioError::ExecutorFailed);
            let driver = FakeDriver::new(vec![failure], Vec::new(), Rc::clone(&events));

            let result = serve_streams_with_policy(
                &mut reader,
                &mut writer,
                &mut clock,
                SessionPolicy::FIXED,
                || {
                    if factory_failure {
                        Err(ServeStdioError::ServiceUnavailable)
                    } else {
                        Ok(driver)
                    }
                },
            );

            assert_eq!(
                result,
                Err(if factory_failure {
                    ServeStdioError::ServiceUnavailable
                } else {
                    ServeStdioError::ExecutorFailed
                })
            );
            assert!(writer.bytes.is_empty());
            assert!(!events.borrow().contains(&Event::ReadFrame(2)));
        }
    }

    #[test]
    fn eof_after_a_complete_prehello_failure_is_not_a_clean_no_session_exit() {
        let events = Rc::new(RefCell::new(Vec::new()));
        let config = LocalFrameConfig::default();
        let mut reader = FrameReader::new(&[control(9, b"not-hello")], &config, Rc::clone(&events));
        let mut writer = RecordingWriter::new(Rc::clone(&events));
        let mut clock = FakeClock::new(Rc::clone(&events));
        let driver = FakeDriver::new(
            vec![DriverStep::success(
                vec![control(9, b"protocol-state")],
                LocalExecutorPhase::AwaitingHello,
                0,
            )],
            Vec::new(),
            Rc::clone(&events),
        );

        let result = serve_streams_with_policy(
            &mut reader,
            &mut writer,
            &mut clock,
            SessionPolicy::FIXED,
            || Ok(driver),
        );

        assert_eq!(result, Err(ServeStdioError::SessionEndedBeforeHello));
        assert_eq!(
            events
                .borrow()
                .iter()
                .filter(|event| **event == Event::Flush)
                .count(),
            1
        );
    }

    #[test]
    fn truncated_or_corrupt_first_frame_rejects_before_driver_creation() {
        for bytes in [vec![0_u8; 1], vec![0xff_u8; 68]] {
            let events = Rc::new(RefCell::new(Vec::new()));
            let mut reader = bytes.as_slice();
            let mut writer = RecordingWriter::new(Rc::clone(&events));
            let mut clock = FakeClock::new(Rc::clone(&events));
            let created = Rc::new(RefCell::new(false));
            let created_for_factory = Rc::clone(&created);
            let events_for_factory = Rc::clone(&events);

            let result = serve_streams_with_policy(
                &mut reader,
                &mut writer,
                &mut clock,
                SessionPolicy::FIXED,
                move || {
                    *created_for_factory.borrow_mut() = true;
                    Ok(FakeDriver::new(Vec::new(), Vec::new(), events_for_factory))
                },
            );

            assert_eq!(result, Err(ServeStdioError::InputFrameRejected));
            assert!(!*created.borrow());
            assert!(writer.bytes.is_empty());
        }
    }

    #[test]
    fn write_zero_and_flush_failure_are_terminal_and_not_retried_by_the_session() {
        for flush_failure in [false, true] {
            let events = Rc::new(RefCell::new(Vec::new()));
            let config = LocalFrameConfig::default();
            let mut reader = FrameReader::new(&[control(1, b"close")], &config, Rc::clone(&events));
            let mut writer = RecordingWriter::new(Rc::clone(&events));
            writer.write_zero = !flush_failure;
            writer.fail_flush = flush_failure;
            let mut clock = FakeClock::new(Rc::clone(&events));
            let driver = FakeDriver::new(
                vec![DriverStep::success(
                    vec![control(1, b"closed")],
                    LocalExecutorPhase::Closed,
                    0,
                )],
                Vec::new(),
                Rc::clone(&events),
            );

            let result = serve_streams_with_policy(
                &mut reader,
                &mut writer,
                &mut clock,
                SessionPolicy::FIXED,
                || Ok(driver),
            );

            assert_eq!(
                result,
                Err(if flush_failure {
                    ServeStdioError::OutputFlushFailed
                } else {
                    ServeStdioError::OutputFrameFailed
                })
            );
            let recorded = events.borrow();
            if flush_failure {
                assert_eq!(
                    recorded
                        .iter()
                        .filter(|event| **event == Event::Flush)
                        .count(),
                    1
                );
            } else {
                assert_eq!(
                    recorded
                        .iter()
                        .filter(|event| **event == Event::Write)
                        .count(),
                    1
                );
                assert!(!recorded.contains(&Event::Flush));
            }
        }
    }

    #[test]
    fn physical_write_failure_after_a_prefix_stops_without_flush_or_next_read() {
        let events = Rc::new(RefCell::new(Vec::new()));
        let config = LocalFrameConfig::default();
        let input = [control(1, b"close"), control(2, b"must-not-be-read")];
        let mut reader = FrameReader::new(&input, &config, Rc::clone(&events));
        let mut writer = RecordingWriter::new(Rc::clone(&events));
        writer.max_chunk = 5;
        writer.fail_after_bytes = Some(5);
        let mut clock = FakeClock::new(Rc::clone(&events));
        let driver = FakeDriver::new(
            vec![DriverStep::success(
                vec![control(1, b"closed")],
                LocalExecutorPhase::Closed,
                0,
            )],
            Vec::new(),
            Rc::clone(&events),
        );

        let result = serve_streams_with_policy(
            &mut reader,
            &mut writer,
            &mut clock,
            SessionPolicy::FIXED,
            || Ok(driver),
        );

        assert_eq!(result, Err(ServeStdioError::OutputFrameFailed));
        assert_eq!(writer.bytes.len(), 5);
        assert!(!events.borrow().contains(&Event::Flush));
        assert!(!events.borrow().contains(&Event::ReadFrame(2)));
    }

    #[test]
    fn terminal_stream_preflight_is_exact() {
        assert_eq!(validate_standard_streams(false, false), Ok(()));
        for (stdin_terminal, stdout_terminal) in [(true, false), (false, true), (true, true)] {
            assert_eq!(
                validate_standard_streams(stdin_terminal, stdout_terminal),
                Err(ServeStdioError::InteractiveStandardStream)
            );
        }
    }

    fn control(correlation_id: u64, bytes: &[u8]) -> LocalFrame {
        LocalFrame::Control {
            correlation_id: NonZeroU64::new(correlation_id).unwrap(),
            bytes: bytes.to_vec(),
        }
    }

    fn position(events: &[Event], target: Event) -> usize {
        events.iter().position(|event| *event == target).unwrap()
    }
}
