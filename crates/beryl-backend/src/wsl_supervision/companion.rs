use std::{
    io::{Read, Write},
    process::{Child, ChildStderr, ChildStdin, ChildStdout},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, SyncSender, TrySendError},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

use crate::ManagedBackendError;
use beryl_wsl_supervisor::{Frame, FrameReader, MAX_PENDING_FRAMES, Nonce, encode_frame};
use wait_timeout::ChildExt;

type Pipe<T> = Arc<Mutex<Option<T>>>;

#[derive(Debug)]
pub(super) struct Companion {
    child: Child,
    stdin: Pipe<ChildStdin>,
    stdout: Pipe<ChildStdout>,
    stderr: Pipe<ChildStderr>,
    requests: Option<SyncSender<Vec<u8>>>,
    events: Option<Receiver<Result<Frame, ManagedBackendError>>>,
    writer: Option<JoinHandle<()>>,
    reader: Option<JoinHandle<()>>,
    diagnostics: Option<JoinHandle<()>>,
    io_failed: Arc<AtomicBool>,
    joined: bool,
    join_failed: bool,
}

impl Companion {
    pub(super) fn new(mut child: Child) -> Self {
        let stdin = Arc::new(Mutex::new(child.stdin.take()));
        let stdout = Arc::new(Mutex::new(child.stdout.take()));
        let stderr = Arc::new(Mutex::new(child.stderr.take()));
        Self {
            child,
            stdin,
            stdout,
            stderr,
            requests: None,
            events: None,
            writer: None,
            reader: None,
            diagnostics: None,
            io_failed: Arc::new(AtomicBool::new(false)),
            joined: false,
            join_failed: false,
        }
    }

    pub(super) fn prepare(&mut self, nonce: Nonce) -> Result<(), ManagedBackendError> {
        let (requests, request_rx) = mpsc::sync_channel::<Vec<u8>>(MAX_PENDING_FRAMES);
        self.requests = Some(requests);
        let stdin = self.stdin.clone();
        let failed = self.io_failed.clone();
        self.writer = Some(
            thread::Builder::new()
                .name("beryl-wsl-control-writer".into())
                .spawn(move || {
                    let Some(mut stdin) = take_pipe(&stdin) else {
                        failed.store(true, Ordering::Release);
                        return;
                    };
                    while let Ok(bytes) = request_rx.recv() {
                        if stdin
                            .write_all(&bytes)
                            .and_then(|()| stdin.flush())
                            .is_err()
                        {
                            break;
                        }
                    }
                })
                .map_err(|source| ManagedBackendError::WslCompanionIo { source })?,
        );
        let (events, event_rx) = mpsc::sync_channel(MAX_PENDING_FRAMES);
        self.events = Some(event_rx);
        let stdout = self.stdout.clone();
        let failed = self.io_failed.clone();
        self.reader = Some(
            thread::Builder::new()
                .name("beryl-wsl-control-reader".into())
                .spawn(move || {
                    let Some(mut stdout) = take_pipe(&stdout) else {
                        failed.store(true, Ordering::Release);
                        return;
                    };
                    let mut reader = FrameReader::new(nonce);
                    loop {
                        match reader.read_frame(&mut stdout) {
                            Ok(Some(frame)) => {
                                if events.try_send(Ok(frame)).is_err() {
                                    failed.store(true, Ordering::Release);
                                    break;
                                }
                            }
                            Ok(None) => break,
                            Err(_) => {
                                failed.store(true, Ordering::Release);
                                let _ = events
                                    .try_send(Err(ManagedBackendError::WslSupervisionUnavailable));
                                break;
                            }
                        }
                    }
                    let mut scratch = [0; 8192];
                    while stdout.read(&mut scratch).is_ok_and(|count| count != 0) {}
                })
                .map_err(|source| ManagedBackendError::WslCompanionIo { source })?,
        );
        let stderr = self.stderr.clone();
        let failed = self.io_failed.clone();
        self.diagnostics = Some(
            thread::Builder::new()
                .name("beryl-wsl-diagnostic-reader".into())
                .spawn(move || {
                    let Some(mut stderr) = take_pipe(&stderr) else {
                        failed.store(true, Ordering::Release);
                        return;
                    };
                    let mut scratch = [0; 4096];
                    loop {
                        match stderr.read(&mut scratch) {
                            Ok(0) => break,
                            Ok(_) => {}
                            Err(_) => {
                                failed.store(true, Ordering::Release);
                                break;
                            }
                        }
                    }
                })
                .map_err(|source| ManagedBackendError::WslCompanionIo { source })?,
        );
        Ok(())
    }

    pub(super) fn send(&self, nonce: &Nonce, frame: &Frame) -> Result<(), ManagedBackendError> {
        let bytes = encode_frame(nonce, frame)
            .map_err(|_| ManagedBackendError::WslSupervisionUnavailable)?;
        match self
            .requests
            .as_ref()
            .ok_or(ManagedBackendError::WslSupervisionUnavailable)?
            .try_send(bytes)
        {
            Ok(()) => Ok(()),
            Err(TrySendError::Full(_) | TrySendError::Disconnected(_)) => {
                Err(ManagedBackendError::WslSupervisionUnavailable)
            }
        }
    }

    pub(super) fn receive(
        &mut self,
        deadline: Instant,
        cancellation: Option<&AtomicBool>,
    ) -> Result<Frame, ManagedBackendError> {
        loop {
            if cancellation.is_some_and(|value| value.load(Ordering::Acquire)) {
                return Err(ManagedBackendError::WslObservationCancelled);
            }
            if self.io_failed.load(Ordering::Acquire) {
                return Err(ManagedBackendError::WslSupervisionUnavailable);
            }
            let remaining = deadline
                .checked_duration_since(Instant::now())
                .ok_or(ManagedBackendError::WslSupervisionTimeout)?;
            match self
                .events
                .as_ref()
                .ok_or(ManagedBackendError::WslSupervisionUnavailable)?
                .recv_timeout(remaining.min(Duration::from_millis(20)))
            {
                Ok(frame) => return frame,
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    return Err(ManagedBackendError::WslSupervisionUnavailable);
                }
            }
        }
    }

    pub(super) fn join(
        &mut self,
        deadline: Instant,
        broker_closed_record: bool,
    ) -> Result<(), ManagedBackendError> {
        if self.joined {
            return Ok(());
        }
        if self.join_failed {
            return Err(ManagedBackendError::WslCompanionJoin);
        }
        let remaining = deadline
            .checked_duration_since(Instant::now())
            .ok_or(ManagedBackendError::WslSupervisionTimeout)?;
        if self
            .child
            .wait_timeout(remaining)
            .map_err(|source| ManagedBackendError::WslCompanionIo { source })?
            .is_none()
        {
            return Err(ManagedBackendError::WslSupervisionTimeout);
        }
        self.requests.take();
        take_pipe(&self.stdin);
        take_pipe(&self.stdout);
        take_pipe(&self.stderr);
        for handle in [&mut self.writer, &mut self.reader, &mut self.diagnostics] {
            while handle.as_ref().is_some_and(|handle| !handle.is_finished()) {
                if Instant::now() >= deadline {
                    return Err(ManagedBackendError::WslSupervisionTimeout);
                }
                thread::sleep(Duration::from_millis(5));
            }
            if let Some(handle) = handle.take() {
                if handle.join().is_err() {
                    self.join_failed = true;
                    return Err(ManagedBackendError::WslCompanionJoin);
                }
            }
        }
        if self.io_failed.load(Ordering::Acquire) {
            return Err(ManagedBackendError::WslSupervisionUnavailable);
        }
        if let Some(events) = self.events.as_ref() {
            let mut broker_closed_seen = false;
            for event in events.try_iter() {
                match event? {
                    Frame::LinuxCompanionsClosed if broker_closed_record && !broker_closed_seen => {
                        broker_closed_seen = true
                    }
                    _ => {
                        self.io_failed.store(true, Ordering::Release);
                        return Err(ManagedBackendError::WslSupervisionUnavailable);
                    }
                }
            }
        }
        self.joined = true;
        Ok(())
    }

    pub(super) fn process_id(&self) -> u32 {
        self.child.id()
    }

    #[cfg(feature = "lifecycle-test-support")]
    pub(super) fn close_control_for_lifecycle_test(&mut self) {
        self.requests.take();
    }

    #[cfg(feature = "lifecycle-test-support")]
    pub(super) fn resource_custody_for_lifecycle_test(
        &self,
    ) -> crate::lifecycle_test_support::WslOwnedResourceCustodyForLifecycleTest {
        crate::lifecycle_test_support::WslOwnedResourceCustodyForLifecycleTest {
            pending_launcher_joins: usize::from(!self.joined),
            retained_control_readers: usize::from(self.reader.is_some()),
            retained_diagnostic_readers: usize::from(self.diagnostics.is_some()),
            retained_control_writers: usize::from(self.writer.is_some()),
        }
    }
    pub(super) fn has_exited(&mut self) -> bool {
        self.child.try_wait().is_ok_and(|status| status.is_some())
    }
}

fn take_pipe<T>(pipe: &Pipe<T>) -> Option<T> {
    pipe.lock().ok()?.take()
}
