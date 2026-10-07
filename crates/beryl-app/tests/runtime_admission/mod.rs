pub use beryl_app::runtime_admission::validation::{RuntimePathValidator, ValidationIssue};
use beryl_app::runtime_admission::validation::{
    RuntimeQualificationBackend, RuntimeQualificationCandidate, ValidationCleanup, ValidationError,
    ValidationFilesystem, ValidationFilesystemPath, ValidationLimits,
};
pub use beryl_app::runtime_admission::{
    AdmissionReconciliation, AdmissionReconciliationOutcome, CommittedAdmission,
    RuntimeAdmissionOutcome, RuntimeAdmissionService, RuntimeAdmissionSource,
};
use beryl_app::{
    cas_projection::RuntimeTokenDirectory,
    process_admission::ProcessAdmissionGate,
    window_acquisition::{
        RuntimeBackedWindowMainWindowReservation, RuntimeBackedWindowProcessRegistry,
    },
};
pub use beryl_home_store::{CommandCancellation, test_faults::FaultPoint};
use beryl_home_store::{
    CommandOutcome, CursorReadLimits, HomeCommand, HomeOpenCandidate, HomeOpenOptions,
    HomeSchemaVersion, HomeStore, test_faults::FaultController,
};
use beryl_model::{
    AdmittedHostPath, PathFlavor, RuntimeId, RuntimeMode, RuntimeNativePath, WindowBounds,
    WindowDisplayState, WindowId, WindowPlacement,
};
use beryl_state::{
    BerylState, CatalogPointReadLimit, InitializeThreadlessWindow, MinimalSessionBootstrap,
    RememberedTarget, UpdateWindowPlacement,
};
pub use std::{path::Path, sync::Arc};
use std::{
    sync::{
        Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};
use syndic_storage::{DraftEditHistoryPolicyV1, SyndicPointReadLimit, SyndicStorage};

pub const WINDOW: WindowId = WindowId::from_bytes([42; 16]);
pub const EXECUTABLE: &str = r"C:\bin\codex.exe";
pub const ALIAS: &str = r"C:\alias\codex.exe";
pub const HOME: &str = r"C:\Users\Operator";

mod injected;
use injected::BackendSeam;
pub use injected::{Backend, Filesystem};

pub struct Fixture {
    pub service: RuntimeAdmissionService,
    pub store: HomeStore,
    pub state: BerylState,
    pub syndic: SyndicStorage,
    pub faults: FaultController,
    pub filesystem: Arc<Filesystem>,
    pub backend: Arc<Backend>,
    pub process: RuntimeBackedWindowProcessRegistry,
    pub process_gate: ProcessAdmissionGate,
    _resident: Option<RuntimeBackedWindowMainWindowReservation>,
    _directory: tempfile::TempDir,
}

impl Fixture {
    pub fn recover_generation(self) -> Self {
        self.recover(None)
    }

    pub fn recover_exact_old(self, reconciliation: AdmissionReconciliation) -> Self {
        self.recover(Some(reconciliation))
    }

    fn recover(self, reconciliation: Option<AdmissionReconciliation>) -> Self {
        let Self {
            service,
            store,
            faults,
            filesystem,
            backend,
            process,
            process_gate,
            _resident,
            _directory,
            ..
        } = self;
        service.retire().unwrap();
        drop(service);
        if store.health().state() == beryl_home_store::HomeHealthState::Healthy {
            faults.fail_next(FaultPoint::BeforeReadConfirmation);
            assert!(store.home_revision().is_err());
        }
        let mut candidate = store.recover_same_home().unwrap();
        if let Some(reconciliation) = reconciliation {
            assert!(matches!(
                reconciliation.reconcile_candidate(&candidate.recovery_access().unwrap()),
                AdmissionReconciliationOutcome::NotCommitted
            ));
        }
        let store = candidate.publish().unwrap();
        let state = BerylState::reacquire(&store).unwrap();
        let syndic = SyndicStorage::reacquire(&store).unwrap();
        let validator = RuntimePathValidator::with_test_seams(
            RuntimeTokenDirectory::from_admitted(
                AdmittedHostPath::from_admitted(PathFlavor::Windows, r"C:\tokens").unwrap(),
            ),
            ValidationLimits::default(),
            filesystem.clone(),
            Arc::new(BackendSeam(backend.clone())),
        )
        .unwrap();
        let service = RuntimeAdmissionService::new(
            Arc::new(store.service_reference()),
            state.clone(),
            syndic.clone(),
            process.clone(),
            validator,
            DraftEditHistoryPolicyV1::new(65_536, 1).unwrap(),
        );
        Self {
            service,
            store,
            state,
            syndic,
            faults,
            filesystem,
            backend,
            process,
            process_gate,
            _resident,
            _directory,
        }
    }

    pub fn new() -> Self {
        let directory = tempfile::tempdir().unwrap();
        let faults = FaultController::new();
        let mut candidate = HomeOpenCandidate::open_with_faults(
            HomeOpenOptions::new(directory.path(), HomeSchemaVersion::CURRENT),
            faults.clone(),
        )
        .unwrap();
        let state = BerylState::register(&mut candidate).unwrap();
        let syndic = SyndicStorage::register(&mut candidate).unwrap();
        let store = candidate
            .prepare_publication(
                BerylState::required_domains()
                    .unwrap()
                    .merge(SyndicStorage::required_domains().unwrap())
                    .unwrap(),
            )
            .unwrap()
            .publish()
            .unwrap();
        let session = state.session();
        let mut command = HomeCommand::new(store.home_revision().unwrap());
        command
            .add(session.initialize_threadless(
                session.revision(&store).unwrap(),
                InitializeThreadlessWindow::new(WINDOW, placement(10)),
            ))
            .unwrap();
        assert!(matches!(
            store.execute(command),
            CommandOutcome::Committed { .. }
        ));
        let process_gate = ProcessAdmissionGate::new();
        let process = RuntimeBackedWindowProcessRegistry::new(process_gate.clone());
        let resident = process.reserve_main_window(WINDOW).unwrap();
        let filesystem = Arc::new(Filesystem::default());
        let backend = Arc::new(Backend::default());
        let validator = RuntimePathValidator::with_test_seams(
            RuntimeTokenDirectory::from_admitted(
                AdmittedHostPath::from_admitted(PathFlavor::Windows, r"C:\tokens").unwrap(),
            ),
            ValidationLimits::default(),
            filesystem.clone(),
            Arc::new(BackendSeam(backend.clone())),
        )
        .unwrap();
        let service = RuntimeAdmissionService::new(
            Arc::new(store.service_reference()),
            state.clone(),
            syndic.clone(),
            process.clone(),
            validator,
            DraftEditHistoryPolicyV1::new(65_536, 1).unwrap(),
        );
        Self {
            service,
            store,
            state,
            syndic,
            faults,
            filesystem,
            backend,
            process,
            process_gate,
            _resident: Some(resident),
            _directory: directory,
        }
    }

    pub fn source(&self) -> RuntimeAdmissionSource {
        self.service.capture_source(WINDOW).unwrap()
    }
    pub fn release_window_reservation(&mut self) {
        drop(self._resident.take());
    }
    pub fn session(&self) -> MinimalSessionBootstrap {
        self.state
            .session()
            .minimal_bootstrap(&self.store)
            .unwrap()
            .unwrap()
    }
    pub fn add_runtime(&self, selected: &str) -> RuntimeAdmissionOutcome {
        self.add_runtime_with_form(selected, beryl_model::RuntimeLaunchForm::CodexCli)
    }

    pub fn add_runtime_with_form(
        &self,
        selected: &str,
        launch_form: beryl_model::RuntimeLaunchForm,
    ) -> RuntimeAdmissionOutcome {
        self.service.add_runtime(
            self.source(),
            &[WINDOW],
            Path::new(selected),
            launch_form,
            CommandCancellation::new(),
        )
    }
    pub fn add_root(&self, runtime: RuntimeId, selected: &str) -> RuntimeAdmissionOutcome {
        self.service.add_root(
            self.source(),
            &[WINDOW],
            runtime,
            Path::new(selected),
            CommandCancellation::new(),
        )
    }
    pub fn assert_selection_held(&self) {
        assert!(self.process.test_close_is_blocked(&[WINDOW]));
        assert!(self.process.test_process_closing_is_blocked());
        assert!(
            self.process
                .test_acquisition_is_blocked(WindowId::from_bytes([43; 16]))
        );
    }
    pub fn assert_selection_released(&self) {
        assert!(!self.process.test_close_is_blocked(&[WINDOW]));
        assert!(!self.process.test_process_closing_is_blocked());
        assert!(
            !self
                .process
                .test_acquisition_is_blocked(WindowId::from_bytes([43; 16]))
        );
    }
    pub fn assert_empty(&self) {
        assert!(
            self.state
                .runtime_roots()
                .list_runtimes(
                    &self.store,
                    None,
                    CursorReadLimits::new(10, 32_768).unwrap()
                )
                .unwrap()
                .records()
                .is_empty()
        );
        let session = self.session();
        assert_eq!(session.windows().len(), 1);
        assert!(session.windows()[0].selected_thread().is_none());
        assert!(session.windows()[0].remembered_target().is_none());
        assert!(session.header().fallback().is_none());
        assert!(
            self.state
                .catalog()
                .recency_page(
                    &self.store,
                    None,
                    CursorReadLimits::new(10, 32_768).unwrap(),
                )
                .unwrap()
                .rows()
                .is_empty()
        );
    }
    pub fn change_placement(&self) {
        let session = self.session();
        let mut command = HomeCommand::new(self.store.home_revision().unwrap());
        command
            .add(self.state.session().update_placement(
                self.state.session().revision(&self.store).unwrap(),
                UpdateWindowPlacement::new(
                    session.header().revision(),
                    WINDOW,
                    session.windows()[0].revision(),
                    placement(20),
                ),
            ))
            .unwrap();
        assert!(matches!(
            self.store.execute(command),
            CommandOutcome::Committed { .. }
        ));
    }
    pub fn assert_closure(&self, admission: &CommittedAdmission) {
        let facts = admission.facts();
        let onboarding = facts.onboarding().expect("first runtime onboarding");
        let target = RememberedTarget::new(facts.runtime_id(), facts.root_id());
        let session = self.session();
        assert_eq!(session.windows().len(), 1);
        assert_eq!(session.windows()[0].window_id(), WINDOW);
        assert_eq!(&session.windows()[0], onboarding.window());
        assert_eq!(
            session.windows()[0].selected_thread(),
            Some(onboarding.claim())
        );
        assert_eq!(session.windows()[0].remembered_target(), Some(target));
        assert_eq!(session.header().fallback(), Some(target));
        let runtime = self
            .state
            .runtime_roots()
            .runtime(&self.store, facts.runtime_id())
            .unwrap()
            .unwrap();
        assert_eq!(runtime.mode(), &RuntimeMode::Host);
        assert_eq!(runtime.canonical_executable().as_str(), EXECUTABLE);
        let root = self
            .state
            .runtime_roots()
            .root_by_path(
                &self.store,
                facts.runtime_id(),
                &RuntimeNativePath::from_admitted(RuntimeMode::Host, PathFlavor::Windows, HOME)
                    .unwrap(),
            )
            .unwrap()
            .unwrap();
        assert_eq!(root.root_id(), facts.root_id());
        assert_eq!(root.runtime_id(), facts.runtime_id());
        assert!(root.non_removable());
        let draft = self
            .syndic
            .current_draft(
                &self.store,
                onboarding.thread_id(),
                SyndicPointReadLimit::new(32_768).unwrap(),
            )
            .unwrap()
            .unwrap();
        assert_eq!(draft.thread().current_draft_id(), onboarding.draft_id());
        assert_eq!(draft.draft().id(), onboarding.draft_id());
        assert_eq!(draft.draft().thread_id(), onboarding.thread_id());
        assert_eq!(draft.root().reference().summary().logical_utf8_bytes(), 0);
        assert_eq!(draft.root().reference().summary().marker_count(), 0);
        let row = self
            .state
            .catalog()
            .row(
                &self.store,
                onboarding.thread_id(),
                CatalogPointReadLimit::schema_maximum(),
            )
            .unwrap()
            .unwrap();
        assert_eq!(row.facts().execution().runtime_id(), facts.runtime_id());
        assert_eq!(row.facts().execution().root_id(), facts.root_id());
        assert_eq!(row.facts().claim().window_id(), Some(WINDOW));
    }
}

fn placement(x: i32) -> WindowPlacement {
    WindowPlacement::new(
        WindowBounds::new(x, 20, 900, 700).unwrap(),
        WindowDisplayState::Normal,
        None,
        None,
    )
}
pub fn committed(outcome: RuntimeAdmissionOutcome) -> CommittedAdmission {
    match outcome {
        RuntimeAdmissionOutcome::Committed {
            admission,
            later_failure,
            ..
        } => {
            assert!(later_failure.is_none());
            admission
        }
        RuntimeAdmissionOutcome::NotCommitted { error } => panic!("expected commit: {error}"),
        _ => panic!("expected committed admission"),
    }
}
pub fn assert_refused(outcome: RuntimeAdmissionOutcome) {
    assert!(matches!(
        outcome,
        RuntimeAdmissionOutcome::NotCommitted { .. }
    ));
}
pub fn indeterminate(outcome: RuntimeAdmissionOutcome) -> AdmissionReconciliation {
    match outcome {
        RuntimeAdmissionOutcome::Indeterminate { reconciliation, .. } => reconciliation,
        RuntimeAdmissionOutcome::NotCommitted { error } => {
            panic!("expected indeterminate: {error}")
        }
        _ => panic!("expected indeterminate admission"),
    }
}
