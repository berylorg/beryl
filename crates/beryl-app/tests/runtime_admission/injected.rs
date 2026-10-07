use super::*;

#[derive(Default)]
pub struct Filesystem {
    pub failure: Mutex<Option<ValidationIssue>>,
    pub home_failure: Mutex<Option<ValidationIssue>>,
    pub executable_gate: Mutex<Option<ExecutableGate>>,
}

pub struct ExecutableGate {
    entered: std::sync::mpsc::SyncSender<()>,
    release: std::sync::mpsc::Receiver<()>,
}

impl Filesystem {
    pub fn pause_executable(
        &self,
    ) -> (
        std::sync::mpsc::Receiver<()>,
        std::sync::mpsc::SyncSender<()>,
    ) {
        let (entered, observation) = std::sync::mpsc::sync_channel(1);
        let (release, released) = std::sync::mpsc::sync_channel(1);
        *self.executable_gate.lock().unwrap() = Some(ExecutableGate {
            entered,
            release: released,
        });
        (observation, release)
    }

    fn facts(&self, path: &str) -> Result<ValidationFilesystemPath, ValidationError> {
        if let Some(failure) = *self.failure.lock().unwrap() {
            return Err(failure.into());
        }
        Ok(ValidationFilesystemPath {
            mode: RuntimeMode::Host,
            host_path: path.into(),
            native_path: path.into(),
        })
    }
}

impl ValidationFilesystem for Filesystem {
    fn executable(
        &self,
        selected: &Path,
        _: Instant,
        _: &CommandCancellation,
        _: usize,
    ) -> Result<ValidationFilesystemPath, ValidationError> {
        if let Some(gate) = self.executable_gate.lock().unwrap().take() {
            gate.entered.send(()).unwrap();
            gate.release.recv_timeout(Duration::from_secs(5)).unwrap();
        }
        let path = selected.to_str().unwrap();
        self.facts(if path == ALIAS { EXECUTABLE } else { path })
    }
    fn directory(
        &self,
        selected: &Path,
        _: &RuntimeMode,
        _: Instant,
        _: &CommandCancellation,
        _: usize,
    ) -> Result<ValidationFilesystemPath, ValidationError> {
        self.facts(selected.to_str().unwrap())
    }
    fn home(
        &self,
        _: &RuntimeMode,
        _: Instant,
        _: &CommandCancellation,
        _: usize,
    ) -> Result<ValidationFilesystemPath, ValidationError> {
        if let Some(failure) = *self.home_failure.lock().unwrap() {
            return Err(failure.into());
        }
        self.facts(HOME)
    }
}

#[derive(Default)]
pub struct Backend {
    pub launch_forms: Mutex<Vec<beryl_model::RuntimeLaunchForm>>,
    launched: AtomicUsize,
    connected: AtomicUsize,
    initialized: AtomicUsize,
    admitted: AtomicUsize,
    cleaned: AtomicUsize,
    pub failure: Mutex<Option<ValidationIssue>>,
    pub cleanup_failures: AtomicUsize,
}

impl Backend {
    pub fn counts(&self) -> (usize, usize, usize, usize, usize) {
        (
            self.launched.load(Ordering::SeqCst),
            self.connected.load(Ordering::SeqCst),
            self.initialized.load(Ordering::SeqCst),
            self.admitted.load(Ordering::SeqCst),
            self.cleaned.load(Ordering::SeqCst),
        )
    }
}

pub(super) struct BackendSeam(pub(super) Arc<Backend>);
struct Candidate {
    backend: Arc<Backend>,
    cleaned: bool,
}

impl RuntimeQualificationBackend for BackendSeam {
    fn launch(
        &self,
        spec: beryl_backend::ManagedBackendLaunchSpec,
        capacity: usize,
    ) -> Result<Box<dyn RuntimeQualificationCandidate>, ValidationError> {
        assert!(capacity > 0);
        self.0.launch_forms.lock().unwrap().push(spec.launch_form());
        self.0.launched.fetch_add(1, Ordering::SeqCst);
        Ok(Box::new(Candidate {
            backend: self.0.clone(),
            cleaned: false,
        }))
    }
}

impl ValidationCleanup for Candidate {
    fn cleanup(&mut self) -> Result<(), ValidationIssue> {
        assert!(!self.cleaned, "qualification resource disposed twice");
        if self
            .backend
            .cleanup_failures
            .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |remaining| {
                remaining.checked_sub(1)
            })
            .is_ok()
        {
            return Err(ValidationIssue::Cleanup);
        }
        self.cleaned = true;
        self.backend.cleaned.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
}

impl Drop for Candidate {
    fn drop(&mut self) {
        assert!(
            self.cleaned,
            "qualification resource dropped before cleanup proof"
        );
    }
}

impl RuntimeQualificationCandidate for Candidate {
    fn connect(&mut self, timeout: Duration) -> Result<(), ValidationIssue> {
        assert!(!timeout.is_zero());
        self.backend.connected.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
    fn initialize(&mut self, _: Duration) -> Result<(), ValidationIssue> {
        self.backend.initialized.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
    fn admit(&mut self, home: &RuntimeNativePath, _: Duration) -> Result<(), ValidationIssue> {
        assert_eq!(home.as_str(), HOME);
        self.backend.admitted.fetch_add(1, Ordering::SeqCst);
        match *self.backend.failure.lock().unwrap() {
            Some(failure) => Err(failure),
            None => Ok(()),
        }
    }
}
