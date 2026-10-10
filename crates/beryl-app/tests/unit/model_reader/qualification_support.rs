use super::{ModelPage, ModelQuery, ModelReaderLimits, PublishedModelReader};
use crate::cas_projection::{
    AdmittedProjectionSession, MinimumTurnCaptureReserve, ProcessScheduledExecutionProvider,
    ProjectionConnectionService, ProjectionConnectionServiceCloseOutcome, ProjectionServiceConfig,
    RuntimeInterestConfig, ScheduledExecutionSessions,
};
use crate::runtime_activity_enrollment::RuntimeActivityEnrollmentOperations;
use beryl_backend::ManagedBackendClientConnector;
use beryl_home_store::{
    CommandOutcome, HomeCommand, HomeOpenCandidate, HomeOpenOptions, HomeSchemaVersion, HomeStore,
    MutationContribution,
};
use beryl_model::{
    CasProcessGeneration, ExecutionBinding, PathFlavor, RootId, RuntimeId, RuntimeMode,
    RuntimeNativePath, SyndicDraftId, SyndicThreadId, WindowBounds, WindowDisplayState, WindowId,
    WindowPlacement,
};
use beryl_state::{
    BerylState, CreateClaimedWindow, InitializeThreadlessWindow, RememberedTarget,
    ReplaceWindowClaim, SessionState, WindowClaimSelection,
};
use std::{num::NonZeroUsize, path::Path, sync::Arc, time::Duration};
use syndic_storage::{CreateThread, DraftEditHistoryPolicyV1, SyndicStorage, SyndicTimestamp};

mod protocol {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/unit/model_reader/protocol.rs"
    ));
}
pub(crate) use protocol::{AUTHORIZATION, HeldResponse, ModelResponse, ProtocolServer};
pub(crate) const TIMEOUT: Duration = Duration::from_secs(5);

pub(crate) struct Fixture {
    pub(crate) reader: PublishedModelReader,
    pub(crate) window: WindowId,
    pub(crate) claim: WindowClaimSelection,
    pub(crate) execution: ExecutionBinding,
    pub(crate) second_window: WindowId,
    pub(crate) second_claim: WindowClaimSelection,
    pub(crate) second_execution: ExecutionBinding,
    pub(crate) session: SessionState,
    pub(crate) sessions: ScheduledExecutionSessions,
    pub(crate) storage: SyndicStorage,
    pub(crate) server: ProtocolServer,
    pub(crate) lifetime: Arc<()>,
    admitted: Option<AdmittedProjectionSession>,
    service: Option<ProjectionConnectionService>,
    _directory: tempfile::TempDir,
}

pub(crate) fn execute(home: &HomeStore, contribution: MutationContribution) {
    let mut command = HomeCommand::new(home.home_revision().unwrap());
    command.add(contribution).unwrap();
    let outcome = home.execute(command);
    assert!(
        matches!(
            outcome,
            CommandOutcome::Committed {
                later_failure: None,
                ..
            }
        ),
        "{outcome:?}"
    );
}

fn placement() -> WindowPlacement {
    WindowPlacement::new(
        WindowBounds::new(0, 0, 900, 700).unwrap(),
        WindowDisplayState::Normal,
        None,
        None,
    )
}

fn binding(root: u8, path: &str) -> ExecutionBinding {
    ExecutionBinding::new(
        RuntimeId::from_bytes([32; 16]),
        RootId::from_bytes([root; 16]),
        RuntimeNativePath::from_admitted(RuntimeMode::host(), PathFlavor::Windows, path).unwrap(),
    )
}

impl Fixture {
    pub(crate) fn new() -> Self {
        Self::with_limits(64, TIMEOUT)
    }

    pub(crate) fn with_limits(records: u32, timeout: Duration) -> Self {
        Self::with_capacity(records, timeout, 2)
    }

    pub(crate) fn with_page_capacity(pages: usize) -> Self {
        Self::with_capacity(64, TIMEOUT, pages)
    }

    fn with_capacity(records: u32, timeout: Duration, pages: usize) -> Self {
        let directory = tempfile::tempdir().unwrap();
        eprintln!("model-reader Home: {}", directory.path().display());
        let mut candidate = HomeOpenCandidate::open(HomeOpenOptions::new(
            directory.path(),
            HomeSchemaVersion::CURRENT,
        ))
        .unwrap();
        let storage = SyndicStorage::register(&mut candidate).unwrap();
        let state = BerylState::register(&mut candidate).unwrap();
        let home = candidate
            .prepare_publication(
                BerylState::required_domains()
                    .unwrap()
                    .merge(SyndicStorage::required_domains().unwrap())
                    .unwrap(),
            )
            .unwrap()
            .publish()
            .unwrap();
        let execution = binding(33, r"C:\model-root-one");
        let second_execution = binding(34, r"C:\model-root-two");
        for (seed, binding) in [(41, &execution), (42, &second_execution)] {
            execute(
                &home,
                storage.create_thread(
                    storage.revision(&home).unwrap(),
                    CreateThread::ordinary(
                        SyndicThreadId::from_bytes([seed; 16]),
                        SyndicDraftId::from_bytes([seed; 16]),
                        binding.clone(),
                        SyndicTimestamp::from_unix_millis(1),
                        DraftEditHistoryPolicyV1::new(65_536, 1).unwrap(),
                    ),
                ),
            );
        }
        let session = state.session();
        let window = WindowId::from_bytes([61; 16]);
        let second_window = WindowId::from_bytes([62; 16]);
        execute(
            &home,
            session.initialize_threadless(
                session.revision(&home).unwrap(),
                InitializeThreadlessWindow::new(window, placement()),
            ),
        );
        let snapshot = session.minimal_bootstrap(&home).unwrap().unwrap();
        execute(
            &home,
            session.replace_claim(
                session.revision(&home).unwrap(),
                ReplaceWindowClaim::new(
                    snapshot.header().revision(),
                    window,
                    snapshot.windows()[0].revision(),
                    None,
                    RememberedTarget::new(execution.runtime_id(), execution.root_id()),
                    SyndicThreadId::from_bytes([41; 16]),
                ),
            ),
        );
        let snapshot = session.minimal_bootstrap(&home).unwrap().unwrap();
        execute(
            &home,
            session.create_claimed_window(
                session.revision(&home).unwrap(),
                CreateClaimedWindow::new(
                    snapshot.header().revision(),
                    second_window,
                    RememberedTarget::new(
                        second_execution.runtime_id(),
                        second_execution.root_id(),
                    ),
                    SyndicThreadId::from_bytes([42; 16]),
                    placement(),
                ),
            ),
        );
        let snapshot = session.minimal_bootstrap(&home).unwrap().unwrap();
        let claim = snapshot
            .windows()
            .iter()
            .find(|entry| entry.window_id() == window)
            .unwrap()
            .selected_thread()
            .unwrap();
        let second_claim = snapshot
            .windows()
            .iter()
            .find(|entry| entry.window_id() == second_window)
            .unwrap()
            .selected_thread()
            .unwrap();
        let enrollments =
            RuntimeActivityEnrollmentOperations::new(home.home_id(), NonZeroUsize::new(1).unwrap());
        let (provider, sessions) = ProcessScheduledExecutionProvider::new();
        let mut service = ProjectionConnectionService::new(
            Default::default(),
            home,
            storage.clone(),
            ProjectionServiceConfig::try_new(8, 8, MinimumTurnCaptureReserve::try_new(1).unwrap())
                .unwrap(),
            Box::new(provider),
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
                enrollments,
            )
            .unwrap();
        let server = ProtocolServer::new();
        let connector = ManagedBackendClientConnector::for_lifecycle_test(
            server.endpoint(),
            protocol::AUTHORIZATION,
        );
        let generation = CasProcessGeneration::new(73_001).unwrap();
        let admitted = service
            .admit_runtime_lifecycle_test_candidate(
                &connector,
                execution.clone(),
                generation,
                Path::new(execution.root_path().as_str()),
                TIMEOUT,
            )
            .unwrap_or_else(|error| {
                panic!(
                    "model reader admission failed: {error:?}; {}",
                    server.diagnostics()
                )
            });
        service.attach_model_connector_for_test(&execution, generation, connector);
        let lifetime = Arc::new(());
        let reader = PublishedModelReader::new(
            service.model_read_source(),
            session.clone(),
            Arc::downgrade(&lifetime),
            ModelReaderLimits::new(
                NonZeroUsize::new(4).unwrap(),
                NonZeroUsize::new(pages).unwrap(),
                records,
                timeout,
            )
            .unwrap(),
        );
        Self {
            reader,
            window,
            claim,
            execution,
            second_window,
            second_claim,
            second_execution,
            session,
            sessions,
            storage,
            server,
            lifetime,
            admitted: Some(admitted),
            service: Some(service),
            _directory: directory,
        }
    }

    pub(crate) fn query(&self) -> Arc<ModelQuery> {
        self.reader
            .prepare(self.window, self.claim, self.execution.clone())
            .unwrap()
    }

    pub(crate) fn first_page(
        &self,
        count: usize,
        next: Option<&str>,
    ) -> (Arc<ModelQuery>, ModelPage) {
        let query = self.query();
        self.server.enqueue(ModelResponse::page(count, next));
        let page = query.read_page(None).unwrap();
        (query, page)
    }

    pub(crate) fn home(&self) -> &HomeStore {
        self.service.as_ref().unwrap().home_for_shutdown_test()
    }

    pub(crate) fn service(&self) -> &ProjectionConnectionService {
        self.service.as_ref().unwrap()
    }

    pub(crate) fn close_service(&mut self) {
        self.admitted.take();
        if let Some(service) = self.service.take() {
            match service.close() {
                Ok(ProjectionConnectionServiceCloseOutcome::Closed) => {}
                Ok(other) => {
                    panic!("model reader service returned unexpected close outcome: {other:?}")
                }
                Err(error) => panic!("model reader service close failed: {error:?}"),
            }
        }
    }

    pub(crate) fn retry_runtime(&mut self) {
        let connector = ManagedBackendClientConnector::for_lifecycle_test(
            self.server.endpoint(),
            AUTHORIZATION,
        );
        let successor = self
            .service()
            .retry_model_runtime_for_test(
                self.execution.clone(),
                &connector,
                CasProcessGeneration::new(73_002).unwrap(),
                TIMEOUT,
            )
            .unwrap();
        self.admitted = Some(successor);
    }

    pub(crate) fn unrelated_write(&self) {
        execute(
            self.home(),
            self.storage.create_thread(
                self.storage.revision(self.home()).unwrap(),
                CreateThread::ordinary(
                    SyndicThreadId::from_bytes([43; 16]),
                    SyndicDraftId::from_bytes([43; 16]),
                    self.execution.clone(),
                    SyndicTimestamp::from_unix_millis(2),
                    DraftEditHistoryPolicyV1::new(65_536, 1).unwrap(),
                ),
            ),
        );
    }

    pub(crate) fn replace_claim(&self) {
        self.unrelated_write();
        let snapshot = self
            .session
            .minimal_bootstrap(self.home())
            .unwrap()
            .unwrap();
        let selected = snapshot
            .windows()
            .iter()
            .find(|entry| entry.window_id() == self.window)
            .unwrap();
        execute(
            self.home(),
            self.session.replace_claim(
                self.session.revision(self.home()).unwrap(),
                ReplaceWindowClaim::new(
                    snapshot.header().revision(),
                    self.window,
                    selected.revision(),
                    Some(self.claim),
                    RememberedTarget::new(self.execution.runtime_id(), self.execution.root_id()),
                    SyndicThreadId::from_bytes([43; 16]),
                ),
            ),
        );
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        self.close_service();
    }
}
