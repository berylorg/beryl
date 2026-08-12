use std::time::Duration;

use beryl_app::catalog_projection::{
    CatalogProjectionBuildError, ThreadCatalogProjectionPreparation,
    prepare_thread_catalog_projection,
};
use beryl_app::draft_persistence::{
    DraftAutosavePublication, DraftFlushAction, DraftPersistenceService, DraftPersistenceTime,
    execute_draft_save, read_draft_persistence_seed,
};
use beryl_home_store::{
    CommandOutcome, HomeCommand, HomeOpenOptions, HomeSchemaVersion, HomeStore,
};
use beryl_model::{
    AdmittedHostPath, Availability, ExecutionBinding, PathFlavor, RootId, RuntimeId, RuntimeMode,
    RuntimeNativePath, SyndicDraftId, SyndicThreadId, WindowBounds, WindowDisplayState, WindowId,
    WindowPlacement,
};
use beryl_state::{
    AvailabilitySnapshot, BerylState, CatalogClaimKind, CatalogPointReadLimit, CatalogTitleSource,
    CreateRuntimeWithHomeRoot, InitializeThreadlessWindow, RememberedTarget, ReplaceWindowClaim,
    RootRegistration, RuntimeRegistration, UnixMillis,
};
use syndic_storage::{
    ComposerAtom, ComposerPayload, CreateThread, SyndicPointReadLimit, SyndicStorage,
    SyndicTimestamp,
};

struct Fixture {
    _directory: tempfile::TempDir,
    store: HomeStore,
    state: BerylState,
    syndic: SyndicStorage,
    thread_id: SyndicThreadId,
    runtime_id: RuntimeId,
    root_id: RootId,
}

impl Fixture {
    fn new(binding_path: &str) -> Self {
        let directory = tempfile::tempdir().expect("temp home");
        let mut store = HomeStore::open(HomeOpenOptions::new(
            directory.path(),
            HomeSchemaVersion::CURRENT,
        ))
        .expect("open home");
        let state = BerylState::register(&mut store).expect("register Beryl state");
        let syndic = SyndicStorage::register(&mut store).expect("register Syndic");
        let runtime_id = RuntimeId::from_bytes([1; 16]);
        let root_id = RootId::from_bytes([2; 16]);
        let thread_id = SyndicThreadId::from_bytes([3; 16]);
        let mode = RuntimeMode::host();
        let canonical_root = native_path(mode.clone(), r"C:\Work\Beryl");
        let runtime = RuntimeRegistration::new(
            runtime_id,
            host_path(r"C:\Program Files\Codex\codex.exe"),
            mode.clone(),
            native_path(mode.clone(), r"C:\Program Files\Codex\codex.exe"),
            UnixMillis::new(1),
            AvailabilitySnapshot::observed(Availability::Available, UnixMillis::new(2))
                .expect("runtime availability"),
        )
        .expect("runtime registration");
        let root = RootRegistration::new(
            root_id,
            canonical_root,
            host_path(r"C:\Work\Beryl"),
            UnixMillis::new(1),
            AvailabilitySnapshot::unknown(),
        );
        let create_runtime =
            CreateRuntimeWithHomeRoot::new(runtime, root).expect("runtime and root agree");
        let create_thread = CreateThread::ordinary(
            thread_id,
            SyndicDraftId::from_bytes([4; 16]),
            ExecutionBinding::new(runtime_id, root_id, native_path(mode, binding_path)),
            SyndicTimestamp::from_unix_millis(3),
        );
        let mut command = HomeCommand::new(store.home_revision().expect("home revision"));
        command
            .add(
                state.runtime_roots().create_runtime_with_home_root(
                    state
                        .runtime_roots()
                        .revision(&store)
                        .expect("runtime/root revision"),
                    create_runtime,
                ),
            )
            .expect("add runtime/root creation");
        command
            .add(syndic.create_thread(
                syndic.revision(&store).expect("Syndic revision"),
                create_thread,
            ))
            .expect("add thread creation");
        match store.execute(command) {
            CommandOutcome::Committed {
                later_failure: None,
                ..
            } => {}
            CommandOutcome::NotCommitted { evidence } => {
                panic!("create sources unexpectedly not committed: {evidence:?}")
            }
            outcome @ CommandOutcome::Committed {
                later_failure: Some(_),
                ..
            } => panic!("create sources committed with later failure: {outcome:?}"),
            outcome @ CommandOutcome::Indeterminate { .. } => {
                panic!("create sources indeterminate: {outcome:?}")
            }
        }
        Self {
            _directory: directory,
            store,
            state,
            syndic,
            thread_id,
            runtime_id,
            root_id,
        }
    }

    fn claim_thread(&self) -> WindowId {
        let session = self.state.session();
        let window_id = WindowId::from_bytes([5; 16]);
        let placement = WindowPlacement::new(
            WindowBounds::new(0, 0, 900, 700).expect("window bounds"),
            WindowDisplayState::Normal,
            None,
            None,
        );
        execute_contribution(
            &self.store,
            session.initialize_threadless(
                session.revision(&self.store).expect("session revision"),
                InitializeThreadlessWindow::new(window_id, placement),
            ),
        );
        let initial = session
            .minimal_bootstrap(&self.store)
            .expect("read session")
            .expect("initialized session");
        execute_contribution(
            &self.store,
            session.replace_claim(
                session.revision(&self.store).expect("session revision"),
                ReplaceWindowClaim::new(
                    initial.header().revision(),
                    window_id,
                    initial.windows()[0].revision(),
                    None,
                    RememberedTarget::new(self.runtime_id, self.root_id),
                    self.thread_id,
                ),
            ),
        );
        window_id
    }
}

fn host_path(value: &str) -> AdmittedHostPath {
    AdmittedHostPath::from_admitted(PathFlavor::Windows, value).expect("admitted host path")
}

fn native_path(mode: RuntimeMode, value: &str) -> RuntimeNativePath {
    RuntimeNativePath::from_admitted(mode, PathFlavor::Windows, value)
        .expect("admitted runtime-native path")
}

fn execute_contribution(store: &HomeStore, contribution: beryl_home_store::MutationContribution) {
    let mut command = HomeCommand::new(store.home_revision().expect("home revision"));
    command.add(contribution).expect("add contribution");
    match store.execute(command) {
        CommandOutcome::Committed {
            later_failure: None,
            ..
        } => {}
        CommandOutcome::NotCommitted { evidence } => {
            panic!("execute contribution unexpectedly not committed: {evidence:?}")
        }
        outcome @ CommandOutcome::Committed {
            later_failure: Some(_),
            ..
        } => panic!("execute contribution committed with later failure: {outcome:?}"),
        outcome @ CommandOutcome::Indeterminate { .. } => {
            panic!("execute contribution indeterminate: {outcome:?}")
        }
    }
}

fn persist_draft(fixture: &Fixture, text: &str, updated_at: u64) {
    let point_limit = SyndicPointReadLimit::new(1024 * 1024).expect("point-read limit");
    let seed = read_draft_persistence_seed(
        &fixture.store,
        &fixture.syndic,
        fixture.thread_id,
        point_limit,
        DraftPersistenceTime::from_duration(Duration::ZERO),
    )
    .expect("read draft seed")
    .expect("current draft");
    let mut service =
        DraftPersistenceService::from_seed(seed, DraftAutosavePublication::absent_default());
    let payload = ComposerPayload::new(vec![ComposerAtom::text(text).expect("bounded text")])
        .expect("bounded payload");
    service
        .edit(payload, SyndicTimestamp::from_unix_millis(updated_at))
        .expect("edit draft");
    let request = match service.flush().expect("flush draft") {
        DraftFlushAction::Started(request) => request,
        other => panic!("unexpected draft flush action: {other:?}"),
    };
    let execution = execute_draft_save(&fixture.store, &fixture.syndic, &request, point_limit);
    assert!(execution.failure().is_none());
}

#[test]
fn projection_publishes_once_then_converges_to_an_exact_no_op() {
    let fixture = Fixture::new(r"C:\Work\Beryl");
    let window_id = fixture.claim_thread();
    let missing = prepare_thread_catalog_projection(
        &fixture.store,
        fixture.syndic,
        fixture.state.clone(),
        SyndicThreadId::from_bytes([9; 16]),
    )
    .expect("prepare missing thread");
    assert!(matches!(
        missing,
        ThreadCatalogProjectionPreparation::ThreadMissing
    ));

    let command = match prepare_thread_catalog_projection(
        &fixture.store,
        fixture.syndic,
        fixture.state.clone(),
        fixture.thread_id,
    )
    .expect("prepare initial projection")
    {
        ThreadCatalogProjectionPreparation::Publish(command) => command,
        ThreadCatalogProjectionPreparation::ThreadMissing => panic!("thread unexpectedly missing"),
        ThreadCatalogProjectionPreparation::ExactCurrent => {
            panic!("catalog unexpectedly current before its first publication")
        }
    };
    match fixture.store.execute(command) {
        CommandOutcome::Committed {
            later_failure: None,
            ..
        } => {}
        CommandOutcome::NotCommitted { evidence } => {
            panic!("publish catalog row unexpectedly not committed: {evidence:?}")
        }
        outcome @ CommandOutcome::Committed {
            later_failure: Some(_),
            ..
        } => panic!("publish catalog row committed with later failure: {outcome:?}"),
        outcome @ CommandOutcome::Indeterminate { .. } => {
            panic!("publish catalog row indeterminate: {outcome:?}")
        }
    }

    let row = fixture
        .state
        .catalog()
        .row(
            &fixture.store,
            fixture.thread_id,
            CatalogPointReadLimit::schema_maximum(),
        )
        .expect("read catalog row")
        .expect("published catalog row");
    assert_eq!(row.title_source(), CatalogTitleSource::Absent);
    assert_eq!(row.facts().execution().runtime_id(), fixture.runtime_id);
    assert_eq!(row.facts().execution().root_id(), fixture.root_id);
    assert_eq!(row.facts().search().title(), "");
    assert_eq!(row.facts().search().full_root_path(), r"c:\work\beryl");
    assert!(row.facts().complete());
    assert_eq!(row.facts().claim().window_id(), Some(window_id));
    assert_eq!(row.facts().claim().kind(), Some(CatalogClaimKind::Active));
    assert!(row.sources().claim().is_some());
    let initial_summary_revision = row.sources().syndic_summary();

    let exact = prepare_thread_catalog_projection(
        &fixture.store,
        fixture.syndic,
        fixture.state.clone(),
        fixture.thread_id,
    )
    .expect("prepare current projection");
    assert!(matches!(
        exact,
        ThreadCatalogProjectionPreparation::ExactCurrent
    ));

    persist_draft(&fixture, "later draft", 20);
    let command = match prepare_thread_catalog_projection(
        &fixture.store,
        fixture.syndic,
        fixture.state.clone(),
        fixture.thread_id,
    )
    .expect("prepare source-stale projection")
    {
        ThreadCatalogProjectionPreparation::Publish(command) => command,
        ThreadCatalogProjectionPreparation::ThreadMissing => panic!("thread unexpectedly missing"),
        ThreadCatalogProjectionPreparation::ExactCurrent => {
            panic!("draft activity must stale the compact summary")
        }
    };
    match fixture.store.execute(command) {
        CommandOutcome::Committed {
            later_failure: None,
            ..
        } => {}
        CommandOutcome::NotCommitted { evidence } => {
            panic!("catalog rebuild unexpectedly not committed: {evidence:?}")
        }
        outcome @ CommandOutcome::Committed {
            later_failure: Some(_),
            ..
        } => panic!("catalog rebuild committed with later failure: {outcome:?}"),
        outcome @ CommandOutcome::Indeterminate { .. } => {
            panic!("catalog rebuild indeterminate: {outcome:?}")
        }
    }
    let rebuilt = fixture
        .state
        .catalog()
        .row(
            &fixture.store,
            fixture.thread_id,
            CatalogPointReadLimit::schema_maximum(),
        )
        .expect("read rebuilt catalog row")
        .expect("rebuilt catalog row");
    assert!(rebuilt.sources().syndic_summary() > initial_summary_revision);
    assert_eq!(rebuilt.facts().last_activity_at(), UnixMillis::new(20));
    assert!(matches!(
        prepare_thread_catalog_projection(
            &fixture.store,
            fixture.syndic,
            fixture.state.clone(),
            fixture.thread_id,
        )
        .expect("prepare rebuilt projection"),
        ThreadCatalogProjectionPreparation::ExactCurrent
    ));
}

#[test]
fn projection_rejects_a_syndic_binding_that_disagrees_with_the_root_authority() {
    let fixture = Fixture::new(r"C:\Work\Elsewhere");
    let error = prepare_thread_catalog_projection(
        &fixture.store,
        fixture.syndic,
        fixture.state.clone(),
        fixture.thread_id,
    )
    .err()
    .expect("mismatched binding must fail");
    assert!(matches!(
        error,
        CatalogProjectionBuildError::ExecutionBindingMismatch
    ));
}
