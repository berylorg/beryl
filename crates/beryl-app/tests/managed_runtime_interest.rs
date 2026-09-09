#![cfg(all(feature = "test-faults", target_os = "windows"))]

use std::{
    fs,
    num::NonZeroUsize,
    path::{Path, PathBuf},
    thread,
    time::{Duration, Instant},
};

use beryl_app::cas_projection::{
    MinimumTurnCaptureReserve, PersistentFailureCutCompletion, ProjectionConnectionService,
    ProjectionConnectionServiceCloseOutcome, ProjectionServiceConfig, RuntimeFailure,
    RuntimeInterest, RuntimeInterestConfig, RuntimeInterestError, RuntimeInterestKind,
    RuntimeInterestStatus, RuntimeReadiness, ScheduledOrdinaryAdmission,
    ScheduledOrdinaryAdmissionError, ScheduledOrdinaryAdmissionResult,
    ScheduledOrdinaryExecutionProvider, ScheduledOrdinaryExecutionUnavailable,
};
use beryl_backend::ManagedBackendLaunchSpec;
use beryl_home_store::{HomeOpenOptions, HomeSchemaVersion, HomeStore};
use beryl_model::{
    AdmittedHostPath, ExecutionBinding, PathFlavor, RootId, RuntimeId, RuntimeMode,
    RuntimeNativePath,
};
use beryl_state::BerylState;
use serde_json::Value;
use syndic_storage::SyndicStorage;
use windows::Win32::{
    Foundation::{CloseHandle, HANDLE, WAIT_OBJECT_0, WAIT_TIMEOUT},
    System::Threading::{OpenProcess, PROCESS_SYNCHRONIZE, WaitForSingleObject},
};

const TIMEOUT: Duration = Duration::from_secs(10);

struct NoQueuedExecution;

impl ScheduledOrdinaryExecutionProvider for NoQueuedExecution {
    fn try_issue(
        &mut self,
        admission: ScheduledOrdinaryAdmission,
    ) -> Result<ScheduledOrdinaryAdmissionResult, ScheduledOrdinaryAdmissionError> {
        Ok(admission.decline(ScheduledOrdinaryExecutionUnavailable::RuntimeNotReady))
    }

    fn shutdown(&mut self) {}
}

struct Fixture {
    service: Option<ProjectionConnectionService>,
    directory: tempfile::TempDir,
    state: BerylState,
    faults: beryl_home_store::test_faults::FaultController,
}

impl Fixture {
    fn new() -> Self {
        let directory = tempfile::tempdir().unwrap();
        fs::create_dir(directory.path().join("root-1")).unwrap();
        fs::create_dir(directory.path().join("root-2")).unwrap();
        fs::create_dir(directory.path().join("tokens")).unwrap();
        let faults = beryl_home_store::test_faults::FaultController::new();
        let mut home = HomeStore::open_with_faults(
            HomeOpenOptions::new(directory.path().join("home"), HomeSchemaVersion::CURRENT),
            faults.clone(),
        )
        .unwrap();
        let storage = SyndicStorage::register(&mut home).unwrap();
        let state = BerylState::register(&mut home).unwrap();
        let mut service = ProjectionConnectionService::new(
            home,
            storage,
            ProjectionServiceConfig::try_new(8, 4, MinimumTurnCaptureReserve::try_new(1).unwrap())
                .unwrap(),
            Box::new(NoQueuedExecution),
        )
        .unwrap();
        service
            .configure_runtime_interest(
                RuntimeInterestConfig::new(
                    NonZeroUsize::new(1).unwrap(),
                    NonZeroUsize::new(4).unwrap(),
                    TIMEOUT,
                )
                .unwrap(),
            )
            .unwrap();
        Self {
            service: Some(service),
            directory,
            state,
            faults,
        }
    }

    fn service(&self) -> &ProjectionConnectionService {
        self.service.as_ref().unwrap()
    }

    fn root(&self, root: u8) -> PathBuf {
        self.directory.path().join(format!("root-{root}"))
    }

    fn acquire(
        &self,
        root: u8,
        kind: RuntimeInterestKind,
    ) -> Result<RuntimeInterest, RuntimeInterestError> {
        let executable = canonical_path(Path::new(env!("CARGO_BIN_EXE_managed-runtime-fixture")));
        let root_path = native(&canonical_path(&self.root(root)));
        let tokens = canonical_path(&self.directory.path().join("tokens"));
        let runtime_id = RuntimeId::from_bytes([99; 16]);
        let spec = ManagedBackendLaunchSpec::new(
            runtime_id,
            AdmittedHostPath::from_admitted(PathFlavor::Windows, &executable).unwrap(),
            RuntimeMode::Host,
            native(&executable),
            root_path.clone(),
            AdmittedHostPath::from_admitted(PathFlavor::Windows, &tokens).unwrap(),
            native(&tokens),
        )
        .unwrap();
        self.service().acquire_runtime_interest(
            spec,
            ExecutionBinding::new(runtime_id, RootId::from_bytes([root; 16]), root_path),
            kind,
        )
    }

    fn token_count(&self) -> usize {
        fs::read_dir(self.directory.path().join("tokens"))
            .unwrap()
            .count()
    }

    fn evidence(&self, root: u8) -> Value {
        serde_json::from_slice(&fs::read(self.root(root).join("runtime-evidence.json")).unwrap())
            .unwrap()
    }

    fn fail_home(&self) {
        use beryl_home_store::HomeCommand;
        use beryl_home_store::test_faults::FaultPoint;
        use beryl_state::{
            ApplySettings, ExpectedSettingRevision, SettingKey, SettingUpdate, SettingValue,
        };
        let live = self.service().live_home_command().unwrap();
        let home = live.home();
        let update = SettingUpdate::new(
            SettingKey::DeveloperInstructions,
            ExpectedSettingRevision::Absent,
            SettingValue::developer_instructions("runtime retirement cut").unwrap(),
        );
        let contribution = self.state.settings().apply(
            self.state.settings().revision(home).unwrap(),
            ApplySettings::new(vec![update]).unwrap(),
        );
        let mut command = HomeCommand::new(home.home_revision().unwrap());
        command.add(contribution).unwrap();
        self.faults.panic_next(FaultPoint::BeforeCommit);
        let outcome =
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| home.execute(command)));
        assert!(outcome.is_err());
    }
}

fn canonical_path(path: &Path) -> String {
    let canonical = path.canonicalize().unwrap();
    let text = canonical.to_str().unwrap();
    text.strip_prefix(r"\\?\").unwrap_or(text).to_owned()
}

fn native(path: &str) -> RuntimeNativePath {
    RuntimeNativePath::from_admitted(RuntimeMode::Host, PathFlavor::Windows, path).unwrap()
}

fn ready(interest: &RuntimeInterest) -> RuntimeReadiness {
    match interest.wait_for_change(RuntimeInterestStatus::Starting, TIMEOUT) {
        RuntimeInterestStatus::Ready(ready) => ready,
        status => panic!("real managed runtime did not become ready: {status:?}"),
    }
}

fn wait_until(mut condition: impl FnMut() -> bool) {
    let deadline = Instant::now() + TIMEOUT;
    while !condition() {
        assert!(
            Instant::now() < deadline,
            "managed runtime condition timed out"
        );
        thread::sleep(Duration::from_millis(2));
    }
}

struct ProcessWitness(HANDLE);

impl ProcessWitness {
    fn open(pid: u32) -> Self {
        Self(unsafe { OpenProcess(PROCESS_SYNCHRONIZE, false, pid).unwrap() })
    }

    fn running(&self) -> bool {
        unsafe { WaitForSingleObject(self.0, 0) == WAIT_TIMEOUT }
    }

    fn assert_exited(&self) {
        assert_eq!(
            unsafe { WaitForSingleObject(self.0, TIMEOUT.as_millis() as u32) },
            WAIT_OBJECT_0
        );
    }
}

impl Drop for ProcessWitness {
    fn drop(&mut self) {
        let _ = unsafe { CloseHandle(self.0) };
    }
}

#[test]
fn real_managed_launch_admits_once_and_required_work_keeps_the_process_alive() {
    let mut fixture = Fixture::new();
    let view = fixture.acquire(1, RuntimeInterestKind::View).unwrap();
    let first = ready(&view);
    let evidence = fixture.evidence(1);
    assert_eq!(evidence["authenticated"], true);
    assert_eq!(evidence["foreground"], true);
    assert_eq!(
        evidence["methods"],
        serde_json::json!(["initialize", "initialized", "config/read"])
    );
    let process = ProcessWitness::open(evidence["pid"].as_u64().unwrap() as u32);
    assert!(process.running());
    assert_eq!(fixture.token_count(), 1);
    assert_eq!(fixture.service().worker_pool_diagnostics().active(), 2);
    let work = fixture
        .acquire(2, RuntimeInterestKind::RequiredWork)
        .unwrap();
    assert_eq!(ready(&work), first);
    drop(view);
    assert!(work.is_current(first.activity_period()));
    assert!(process.running());
    drop(work);
    wait_until(|| {
        fixture.token_count() == 0 && fixture.service().worker_pool_diagnostics().active() == 0
    });
    process.assert_exited();
    let mut replacement = None;
    wait_until(|| match fixture.acquire(2, RuntimeInterestKind::View) {
        Ok(interest) => {
            replacement = Some(interest);
            true
        }
        Err(RuntimeInterestError::Retiring) => false,
        Err(error) => panic!("fresh runtime failed: {error}"),
    });
    let replacement = replacement.unwrap();
    assert_ne!(
        ready(&replacement).activity_period(),
        first.activity_period()
    );
    drop(replacement);
    assert!(matches!(
        fixture.service.take().unwrap().close().unwrap(),
        ProjectionConnectionServiceCloseOutcome::Closed
    ));
    assert_eq!(fixture.token_count(), 0);
}

#[test]
fn real_admission_failure_removes_launch_token_and_releases_app_workers() {
    let mut fixture = Fixture::new();
    fs::write(fixture.root(1).join("fixture-mode"), "reject-config").unwrap();
    let interest = fixture.acquire(1, RuntimeInterestKind::View).unwrap();
    assert_eq!(
        interest.wait_for_change(RuntimeInterestStatus::Starting, TIMEOUT),
        RuntimeInterestStatus::Unavailable(RuntimeFailure::Admission)
    );
    assert_eq!(fixture.evidence(1)["rejected_config"], true);
    assert_eq!(fixture.token_count(), 0);
    assert_eq!(fixture.service().worker_pool_diagnostics().active(), 0);
    assert!(matches!(
        fixture.service.take().unwrap().close().unwrap(),
        ProjectionConnectionServiceCloseOutcome::Closed
    ));
    assert_eq!(interest.status(), RuntimeInterestStatus::Retired);
}

#[test]
fn service_close_rejects_a_late_real_admission_and_joins_registered_resources() {
    let mut fixture = Fixture::new();
    fs::write(fixture.root(1).join("fixture-mode"), "pause-config").unwrap();
    let interest = fixture
        .acquire(1, RuntimeInterestKind::RequiredWork)
        .unwrap();
    wait_until(|| fixture.root(1).join("runtime-evidence.json").exists());
    let process = ProcessWitness::open(fixture.evidence(1)["pid"].as_u64().unwrap() as u32);
    let service = fixture.service.take().unwrap();
    let close = thread::spawn(move || service.close());
    wait_until(|| interest.status() == RuntimeInterestStatus::Retired);
    fs::write(fixture.root(1).join("release-config"), "release").unwrap();
    assert!(matches!(
        close.join().unwrap().unwrap(),
        ProjectionConnectionServiceCloseOutcome::Closed
    ));
    process.assert_exited();
    assert_eq!(fixture.token_count(), 0);
}

#[test]
fn persistent_failure_cut_finishes_before_runtime_retirement_detaches_its_connection() {
    let mut fixture = Fixture::new();
    let interest = fixture
        .acquire(1, RuntimeInterestKind::RequiredWork)
        .unwrap();
    let prior = ready(&interest);
    let process = ProcessWitness::open(fixture.evidence(1)["pid"].as_u64().unwrap() as u32);
    let admitted = fixture.service().live_home_command().unwrap();
    fixture.fail_home();
    wait_until(|| fixture.service().runtime_retirement_waiters_for_test() == 1);
    assert!(!interest.is_current(prior.activity_period()));
    assert!(process.running());
    assert_eq!(fixture.service().worker_pool_diagnostics().active(), 2);
    drop(admitted);
    let outcome = fixture.service.take().unwrap().close().unwrap();
    match outcome {
        ProjectionConnectionServiceCloseOutcome::PersistentFailure(evidence) => {
            assert_eq!(
                evidence.completion(),
                PersistentFailureCutCompletion::Finished
            );
        }
        ProjectionConnectionServiceCloseOutcome::Closed => {
            panic!("persistent failure lost its close election")
        }
    }
    process.assert_exited();
    assert_eq!(fixture.token_count(), 0);
}

#[test]
fn ordinary_service_drop_does_not_wait_for_a_pending_managed_admission() {
    let mut fixture = Fixture::new();
    fs::write(fixture.root(1).join("fixture-mode"), "pause-config").unwrap();
    let interest = fixture.acquire(1, RuntimeInterestKind::View).unwrap();
    wait_until(|| fixture.root(1).join("runtime-evidence.json").exists());
    let process = ProcessWitness::open(fixture.evidence(1)["pid"].as_u64().unwrap() as u32);
    let workers = fixture.service().worker_pool_observer_for_test();
    let service = fixture.service.take().unwrap();
    let (dropped, observed) = std::sync::mpsc::sync_channel(1);
    let dropper = thread::spawn(move || {
        drop(service);
        dropped.send(()).unwrap();
    });
    observed
        .recv_timeout(Duration::from_secs(1))
        .expect("ordinary Drop must not wait for admission");
    dropper.join().unwrap();
    assert_eq!(interest.status(), RuntimeInterestStatus::Retired);
    assert!(process.running());
    fs::write(fixture.root(1).join("release-config"), "release").unwrap();
    process.assert_exited();
    wait_until(|| fixture.token_count() == 0);
    assert_eq!(workers().active(), 0);
}

#[test]
fn ordinary_service_drop_preserves_ready_connection_custody_until_joined_disposal() {
    let mut fixture = Fixture::new();
    let interest = fixture.acquire(1, RuntimeInterestKind::View).unwrap();
    ready(&interest);
    let process = ProcessWitness::open(fixture.evidence(1)["pid"].as_u64().unwrap() as u32);
    let workers = fixture.service().worker_pool_observer_for_test();
    assert_eq!(workers().active(), 2);
    drop(fixture.service.take());
    assert_eq!(interest.status(), RuntimeInterestStatus::Retired);
    process.assert_exited();
    wait_until(|| fixture.token_count() == 0);
    assert_eq!(workers().active(), 0);
}
