#![cfg(feature = "test-faults")]

use beryl_app::{
    cas_projection::RuntimeTokenDirectory,
    runtime_admission::validation::{
        FilesystemWorkerTestControl, RuntimePathValidator, RuntimeQualificationBackend,
        RuntimeQualificationCandidate, ValidationCleanup, ValidationError, ValidationFilesystem,
        ValidationFilesystemPath, ValidationIssue, ValidationLimits,
    },
};
use beryl_backend::ManagedBackendLaunchSpec;
use beryl_home_store::CommandCancellation;
use beryl_model::{
    AdmittedHostPath, PathFlavor, RuntimeId, RuntimeLaunchForm, RuntimeMode, RuntimeNativePath,
};
use std::{
    path::Path,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

#[cfg(target_os = "windows")]
const EXECUTABLE: &str = r"C:\canonical\codex.exe";
#[cfg(not(target_os = "windows"))]
const EXECUTABLE: &str = "/canonical/codex";
#[cfg(target_os = "windows")]
const SELECTED: &str = r"C:\alias\codex.exe";
#[cfg(not(target_os = "windows"))]
const SELECTED: &str = "/alias/codex";
#[cfg(target_os = "windows")]
const HOME: &str = r"C:\Users\Operator";
#[cfg(not(target_os = "windows"))]
const HOME: &str = "/home/Operator";

fn facts(mode: RuntimeMode, host: &str, native: &str) -> ValidationFilesystemPath {
    ValidationFilesystemPath {
        mode,
        host_path: host.into(),
        native_path: native.into(),
    }
}

struct Filesystem {
    executable: ValidationFilesystemPath,
    home: ValidationFilesystemPath,
    root: ValidationFilesystemPath,
    reads: AtomicUsize,
    failure: Option<ValidationIssue>,
}

impl Filesystem {
    fn host() -> Self {
        Self {
            executable: facts(RuntimeMode::Host, EXECUTABLE, EXECUTABLE),
            home: facts(RuntimeMode::Host, HOME, HOME),
            root: facts(RuntimeMode::Host, HOME, HOME),
            reads: AtomicUsize::new(0),
            failure: None,
        }
    }
    fn result(
        &self,
        facts: &ValidationFilesystemPath,
    ) -> Result<ValidationFilesystemPath, ValidationError> {
        self.reads.fetch_add(1, Ordering::SeqCst);
        match self.failure {
            Some(issue) => Err(issue.into()),
            None => Ok(facts.clone()),
        }
    }
}

impl ValidationFilesystem for Filesystem {
    fn executable(
        &self,
        _: &Path,
        _: Instant,
        _: &CommandCancellation,
        _: usize,
    ) -> Result<ValidationFilesystemPath, ValidationError> {
        self.result(&self.executable)
    }
    fn directory(
        &self,
        _: &Path,
        _: &RuntimeMode,
        _: Instant,
        _: &CommandCancellation,
        _: usize,
    ) -> Result<ValidationFilesystemPath, ValidationError> {
        self.result(&self.root)
    }
    fn home(
        &self,
        _: &RuntimeMode,
        _: Instant,
        _: &CommandCancellation,
        _: usize,
    ) -> Result<ValidationFilesystemPath, ValidationError> {
        self.result(&self.home)
    }
}

#[derive(Default)]
struct BackendState {
    events: Mutex<Vec<&'static str>>,
    spec: Mutex<Option<ManagedBackendLaunchSpec>>,
    failure: Option<ValidationIssue>,
    cleanup_failures: AtomicUsize,
    cleaned: AtomicBool,
    cancellation: CommandCancellation,
    cancel_after_initialize: bool,
    cancel_during_cleanup: bool,
}

struct Backend(Arc<BackendState>);
struct Candidate(Arc<BackendState>);

impl RuntimeQualificationBackend for Backend {
    fn launch(
        &self,
        spec: ManagedBackendLaunchSpec,
        capacity: usize,
    ) -> Result<Box<dyn RuntimeQualificationCandidate>, ValidationError> {
        self.0.events.lock().unwrap().push("launch");
        assert_eq!(capacity, 32);
        *self.0.spec.lock().unwrap() = Some(spec);
        if self.0.failure == Some(ValidationIssue::Launch) {
            return Err(ValidationIssue::Launch.into());
        }
        Ok(Box::new(Candidate(self.0.clone())))
    }
}

impl RuntimeQualificationCandidate for Candidate {
    fn connect(&mut self, timeout: Duration) -> Result<(), ValidationIssue> {
        assert!(!timeout.is_zero() && timeout <= Duration::from_secs(30));
        self.0.events.lock().unwrap().push("connect");
        if self.0.failure == Some(ValidationIssue::ForegroundConnection) {
            return Err(ValidationIssue::ForegroundConnection);
        }
        Ok(())
    }
    fn initialize(&mut self, _: Duration) -> Result<(), ValidationIssue> {
        self.0.events.lock().unwrap().push("initialize");
        if self.0.cancel_after_initialize {
            self.0.cancellation.cancel();
        }
        if self.0.failure == Some(ValidationIssue::Initialize) {
            return Err(ValidationIssue::Initialize);
        }
        Ok(())
    }
    fn admit(&mut self, home: &RuntimeNativePath, _: Duration) -> Result<(), ValidationIssue> {
        assert_eq!(
            home,
            self.0
                .spec
                .lock()
                .unwrap()
                .as_ref()
                .unwrap()
                .working_directory()
        );
        self.0.events.lock().unwrap().push("admit");
        if self.0.failure == Some(ValidationIssue::ReleaseRejected) {
            return Err(ValidationIssue::ReleaseRejected);
        }
        Ok(())
    }
}

impl ValidationCleanup for Candidate {
    fn cleanup(&mut self) -> Result<(), ValidationIssue> {
        self.0.events.lock().unwrap().push("cleanup");
        if self.0.cancel_during_cleanup {
            self.0.cancellation.cancel();
        }
        if self
            .0
            .cleanup_failures
            .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |value| {
                value.checked_sub(1)
            })
            .is_ok()
        {
            return Err(ValidationIssue::Cleanup);
        }
        assert!(!self.0.cleaned.swap(true, Ordering::SeqCst));
        Ok(())
    }
}

fn tokens() -> RuntimeTokenDirectory {
    let flavor = if cfg!(target_os = "windows") {
        PathFlavor::Windows
    } else {
        PathFlavor::Posix
    };
    RuntimeTokenDirectory::from_admitted(
        AdmittedHostPath::from_admitted(
            flavor,
            if cfg!(target_os = "windows") {
                r"C:\temporary\tokens"
            } else {
                "/tmp/tokens"
            },
        )
        .unwrap(),
    )
}

fn validator(filesystem: Filesystem, backend: Arc<BackendState>) -> RuntimePathValidator {
    RuntimePathValidator::with_test_seams(
        tokens(),
        ValidationLimits::default(),
        Arc::new(filesystem),
        Arc::new(Backend(backend)),
    )
    .unwrap()
}

fn runtime_id() -> RuntimeId {
    RuntimeId::from_bytes([71; 16])
}

#[test]
fn canonical_resolution_precedes_launch_and_admission_uses_exact_home_spec() {
    let backend = Arc::new(BackendState::default());
    let validator = validator(Filesystem::host(), backend.clone());
    let cancellation = CommandCancellation::new();
    let executable = validator
        .resolve_executable(Path::new(SELECTED), &cancellation)
        .unwrap();
    assert_eq!(executable.canonical_executable().as_str(), EXECUTABLE);
    assert!(backend.events.lock().unwrap().is_empty());
    let runtime = validator
        .qualify_runtime(
            runtime_id(),
            executable,
            RuntimeLaunchForm::StandaloneAppServer,
            &cancellation,
        )
        .unwrap();
    assert_eq!(runtime.runtime_id(), runtime_id());
    assert_eq!(runtime.home_root().as_str(), HOME);
    assert_eq!(runtime.home_display_path().as_str(), HOME);
    let spec = backend.spec.lock().unwrap();
    let spec = spec.as_ref().unwrap();
    assert_eq!(spec.canonical_executable().as_str(), EXECUTABLE);
    assert_eq!(spec.runtime_native_executable().as_str(), EXECUTABLE);
    assert_eq!(spec.working_directory().as_str(), HOME);
    assert_eq!(
        backend.events.lock().unwrap().as_slice(),
        ["launch", "connect", "initialize", "admit", "cleanup"]
    );
    assert!(backend.cleaned.load(Ordering::SeqCst));
}

#[test]
fn backend_refusal_at_each_stage_returns_only_after_joined_cleanup() {
    for issue in [
        ValidationIssue::Launch,
        ValidationIssue::ForegroundConnection,
        ValidationIssue::Initialize,
        ValidationIssue::ReleaseRejected,
    ] {
        let backend = Arc::new(BackendState {
            failure: Some(issue),
            ..Default::default()
        });
        let validator = validator(Filesystem::host(), backend.clone());
        let cancellation = CommandCancellation::new();
        let executable = validator
            .resolve_executable(Path::new(SELECTED), &cancellation)
            .unwrap();
        let error = validator
            .qualify_runtime(
                runtime_id(),
                executable,
                RuntimeLaunchForm::StandaloneAppServer,
                &cancellation,
            )
            .unwrap_err();
        assert_eq!(error.issue(), issue);
        assert!(!error.has_cleanup_custody());
        assert_eq!(
            backend.cleaned.load(Ordering::SeqCst),
            issue != ValidationIssue::Launch
        );
        if issue != ValidationIssue::Launch {
            assert_eq!(backend.events.lock().unwrap().last(), Some(&"cleanup"));
        }
    }
}

#[test]
fn cancellation_after_initialize_never_sends_config_admission_and_joins_owner() {
    let backend = Arc::new(BackendState {
        cancel_after_initialize: true,
        ..Default::default()
    });
    let validator = validator(Filesystem::host(), backend.clone());
    let executable = validator
        .resolve_executable(Path::new(SELECTED), &backend.cancellation)
        .unwrap();
    let error = validator
        .qualify_runtime(
            runtime_id(),
            executable,
            RuntimeLaunchForm::StandaloneAppServer,
            &backend.cancellation,
        )
        .unwrap_err();
    assert_eq!(error.issue(), ValidationIssue::Cancelled);
    assert!(!error.has_cleanup_custody());
    assert_eq!(
        backend.events.lock().unwrap().as_slice(),
        ["launch", "connect", "initialize", "cleanup"]
    );
    assert!(backend.cleaned.load(Ordering::SeqCst));
}

#[test]
fn cleanup_failure_keeps_original_owner_and_never_returns_admitted_facts() {
    let backend = Arc::new(BackendState {
        cleanup_failures: AtomicUsize::new(2),
        ..Default::default()
    });
    let validator = validator(Filesystem::host(), backend.clone());
    let cancellation = CommandCancellation::new();
    let executable = validator
        .resolve_executable(Path::new(SELECTED), &cancellation)
        .unwrap();
    let mut error = validator
        .qualify_runtime(
            runtime_id(),
            executable,
            RuntimeLaunchForm::StandaloneAppServer,
            &cancellation,
        )
        .unwrap_err();
    assert_eq!(error.issue(), ValidationIssue::Cleanup);
    assert!(error.has_cleanup_custody());
    assert!(error.dispose_cleanup().is_err());
    assert!(error.has_cleanup_custody());
    assert!(!backend.cleaned.load(Ordering::SeqCst));
    error.dispose_cleanup().unwrap();
    assert!(!error.has_cleanup_custody());
    error.dispose_cleanup().unwrap();
    assert!(backend.cleaned.load(Ordering::SeqCst));
    assert_eq!(
        backend
            .events
            .lock()
            .unwrap()
            .iter()
            .filter(|event| **event == "launch")
            .count(),
        1
    );
}

#[test]
fn invalid_selection_and_pre_cancel_never_read_filesystem_or_launch() {
    let filesystem = Arc::new(Filesystem::host());
    let backend = Arc::new(BackendState::default());
    let validator = RuntimePathValidator::with_test_seams(
        tokens(),
        ValidationLimits::default(),
        filesystem.clone(),
        Arc::new(Backend(backend.clone())),
    )
    .unwrap();
    for path in ["relative/codex", "", "C:\\bad\nname"] {
        assert!(
            validator
                .resolve_executable(Path::new(path), &CommandCancellation::new())
                .is_err()
        );
    }
    let oversized = format!("{}{}", SELECTED, "x".repeat(4096));
    assert_eq!(
        validator
            .resolve_executable(Path::new(&oversized), &CommandCancellation::new())
            .unwrap_err()
            .issue(),
        ValidationIssue::InvalidPath
    );
    let cancellation = CommandCancellation::new();
    cancellation.cancel();
    assert_eq!(
        validator
            .resolve_executable(Path::new(SELECTED), &cancellation)
            .unwrap_err()
            .issue(),
        ValidationIssue::Cancelled
    );
    assert_eq!(filesystem.reads.load(Ordering::SeqCst), 0);
    assert!(backend.events.lock().unwrap().is_empty());
}

#[test]
fn invalid_foreign_and_inconsistent_home_facts_cannot_create_launch() {
    for home in [
        facts(RuntimeMode::Host, HOME, EXECUTABLE),
        facts(RuntimeMode::wsl("Other").unwrap(), HOME, "/home/Operator"),
        facts(RuntimeMode::Host, HOME, "relative"),
    ] {
        let backend = Arc::new(BackendState::default());
        let validator = validator(
            Filesystem {
                home,
                ..Filesystem::host()
            },
            backend.clone(),
        );
        let cancellation = CommandCancellation::new();
        let executable = validator
            .resolve_executable(Path::new(SELECTED), &cancellation)
            .unwrap();
        assert!(
            validator
                .qualify_runtime(
                    runtime_id(),
                    executable,
                    RuntimeLaunchForm::StandaloneAppServer,
                    &cancellation
                )
                .is_err()
        );
        assert!(backend.events.lock().unwrap().is_empty());
    }
}

#[cfg(target_os = "windows")]
#[test]
fn exact_wsl_distribution_and_canonical_native_paths_are_preserved() {
    let mode = RuntimeMode::wsl("Operator-Distro").unwrap();
    let executable = facts(
        mode.clone(),
        r"\\wsl.localhost\Operator-Distro\usr\bin\codex",
        "/usr/bin/codex",
    );
    let home = facts(
        mode.clone(),
        r"\\wsl.localhost\Operator-Distro\home\Operator",
        "/home/Operator",
    );
    let backend = Arc::new(BackendState::default());
    let validator = validator(
        Filesystem {
            executable,
            home: home.clone(),
            root: home,
            reads: AtomicUsize::new(0),
            failure: None,
        },
        backend.clone(),
    )
    .with_test_supervisor_artifact(Arc::new(
        beryl_backend::WslSupervisorArtifact::from_verified_release(
            r"C:\Beryl\beryl-wsl-supervisor".into(),
            "/mnt/c/Beryl/beryl-wsl-supervisor".into(),
            [8; 32],
            1,
            Arc::new(tempfile::tempfile().unwrap()),
        )
        .unwrap(),
    ));
    let cancellation = CommandCancellation::new();
    let executable = validator
        .resolve_executable(
            Path::new(r"\\wsl$\Operator-Distro\bin\codex"),
            &cancellation,
        )
        .unwrap();
    let runtime = validator
        .qualify_runtime(
            runtime_id(),
            executable,
            RuntimeLaunchForm::StandaloneAppServer,
            &cancellation,
        )
        .unwrap();
    assert_eq!(runtime.mode(), &mode);
    assert_eq!(
        runtime.runtime_native_executable().as_str(),
        "/usr/bin/codex"
    );
    assert_eq!(runtime.home_root().as_str(), "/home/Operator");
    let spec = backend.spec.lock().unwrap();
    assert_eq!(
        spec.as_ref().unwrap().runtime_token_directory().as_str(),
        "/mnt/c/temporary/tokens"
    );
    assert_eq!(
        validator
            .resolve_root(Path::new(HOME), &mode, &cancellation)
            .unwrap_err()
            .issue(),
        ValidationIssue::EnvironmentMismatch
    );
    assert_eq!(
        validator
            .resolve_root(
                Path::new(r"\\wsl.localhost\Other\home\Operator"),
                &mode,
                &cancellation
            )
            .unwrap_err()
            .issue(),
        ValidationIssue::EnvironmentMismatch
    );
}

#[cfg(target_os = "windows")]
#[test]
fn directory_mode_is_derived_before_filesystem_reads() {
    let filesystem = Arc::new(Filesystem::host());
    let backend = Arc::new(BackendState::default());
    let validator = RuntimePathValidator::with_test_seams(
        tokens(),
        ValidationLimits::default(),
        filesystem.clone(),
        Arc::new(Backend(backend.clone())),
    )
    .unwrap();
    assert_eq!(
        validator
            .resolve_root(
                Path::new(r"\\wsl.localhost\Other\home\Operator"),
                &RuntimeMode::Host,
                &CommandCancellation::new()
            )
            .unwrap_err()
            .issue(),
        ValidationIssue::EnvironmentMismatch
    );
    assert_eq!(filesystem.reads.load(Ordering::SeqCst), 0);
    assert!(backend.events.lock().unwrap().is_empty());
}

#[test]
fn native_worker_cancellation_joins_instead_of_detaching() {
    let validator = validator(Filesystem::host(), Arc::new(BackendState::default()));
    let cancellation = CommandCancellation::new();
    let request = cancellation.clone();
    let exited = Arc::new(AtomicBool::new(false));
    let worker_exited = exited.clone();
    let (started, ready) = std::sync::mpsc::sync_channel(1);
    let observation = thread::spawn(move || {
        validator.observe_test_native_worker(&request, move |worker_signal| {
            started.send(()).unwrap();
            while !worker_signal.is_cancelled() {
                thread::sleep(Duration::from_millis(1));
            }
            worker_exited.store(true, Ordering::SeqCst);
            Err(ValidationIssue::Cancelled.into())
        })
    });
    ready.recv_timeout(Duration::from_secs(1)).unwrap();
    cancellation.cancel();
    let error = observation.join().unwrap().unwrap_err();
    assert_eq!(error.issue(), ValidationIssue::Cancelled);
    assert!(!error.has_cleanup_custody());
    assert!(exited.load(Ordering::SeqCst));
}

#[derive(Default)]
struct NestedCleanupState {
    calls: AtomicUsize,
    completed: AtomicBool,
    drops: AtomicUsize,
}

#[derive(Debug)]
struct NestedDiagnostic {
    cleanup_failed: bool,
}

impl std::fmt::Display for NestedDiagnostic {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("nested observation cause")?;
        if self.cleanup_failed {
            f.write_str("; first nested cleanup failure")?;
        }
        Ok(())
    }
}

impl std::error::Error for NestedDiagnostic {}

struct NestedCleanup {
    state: Arc<NestedCleanupState>,
    diagnostic: Option<NestedDiagnostic>,
}

impl ValidationCleanup for NestedCleanup {
    fn cleanup(&mut self) -> Result<(), ValidationIssue> {
        if self.state.completed.load(Ordering::Acquire) {
            return Ok(());
        }
        if self.state.calls.fetch_add(1, Ordering::AcqRel) == 0 {
            self.diagnostic.as_mut().unwrap().cleanup_failed = true;
            return Err(ValidationIssue::Cleanup);
        }
        self.state.completed.store(true, Ordering::Release);
        Ok(())
    }

    fn failure(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.diagnostic
            .as_ref()
            .map(|diagnostic| diagnostic as &(dyn std::error::Error + 'static))
    }

    fn take_cleanup_failure(&mut self) -> Option<Box<dyn std::error::Error + Send>> {
        self.diagnostic
            .take()
            .map(|diagnostic| Box::new(diagnostic) as Box<dyn std::error::Error + Send>)
    }
}

impl Drop for NestedCleanup {
    fn drop(&mut self) {
        self.state.drops.fetch_add(1, Ordering::AcqRel);
    }
}

fn nested_error(state: Arc<NestedCleanupState>) -> ValidationError {
    ValidationError::with_test_cleanup(
        ValidationIssue::HomeUnavailable,
        Box::new(NestedCleanup {
            state,
            diagnostic: Some(NestedDiagnostic {
                cleanup_failed: false,
            }),
        }),
    )
}

struct ReleaseGates(Vec<Arc<AtomicBool>>);

impl Drop for ReleaseGates {
    fn drop(&mut self) {
        for gate in &self.0 {
            gate.store(true, Ordering::Release);
        }
    }
}

fn wait_flag(flag: &AtomicBool) {
    let deadline = Instant::now() + Duration::from_secs(2);
    while !flag.load(Ordering::Acquire) {
        assert!(Instant::now() < deadline);
        thread::sleep(Duration::from_millis(1));
    }
}

fn settle_nested_custody(error: &mut ValidationError, state: &NestedCleanupState) {
    let deadline = Instant::now() + Duration::from_secs(2);
    while state.calls.load(Ordering::Acquire) == 0 {
        assert!(error.dispose_cleanup().is_err());
        assert!(error.has_cleanup_custody());
        assert!(Instant::now() < deadline);
        thread::sleep(Duration::from_millis(1));
    }
    assert_eq!(state.calls.load(Ordering::Acquire), 1);
    assert_eq!(state.drops.load(Ordering::Acquire), 0);
    assert!(!state.completed.load(Ordering::Acquire));
    error.dispose_cleanup().unwrap();
    assert!(!error.has_cleanup_custody());
    assert_eq!(state.calls.load(Ordering::Acquire), 2);
    assert!(state.completed.load(Ordering::Acquire));
    assert_eq!(state.drops.load(Ordering::Acquire), 1);
    let source = std::error::Error::source(error).unwrap();
    assert_eq!(
        source.to_string(),
        ValidationIssue::HomeUnavailable.to_string()
    );
    let nested = source.downcast_ref::<ValidationError>().unwrap();
    assert!(!nested.has_cleanup_custody());
    assert_eq!(
        std::error::Error::source(nested).unwrap().to_string(),
        "nested observation cause; first nested cleanup failure"
    );
    error.dispose_cleanup().unwrap();
    assert_eq!(state.calls.load(Ordering::Acquire), 2);
}

#[test]
fn received_nested_error_retains_unfinished_wrapper_and_original_cleanup_owner() {
    let validator = validator(Filesystem::host(), Arc::new(BackendState::default()));
    let wrapper_gate = Arc::new(AtomicBool::new(false));
    let _release = ReleaseGates(vec![wrapper_gate.clone()]);
    let received = Arc::new(AtomicBool::new(false));
    let state = Arc::new(NestedCleanupState::default());
    let nested_state = state.clone();
    let mut error = validator
        .observe_test_native_worker_with_control(
            &CommandCancellation::new(),
            move |_| Err(nested_error(nested_state)),
            FilesystemWorkerTestControl {
                after_result_gate: Some(wrapper_gate.clone()),
                result_received: Some(received.clone()),
                join_grace: Some(Duration::ZERO),
                ..Default::default()
            },
        )
        .unwrap_err();
    assert!(received.load(Ordering::Acquire));
    assert_eq!(error.issue(), ValidationIssue::HomeUnavailable);
    assert!(error.has_cleanup_custody());
    assert_eq!(state.calls.load(Ordering::Acquire), 0);
    assert_eq!(state.drops.load(Ordering::Acquire), 0);
    wrapper_gate.store(true, Ordering::Release);
    settle_nested_custody(&mut error, &state);
    assert_eq!(error.issue(), ValidationIssue::HomeUnavailable);
}

#[test]
fn cancelled_worker_retains_late_queued_nested_error_and_transfers_diagnostics() {
    let validator = validator(Filesystem::host(), Arc::new(BackendState::default()));
    let cancellation = CommandCancellation::new();
    let request = cancellation.clone();
    let operation_gate = Arc::new(AtomicBool::new(false));
    let wrapper_gate = Arc::new(AtomicBool::new(false));
    let _release = ReleaseGates(vec![operation_gate.clone(), wrapper_gate.clone()]);
    let sent = Arc::new(AtomicBool::new(false));
    let received = Arc::new(AtomicBool::new(false));
    let state = Arc::new(NestedCleanupState::default());
    let nested_state = state.clone();
    let worker_gate = operation_gate.clone();
    let (started, ready) = std::sync::mpsc::sync_channel(1);
    let control = FilesystemWorkerTestControl {
        after_result_gate: Some(wrapper_gate.clone()),
        result_sent: Some(sent.clone()),
        result_received: Some(received.clone()),
        join_grace: Some(Duration::ZERO),
    };
    let observation = thread::spawn(move || {
        validator.observe_test_native_worker_with_control(
            &request,
            move |signal| {
                started.send(()).unwrap();
                wait_flag(&worker_gate);
                assert!(signal.is_cancelled());
                Err(nested_error(nested_state))
            },
            control,
        )
    });
    ready.recv_timeout(Duration::from_secs(2)).unwrap();
    cancellation.cancel();
    let mut error = observation.join().unwrap().unwrap_err();
    assert_eq!(error.issue(), ValidationIssue::Cancelled);
    assert!(error.has_cleanup_custody());
    assert!(!received.load(Ordering::Acquire));
    operation_gate.store(true, Ordering::Release);
    wait_flag(&sent);
    assert!(error.dispose_cleanup().is_err());
    assert!(received.load(Ordering::Acquire));
    assert_eq!(state.calls.load(Ordering::Acquire), 0);
    assert_eq!(state.drops.load(Ordering::Acquire), 0);
    wrapper_gate.store(true, Ordering::Release);
    settle_nested_custody(&mut error, &state);
    assert_eq!(error.issue(), ValidationIssue::Cancelled);
}

#[test]
fn cancellation_during_join_rejects_otherwise_valid_filesystem_facts() {
    let validator = validator(Filesystem::host(), Arc::new(BackendState::default()));
    let cancellation = CommandCancellation::new();
    let request = cancellation.clone();
    let gate = Arc::new(AtomicBool::new(false));
    let received = Arc::new(AtomicBool::new(false));
    let _release = ReleaseGates(vec![gate.clone()]);
    let control = FilesystemWorkerTestControl {
        after_result_gate: Some(gate.clone()),
        result_received: Some(received.clone()),
        ..Default::default()
    };
    let observation = thread::spawn(move || {
        validator.observe_test_native_worker_with_control(
            &request,
            |_| Ok(facts(RuntimeMode::Host, HOME, HOME)),
            control,
        )
    });
    wait_flag(&received);
    cancellation.cancel();
    gate.store(true, Ordering::Release);
    let error = observation.join().unwrap().unwrap_err();
    assert_eq!(error.issue(), ValidationIssue::Cancelled);
    assert!(!error.has_cleanup_custody());
}

#[test]
fn cancellation_during_qualification_cleanup_rejects_admitted_runtime() {
    let backend = Arc::new(BackendState {
        cancel_during_cleanup: true,
        ..Default::default()
    });
    let validator = validator(Filesystem::host(), backend.clone());
    let executable = validator
        .resolve_executable(Path::new(SELECTED), &backend.cancellation)
        .unwrap();
    let error = validator
        .qualify_runtime(
            runtime_id(),
            executable,
            RuntimeLaunchForm::CodexCli,
            &backend.cancellation,
        )
        .unwrap_err();
    assert_eq!(error.issue(), ValidationIssue::Cancelled);
    assert!(!error.has_cleanup_custody());
    assert!(backend.cleaned.load(Ordering::Acquire));
    assert_eq!(
        backend.spec.lock().unwrap().as_ref().unwrap().launch_form(),
        RuntimeLaunchForm::CodexCli
    );
}

#[test]
fn qualification_preserves_both_explicit_launch_forms() {
    for launch_form in [
        RuntimeLaunchForm::StandaloneAppServer,
        RuntimeLaunchForm::CodexCli,
    ] {
        let backend = Arc::new(BackendState::default());
        let validator = validator(Filesystem::host(), backend.clone());
        let cancellation = CommandCancellation::new();
        let executable = validator
            .resolve_executable(Path::new(SELECTED), &cancellation)
            .unwrap();
        let admitted = validator
            .qualify_runtime(runtime_id(), executable, launch_form, &cancellation)
            .unwrap();
        assert_eq!(admitted.launch_form(), launch_form);
        assert_eq!(
            backend.spec.lock().unwrap().as_ref().unwrap().launch_form(),
            launch_form
        );
    }
}

#[test]
fn qualification_expiry_during_cleanup_rejects_admitted_runtime() {
    struct ExpiringCandidate;
    impl ValidationCleanup for ExpiringCandidate {
        fn cleanup(&mut self) -> Result<(), ValidationIssue> {
            thread::sleep(Duration::from_millis(20));
            Ok(())
        }
    }
    impl RuntimeQualificationCandidate for ExpiringCandidate {
        fn connect(&mut self, _: Duration) -> Result<(), ValidationIssue> {
            Ok(())
        }
        fn initialize(&mut self, _: Duration) -> Result<(), ValidationIssue> {
            Ok(())
        }
        fn admit(&mut self, _: &RuntimeNativePath, _: Duration) -> Result<(), ValidationIssue> {
            Ok(())
        }
    }
    struct ExpiringBackend;
    impl RuntimeQualificationBackend for ExpiringBackend {
        fn launch(
            &self,
            _: ManagedBackendLaunchSpec,
            _: usize,
        ) -> Result<Box<dyn RuntimeQualificationCandidate>, ValidationError> {
            Ok(Box::new(ExpiringCandidate))
        }
    }
    let validator = RuntimePathValidator::with_test_seams(
        tokens(),
        ValidationLimits {
            qualification_timeout: Duration::from_millis(10),
            ..Default::default()
        },
        Arc::new(Filesystem::host()),
        Arc::new(ExpiringBackend),
    )
    .unwrap();
    let cancellation = CommandCancellation::new();
    let executable = validator
        .resolve_executable(Path::new(SELECTED), &cancellation)
        .unwrap();
    let error = validator
        .qualify_runtime(
            runtime_id(),
            executable,
            RuntimeLaunchForm::StandaloneAppServer,
            &cancellation,
        )
        .unwrap_err();
    assert_eq!(error.issue(), ValidationIssue::Timeout);
    assert!(!error.has_cleanup_custody());
}

#[test]
fn unsupported_limits_fail_before_any_observation() {
    for limits in [
        ValidationLimits {
            path_bytes: 4097,
            ..Default::default()
        },
        ValidationLimits {
            filesystem_timeout: Duration::ZERO,
            ..Default::default()
        },
        ValidationLimits {
            filesystem_timeout: Duration::from_secs(31),
            ..Default::default()
        },
        ValidationLimits {
            foreground_control_capacity: 0,
            ..Default::default()
        },
    ] {
        assert!(
            RuntimePathValidator::with_test_seams(
                tokens(),
                limits,
                Arc::new(Filesystem::host()),
                Arc::new(Backend(Arc::new(BackendState::default())))
            )
            .is_err()
        );
    }
}
