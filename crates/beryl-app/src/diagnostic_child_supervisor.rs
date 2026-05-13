#![allow(dead_code)]

use std::{
    io::{self, BufReader, Write},
    path::{Path, PathBuf},
    process::{Child, ChildStdin, ChildStdout, Command, Stdio},
    sync::mpsc::{self, Receiver, RecvTimeoutError},
    thread,
    time::{Duration, Instant},
};

use serde_json::Value;
use thiserror::Error;
use tracing::{debug, warn};

use crate::{
    BerylHomeDir, BerylHomeDirError,
    diagnostic_child_protocol::{
        BoundedLineRead, DIAGNOSTIC_CHILD_PROTOCOL_NAME, DIAGNOSTIC_CHILD_PROTOCOL_VERSION,
        DiagnosticChildCommand, DiagnosticProtocolError, DiagnosticProtocolErrorBody,
        DiagnosticProtocolResponse, MAX_DIAGNOSTIC_PROTOCOL_FRAME_BYTES, parse_response_frame,
        read_bounded_line_bytes, request_frame,
    },
};

#[path = "diagnostic_child_supervisor/launch.rs"]
mod launch;

pub(crate) use launch::{DiagnosticChildLaunch, MAX_DIAGNOSTIC_CHILD_EXECUTABLE_PATH_BYTES};

const CHILD_SHUTDOWN_GRACE_TIMEOUT: Duration = Duration::from_millis(250);
const CHILD_KILL_TIMEOUT: Duration = Duration::from_secs(5);
pub(crate) const DIAGNOSTIC_CHILD_STOP_BUDGET: Duration = Duration::from_secs(11);
pub(crate) const DIAGNOSTIC_CHILD_STOP_RESPONSE_TIMEOUT: Duration = Duration::from_secs(12);
const DIAGNOSTIC_CHILD_STARTUP_RESPONSE_TIMEOUT: Duration = Duration::from_secs(5);
const CHILD_WAIT_POLL_INTERVAL: Duration = Duration::from_millis(25);
const STDERR_LOG_LIMIT: usize = 512;

pub(crate) struct DiagnosticChildSupervisor {
    child: Option<DiagnosticChildProcess>,
    next_request_id: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct DiagnosticChildIdentity {
    pub pid: u32,
    pub home_dir: PathBuf,
    pub executable_path: PathBuf,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum DiagnosticChildStartOutcome {
    Started(DiagnosticChildIdentity),
    AlreadyRunning(DiagnosticChildIdentity),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum DiagnosticChildStopOutcome {
    Stopped(DiagnosticChildIdentity),
    NotRunning,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum DiagnosticChildStatus {
    Running(DiagnosticChildIdentity),
    NotRunning,
}

#[derive(Debug, Error)]
pub(crate) enum DiagnosticChildSupervisorError {
    #[error("failed to resolve diagnostic child Beryl home: {0}")]
    BerylHomeDir(#[from] BerylHomeDirError),
    #[error(
        "diagnostic child home {child_home} must be isolated from supervisor home {supervisor_home}"
    )]
    HomeCollidesWithSupervisor {
        child_home: PathBuf,
        supervisor_home: PathBuf,
    },
    #[error("failed to resolve current Beryl executable path: {source}")]
    CurrentExecutable { source: io::Error },
    #[error("invalid diagnostic child executable path {path}: {reason}")]
    InvalidExecutablePath { path: PathBuf, reason: &'static str },
    #[error("failed to inspect diagnostic child executable path {path}: {source}")]
    ExecutablePathAccess { path: PathBuf, source: io::Error },
    #[error("failed to spawn diagnostic child Beryl process from {executable_path}: {source}")]
    Spawn {
        executable_path: PathBuf,
        source: io::Error,
    },
    #[error("diagnostic child process did not expose piped stdin")]
    MissingStdin,
    #[error("diagnostic child process did not expose piped stdout")]
    MissingStdout,
    #[error("failed to write diagnostic child protocol request: {source}")]
    WriteRequest { source: io::Error },
    #[error("timed out waiting for diagnostic child protocol response after {timeout:?}")]
    RequestTimeout { timeout: Duration },
    #[error("diagnostic child protocol stream ended")]
    ProtocolEof,
    #[error("diagnostic child protocol error: {0}")]
    Protocol(#[from] DiagnosticProtocolError),
    #[error("diagnostic child returned {kind}: {message}")]
    ChildError { kind: String, message: String },
    #[error("timed out waiting for diagnostic child startup protocol after {timeout:?}")]
    StartupProtocolTimeout { timeout: Duration },
    #[error("diagnostic child startup protocol stream ended before readiness")]
    StartupProtocolEof,
    #[error("diagnostic child startup protocol returned malformed response: {source}")]
    StartupProtocolMalformed { source: DiagnosticProtocolError },
    #[error("diagnostic child startup protocol returned {kind}: {message}")]
    StartupProtocolRejected { kind: String, message: String },
    #[error("diagnostic child startup protocol is incompatible: {message}")]
    StartupProtocolIncompatible { message: String },
    #[error("failed to query diagnostic child process status: {source}")]
    QueryStatus { source: io::Error },
    #[error("failed to terminate diagnostic child process: {source}")]
    Terminate { source: io::Error },
    #[cfg(target_os = "windows")]
    #[error("failed to create diagnostic child process job: {source}")]
    CreateProcessJob { source: io::Error },
    #[cfg(target_os = "windows")]
    #[error("failed to configure diagnostic child process job: {source}")]
    ConfigureProcessJob { source: io::Error },
    #[cfg(target_os = "windows")]
    #[error("failed to assign diagnostic child process to job: {source}")]
    AssignProcessToJob { source: io::Error },
    #[cfg(target_os = "windows")]
    #[error("failed to terminate diagnostic child process job: {source}")]
    TerminateProcessJob { source: io::Error },
}

struct DiagnosticChildProcess {
    child: Child,
    stdin: ChildStdin,
    stdout_receiver: Receiver<Result<DiagnosticProtocolResponse, DiagnosticProtocolError>>,
    host_process_tree: DiagnosticHostProcessTree,
    home_dir: BerylHomeDir,
    executable_path: PathBuf,
}

pub(crate) struct SpawnedDiagnosticChildGuard {
    child: Option<Child>,
}

impl Default for DiagnosticChildSupervisor {
    fn default() -> Self {
        Self {
            child: None,
            next_request_id: 1,
        }
    }
}

impl DiagnosticChildSupervisor {
    pub(crate) fn start(
        &mut self,
        supervisor_home: &BerylHomeDir,
        launch: DiagnosticChildLaunch,
    ) -> Result<DiagnosticChildStartOutcome, DiagnosticChildSupervisorError> {
        self.start_with_startup_timeout(
            supervisor_home,
            launch,
            DIAGNOSTIC_CHILD_STARTUP_RESPONSE_TIMEOUT,
        )
    }

    fn start_with_startup_timeout(
        &mut self,
        supervisor_home: &BerylHomeDir,
        launch: DiagnosticChildLaunch,
        startup_timeout: Duration,
    ) -> Result<DiagnosticChildStartOutcome, DiagnosticChildSupervisorError> {
        self.clear_if_exited()?;
        if let Some(child) = self.child.as_ref() {
            return Ok(DiagnosticChildStartOutcome::AlreadyRunning(
                child.identity(),
            ));
        }

        let child_home = BerylHomeDir::from_explicit_path(launch.child_home().to_path_buf())?;
        if same_home_path(supervisor_home.root_dir(), child_home.root_dir()) {
            return Err(DiagnosticChildSupervisorError::HomeCollidesWithSupervisor {
                child_home: child_home.root_dir().to_path_buf(),
                supervisor_home: supervisor_home.root_dir().to_path_buf(),
            });
        }

        let executable_path = launch::resolve_executable_path(launch.executable_path())?;
        let mut command = Command::new(&executable_path);
        command
            .arg("--diagnostic-target-stdio")
            .arg("--beryl-home-dir")
            .arg(child_home.root_dir())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let child = command
            .spawn()
            .map_err(|source| DiagnosticChildSupervisorError::Spawn {
                executable_path: executable_path.clone(),
                source,
            })?;
        let mut child_guard = SpawnedDiagnosticChildGuard::new(child);
        let host_process_tree = DiagnosticHostProcessTree::create_for_child(child_guard.child())?;
        let stdin = child_guard
            .child_mut()
            .stdin
            .take()
            .ok_or(DiagnosticChildSupervisorError::MissingStdin)?;
        let stdout = child_guard
            .child_mut()
            .stdout
            .take()
            .ok_or(DiagnosticChildSupervisorError::MissingStdout)?;
        if let Some(stderr) = child_guard.child_mut().stderr.take() {
            spawn_stderr_logger(stderr);
        }

        let mut process = DiagnosticChildProcess {
            child: child_guard.into_child(),
            stdin,
            stdout_receiver: spawn_stdout_reader(stdout),
            host_process_tree,
            home_dir: child_home,
            executable_path,
        };
        let request_id = self.next_request_id();
        if let Err(error) = process.verify_startup_protocol(&request_id, startup_timeout) {
            return self.handle_startup_verification_failure(process, error, |process| {
                process.shutdown(Duration::ZERO, CHILD_KILL_TIMEOUT)
            });
        }
        let identity = process.identity();
        self.child = Some(process);
        Ok(DiagnosticChildStartOutcome::Started(identity))
    }

    #[cfg(test)]
    pub(crate) fn start_for_test(
        &mut self,
        supervisor_home: &BerylHomeDir,
        launch: DiagnosticChildLaunch,
        startup_timeout: Duration,
    ) -> Result<DiagnosticChildStartOutcome, DiagnosticChildSupervisorError> {
        self.start_with_startup_timeout(supervisor_home, launch, startup_timeout)
    }

    pub(crate) fn stop(
        &mut self,
    ) -> Result<DiagnosticChildStopOutcome, DiagnosticChildSupervisorError> {
        self.stop_with_timeouts(CHILD_SHUTDOWN_GRACE_TIMEOUT, CHILD_KILL_TIMEOUT)
    }

    fn stop_with_timeouts(
        &mut self,
        grace_timeout: Duration,
        kill_timeout: Duration,
    ) -> Result<DiagnosticChildStopOutcome, DiagnosticChildSupervisorError> {
        self.stop_with_shutdown(|child| child.shutdown(grace_timeout, kill_timeout))
    }

    fn stop_with_shutdown(
        &mut self,
        shutdown: impl FnOnce(&mut DiagnosticChildProcess) -> Result<(), DiagnosticChildSupervisorError>,
    ) -> Result<DiagnosticChildStopOutcome, DiagnosticChildSupervisorError> {
        let Some(mut child) = self.child.take() else {
            return Ok(DiagnosticChildStopOutcome::NotRunning);
        };
        let identity = child.identity();
        match shutdown(&mut child) {
            Ok(()) => Ok(DiagnosticChildStopOutcome::Stopped(identity)),
            Err(error) => {
                self.child = Some(child);
                Err(error)
            }
        }
    }

    pub(crate) fn status(
        &mut self,
    ) -> Result<DiagnosticChildStatus, DiagnosticChildSupervisorError> {
        self.clear_if_exited()?;
        Ok(self
            .child
            .as_ref()
            .map(|child| DiagnosticChildStatus::Running(child.identity()))
            .unwrap_or(DiagnosticChildStatus::NotRunning))
    }

    pub(crate) fn request(
        &mut self,
        command: DiagnosticChildCommand,
        params: Value,
        timeout: Duration,
    ) -> Result<Value, DiagnosticChildSupervisorError> {
        self.clear_if_exited()?;
        let request_id = self.next_request_id();
        let child = self
            .child
            .as_mut()
            .ok_or(DiagnosticChildSupervisorError::ProtocolEof)?;

        match child.request(&request_id, command, params, timeout) {
            Err(
                error @ (DiagnosticChildSupervisorError::Protocol(_)
                | DiagnosticChildSupervisorError::ProtocolEof),
            ) => {
                self.child = None;
                Err(error)
            }
            result => result,
        }
    }

    fn next_request_id(&mut self) -> String {
        let request_id = self.next_request_id.to_string();
        self.next_request_id = self.next_request_id.saturating_add(1);
        request_id
    }

    #[cfg(test)]
    pub(crate) fn adopt_child_for_test(
        &mut self,
        child: Child,
        home_dir: BerylHomeDir,
        executable_path: PathBuf,
    ) -> Result<(), DiagnosticChildSupervisorError> {
        self.child = Some(DiagnosticChildProcess::from_child_for_test(
            child,
            home_dir,
            executable_path,
        )?);
        Ok(())
    }

    #[cfg(test)]
    pub(crate) fn retain_startup_failure_child_for_test(
        &mut self,
        child: Child,
        home_dir: BerylHomeDir,
        executable_path: PathBuf,
    ) -> Result<DiagnosticChildStartOutcome, DiagnosticChildSupervisorError> {
        let process =
            DiagnosticChildProcess::from_child_for_test(child, home_dir, executable_path)?;
        self.handle_startup_verification_failure(
            process,
            DiagnosticChildSupervisorError::StartupProtocolEof,
            |_| {
                Err(DiagnosticChildSupervisorError::RequestTimeout {
                    timeout: Duration::ZERO,
                })
            },
        )
    }

    #[cfg(test)]
    pub(crate) fn force_stop_error_for_test(
        &mut self,
    ) -> Result<DiagnosticChildStopOutcome, DiagnosticChildSupervisorError> {
        self.stop_with_shutdown(|_| {
            Err(DiagnosticChildSupervisorError::RequestTimeout {
                timeout: Duration::ZERO,
            })
        })
    }

    #[cfg(test)]
    pub(crate) fn has_child_for_test(&self) -> bool {
        self.child.is_some()
    }

    fn clear_if_exited(&mut self) -> Result<(), DiagnosticChildSupervisorError> {
        if self
            .child
            .as_mut()
            .map(DiagnosticChildProcess::has_exited)
            .transpose()?
            .unwrap_or(false)
        {
            self.child = None;
        }
        Ok(())
    }

    fn handle_startup_verification_failure(
        &mut self,
        mut process: DiagnosticChildProcess,
        startup_error: DiagnosticChildSupervisorError,
        shutdown: impl FnOnce(&mut DiagnosticChildProcess) -> Result<(), DiagnosticChildSupervisorError>,
    ) -> Result<DiagnosticChildStartOutcome, DiagnosticChildSupervisorError> {
        if let Err(cleanup_error) = shutdown(&mut process) {
            warn!(
                %startup_error,
                %cleanup_error,
                "failed to clean up diagnostic child after startup verification failure; retaining child for stop retry"
            );
            self.child = Some(process);
            return Err(cleanup_error);
        }
        Err(startup_error)
    }
}

impl Drop for DiagnosticChildSupervisor {
    fn drop(&mut self) {
        if let Err(error) = self.stop() {
            warn!(%error, "failed to drop diagnostic child process");
        }
    }
}

impl SpawnedDiagnosticChildGuard {
    pub(crate) fn new(child: Child) -> Self {
        Self { child: Some(child) }
    }

    fn child(&self) -> &Child {
        self.child
            .as_ref()
            .expect("spawned diagnostic child guard must contain child")
    }

    fn child_mut(&mut self) -> &mut Child {
        self.child
            .as_mut()
            .expect("spawned diagnostic child guard must contain child")
    }

    fn into_child(mut self) -> Child {
        self.child
            .take()
            .expect("spawned diagnostic child guard must contain child")
    }

    fn cleanup(&mut self, kill_timeout: Duration) -> Result<bool, DiagnosticChildSupervisorError> {
        let Some(mut child) = self.child.take() else {
            return Ok(true);
        };
        let cleanup_result = match child.kill() {
            Ok(()) => wait_for_exit(&mut child, kill_timeout),
            Err(source) if source.kind() == io::ErrorKind::InvalidInput => {
                wait_for_exit(&mut child, Duration::ZERO)
            }
            Err(source) => Err(DiagnosticChildSupervisorError::Terminate { source }),
        };
        match cleanup_result {
            Ok(true) => Ok(true),
            Ok(false) => {
                self.child = Some(child);
                Ok(false)
            }
            Err(error) => {
                self.child = Some(child);
                Err(error)
            }
        }
    }

    #[cfg(test)]
    pub(crate) fn cleanup_for_test(
        &mut self,
        kill_timeout: Duration,
    ) -> Result<bool, DiagnosticChildSupervisorError> {
        self.cleanup(kill_timeout)
    }
}

impl Drop for SpawnedDiagnosticChildGuard {
    fn drop(&mut self) {
        match self.cleanup(CHILD_KILL_TIMEOUT) {
            Ok(true) => {}
            Ok(false) => warn!("timed out cleaning up unclaimed diagnostic child process"),
            Err(error) => warn!(%error, "failed to clean up unclaimed diagnostic child process"),
        }
    }
}

impl DiagnosticChildProcess {
    fn identity(&self) -> DiagnosticChildIdentity {
        DiagnosticChildIdentity {
            pid: self.child.id(),
            home_dir: self.home_dir.root_dir().to_path_buf(),
            executable_path: self.executable_path.clone(),
        }
    }

    fn verify_startup_protocol(
        &mut self,
        request_id: &str,
        timeout: Duration,
    ) -> Result<(), DiagnosticChildSupervisorError> {
        let result = self
            .request(
                request_id,
                DiagnosticChildCommand::Handshake,
                serde_json::json!({}),
                timeout,
            )
            .map_err(startup_protocol_error)?;
        validate_startup_handshake_result(&result)
    }

    #[cfg(test)]
    fn from_child_for_test(
        mut child: Child,
        home_dir: BerylHomeDir,
        executable_path: PathBuf,
    ) -> Result<Self, DiagnosticChildSupervisorError> {
        let stdin = child
            .stdin
            .take()
            .ok_or(DiagnosticChildSupervisorError::MissingStdin)?;
        let stdout = child
            .stdout
            .take()
            .ok_or(DiagnosticChildSupervisorError::MissingStdout)?;
        Ok(Self {
            child,
            stdin,
            stdout_receiver: spawn_stdout_reader(stdout),
            host_process_tree: DiagnosticHostProcessTree::empty_for_test(),
            home_dir,
            executable_path,
        })
    }

    fn request(
        &mut self,
        request_id: &str,
        command: DiagnosticChildCommand,
        params: Value,
        timeout: Duration,
    ) -> Result<Value, DiagnosticChildSupervisorError> {
        let frame = request_frame(request_id, command, params)?;
        self.stdin
            .write_all(&frame)
            .and_then(|_| self.stdin.flush())
            .map_err(|source| DiagnosticChildSupervisorError::WriteRequest { source })?;

        let deadline = Instant::now() + timeout;
        loop {
            let Some(remaining) = deadline.checked_duration_since(Instant::now()) else {
                return Err(DiagnosticChildSupervisorError::RequestTimeout { timeout });
            };
            match self.stdout_receiver.recv_timeout(remaining) {
                Ok(Ok(response)) => {
                    if response.id() != Some(request_id) {
                        continue;
                    }
                    return response.into_result().map_err(child_protocol_error);
                }
                Ok(Err(error)) => return Err(DiagnosticChildSupervisorError::Protocol(error)),
                Err(RecvTimeoutError::Timeout) => {
                    return Err(DiagnosticChildSupervisorError::RequestTimeout { timeout });
                }
                Err(RecvTimeoutError::Disconnected) => {
                    return Err(DiagnosticChildSupervisorError::ProtocolEof);
                }
            }
        }
    }

    fn has_exited(&mut self) -> Result<bool, DiagnosticChildSupervisorError> {
        self.child
            .try_wait()
            .map(|status| status.is_some())
            .map_err(|source| DiagnosticChildSupervisorError::QueryStatus { source })
    }

    fn shutdown(
        &mut self,
        grace_timeout: Duration,
        kill_timeout: Duration,
    ) -> Result<(), DiagnosticChildSupervisorError> {
        if wait_for_exit(&mut self.child, grace_timeout)? {
            self.host_process_tree.release();
            return Ok(());
        }
        match self.child.kill() {
            Ok(()) => {
                if wait_for_exit(&mut self.child, kill_timeout)? {
                    self.host_process_tree.release();
                    return Ok(());
                }
            }
            Err(source) if source.kind() == io::ErrorKind::InvalidInput => {
                if wait_for_exit(&mut self.child, Duration::ZERO)? {
                    self.host_process_tree.release();
                    return Ok(());
                }
                return Err(DiagnosticChildSupervisorError::Terminate { source });
            }
            Err(source) => return Err(DiagnosticChildSupervisorError::Terminate { source }),
        }

        if self.host_process_tree.terminate()? && wait_for_exit(&mut self.child, kill_timeout)? {
            self.host_process_tree.release();
            return Ok(());
        }

        Err(DiagnosticChildSupervisorError::RequestTimeout {
            timeout: kill_timeout,
        })
    }
}

fn startup_protocol_error(error: DiagnosticChildSupervisorError) -> DiagnosticChildSupervisorError {
    match error {
        DiagnosticChildSupervisorError::WriteRequest { source } => {
            DiagnosticChildSupervisorError::StartupProtocolIncompatible {
                message: format!("failed to write startup handshake request: {source}"),
            }
        }
        DiagnosticChildSupervisorError::RequestTimeout { timeout } => {
            DiagnosticChildSupervisorError::StartupProtocolTimeout { timeout }
        }
        DiagnosticChildSupervisorError::ProtocolEof => {
            DiagnosticChildSupervisorError::StartupProtocolEof
        }
        DiagnosticChildSupervisorError::Protocol(source) => {
            DiagnosticChildSupervisorError::StartupProtocolMalformed { source }
        }
        DiagnosticChildSupervisorError::ChildError { kind, message } => {
            DiagnosticChildSupervisorError::StartupProtocolRejected { kind, message }
        }
        error => error,
    }
}

fn validate_startup_handshake_result(result: &Value) -> Result<(), DiagnosticChildSupervisorError> {
    let protocol = result.get("protocol").and_then(Value::as_str);
    let version = result.get("protocolVersion").and_then(Value::as_u64);
    if protocol != Some(DIAGNOSTIC_CHILD_PROTOCOL_NAME) {
        return Err(
            DiagnosticChildSupervisorError::StartupProtocolIncompatible {
                message: "handshake protocol name did not match Beryl diagnostic child protocol"
                    .to_string(),
            },
        );
    }
    if version != Some(DIAGNOSTIC_CHILD_PROTOCOL_VERSION) {
        return Err(
            DiagnosticChildSupervisorError::StartupProtocolIncompatible {
                message: "handshake protocol version did not match supervisor protocol version"
                    .to_string(),
            },
        );
    }
    Ok(())
}

fn spawn_stdout_reader(
    stdout: ChildStdout,
) -> Receiver<Result<DiagnosticProtocolResponse, DiagnosticProtocolError>> {
    let (sender, receiver) = mpsc::sync_channel(16);
    thread::spawn(move || {
        let mut reader = BufReader::new(stdout);
        loop {
            match read_bounded_line_bytes(&mut reader, MAX_DIAGNOSTIC_PROTOCOL_FRAME_BYTES) {
                Ok(BoundedLineRead::Eof) => break,
                Ok(BoundedLineRead::Line(line)) => match parse_response_frame(&line) {
                    Ok(Some(response)) => {
                        if sender.send(Ok(response)).is_err() {
                            break;
                        }
                    }
                    Ok(None) => {}
                    Err(error) => {
                        let _ = sender.send(Err(error));
                        break;
                    }
                },
                Ok(BoundedLineRead::LineTooLong { .. }) => {
                    let _ = sender.send(Err(DiagnosticProtocolError::FrameTooLarge {
                        limit: MAX_DIAGNOSTIC_PROTOCOL_FRAME_BYTES,
                    }));
                    break;
                }
                Err(error) => {
                    let _ = sender.send(Err(DiagnosticProtocolError::InvalidJson {
                        message: error.to_string(),
                    }));
                    break;
                }
            }
        }
    });
    receiver
}

fn spawn_stderr_logger(stderr: impl io::Read + Send + 'static) {
    thread::spawn(move || {
        let mut reader = BufReader::new(stderr);
        loop {
            match read_bounded_line_bytes(&mut reader, 8 * 1024) {
                Ok(BoundedLineRead::Eof) => break,
                Ok(BoundedLineRead::Line(line))
                | Ok(BoundedLineRead::LineTooLong { prefix: line }) => {
                    let line = String::from_utf8_lossy(&line);
                    if line.trim().is_empty() {
                        continue;
                    }
                    debug!(
                        message = %truncate_for_log(&line, STDERR_LOG_LIMIT),
                        "diagnostic child stderr"
                    );
                }
                Err(error) => {
                    warn!(%error, "failed to read diagnostic child stderr");
                    break;
                }
            }
        }
    });
}

fn wait_for_exit(
    child: &mut Child,
    timeout: Duration,
) -> Result<bool, DiagnosticChildSupervisorError> {
    let deadline = Instant::now() + timeout;
    loop {
        if child
            .try_wait()
            .map_err(|source| DiagnosticChildSupervisorError::QueryStatus { source })?
            .is_some()
        {
            return Ok(true);
        }
        if Instant::now() >= deadline {
            return Ok(false);
        }
        thread::sleep(CHILD_WAIT_POLL_INTERVAL);
    }
}

pub(crate) fn same_home_path(left: &Path, right: &Path) -> bool {
    let left = std::fs::canonicalize(left).unwrap_or_else(|_| left.to_path_buf());
    let right = std::fs::canonicalize(right).unwrap_or_else(|_| right.to_path_buf());
    same_path_label(&left) == same_path_label(&right)
}

#[cfg(target_os = "windows")]
fn same_path_label(path: &Path) -> String {
    path.display().to_string().to_ascii_lowercase()
}

#[cfg(not(target_os = "windows"))]
fn same_path_label(path: &Path) -> String {
    path.display().to_string()
}

fn child_protocol_error(error: DiagnosticProtocolErrorBody) -> DiagnosticChildSupervisorError {
    DiagnosticChildSupervisorError::ChildError {
        kind: error.kind().to_string(),
        message: error.message().to_string(),
    }
}

fn truncate_for_log(line: &str, limit: usize) -> String {
    if line.chars().count() <= limit {
        return line.to_string();
    }
    let truncated = line.chars().take(limit).collect::<String>();
    format!("{truncated}...")
}

#[cfg(target_os = "windows")]
struct DiagnosticHostProcessTree {
    job: Option<windows::core::Owned<windows::Win32::Foundation::HANDLE>>,
}

#[cfg(target_os = "windows")]
unsafe impl Send for DiagnosticHostProcessTree {}

#[cfg(target_os = "windows")]
impl DiagnosticHostProcessTree {
    fn create_for_child(child: &Child) -> Result<Self, DiagnosticChildSupervisorError> {
        use std::{mem::size_of, os::windows::io::AsRawHandle};

        use windows::{
            Win32::{
                Foundation::HANDLE,
                System::JobObjects::{
                    AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
                    JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectExtendedLimitInformation,
                    SetInformationJobObject,
                },
            },
            core::{Owned, PCWSTR},
        };

        let job = unsafe { CreateJobObjectW(None, PCWSTR::null()) }.map_err(|source| {
            DiagnosticChildSupervisorError::CreateProcessJob {
                source: windows_io_error(source),
            }
        })?;
        let job = unsafe { Owned::new(job) };

        let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
        limits.BasicLimitInformation.LimitFlags |= JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        unsafe {
            SetInformationJobObject(
                *job,
                JobObjectExtendedLimitInformation,
                &limits as *const JOBOBJECT_EXTENDED_LIMIT_INFORMATION as *const _,
                size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            )
        }
        .map_err(
            |source| DiagnosticChildSupervisorError::ConfigureProcessJob {
                source: windows_io_error(source),
            },
        )?;

        let process_handle = HANDLE(child.as_raw_handle());
        unsafe { AssignProcessToJobObject(*job, process_handle) }.map_err(|source| {
            DiagnosticChildSupervisorError::AssignProcessToJob {
                source: windows_io_error(source),
            }
        })?;

        Ok(Self { job: Some(job) })
    }

    fn terminate(&self) -> Result<bool, DiagnosticChildSupervisorError> {
        use windows::Win32::System::JobObjects::TerminateJobObject;

        let Some(job) = &self.job else {
            return Ok(false);
        };
        unsafe { TerminateJobObject(**job, 1) }.map_err(|source| {
            DiagnosticChildSupervisorError::TerminateProcessJob {
                source: windows_io_error(source),
            }
        })?;
        Ok(true)
    }

    fn release(&mut self) {
        drop(self.job.take());
    }

    #[cfg(test)]
    fn empty_for_test() -> Self {
        Self { job: None }
    }
}

#[cfg(target_os = "windows")]
fn windows_io_error(source: windows::core::Error) -> io::Error {
    io::Error::other(source.to_string())
}

#[cfg(not(target_os = "windows"))]
struct DiagnosticHostProcessTree;

#[cfg(not(target_os = "windows"))]
impl DiagnosticHostProcessTree {
    fn create_for_child(_child: &Child) -> Result<Self, DiagnosticChildSupervisorError> {
        Ok(Self)
    }

    fn terminate(&self) -> Result<bool, DiagnosticChildSupervisorError> {
        Ok(false)
    }

    fn release(&mut self) {}

    #[cfg(test)]
    fn empty_for_test() -> Self {
        Self
    }
}
