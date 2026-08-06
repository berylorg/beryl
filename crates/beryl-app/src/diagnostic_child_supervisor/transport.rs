#[cfg(test)]
use std::path::PathBuf;
use std::{
    io::{self, BufReader, Write},
    process::{ChildStdin, ChildStdout},
    sync::{
        Arc, Mutex,
        mpsc::{self, Receiver, RecvTimeoutError, SyncSender, TrySendError},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

use crate::diagnostic_child_protocol::{
    BoundedLineRead, DiagnosticProtocolError, DiagnosticProtocolResponse,
    MAX_DIAGNOSTIC_PROTOCOL_FRAME_BYTES, parse_response_frame, read_bounded_line_bytes,
};

use super::DiagnosticChildSupervisorError;
#[cfg(test)]
use super::acceptance_gate::DIAGNOSTIC_ACCEPTANCE_STARTUP_GATE_FRAME;
use super::acceptance_gate::DIAGNOSTIC_ACCEPTANCE_STARTUP_READY_FRAME;

pub(super) enum DiagnosticStdoutEvent {
    AcceptanceGateReady,
    Response(DiagnosticProtocolResponse),
}

pub(super) struct DiagnosticStdoutReader {
    receiver: Receiver<Result<DiagnosticStdoutEvent, DiagnosticProtocolError>>,
    thread: Option<JoinHandle<()>>,
}

pub(super) struct DiagnosticStdinWriter {
    sender: Option<SyncSender<WriteCommand>>,
    thread: Option<JoinHandle<()>>,
    #[cfg(test)]
    force_join_timeout_once: bool,
    #[cfg(test)]
    next_write_delay: Arc<Mutex<Option<Duration>>>,
    #[cfg(test)]
    non_gate_write_marker: Arc<Mutex<Option<PathBuf>>>,
}

pub(super) struct DiagnosticStdinWriterSpawnError {
    source: io::Error,
    stdin: ChildStdin,
}

pub(super) struct DiagnosticStdoutReaderSpawnError {
    source: io::Error,
    stdout: ChildStdout,
}

struct WriteCommand {
    frame: Vec<u8>,
    acknowledgement: SyncSender<Result<(), io::Error>>,
}

impl DiagnosticStdinWriter {
    pub(super) fn spawn(stdin: ChildStdin) -> Result<Self, DiagnosticStdinWriterSpawnError> {
        let (sender, receiver) = mpsc::sync_channel::<WriteCommand>(1);
        let stdin_owner = Arc::new(Mutex::new(Some(stdin)));
        let thread_stdin_owner = Arc::clone(&stdin_owner);
        #[cfg(test)]
        let next_write_delay = Arc::new(Mutex::new(None));
        #[cfg(test)]
        let thread_write_delay = Arc::clone(&next_write_delay);
        #[cfg(test)]
        let non_gate_write_marker = Arc::new(Mutex::new(None));
        #[cfg(test)]
        let thread_non_gate_write_marker = Arc::clone(&non_gate_write_marker);
        let thread = match thread::Builder::new()
            .name("beryl-diagnostic-stdin".to_string())
            .spawn(move || {
                let mut stdin = thread_stdin_owner
                    .lock()
                    .expect("diagnostic stdin owner lock must remain available")
                    .take()
                    .expect("diagnostic stdin writer thread owns its pipe");
                while let Ok(command) = receiver.recv() {
                    #[cfg(test)]
                    if command.frame.as_slice() != DIAGNOSTIC_ACCEPTANCE_STARTUP_GATE_FRAME {
                        if let Some(marker) = thread_non_gate_write_marker
                            .lock()
                            .expect("diagnostic write marker lock must remain available")
                            .as_ref()
                        {
                            let _ = std::fs::write(marker, b"non-gate-write");
                        }
                    }
                    #[cfg(test)]
                    if let Some(delay) = thread_write_delay
                        .lock()
                        .expect("diagnostic write delay lock must remain available")
                        .take()
                    {
                        thread::sleep(delay);
                    }
                    let result = stdin.write_all(&command.frame).and_then(|_| stdin.flush());
                    let _ = command.acknowledgement.send(result);
                }
            }) {
            Ok(thread) => thread,
            Err(source) => {
                let stdin = stdin_owner
                    .lock()
                    .expect("failed writer spawn leaves diagnostic stdin owner available")
                    .take()
                    .expect("failed writer spawn retains diagnostic stdin pipe");
                return Err(DiagnosticStdinWriterSpawnError { source, stdin });
            }
        };
        Ok(Self {
            sender: Some(sender),
            thread: Some(thread),
            #[cfg(test)]
            force_join_timeout_once: false,
            #[cfg(test)]
            next_write_delay,
            #[cfg(test)]
            non_gate_write_marker,
        })
    }

    #[cfg(test)]
    pub(super) fn forced_spawn_failure(stdin: ChildStdin) -> DiagnosticStdinWriterSpawnError {
        DiagnosticStdinWriterSpawnError {
            source: io::Error::other("forced writer spawn failure for test"),
            stdin,
        }
    }

    pub(super) fn write_frame(
        &self,
        frame: Vec<u8>,
        deadline: Instant,
        timeout: std::time::Duration,
    ) -> Result<(), DiagnosticChildSupervisorError> {
        deadline
            .checked_duration_since(Instant::now())
            .ok_or(DiagnosticChildSupervisorError::RequestTimeout { timeout })?;
        let (acknowledgement, receiver) = mpsc::sync_channel(1);
        let sender = self.sender.as_ref().ok_or_else(closed_pipe_error)?;
        match sender.try_send(WriteCommand {
            frame,
            acknowledgement,
        }) {
            Ok(()) => {}
            Err(TrySendError::Full(_)) => {
                return Err(DiagnosticChildSupervisorError::WriteRequest {
                    source: io::Error::new(
                        io::ErrorKind::WouldBlock,
                        "diagnostic stdin writer is still completing an earlier frame",
                    ),
                });
            }
            Err(TrySendError::Disconnected(_)) => return Err(closed_pipe_error()),
        }
        let remaining = deadline
            .checked_duration_since(Instant::now())
            .ok_or(DiagnosticChildSupervisorError::RequestTimeout { timeout })?;
        match receiver.recv_timeout(remaining) {
            Ok(Ok(())) => Ok(()),
            Ok(Err(source)) => Err(DiagnosticChildSupervisorError::WriteRequest { source }),
            Err(RecvTimeoutError::Timeout) => {
                Err(DiagnosticChildSupervisorError::RequestTimeout { timeout })
            }
            Err(RecvTimeoutError::Disconnected) => Err(closed_pipe_error()),
        }
    }

    pub(super) fn close(&mut self) {
        drop(self.sender.take());
    }

    pub(super) fn join_after_child_reaped(&mut self) -> Result<(), DiagnosticChildSupervisorError> {
        self.close();
        let Some(thread) = self.thread.take() else {
            return Ok(());
        };
        thread
            .join()
            .map_err(|_| DiagnosticChildSupervisorError::WriterThreadPanicked)
    }

    #[cfg(test)]
    pub(super) fn joined_for_test(&self) -> bool {
        self.thread.is_none()
    }

    pub(super) fn join_by(
        &mut self,
        deadline: Instant,
        timeout: Duration,
    ) -> Result<(), DiagnosticChildSupervisorError> {
        #[cfg(test)]
        if std::mem::take(&mut self.force_join_timeout_once) {
            return Err(DiagnosticChildSupervisorError::RequestTimeout { timeout });
        }
        self.close();
        let Some(thread) = self.thread.as_ref() else {
            return Ok(());
        };
        while !thread.is_finished() {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Err(DiagnosticChildSupervisorError::RequestTimeout { timeout });
            }
            std::thread::sleep(remaining.min(Duration::from_millis(1)));
        }
        self.join_after_child_reaped()
    }

    #[cfg(test)]
    pub(super) fn thread_is_finished_for_test(&self) -> bool {
        self.thread.as_ref().is_none_or(JoinHandle::is_finished)
    }

    #[cfg(test)]
    pub(super) fn force_join_timeout_once_for_test(&mut self) {
        self.force_join_timeout_once = true;
    }

    #[cfg(test)]
    pub(super) fn delay_next_write_for_test(&self, delay: Duration) {
        *self
            .next_write_delay
            .lock()
            .expect("diagnostic write delay lock must remain available") = Some(delay);
    }

    #[cfg(test)]
    pub(super) fn mark_non_gate_writes_for_test(&self, marker: PathBuf) {
        *self
            .non_gate_write_marker
            .lock()
            .expect("diagnostic write marker lock must remain available") = Some(marker);
    }
}

impl DiagnosticStdinWriterSpawnError {
    pub(super) fn into_parts(self) -> (io::Error, ChildStdin) {
        (self.source, self.stdin)
    }
}

impl DiagnosticStdoutReaderSpawnError {
    pub(super) fn into_parts(self) -> (io::Error, ChildStdout) {
        (self.source, self.stdout)
    }
}

impl DiagnosticStdoutReader {
    pub(super) fn recv_timeout(
        &self,
        timeout: std::time::Duration,
    ) -> Result<Result<DiagnosticStdoutEvent, DiagnosticProtocolError>, RecvTimeoutError> {
        self.receiver.recv_timeout(timeout)
    }

    pub(super) fn join_after_child_reaped(&mut self) -> Result<(), DiagnosticChildSupervisorError> {
        let Some(thread) = self.thread.take() else {
            return Ok(());
        };
        thread
            .join()
            .map_err(|_| DiagnosticChildSupervisorError::ReaderThreadPanicked)
    }

    pub(super) fn join_by(
        &mut self,
        deadline: Instant,
        timeout: Duration,
    ) -> Result<(), DiagnosticChildSupervisorError> {
        let Some(thread) = self.thread.as_ref() else {
            return Ok(());
        };
        while !thread.is_finished() {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Err(DiagnosticChildSupervisorError::RequestTimeout { timeout });
            }
            std::thread::sleep(remaining.min(Duration::from_millis(1)));
        }
        self.join_after_child_reaped()
    }
}

fn closed_pipe_error() -> DiagnosticChildSupervisorError {
    DiagnosticChildSupervisorError::WriteRequest {
        source: io::Error::new(io::ErrorKind::BrokenPipe, "diagnostic stdin writer stopped"),
    }
}

pub(super) fn spawn_stdout_reader(
    stdout: ChildStdout,
    expect_acceptance_gate_ready: bool,
) -> DiagnosticStdoutReader {
    let (sender, receiver) = mpsc::sync_channel(16);
    let thread =
        thread::spawn(move || run_stdout_reader(stdout, sender, expect_acceptance_gate_ready));
    DiagnosticStdoutReader {
        receiver,
        thread: Some(thread),
    }
}

pub(super) fn spawn_stdout_reader_fallible(
    stdout: ChildStdout,
    expect_acceptance_gate_ready: bool,
) -> Result<DiagnosticStdoutReader, DiagnosticStdoutReaderSpawnError> {
    spawn_stdout_reader_fallible_with_forced_error(stdout, expect_acceptance_gate_ready, None)
}

#[cfg(test)]
pub(super) fn force_stdout_reader_spawn_failure(
    stdout: ChildStdout,
    expect_acceptance_gate_ready: bool,
) -> Result<DiagnosticStdoutReader, DiagnosticStdoutReaderSpawnError> {
    spawn_stdout_reader_fallible_with_forced_error(
        stdout,
        expect_acceptance_gate_ready,
        Some(io::Error::other(
            "forced stdout reader spawn failure for test",
        )),
    )
}

fn spawn_stdout_reader_fallible_with_forced_error(
    stdout: ChildStdout,
    expect_acceptance_gate_ready: bool,
    forced_error: Option<io::Error>,
) -> Result<DiagnosticStdoutReader, DiagnosticStdoutReaderSpawnError> {
    let (sender, receiver) = mpsc::sync_channel(16);
    let stdout_owner = Arc::new(Mutex::new(Some(stdout)));
    let thread_stdout_owner = Arc::clone(&stdout_owner);
    let spawned = forced_error.map_or_else(
        || {
            thread::Builder::new()
                .name("beryl-diagnostic-stdout".to_string())
                .spawn(move || {
                    let stdout = thread_stdout_owner
                        .lock()
                        .expect("diagnostic stdout owner lock must remain available")
                        .take()
                        .expect("diagnostic stdout reader thread owns its pipe");
                    run_stdout_reader(stdout, sender, expect_acceptance_gate_ready);
                })
        },
        Err,
    );
    let thread = match spawned {
        Ok(thread) => thread,
        Err(source) => {
            let stdout = stdout_owner
                .lock()
                .expect("failed reader spawn leaves diagnostic stdout owner available")
                .take()
                .expect("failed reader spawn retains diagnostic stdout pipe");
            return Err(DiagnosticStdoutReaderSpawnError { source, stdout });
        }
    };
    Ok(DiagnosticStdoutReader {
        receiver,
        thread: Some(thread),
    })
}

fn run_stdout_reader(
    stdout: ChildStdout,
    sender: SyncSender<Result<DiagnosticStdoutEvent, DiagnosticProtocolError>>,
    expect_acceptance_gate_ready: bool,
) {
    let mut reader = BufReader::new(stdout);
    let mut waiting_for_gate = expect_acceptance_gate_ready;
    loop {
        match read_bounded_line_bytes(&mut reader, MAX_DIAGNOSTIC_PROTOCOL_FRAME_BYTES) {
            Ok(BoundedLineRead::Eof) => break,
            Ok(BoundedLineRead::Line(line)) if waiting_for_gate => {
                waiting_for_gate = false;
                if line.as_slice() == DIAGNOSTIC_ACCEPTANCE_STARTUP_READY_FRAME {
                    if sender
                        .try_send(Ok(DiagnosticStdoutEvent::AcceptanceGateReady))
                        .is_err()
                    {
                        break;
                    }
                } else {
                    let _ = sender.try_send(Err(DiagnosticProtocolError::InvalidJson {
                        message: "diagnostic acceptance startup ready frame was invalid"
                            .to_string(),
                    }));
                    break;
                }
            }
            Ok(BoundedLineRead::Line(line)) => match parse_response_frame(&line) {
                Ok(Some(response)) => {
                    if sender
                        .try_send(Ok(DiagnosticStdoutEvent::Response(response)))
                        .is_err()
                    {
                        break;
                    }
                }
                Ok(None) => {}
                Err(error) => {
                    let _ = sender.try_send(Err(error));
                    break;
                }
            },
            Ok(BoundedLineRead::LineTooLong { .. }) => {
                let _ = sender.try_send(Err(DiagnosticProtocolError::FrameTooLarge {
                    limit: MAX_DIAGNOSTIC_PROTOCOL_FRAME_BYTES,
                }));
                break;
            }
            Err(error) => {
                let _ = sender.try_send(Err(DiagnosticProtocolError::InvalidJson {
                    message: error.to_string(),
                }));
                break;
            }
        }
    }
}
