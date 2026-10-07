use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, TryRecvError},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

use super::{
    ValidationError, ValidationIssue, check_running,
    seams::{ValidationCleanup, ValidationFilesystemPath},
};
use beryl_home_store::CommandCancellation;

const JOIN_GRACE: Duration = Duration::from_secs(5);
const POLL_INTERVAL: Duration = Duration::from_millis(10);

pub(super) enum HostObservation {
    Executable(PathBuf),
    Directory(PathBuf),
    Home,
}

#[cfg(feature = "test-faults")]
#[derive(Default)]
pub struct FilesystemWorkerTestControl {
    pub after_result_gate: Option<Arc<AtomicBool>>,
    pub result_sent: Option<Arc<AtomicBool>>,
    pub result_received: Option<Arc<AtomicBool>>,
    pub join_grace: Option<Duration>,
}

pub(super) fn observe(
    operation: HostObservation,
    deadline: Instant,
    cancellation: &CommandCancellation,
    path_bytes: usize,
) -> Result<ValidationFilesystemPath, ValidationError> {
    observe_with(deadline, cancellation, move |signal| match operation {
        HostObservation::Executable(path) => {
            super::filesystem::host_path(&path, true, deadline, &signal, path_bytes)
        }
        HostObservation::Directory(path) => {
            super::filesystem::host_path(&path, false, deadline, &signal, path_bytes)
        }
        HostObservation::Home => (|| {
            check_running(deadline, &signal)?;
            let home = super::filesystem::host_home(path_bytes)?;
            check_running(deadline, &signal)?;
            super::filesystem::host_path(&home, false, deadline, &signal, path_bytes)
        })(),
    })
}

pub(super) fn observe_with(
    deadline: Instant,
    cancellation: &CommandCancellation,
    operation: impl FnOnce(CommandCancellation) -> Result<ValidationFilesystemPath, ValidationError>
    + Send
    + 'static,
) -> Result<ValidationFilesystemPath, ValidationError> {
    observe_with_signals(deadline, cancellation, true, move |signal, _| {
        operation(signal)
    })
}

pub(super) fn observe_with_signals(
    deadline: Instant,
    cancellation: &CommandCancellation,
    cancel_synchronous_io: bool,
    operation: impl FnOnce(
        CommandCancellation,
        Arc<AtomicBool>,
    ) -> Result<ValidationFilesystemPath, ValidationError>
    + Send
    + 'static,
) -> Result<ValidationFilesystemPath, ValidationError> {
    run(
        deadline,
        cancellation,
        cancel_synchronous_io,
        operation,
        JOIN_GRACE,
        None,
        None,
        None,
    )
}

#[cfg(feature = "test-faults")]
pub(super) fn observe_test(
    deadline: Instant,
    cancellation: &CommandCancellation,
    operation: impl FnOnce(CommandCancellation) -> Result<ValidationFilesystemPath, ValidationError>
    + Send
    + 'static,
    control: FilesystemWorkerTestControl,
) -> Result<ValidationFilesystemPath, ValidationError> {
    run(
        deadline,
        cancellation,
        false,
        move |signal, _| operation(signal),
        control.join_grace.unwrap_or(JOIN_GRACE),
        control.after_result_gate,
        control.result_sent,
        control.result_received,
    )
}

fn run(
    deadline: Instant,
    cancellation: &CommandCancellation,
    cancel_synchronous_io: bool,
    operation: impl FnOnce(
        CommandCancellation,
        Arc<AtomicBool>,
    ) -> Result<ValidationFilesystemPath, ValidationError>
    + Send
    + 'static,
    join_grace: Duration,
    after_result_gate: Option<Arc<AtomicBool>>,
    result_sent: Option<Arc<AtomicBool>>,
    result_received: Option<Arc<AtomicBool>>,
) -> Result<ValidationFilesystemPath, ValidationError> {
    check_running(deadline, cancellation)?;
    let worker_cancellation = CommandCancellation::new();
    let signal = worker_cancellation.clone();
    let native_cancellation = Arc::new(AtomicBool::new(false));
    let native_signal = native_cancellation.clone();
    let (sender, response) = mpsc::sync_channel(1);
    let worker = thread::Builder::new()
        .name("runtime-filesystem-observation".into())
        .spawn(move || {
            let result = operation(signal, native_signal);
            let _ = sender.send(result);
            if let Some(sent) = result_sent {
                sent.store(true, Ordering::Release);
            }
            if let Some(gate) = after_result_gate {
                while !gate.load(Ordering::Acquire) {
                    thread::sleep(POLL_INTERVAL);
                }
            }
        })
        .map_err(|_| ValidationIssue::FilesystemWorker)?;
    let mut owner = FilesystemWorkerCleanup {
        worker: Some(worker),
        response,
        result: None,
        cancellation: worker_cancellation,
        native_cancellation,
        cancel_synchronous_io,
        join_grace,
        complete: false,
        result_received,
    };
    let interrupted = loop {
        if let Err(issue) = check_running(deadline, cancellation) {
            break Some(issue);
        }
        owner.receive();
        if owner.result.is_some() {
            break None;
        }
        thread::sleep(POLL_INTERVAL);
    };
    let cleanup = owner.cleanup();
    let interrupted = interrupted.or_else(|| check_running(deadline, cancellation).err());
    if let Err(cleanup_issue) = cleanup {
        let issue = interrupted
            .or_else(|| {
                owner
                    .result
                    .as_ref()
                    .and_then(|result| result.as_ref().err().map(ValidationError::issue))
            })
            .unwrap_or(cleanup_issue);
        return Err(ValidationError::with_cleanup(issue, Box::new(owner)));
    }
    let result = owner
        .result
        .take()
        .unwrap_or_else(|| Err(ValidationIssue::FilesystemWorker.into()));
    if let Some(issue) = interrupted {
        return Err(ValidationError::new(issue).with_source(
            result
                .err()
                .map(|error| Box::new(error) as Box<dyn std::error::Error + Send>),
        ));
    }
    result
}

struct FilesystemWorkerCleanup {
    worker: Option<JoinHandle<()>>,
    response: Receiver<Result<ValidationFilesystemPath, ValidationError>>,
    result: Option<Result<ValidationFilesystemPath, ValidationError>>,
    cancellation: CommandCancellation,
    native_cancellation: Arc<AtomicBool>,
    cancel_synchronous_io: bool,
    join_grace: Duration,
    complete: bool,
    result_received: Option<Arc<AtomicBool>>,
}

impl FilesystemWorkerCleanup {
    fn receive(&mut self) {
        if self.result.is_some() {
            return;
        }
        match self.response.try_recv() {
            Ok(result) => {
                self.result = Some(result);
                if let Some(received) = &self.result_received {
                    received.store(true, Ordering::Release);
                }
            }
            Err(TryRecvError::Disconnected) => {
                self.result = Some(Err(ValidationIssue::FilesystemWorker.into()))
            }
            Err(TryRecvError::Empty) => {}
        }
    }
}

impl ValidationCleanup for FilesystemWorkerCleanup {
    fn cleanup(&mut self) -> Result<(), ValidationIssue> {
        if self.complete {
            return Ok(());
        }
        self.cancellation.cancel();
        self.native_cancellation.store(true, Ordering::Release);
        let deadline = Instant::now() + self.join_grace;
        while self
            .worker
            .as_ref()
            .is_some_and(|worker| !worker.is_finished())
        {
            self.receive();
            #[cfg(target_os = "windows")]
            if self.cancel_synchronous_io {
                use std::os::windows::io::AsRawHandle;
                use windows::Win32::{Foundation::HANDLE, System::IO::CancelSynchronousIo};
                if let Some(worker) = self.worker.as_ref() {
                    let _ = unsafe { CancelSynchronousIo(HANDLE(worker.as_raw_handle())) };
                }
            }
            if Instant::now() >= deadline {
                return Err(ValidationIssue::Cleanup);
            }
            thread::sleep(POLL_INTERVAL);
        }
        if let Some(worker) = self.worker.take() {
            if worker.join().is_err() && self.result.is_none() {
                self.result = Some(Err(ValidationIssue::FilesystemWorker.into()));
            }
        }
        self.receive();
        if let Some(Err(error)) = self.result.as_mut() {
            error.dispose_cleanup()?;
        }
        self.complete = true;
        Ok(())
    }

    fn failure(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.result
            .as_ref()
            .and_then(|result| result.as_ref().err())
            .map(|error| error as &(dyn std::error::Error + 'static))
    }
    fn take_cleanup_failure(&mut self) -> Option<Box<dyn std::error::Error + Send>> {
        self.result
            .take()
            .and_then(Result::err)
            .map(|error| Box::new(error) as Box<dyn std::error::Error + Send>)
    }
}

impl Drop for FilesystemWorkerCleanup {
    fn drop(&mut self) {
        if self.cleanup().is_err() {
            tracing::error!(
                "filesystem validation dropped unresolved original worker or nested cleanup custody"
            );
        }
    }
}
