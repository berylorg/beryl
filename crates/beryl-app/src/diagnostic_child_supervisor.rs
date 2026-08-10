#![allow(dead_code)]

use std::{
    io,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::mpsc::RecvTimeoutError,
    thread,
    time::{Duration, Instant},
};

use serde_json::Value;
use thiserror::Error;
use tracing::warn;

#[cfg(test)]
use std::{cell::RefCell, sync::mpsc::SyncSender as TestSyncSender};

use crate::{
    BerylHomeDir, BerylHomeDirError,
    diagnostic_child_protocol::{
        DIAGNOSTIC_CHILD_PROTOCOL_NAME, DIAGNOSTIC_CHILD_PROTOCOL_VERSION, DiagnosticChildCommand,
        DiagnosticProtocolError, DiagnosticProtocolErrorBody, request_frame,
    },
};

#[path = "diagnostic_acceptance_gate.rs"]
mod acceptance_gate;
#[path = "diagnostic_child_supervisor/launch.rs"]
mod launch;
#[path = "diagnostic_child_supervisor/process_tree.rs"]
mod process_tree;
#[path = "diagnostic_child_supervisor/stderr_capture.rs"]
mod stderr_capture;
#[cfg(test)]
#[path = "diagnostic_child_supervisor/test_support.rs"]
mod test_support;
#[path = "diagnostic_child_supervisor/transport.rs"]
mod transport;

#[allow(unused_imports)]
pub(crate) use launch::{
    DiagnosticChildLaunch, MAX_DIAGNOSTIC_CHILD_EXECUTABLE_PATH_BYTES,
    MAX_DIAGNOSTIC_CHILD_WORKSPACE_PATH_BYTES,
};
use process_tree::DiagnosticHostProcessTree;
use stderr_capture::DiagnosticStderrCapture;
pub(crate) use stderr_capture::DiagnosticStderrSnapshot;
#[cfg(test)]
use test_support::AcceptanceTestControl;
#[cfg(test)]
pub(crate) use test_support::{
    AcceptanceStartupFailureStage, AcceptanceTestObservation, AcceptanceTestPlan,
};
#[cfg(test)]
use transport::force_stdout_reader_spawn_failure;
use transport::{
    DiagnosticStdinWriter, DiagnosticStdoutEvent, DiagnosticStdoutReader, spawn_stdout_reader,
    spawn_stdout_reader_fallible,
};

const CHILD_SHUTDOWN_GRACE_TIMEOUT: Duration = Duration::from_millis(250);
const CHILD_KILL_TIMEOUT: Duration = Duration::from_secs(5);
pub(crate) const DIAGNOSTIC_CHILD_STOP_BUDGET: Duration = Duration::from_secs(11);
pub(crate) const DIAGNOSTIC_CHILD_STOP_RESPONSE_TIMEOUT: Duration = Duration::from_secs(12);
const DIAGNOSTIC_CHILD_STARTUP_RESPONSE_TIMEOUT: Duration = Duration::from_secs(5);
const CHILD_WAIT_POLL_INTERVAL: Duration = Duration::from_millis(25);

#[cfg(test)]
thread_local! {
    static CHILD_WAIT_POLL_OBSERVER: RefCell<Option<TestSyncSender<Duration>>> =
        const { RefCell::new(None) };
}

pub(crate) struct DiagnosticChildSupervisor {
    child: Option<DiagnosticChildProcess>,
    next_request_id: u64,
    last_stderr: DiagnosticStderrSnapshot,
    last_stop_method: &'static str,
    #[cfg(test)]
    last_cleanup_writer_joined_before_job_release: Option<bool>,
    #[cfg(test)]
    non_gate_write_marker: Option<PathBuf>,
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
    #[error("invalid diagnostic child host workspace path {path}: {reason}")]
    InvalidHostWorkspacePath { path: PathBuf, reason: &'static str },
    #[error("failed to inspect diagnostic child host workspace path {path}: {source}")]
    HostWorkspacePathAccess { path: PathBuf, source: io::Error },
    #[error("failed to spawn diagnostic child Beryl process from {executable_path}: {source}")]
    Spawn {
        executable_path: PathBuf,
        source: io::Error,
    },
    #[error("diagnostic child process did not expose piped stdin")]
    MissingStdin,
    #[error("diagnostic child process did not expose piped stdout")]
    MissingStdout,
    #[error("diagnostic child process did not expose piped stderr")]
    MissingStderr,
    #[error("failed to write diagnostic child protocol request: {source}")]
    WriteRequest { source: io::Error },
    #[error("diagnostic child stdin writer thread panicked during exact cleanup")]
    WriterThreadPanicked,
    #[error("diagnostic child stdout reader thread panicked during exact cleanup")]
    ReaderThreadPanicked,
    #[error("diagnostic child stderr reader thread panicked during exact cleanup")]
    StderrThreadPanicked,
    #[error("failed to spawn diagnostic child stdin writer: {source}")]
    SpawnWriter { source: io::Error },
    #[error("failed to spawn diagnostic child stdout reader: {source}")]
    SpawnStdoutReader { source: io::Error },
    #[error("failed to spawn diagnostic child stderr reader: {source}")]
    SpawnStderrReader { source: io::Error },
    #[error("diagnostic acceptance sessions require host Windows process ownership")]
    UnsupportedAcceptanceHost,
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
    stdin_writer: Option<DiagnosticStdinWriter>,
    stdout_reader: Option<DiagnosticStdoutReader>,
    host_process_tree: DiagnosticHostProcessTree,
    home_dir: BerylHomeDir,
    executable_path: PathBuf,
    stderr_capture: DiagnosticStderrCapture,
    cleanup_phase: &'static str,
    shutdown_method: &'static str,
    join_readers_on_cleanup: bool,
    #[cfg(test)]
    acceptance_test_control: Option<AcceptanceTestControl>,
    #[cfg(test)]
    writer_joined_before_job_release: Option<bool>,
}

pub(crate) struct DiagnosticAcceptanceStartupFailure {
    cause: DiagnosticChildSupervisorError,
    initial_cleanup_error: Option<DiagnosticChildSupervisorError>,
    owner: Option<DiagnosticAcceptanceProcessOwner>,
}

pub(crate) struct DiagnosticAcceptanceProcessOwner {
    process: Option<DiagnosticChildProcess>,
}

pub(crate) type DiagnosticAcceptanceStartupOwner = DiagnosticAcceptanceProcessOwner;

pub(crate) struct DiagnosticAcceptanceCleanupAttempt {
    pub(crate) identity: DiagnosticChildIdentity,
    pub(crate) phase: &'static str,
    pub(crate) termination_method: &'static str,
    pub(crate) error: Option<DiagnosticChildSupervisorError>,
    pub(crate) stderr: DiagnosticStderrSnapshot,
}

impl DiagnosticAcceptanceCleanupAttempt {
    pub(crate) fn reclaimed(&self) -> bool {
        self.error.is_none()
    }
}

pub(crate) enum DiagnosticAcceptanceCleanupRetry {
    Reclaimed(DiagnosticChildIdentity),
    StillRetained {
        identity: DiagnosticChildIdentity,
        error: DiagnosticChildSupervisorError,
    },
    AlreadyReclaimed,
}

pub(crate) struct SpawnedDiagnosticChildGuard {
    child: Option<Child>,
}

impl Default for DiagnosticChildSupervisor {
    fn default() -> Self {
        Self {
            child: None,
            next_request_id: 1,
            last_stderr: DiagnosticStderrSnapshot::default(),
            last_stop_method: "not_run",
            #[cfg(test)]
            last_cleanup_writer_joined_before_job_release: None,
            #[cfg(test)]
            non_gate_write_marker: None,
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

    pub(crate) fn start_with_startup_timeout(
        &mut self,
        supervisor_home: &BerylHomeDir,
        launch: DiagnosticChildLaunch,
        startup_timeout: Duration,
    ) -> Result<DiagnosticChildStartOutcome, DiagnosticChildSupervisorError> {
        self.start_with_optional_supervisor_home(Some(supervisor_home), launch, startup_timeout)
    }

    pub(crate) fn start_for_acceptance(
        &mut self,
        launch: DiagnosticChildLaunch,
        startup_timeout: Duration,
        cleanup_grace_timeout: Duration,
        cleanup_termination_timeout: Duration,
    ) -> Result<DiagnosticChildStartOutcome, DiagnosticAcceptanceStartupFailure> {
        #[cfg(not(target_os = "windows"))]
        return Err(DiagnosticAcceptanceStartupFailure::without_owner(
            DiagnosticChildSupervisorError::UnsupportedAcceptanceHost,
        ));

        #[cfg(target_os = "windows")]
        self.start_acceptance_gated(
            launch,
            startup_timeout,
            cleanup_grace_timeout,
            cleanup_termination_timeout,
            #[cfg(test)]
            None,
        )
    }

    #[cfg(test)]
    pub(crate) fn start_for_acceptance_with_test_plan(
        &mut self,
        launch: DiagnosticChildLaunch,
        startup_timeout: Duration,
        cleanup_grace_timeout: Duration,
        cleanup_termination_timeout: Duration,
        test_plan: AcceptanceTestPlan,
    ) -> Result<DiagnosticChildStartOutcome, DiagnosticAcceptanceStartupFailure> {
        #[cfg(not(target_os = "windows"))]
        {
            drop(test_plan);
            return self.start_for_acceptance(
                launch,
                startup_timeout,
                cleanup_grace_timeout,
                cleanup_termination_timeout,
            );
        }

        #[cfg(target_os = "windows")]
        self.start_acceptance_gated(
            launch,
            startup_timeout,
            cleanup_grace_timeout,
            cleanup_termination_timeout,
            Some(test_plan),
        )
    }

    fn start_with_optional_supervisor_home(
        &mut self,
        supervisor_home: Option<&BerylHomeDir>,
        launch: DiagnosticChildLaunch,
        startup_timeout: Duration,
    ) -> Result<DiagnosticChildStartOutcome, DiagnosticChildSupervisorError> {
        self.reap_observed_exit()?;
        if let Some(child) = self.child.as_ref() {
            return Ok(DiagnosticChildStartOutcome::AlreadyRunning(
                child.identity(),
            ));
        }

        let child_home = BerylHomeDir::from_explicit_path(launch.child_home().to_path_buf())?;
        if supervisor_home.is_some_and(|supervisor_home| {
            same_home_path(supervisor_home.root_dir(), child_home.root_dir())
        }) {
            let supervisor_home = supervisor_home
                .expect("diagnostic supervisor home was present after collision check");
            return Err(DiagnosticChildSupervisorError::HomeCollidesWithSupervisor {
                child_home: child_home.root_dir().to_path_buf(),
                supervisor_home: supervisor_home.root_dir().to_path_buf(),
            });
        }

        let executable_path = launch::resolve_executable_path(launch.executable_path())?;
        let host_workspace = launch
            .host_workspace()
            .map(launch::resolve_host_workspace)
            .transpose()?;
        let mut command = Command::new(&executable_path);
        command
            .arg("--diagnostic-target-stdio")
            .arg("--beryl-home-dir")
            .arg(child_home.root_dir());
        if let Some(host_workspace) = &host_workspace {
            command.arg("--host-path").arg(host_workspace);
        }
        command
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
        let stderr_capture = child_guard
            .child_mut()
            .stderr
            .take()
            .map(DiagnosticStderrCapture::spawn)
            .unwrap_or_default();

        let mut process = DiagnosticChildProcess {
            child: child_guard.into_child(),
            stdin_writer: Some(DiagnosticStdinWriter::spawn(stdin).map_err(|failure| {
                let (source, _stdin) = failure.into_parts();
                DiagnosticChildSupervisorError::SpawnWriter { source }
            })?),
            stdout_reader: Some(spawn_stdout_reader(stdout, false)),
            host_process_tree,
            home_dir: child_home,
            executable_path,
            stderr_capture,
            cleanup_phase: "not_started",
            shutdown_method: "not_run",
            join_readers_on_cleanup: false,
            #[cfg(test)]
            acceptance_test_control: None,
            #[cfg(test)]
            writer_joined_before_job_release: None,
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

    #[cfg(target_os = "windows")]
    fn start_acceptance_gated(
        &mut self,
        launch: DiagnosticChildLaunch,
        startup_timeout: Duration,
        cleanup_grace_timeout: Duration,
        cleanup_termination_timeout: Duration,
        #[cfg(test)] acceptance_test_plan: Option<AcceptanceTestPlan>,
    ) -> Result<DiagnosticChildStartOutcome, DiagnosticAcceptanceStartupFailure> {
        self.reap_observed_exit()
            .map_err(DiagnosticAcceptanceStartupFailure::without_owner)?;
        if let Some(child) = self.child.as_ref() {
            return Ok(DiagnosticChildStartOutcome::AlreadyRunning(
                child.identity(),
            ));
        }

        let child_home = BerylHomeDir::from_explicit_path(launch.child_home().to_path_buf())
            .map_err(|error| DiagnosticAcceptanceStartupFailure::without_owner(error.into()))?;
        let executable_path = launch::resolve_executable_path(launch.executable_path())
            .map_err(DiagnosticAcceptanceStartupFailure::without_owner)?;
        let host_workspace = launch
            .host_workspace()
            .map(launch::resolve_host_workspace)
            .transpose()
            .map_err(DiagnosticAcceptanceStartupFailure::without_owner)?;
        let mut command = Command::new(&executable_path);
        command
            .arg("--diagnostic-target-stdio")
            .arg("--beryl-home-dir")
            .arg(child_home.root_dir())
            .arg("--diagnostic-acceptance-startup-gate");
        if let Some(host_workspace) = &host_workspace {
            command.arg("--host-path").arg(host_workspace);
        }
        command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let child = command.spawn().map_err(|source| {
            DiagnosticAcceptanceStartupFailure::without_owner(
                DiagnosticChildSupervisorError::Spawn {
                    executable_path: executable_path.clone(),
                    source,
                },
            )
        })?;
        let mut process = DiagnosticChildProcess::new_startup(
            child,
            child_home,
            executable_path,
            #[cfg(test)]
            acceptance_test_plan,
        );
        #[cfg(test)]
        process.run_acceptance_spawn_barrier();
        let (host_process_tree, job_error) = DiagnosticHostProcessTree::create_for_child_retaining(
            &process.child,
            #[cfg(test)]
            process.acceptance_test_control.as_mut(),
        );
        process.host_process_tree = host_process_tree;
        if let Some(error) = job_error {
            return Err(Self::fail_acceptance_startup(
                process,
                error,
                cleanup_grace_timeout,
                cleanup_termination_timeout,
            ));
        }

        let Some(stdin) = process.child.stdin.take() else {
            return Err(Self::fail_acceptance_startup(
                process,
                DiagnosticChildSupervisorError::MissingStdin,
                cleanup_grace_timeout,
                cleanup_termination_timeout,
            ));
        };
        #[cfg(test)]
        let forced_writer_spawn_failure =
            process.force_acceptance_startup_failure(AcceptanceStartupFailureStage::WriterSpawn);
        #[cfg(test)]
        let writer = if forced_writer_spawn_failure {
            Err(DiagnosticStdinWriter::forced_spawn_failure(stdin))
        } else {
            DiagnosticStdinWriter::spawn(stdin)
        };
        #[cfg(not(test))]
        let writer = DiagnosticStdinWriter::spawn(stdin);
        process.stdin_writer = match writer {
            Ok(writer) => {
                #[cfg(test)]
                if let Some(marker) = self.non_gate_write_marker.clone() {
                    writer.mark_non_gate_writes_for_test(marker);
                }
                Some(writer)
            }
            Err(failure) => {
                let (source, stdin) = failure.into_parts();
                process.child.stdin = Some(stdin);
                return Err(Self::fail_acceptance_startup(
                    process,
                    DiagnosticChildSupervisorError::SpawnWriter { source },
                    cleanup_grace_timeout,
                    cleanup_termination_timeout,
                ));
            }
        };
        let Some(stdout) = process.child.stdout.take() else {
            return Err(Self::fail_acceptance_startup(
                process,
                DiagnosticChildSupervisorError::MissingStdout,
                cleanup_grace_timeout,
                cleanup_termination_timeout,
            ));
        };
        #[cfg(test)]
        let forced_stdout_reader_failure = process
            .force_acceptance_startup_failure(AcceptanceStartupFailureStage::StdoutReaderSpawn);
        #[cfg(test)]
        let stdout_reader = if forced_stdout_reader_failure {
            force_stdout_reader_spawn_failure(stdout, true)
        } else {
            spawn_stdout_reader_fallible(stdout, true)
        };
        #[cfg(not(test))]
        let stdout_reader = spawn_stdout_reader_fallible(stdout, true);
        process.stdout_reader = match stdout_reader {
            Ok(reader) => Some(reader),
            Err(failure) => {
                let (source, stdout) = failure.into_parts();
                process.child.stdout = Some(stdout);
                return Err(Self::fail_acceptance_startup(
                    process,
                    DiagnosticChildSupervisorError::SpawnStdoutReader { source },
                    cleanup_grace_timeout,
                    cleanup_termination_timeout,
                ));
            }
        };
        let Some(stderr) = process.child.stderr.take() else {
            process.stderr_capture = DiagnosticStderrCapture::default();
            return Err(Self::fail_acceptance_startup(
                process,
                DiagnosticChildSupervisorError::MissingStderr,
                cleanup_grace_timeout,
                cleanup_termination_timeout,
            ));
        };
        #[cfg(test)]
        let forced_stderr_reader_failure = process
            .force_acceptance_startup_failure(AcceptanceStartupFailureStage::StderrReaderSpawn);
        #[cfg(test)]
        let stderr_capture = if forced_stderr_reader_failure {
            DiagnosticStderrCapture::force_child_spawn_failure(stderr)
        } else {
            DiagnosticStderrCapture::spawn_child_fallible(stderr)
        };
        #[cfg(not(test))]
        let stderr_capture = DiagnosticStderrCapture::spawn_child_fallible(stderr);
        process.stderr_capture = match stderr_capture {
            Ok(capture) => capture,
            Err(failure) => {
                let (source, stderr) = failure.into_parts();
                process.child.stderr = Some(stderr);
                return Err(Self::fail_acceptance_startup(
                    process,
                    DiagnosticChildSupervisorError::SpawnStderrReader { source },
                    cleanup_grace_timeout,
                    cleanup_termination_timeout,
                ));
            }
        };

        let gate_deadline = Instant::now() + startup_timeout;
        #[cfg(test)]
        let forced_gate_write_failure =
            process.force_acceptance_startup_failure(AcceptanceStartupFailureStage::GateWrite);
        #[cfg(not(test))]
        let forced_gate_write_failure = false;
        let gate_result = if forced_gate_write_failure {
            Err(DiagnosticChildSupervisorError::WriteRequest {
                source: io::Error::other("forced gate write failure for test"),
            })
        } else {
            process
                .stdin_writer
                .as_ref()
                .expect("acceptance startup writer was installed")
                .write_frame(
                    acceptance_gate::DIAGNOSTIC_ACCEPTANCE_STARTUP_GATE_FRAME.to_vec(),
                    gate_deadline,
                    startup_timeout,
                )
                .and_then(|()| {
                    #[cfg(test)]
                    process.observe_gate_write_completed_for_test();
                    #[cfg(test)]
                    let forced_gate_ready_failure = process
                        .force_acceptance_startup_failure(AcceptanceStartupFailureStage::GateReady);
                    #[cfg(not(test))]
                    let forced_gate_ready_failure = false;
                    if forced_gate_ready_failure {
                        Err(
                            DiagnosticChildSupervisorError::StartupProtocolIncompatible {
                                message: "forced startup ready failure for test".to_string(),
                            },
                        )
                    } else {
                        process
                            .wait_for_acceptance_gate_ready(gate_deadline, startup_timeout)
                            .map_err(startup_protocol_error)
                    }
                })
        };
        if let Err(error) = gate_result {
            return Err(Self::fail_acceptance_startup(
                process,
                error,
                cleanup_grace_timeout,
                cleanup_termination_timeout,
            ));
        }
        let request_id = self.next_request_id();
        #[cfg(test)]
        let forced_handshake_failure =
            process.force_acceptance_startup_failure(AcceptanceStartupFailureStage::Handshake);
        #[cfg(not(test))]
        let forced_handshake_failure = false;
        let handshake_result = if forced_handshake_failure {
            Err(
                DiagnosticChildSupervisorError::StartupProtocolIncompatible {
                    message: "forced startup handshake failure for test".to_string(),
                },
            )
        } else {
            process.verify_startup_protocol(&request_id, startup_timeout)
        };
        if let Err(error) = handshake_result {
            return Err(Self::fail_acceptance_startup(
                process,
                error,
                cleanup_grace_timeout,
                cleanup_termination_timeout,
            ));
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

    pub(crate) fn stop_with_timeouts(
        &mut self,
        grace_timeout: Duration,
        kill_timeout: Duration,
    ) -> Result<DiagnosticChildStopOutcome, DiagnosticChildSupervisorError> {
        self.stop_with_shutdown(|child| child.shutdown(grace_timeout, kill_timeout))
    }

    pub(crate) fn into_acceptance_process_owner(
        mut self,
    ) -> Option<DiagnosticAcceptanceProcessOwner> {
        self.child
            .take()
            .map(|process| DiagnosticAcceptanceProcessOwner {
                process: Some(process),
            })
    }

    fn stop_with_shutdown(
        &mut self,
        shutdown: impl FnOnce(&mut DiagnosticChildProcess) -> Result<(), DiagnosticChildSupervisorError>,
    ) -> Result<DiagnosticChildStopOutcome, DiagnosticChildSupervisorError> {
        let Some(mut child) = self.child.take() else {
            self.last_stop_method = "not_running";
            return Ok(DiagnosticChildStopOutcome::NotRunning);
        };
        let identity = child.identity();
        let result = match shutdown(&mut child) {
            Ok(()) => Ok(DiagnosticChildStopOutcome::Stopped(identity)),
            Err(error) => {
                self.child = Some(child);
                return Err(error);
            }
        };
        self.last_stderr = child.stderr_capture.snapshot();
        self.last_stop_method = child.shutdown_method;
        #[cfg(test)]
        {
            self.last_cleanup_writer_joined_before_job_release =
                child.writer_joined_before_job_release;
        }
        result
    }

    pub(crate) fn status(
        &mut self,
    ) -> Result<DiagnosticChildStatus, DiagnosticChildSupervisorError> {
        self.reap_observed_exit()?;
        Ok(self
            .child
            .as_ref()
            .map(|child| DiagnosticChildStatus::Running(child.identity()))
            .unwrap_or(DiagnosticChildStatus::NotRunning))
    }

    pub(crate) fn stderr_snapshot(&self) -> DiagnosticStderrSnapshot {
        self.child
            .as_ref()
            .map(|child| child.stderr_capture.snapshot())
            .unwrap_or_else(|| self.last_stderr.clone())
    }

    pub(crate) fn last_stop_method(&self) -> &'static str {
        self.last_stop_method
    }

    pub(crate) fn request(
        &mut self,
        command: DiagnosticChildCommand,
        params: Value,
        timeout: Duration,
    ) -> Result<Value, DiagnosticChildSupervisorError> {
        self.request_with_id(command, params, timeout).1
    }

    pub(crate) fn request_until(
        &mut self,
        command: DiagnosticChildCommand,
        params: Value,
        deadline: Instant,
    ) -> Result<Value, DiagnosticChildSupervisorError> {
        self.request_with_id_until(command, params, deadline).1
    }

    pub(crate) fn request_with_id(
        &mut self,
        command: DiagnosticChildCommand,
        params: Value,
        timeout: Duration,
    ) -> (
        Option<String>,
        Result<Value, DiagnosticChildSupervisorError>,
    ) {
        if let Err(error) = self.reap_observed_exit() {
            return (None, Err(error));
        }
        self.request_with_id_retaining_observed_exit(command, params, timeout)
    }

    pub(crate) fn request_with_id_until(
        &mut self,
        command: DiagnosticChildCommand,
        params: Value,
        deadline: Instant,
    ) -> (
        Option<String>,
        Result<Value, DiagnosticChildSupervisorError>,
    ) {
        if let Err(error) = self.reap_observed_exit() {
            return (None, Err(error));
        }
        self.request_with_id_retaining_observed_exit_until(command, params, deadline)
    }

    pub(crate) fn request_with_id_retaining_observed_exit(
        &mut self,
        command: DiagnosticChildCommand,
        params: Value,
        timeout: Duration,
    ) -> (
        Option<String>,
        Result<Value, DiagnosticChildSupervisorError>,
    ) {
        let deadline = Instant::now() + timeout;
        self.request_with_id_retaining_observed_exit_by_deadline(command, params, deadline, timeout)
    }

    pub(crate) fn request_with_id_retaining_observed_exit_until(
        &mut self,
        command: DiagnosticChildCommand,
        params: Value,
        deadline: Instant,
    ) -> (
        Option<String>,
        Result<Value, DiagnosticChildSupervisorError>,
    ) {
        let timeout = deadline.saturating_duration_since(Instant::now());
        self.request_with_id_retaining_observed_exit_by_deadline(command, params, deadline, timeout)
    }

    fn request_with_id_retaining_observed_exit_by_deadline(
        &mut self,
        command: DiagnosticChildCommand,
        params: Value,
        deadline: Instant,
        timeout: Duration,
    ) -> (
        Option<String>,
        Result<Value, DiagnosticChildSupervisorError>,
    ) {
        let request_id = self.next_request_id();
        let Some(child) = self.child.as_mut() else {
            return (
                Some(request_id),
                Err(DiagnosticChildSupervisorError::ProtocolEof),
            );
        };

        let result = child.request_until(&request_id, command, params, deadline, timeout);
        (Some(request_id), result)
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
    pub(crate) fn adopt_acceptance_child_for_test(
        &mut self,
        child: Child,
        home_dir: BerylHomeDir,
        executable_path: PathBuf,
    ) -> Result<(), DiagnosticChildSupervisorError> {
        let mut process =
            DiagnosticChildProcess::from_child_for_test(child, home_dir, executable_path)?;
        process.join_readers_on_cleanup = true;
        self.child = Some(process);
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

    #[cfg(test)]
    pub(crate) fn wait_for_child_exit_for_test(
        &mut self,
        timeout: Duration,
    ) -> Result<bool, DiagnosticChildSupervisorError> {
        let Some(child) = self.child.as_mut() else {
            return Ok(true);
        };
        wait_for_exit(&mut child.child, timeout)
    }

    #[cfg(test)]
    pub(crate) fn stdin_writer_is_finished_for_test(&self) -> Option<bool> {
        self.child
            .as_ref()
            .and_then(|child| child.stdin_writer.as_ref())
            .map(DiagnosticStdinWriter::thread_is_finished_for_test)
    }

    #[cfg(test)]
    pub(crate) fn delay_next_write_for_test(&self, delay: Duration) {
        self.child
            .as_ref()
            .and_then(|child| child.stdin_writer.as_ref())
            .expect("diagnostic child has an owned stdin writer")
            .delay_next_write_for_test(delay);
    }

    #[cfg(test)]
    pub(crate) fn mark_non_gate_writes_for_test(&mut self, marker: PathBuf) {
        self.non_gate_write_marker = Some(marker);
    }

    #[cfg(test)]
    pub(crate) fn last_cleanup_writer_joined_before_job_release_for_test(&self) -> Option<bool> {
        self.last_cleanup_writer_joined_before_job_release
    }

    fn reap_observed_exit(&mut self) -> Result<(), DiagnosticChildSupervisorError> {
        let Some(child) = self.child.as_mut() else {
            return Ok(());
        };
        if !child.has_exited()? {
            return Ok(());
        }
        child.finish_reaped("observed_exit")?;
        let child = self
            .child
            .take()
            .expect("observed diagnostic child remained owned until reap completion");
        self.last_stderr = child.stderr_capture.snapshot();
        self.last_stop_method = child.shutdown_method;
        #[cfg(test)]
        {
            self.last_cleanup_writer_joined_before_job_release =
                child.writer_joined_before_job_release;
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

    fn fail_acceptance_startup(
        mut process: DiagnosticChildProcess,
        cause: DiagnosticChildSupervisorError,
        cleanup_grace_timeout: Duration,
        cleanup_termination_timeout: Duration,
    ) -> DiagnosticAcceptanceStartupFailure {
        match process.shutdown(cleanup_grace_timeout, cleanup_termination_timeout) {
            Ok(()) => DiagnosticAcceptanceStartupFailure {
                cause,
                initial_cleanup_error: None,
                owner: None,
            },
            Err(initial_cleanup_error) => {
                warn!(
                    %cause,
                    %initial_cleanup_error,
                    "acceptance startup cleanup remained indeterminate; transferring exact owner"
                );
                DiagnosticAcceptanceStartupFailure {
                    cause,
                    initial_cleanup_error: Some(initial_cleanup_error),
                    owner: Some(DiagnosticAcceptanceProcessOwner {
                        process: Some(process),
                    }),
                }
            }
        }
    }
}

impl Drop for DiagnosticChildSupervisor {
    fn drop(&mut self) {
        if let Err(error) = self.stop() {
            warn!(%error, "failed to drop diagnostic child process");
        }
    }
}

impl DiagnosticAcceptanceStartupFailure {
    fn without_owner(cause: DiagnosticChildSupervisorError) -> Self {
        Self {
            cause,
            initial_cleanup_error: None,
            owner: None,
        }
    }

    pub(crate) fn into_parts(
        self,
    ) -> (
        DiagnosticChildSupervisorError,
        Option<DiagnosticChildSupervisorError>,
        Option<DiagnosticAcceptanceProcessOwner>,
    ) {
        (self.cause, self.initial_cleanup_error, self.owner)
    }

    #[cfg(test)]
    pub(crate) fn cause_for_test(&self) -> &DiagnosticChildSupervisorError {
        &self.cause
    }
}

impl DiagnosticAcceptanceProcessOwner {
    pub(crate) fn identity(&self) -> DiagnosticChildIdentity {
        self.process
            .as_ref()
            .expect("retained acceptance startup owner contains its process")
            .identity()
    }

    pub(crate) fn retry_cleanup(
        &mut self,
        grace_timeout: Duration,
        termination_timeout: Duration,
    ) -> DiagnosticAcceptanceCleanupRetry {
        let Some(process) = self.process.as_mut() else {
            return DiagnosticAcceptanceCleanupRetry::AlreadyReclaimed;
        };
        let identity = process.identity();
        match process.shutdown(grace_timeout, termination_timeout) {
            Ok(()) => {
                drop(self.process.take());
                DiagnosticAcceptanceCleanupRetry::Reclaimed(identity)
            }
            Err(error) => DiagnosticAcceptanceCleanupRetry::StillRetained { identity, error },
        }
    }

    pub(crate) fn cleanup_attempt(
        &mut self,
        grace_timeout: Duration,
        termination_timeout: Duration,
    ) -> Option<DiagnosticAcceptanceCleanupAttempt> {
        let process = self.process.as_mut()?;
        let identity = process.identity();
        let error = process.shutdown(grace_timeout, termination_timeout).err();
        let attempt = DiagnosticAcceptanceCleanupAttempt {
            identity,
            phase: process.cleanup_phase,
            termination_method: process.shutdown_method,
            error,
            stderr: process.stderr_capture.snapshot(),
        };
        if attempt.reclaimed() {
            drop(self.process.take());
        }
        Some(attempt)
    }

    pub(crate) fn release_fail_safe_nonblocking(&mut self) -> Option<DiagnosticChildIdentity> {
        let mut process = self.process.take()?;
        let identity = process.identity();
        process.fail_safe_release_nonblocking();
        Some(identity)
    }

    #[cfg(test)]
    pub(crate) fn owns_job_for_test(&self) -> bool {
        self.process
            .as_ref()
            .is_some_and(|process| process.host_process_tree.has_job_for_test())
    }

    #[cfg(test)]
    pub(crate) fn owns_raw_stdin_for_test(&self) -> bool {
        self.process
            .as_ref()
            .is_some_and(|process| process.child.stdin.is_some())
    }

    #[cfg(test)]
    pub(crate) fn owns_raw_stdout_for_test(&self) -> bool {
        self.process
            .as_ref()
            .is_some_and(|process| process.child.stdout.is_some())
    }

    #[cfg(test)]
    pub(crate) fn owns_raw_stderr_for_test(&self) -> bool {
        self.process
            .as_ref()
            .is_some_and(|process| process.child.stderr.is_some())
    }

    #[cfg(test)]
    pub(crate) fn force_writer_join_timeout_once_for_test(&mut self) {
        self.process
            .as_mut()
            .and_then(|process| process.stdin_writer.as_mut())
            .expect("retained acceptance owner has a stdin writer")
            .force_join_timeout_once_for_test();
    }
}

impl Drop for DiagnosticAcceptanceProcessOwner {
    fn drop(&mut self) {
        if let Some(mut process) = self.process.take() {
            process.fail_safe_release_nonblocking();
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
    fn new_startup(
        child: Child,
        home_dir: BerylHomeDir,
        executable_path: PathBuf,
        #[cfg(test)] acceptance_test_plan: Option<AcceptanceTestPlan>,
    ) -> Self {
        Self {
            child,
            stdin_writer: None,
            stdout_reader: None,
            host_process_tree: DiagnosticHostProcessTree::empty(),
            home_dir,
            executable_path,
            stderr_capture: DiagnosticStderrCapture::default(),
            cleanup_phase: "not_started",
            shutdown_method: "not_run",
            join_readers_on_cleanup: true,
            #[cfg(test)]
            acceptance_test_control: acceptance_test_plan.map(AcceptanceTestControl::new),
            #[cfg(test)]
            writer_joined_before_job_release: None,
        }
    }

    #[cfg(test)]
    fn run_acceptance_spawn_barrier(&mut self) {
        if let Some(control) = self.acceptance_test_control.as_mut() {
            control.run_spawn_barrier(self.child.id());
        }
    }

    #[cfg(test)]
    fn force_acceptance_startup_failure(&mut self, stage: AcceptanceStartupFailureStage) -> bool {
        let pid = self.child.id();
        self.acceptance_test_control
            .as_mut()
            .is_some_and(|control| control.force_startup_failure(pid, stage))
    }

    #[cfg(test)]
    fn observe_gate_write_completed_for_test(&self) {
        if let Some(control) = self.acceptance_test_control.as_ref() {
            control.observe_gate_write_completed(self.child.id());
        }
    }

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
            stdin_writer: Some(DiagnosticStdinWriter::spawn(stdin).map_err(|failure| {
                let (source, _stdin) = failure.into_parts();
                DiagnosticChildSupervisorError::SpawnWriter { source }
            })?),
            stdout_reader: Some(spawn_stdout_reader(stdout, false)),
            host_process_tree: DiagnosticHostProcessTree::empty_for_test(),
            home_dir,
            executable_path,
            stderr_capture: DiagnosticStderrCapture::default(),
            cleanup_phase: "not_started",
            shutdown_method: "not_run",
            join_readers_on_cleanup: false,
            #[cfg(test)]
            acceptance_test_control: None,
            #[cfg(test)]
            writer_joined_before_job_release: None,
        })
    }

    fn request(
        &mut self,
        request_id: &str,
        command: DiagnosticChildCommand,
        params: Value,
        timeout: Duration,
    ) -> Result<Value, DiagnosticChildSupervisorError> {
        let deadline = Instant::now() + timeout;
        self.request_until(request_id, command, params, deadline, timeout)
    }

    fn request_until(
        &mut self,
        request_id: &str,
        command: DiagnosticChildCommand,
        params: Value,
        deadline: Instant,
        timeout: Duration,
    ) -> Result<Value, DiagnosticChildSupervisorError> {
        let frame = request_frame(request_id, command, params)?;
        self.stdin_writer
            .as_ref()
            .expect("active diagnostic process owns its stdin writer")
            .write_frame(frame, deadline, timeout)?;
        loop {
            let Some(remaining) = deadline.checked_duration_since(Instant::now()) else {
                return Err(DiagnosticChildSupervisorError::RequestTimeout { timeout });
            };
            match self
                .stdout_reader
                .as_ref()
                .expect("active diagnostic process owns its stdout reader")
                .recv_timeout(remaining)
            {
                Ok(Ok(DiagnosticStdoutEvent::Response(response))) => {
                    if response.id() != Some(request_id) {
                        continue;
                    }
                    return response.into_result().map_err(child_protocol_error);
                }
                Ok(Ok(DiagnosticStdoutEvent::AcceptanceGateReady)) => continue,
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

    fn wait_for_acceptance_gate_ready(
        &mut self,
        deadline: Instant,
        timeout: Duration,
    ) -> Result<(), DiagnosticChildSupervisorError> {
        let remaining = deadline
            .checked_duration_since(Instant::now())
            .ok_or(DiagnosticChildSupervisorError::RequestTimeout { timeout })?;
        match self
            .stdout_reader
            .as_ref()
            .expect("acceptance startup owns its stdout reader")
            .recv_timeout(remaining)
        {
            Ok(Ok(DiagnosticStdoutEvent::AcceptanceGateReady)) => Ok(()),
            Ok(Ok(DiagnosticStdoutEvent::Response(_))) => Err(
                DiagnosticChildSupervisorError::StartupProtocolIncompatible {
                    message: "diagnostic acceptance target skipped its startup ready frame"
                        .to_string(),
                },
            ),
            Ok(Err(error)) => Err(DiagnosticChildSupervisorError::Protocol(error)),
            Err(RecvTimeoutError::Timeout) => {
                Err(DiagnosticChildSupervisorError::RequestTimeout { timeout })
            }
            Err(RecvTimeoutError::Disconnected) => Err(DiagnosticChildSupervisorError::ProtocolEof),
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
        self.cleanup_phase = "initializing";
        self.shutdown_method = "none";
        #[cfg(test)]
        if self
            .acceptance_test_control
            .as_mut()
            .is_some_and(|control| control.begin_cleanup_attempt(self.child.id()))
        {
            self.cleanup_phase = "forced_failure";
            return Err(DiagnosticChildSupervisorError::RequestTimeout {
                timeout: grace_timeout.saturating_add(kill_timeout.saturating_mul(2)),
            });
        }
        let cleanup_timeout = grace_timeout.saturating_add(kill_timeout.saturating_mul(2));
        let cleanup_deadline = Instant::now() + cleanup_timeout;
        drop(self.child.stdin.take());
        self.cleanup_phase = "observed_exit_check";
        if self.has_exited()? {
            self.shutdown_method = "observed_exit";
            return self.finish_reaped_bounded("observed_exit", cleanup_deadline, cleanup_timeout);
        }
        if let Some(stdin_writer) = self.stdin_writer.as_mut() {
            stdin_writer.close();
        }
        self.cleanup_phase = "graceful_eof_wait";
        if wait_for_exit(&mut self.child, grace_timeout)? {
            self.shutdown_method = "graceful_eof";
            return self.finish_reaped_bounded("graceful_eof", cleanup_deadline, cleanup_timeout);
        }
        self.cleanup_phase = "direct_child_termination";
        match self.child.kill() {
            Ok(()) => {
                self.shutdown_method = "direct_kill";
                if wait_for_exit(&mut self.child, kill_timeout)? {
                    return self.finish_reaped_bounded(
                        "direct_kill",
                        cleanup_deadline,
                        cleanup_timeout,
                    );
                }
            }
            Err(source) if source.kind() == io::ErrorKind::InvalidInput => {
                if wait_for_exit(&mut self.child, Duration::ZERO)? {
                    self.shutdown_method = "already_exited";
                    return self.finish_reaped_bounded(
                        "already_exited",
                        cleanup_deadline,
                        cleanup_timeout,
                    );
                }
                return Err(DiagnosticChildSupervisorError::Terminate { source });
            }
            Err(source) => return Err(DiagnosticChildSupervisorError::Terminate { source }),
        }

        self.cleanup_phase = "exact_process_tree_termination";
        self.shutdown_method = "job_termination";
        if self.host_process_tree.terminate()? && wait_for_exit(&mut self.child, kill_timeout)? {
            return self.finish_reaped_bounded(
                "job_termination",
                cleanup_deadline,
                cleanup_timeout,
            );
        }

        Err(DiagnosticChildSupervisorError::RequestTimeout {
            timeout: kill_timeout,
        })
    }

    fn finish_reaped(
        &mut self,
        shutdown_method: &'static str,
    ) -> Result<(), DiagnosticChildSupervisorError> {
        self.cleanup_phase = "transport_join";
        // Terminating the retained Job closes inherited pipe handles held by descendants.
        // Keep the exact Job owner until every owned transport thread has joined so a
        // failed join can be retried without weakening process-tree identity.
        #[cfg(test)]
        let job_was_owned = self.host_process_tree.has_job_for_test();
        self.host_process_tree.terminate()?;
        if let Some(stdin_writer) = self.stdin_writer.as_mut() {
            stdin_writer.join_after_child_reaped()?;
            #[cfg(test)]
            if job_was_owned {
                self.writer_joined_before_job_release = Some(
                    job_was_owned
                        && self.host_process_tree.has_job_for_test()
                        && stdin_writer.joined_for_test(),
                );
            }
        }
        if self.join_readers_on_cleanup {
            if let Some(stdout_reader) = self.stdout_reader.as_mut() {
                stdout_reader.join_after_child_reaped()?;
            }
            self.stderr_capture.join_after_child_reaped()?;
        }
        self.host_process_tree.release();
        self.cleanup_phase = "complete";
        self.shutdown_method = shutdown_method;
        Ok(())
    }

    fn finish_reaped_bounded(
        &mut self,
        shutdown_method: &'static str,
        deadline: Instant,
        timeout: Duration,
    ) -> Result<(), DiagnosticChildSupervisorError> {
        self.cleanup_phase = "transport_join";
        if !self.join_readers_on_cleanup {
            return self.finish_reaped(shutdown_method);
        }
        #[cfg(test)]
        let job_was_owned = self.host_process_tree.has_job_for_test();
        self.host_process_tree.terminate()?;
        if let Some(stdin_writer) = self.stdin_writer.as_mut() {
            stdin_writer.join_by(deadline, timeout)?;
            #[cfg(test)]
            if job_was_owned {
                self.writer_joined_before_job_release = Some(
                    job_was_owned
                        && self.host_process_tree.has_job_for_test()
                        && stdin_writer.joined_for_test(),
                );
            }
        }
        if let Some(stdout_reader) = self.stdout_reader.as_mut() {
            stdout_reader.join_by(deadline, timeout)?;
        }
        self.stderr_capture.join_by(deadline, timeout)?;
        self.host_process_tree.release();
        self.cleanup_phase = "complete";
        self.shutdown_method = shutdown_method;
        Ok(())
    }

    fn fail_safe_release_nonblocking(&mut self) {
        #[cfg(test)]
        if let Some(control) = self.acceptance_test_control.as_ref() {
            control.observe_fail_safe_release(self.child.id());
        }
        if let Some(stdin_writer) = self.stdin_writer.as_mut() {
            stdin_writer.close();
        }
        let _ = self.child.kill();
        self.host_process_tree.release();
    }
}

#[cfg(test)]
pub(crate) fn install_child_wait_poll_observer_for_test(observer: TestSyncSender<Duration>) {
    CHILD_WAIT_POLL_OBSERVER.with(|installed| {
        *installed.borrow_mut() = Some(observer);
    });
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
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Ok(false);
        }
        sleep_for_child_wait_poll(remaining.min(CHILD_WAIT_POLL_INTERVAL));
    }
}

fn sleep_for_child_wait_poll(duration: Duration) {
    #[cfg(test)]
    CHILD_WAIT_POLL_OBSERVER.with(|installed| {
        if let Some(observer) = installed.borrow_mut().take() {
            let _ = observer.try_send(duration);
        }
    });
    thread::sleep(duration);
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
