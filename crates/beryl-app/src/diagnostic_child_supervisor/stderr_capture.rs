use std::{
    io::{self, Read},
    process::ChildStderr,
    sync::{Arc, Mutex},
    thread,
    time::{Duration, Instant},
};

use tracing::{debug, warn};

use crate::acceptance_digest::Sha256;

const STDERR_PREFIX_LIMIT: usize = 4 * 1024;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct DiagnosticStderrSnapshot {
    pub(crate) total_bytes: u64,
    pub(crate) sha256: String,
    pub(crate) raw_prefix: Vec<u8>,
    pub(crate) truncated: bool,
    pub(crate) complete: bool,
}

impl Default for DiagnosticStderrSnapshot {
    fn default() -> Self {
        Self {
            total_bytes: 0,
            sha256: Sha256::digest_hex(&[]),
            raw_prefix: Vec::new(),
            truncated: false,
            complete: false,
        }
    }
}

pub(super) struct DiagnosticStderrCapture {
    state: Arc<Mutex<CaptureState>>,
    thread: Option<thread::JoinHandle<()>>,
}

pub(super) struct DiagnosticStderrCaptureSpawnError {
    source: io::Error,
    stderr: ChildStderr,
}

impl Default for DiagnosticStderrCapture {
    fn default() -> Self {
        Self {
            state: Arc::new(Mutex::new(CaptureState::default())),
            thread: None,
        }
    }
}

struct CaptureState {
    total_bytes: u64,
    sha256: Sha256,
    prefix: Vec<u8>,
    complete: bool,
}

impl Default for CaptureState {
    fn default() -> Self {
        Self {
            total_bytes: 0,
            sha256: Sha256::new(),
            prefix: Vec::new(),
            complete: false,
        }
    }
}

impl DiagnosticStderrCapture {
    pub(super) fn spawn(stderr: impl Read + Send + 'static) -> Self {
        let capture = Self::default();
        let state = Arc::clone(&capture.state);
        let thread = thread::spawn(move || run_stderr_capture(stderr, state));
        Self {
            thread: Some(thread),
            ..capture
        }
    }

    pub(super) fn spawn_child_fallible(
        stderr: ChildStderr,
    ) -> Result<Self, DiagnosticStderrCaptureSpawnError> {
        Self::spawn_child_fallible_with_forced_error(stderr, None)
    }

    #[cfg(test)]
    pub(super) fn force_child_spawn_failure(
        stderr: ChildStderr,
    ) -> Result<Self, DiagnosticStderrCaptureSpawnError> {
        Self::spawn_child_fallible_with_forced_error(
            stderr,
            Some(io::Error::other(
                "forced stderr reader spawn failure for test",
            )),
        )
    }

    fn spawn_child_fallible_with_forced_error(
        stderr: ChildStderr,
        forced_error: Option<io::Error>,
    ) -> Result<Self, DiagnosticStderrCaptureSpawnError> {
        let capture = Self::default();
        let state = Arc::clone(&capture.state);
        let stderr_owner = Arc::new(Mutex::new(Some(stderr)));
        let thread_stderr_owner = Arc::clone(&stderr_owner);
        let spawned = forced_error.map_or_else(
            || {
                thread::Builder::new()
                    .name("beryl-diagnostic-stderr".to_string())
                    .spawn(move || {
                        let stderr = thread_stderr_owner
                            .lock()
                            .expect("diagnostic stderr owner lock must remain available")
                            .take()
                            .expect("diagnostic stderr reader thread owns its pipe");
                        run_stderr_capture(stderr, state);
                    })
            },
            Err,
        );
        let thread = match spawned {
            Ok(thread) => thread,
            Err(source) => {
                let stderr = stderr_owner
                    .lock()
                    .expect("failed reader spawn leaves diagnostic stderr owner available")
                    .take()
                    .expect("failed reader spawn retains diagnostic stderr pipe");
                return Err(DiagnosticStderrCaptureSpawnError { source, stderr });
            }
        };
        Ok(Self {
            thread: Some(thread),
            ..capture
        })
    }

    pub(super) fn snapshot(&self) -> DiagnosticStderrSnapshot {
        let Ok(state) = self.state.lock() else {
            return DiagnosticStderrSnapshot::default();
        };
        DiagnosticStderrSnapshot {
            total_bytes: state.total_bytes,
            sha256: state.sha256.clone().finalize_hex(),
            raw_prefix: state.prefix.clone(),
            truncated: state.total_bytes > state.prefix.len() as u64,
            complete: state.complete,
        }
    }

    pub(super) fn join_after_child_reaped(
        &mut self,
    ) -> Result<(), super::DiagnosticChildSupervisorError> {
        let Some(thread) = self.thread.take() else {
            return Ok(());
        };
        thread
            .join()
            .map_err(|_| super::DiagnosticChildSupervisorError::StderrThreadPanicked)
    }

    pub(super) fn join_by(
        &mut self,
        deadline: Instant,
        timeout: Duration,
    ) -> Result<(), super::DiagnosticChildSupervisorError> {
        let Some(handle) = self.thread.as_ref() else {
            return Ok(());
        };
        while !handle.is_finished() {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Err(super::DiagnosticChildSupervisorError::RequestTimeout { timeout });
            }
            thread::sleep(remaining.min(Duration::from_millis(1)));
        }
        self.join_after_child_reaped()
    }
}

impl DiagnosticStderrCaptureSpawnError {
    pub(super) fn into_parts(self) -> (io::Error, ChildStderr) {
        (self.source, self.stderr)
    }
}

fn run_stderr_capture(mut stderr: impl Read, state: Arc<Mutex<CaptureState>>) {
    let mut buffer = [0_u8; 8 * 1024];
    let mut complete = false;
    loop {
        let read = match stderr.read(&mut buffer) {
            Ok(0) => {
                complete = true;
                break;
            }
            Ok(read) => read,
            Err(error) => {
                warn!(%error, "failed to read diagnostic child stderr");
                break;
            }
        };
        if let Ok(mut state) = state.lock() {
            state.total_bytes = state.total_bytes.saturating_add(read as u64);
            state.sha256.update(&buffer[..read]);
            let remaining = STDERR_PREFIX_LIMIT.saturating_sub(state.prefix.len());
            state
                .prefix
                .extend_from_slice(&buffer[..read.min(remaining)]);
        }
        let text = String::from_utf8_lossy(&buffer[..read]);
        if !text.trim().is_empty() {
            debug!(message = %bounded_log_text(&text), "diagnostic child stderr");
        }
    }
    if let Ok(mut state) = state.lock() {
        state.complete = complete;
    }
}

fn bounded_log_text(value: &str) -> String {
    value.chars().take(512).collect()
}
