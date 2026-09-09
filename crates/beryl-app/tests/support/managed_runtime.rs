use std::{
    fs,
    num::NonZeroUsize,
    path::{Path, PathBuf},
    thread,
    time::{Duration, Instant},
};

use beryl_app::cas_projection::{
    MinimumTurnCaptureReserve, ProjectionConnectionService, ProjectionServiceConfig,
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

pub(crate) const TIMEOUT: Duration = Duration::from_secs(10);

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

pub(crate) struct Fixture {
    pub(crate) service: Option<ProjectionConnectionService>,
    directory: tempfile::TempDir,
    pub(crate) state: BerylState,
    faults: beryl_home_store::test_faults::FaultController,
}

impl Fixture {
    pub(crate) fn new() -> Self {
        Self::with_provider(Box::new(NoQueuedExecution))
    }

    pub(crate) fn with_provider(provider: Box<dyn ScheduledOrdinaryExecutionProvider>) -> Self {
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
            ProjectionServiceConfig::try_new(8, 8, MinimumTurnCaptureReserve::try_new(1).unwrap())
                .unwrap(),
            provider,
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

    pub(crate) fn service(&self) -> &ProjectionConnectionService {
        self.service.as_ref().unwrap()
    }

    pub(crate) fn root(&self, root: u8) -> PathBuf {
        self.directory.path().join(format!("root-{root}"))
    }

    pub(crate) fn acquire(
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

    pub(crate) fn token_count(&self) -> usize {
        fs::read_dir(self.directory.path().join("tokens"))
            .unwrap()
            .count()
    }

    pub(crate) fn evidence(&self, root: u8) -> Value {
        serde_json::from_slice(&fs::read(self.root(root).join("runtime-evidence.json")).unwrap())
            .unwrap()
    }

    pub(crate) fn fail_home(&self) {
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

pub(crate) fn canonical_path(path: &Path) -> String {
    let canonical = path.canonicalize().unwrap();
    let text = canonical.to_str().unwrap();
    text.strip_prefix(r"\\?\").unwrap_or(text).to_owned()
}

pub(crate) fn native(path: &str) -> RuntimeNativePath {
    RuntimeNativePath::from_admitted(RuntimeMode::Host, PathFlavor::Windows, path).unwrap()
}

pub(crate) fn ready(interest: &RuntimeInterest) -> RuntimeReadiness {
    match interest.wait_for_change(RuntimeInterestStatus::Starting, TIMEOUT) {
        RuntimeInterestStatus::Ready(ready) => ready,
        status => panic!("real managed runtime did not become ready: {status:?}"),
    }
}

pub(crate) fn wait_until(mut condition: impl FnMut() -> bool) {
    let deadline = Instant::now() + TIMEOUT;
    while !condition() {
        assert!(
            Instant::now() < deadline,
            "managed runtime condition timed out"
        );
        thread::sleep(Duration::from_millis(2));
    }
}

pub(crate) struct ProcessWitness(HANDLE);

impl ProcessWitness {
    pub(crate) fn open(pid: u32) -> Self {
        Self(unsafe { OpenProcess(PROCESS_SYNCHRONIZE, false, pid).unwrap() })
    }

    pub(crate) fn running(&self) -> bool {
        unsafe { WaitForSingleObject(self.0, 0) == WAIT_TIMEOUT }
    }

    pub(crate) fn assert_exited(&self) {
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
