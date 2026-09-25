use super::*;
use beryl_home_store::{
    CommandOutcome, HomeCommand, HomeOpenCandidate, HomeOpenOptions, HomeSchemaVersion,
};
use beryl_state::{InitializeThreadlessWindow, UpdateWindowPlacement};

struct ThreadlessFixture {
    store: Arc<HomeStore>,
    state: BerylState,
    storage: SyndicStorage,
    _directory: tempfile::TempDir,
}

impl ThreadlessFixture {
    fn new() -> Self {
        let directory = tempfile::tempdir().unwrap();
        let mut candidate = HomeOpenCandidate::open(HomeOpenOptions::new(
            directory.path(),
            HomeSchemaVersion::CURRENT,
        ))
        .unwrap();
        let state = BerylState::register(&mut candidate).unwrap();
        let storage = SyndicStorage::register(&mut candidate).unwrap();
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
        execute(
            &store,
            session.initialize_threadless(
                session.revision(&store).unwrap(),
                InitializeThreadlessWindow::new(WindowId::from_bytes([0; 16]), placement()),
            ),
        );
        Self {
            store: Arc::new(store),
            state,
            storage,
            _directory: directory,
        }
    }

    fn attempt(
        &self,
    ) -> (
        RestoredWindowPreparationAttempt,
        RestoredWindowServiceTestLifetime,
    ) {
        RestoredWindowPreparationAttempt::new_for_test(
            self.store.clone(),
            self.state.session(),
            self.storage.clone(),
        )
        .unwrap()
    }

    fn begin(
        &self,
        attempt: &RestoredWindowPreparationAttempt,
    ) -> Result<ThreadlessWindowSource, String> {
        let snapshot = self
            .state
            .session()
            .minimal_bootstrap(&self.store)
            .unwrap()
            .unwrap();
        attempt.begin_threadless(
            snapshot.header().revision(),
            snapshot.windows()[0].window_id(),
            self.state.runtime_roots(),
        )
    }
}

fn execute(store: &HomeStore, contribution: beryl_home_store::MutationContribution) {
    let mut command = HomeCommand::new(store.home_revision().unwrap());
    command.add(contribution).unwrap();
    assert!(matches!(
        store.execute(command),
        CommandOutcome::Committed {
            later_failure: None,
            ..
        }
    ));
}

#[test]
fn threadless_source_preserves_exact_member_and_has_no_editor_or_home_lifecycle_ownership() {
    let fixture = ThreadlessFixture::new();
    let (attempt, service) = fixture.attempt();
    let revision = fixture.store.home_revision().unwrap();
    let source = fixture.begin(&attempt).unwrap();
    assert_eq!(source.window_id(), WindowId::from_bytes([0; 16]));
    assert_eq!(source.placement(), &placement());
    assert_eq!(source.home_id(), fixture.store.home_id());
    assert_eq!(
        source.home_generation(),
        fixture.store.health().generation().unwrap()
    );
    source.revalidate().unwrap();
    assert_eq!(fixture.store.home_revision().unwrap(), revision);
    assert!(
        fixture
            .state
            .session()
            .window_claim_catalog_source(&fixture.store, source.window_id())
            .unwrap()
            .claim()
            .is_none()
    );
    drop((attempt, service));
    assert!(source.revalidate().is_err());
    let home = Arc::try_unwrap(fixture.store)
        .ok()
        .expect("source does not retain the owning home");
    home.close().unwrap();
    assert!(source.revalidate().is_err());
    drop(source);
}

#[test]
fn runtime_present_or_foreign_registry_cannot_authorize_threadless_source() {
    let fixture = ThreadlessFixture::new();
    let foreign = ThreadlessFixture::new();
    let (attempt, _service) = fixture.attempt();
    let snapshot = fixture
        .state
        .session()
        .minimal_bootstrap(&fixture.store)
        .unwrap()
        .unwrap();
    assert!(
        attempt
            .begin_threadless(
                snapshot.header().revision(),
                snapshot.windows()[0].window_id(),
                foreign.state.runtime_roots()
            )
            .is_err()
    );
    let runtime_fixture = Fixture::new(81);
    let session = runtime_fixture.state.session();
    let empty = session
        .minimal_bootstrap(&runtime_fixture.store)
        .unwrap()
        .unwrap();
    execute(
        &runtime_fixture.store,
        session.initialize_threadless(
            session.revision(&runtime_fixture.store).unwrap(),
            InitializeThreadlessWindow::for_empty_session(
                empty.header().revision(),
                WindowId::from_bytes([82; 16]),
                placement(),
            ),
        ),
    );
    let (attempt, _service) = restoration_support::attempt(&runtime_fixture);
    let snapshot = restoration_support::snapshot(&runtime_fixture);
    assert!(
        attempt
            .begin_threadless(
                snapshot.header().revision(),
                snapshot.windows()[0].window_id(),
                runtime_fixture.state.runtime_roots()
            )
            .is_err()
    );
}

#[test]
fn changed_session_member_or_retired_attempt_invalidates_threadless_source() {
    let fixture = ThreadlessFixture::new();
    let (attempt, service) = fixture.attempt();
    let source = fixture.begin(&attempt).unwrap();
    let session = fixture.state.session();
    let snapshot = session.minimal_bootstrap(&fixture.store).unwrap().unwrap();
    let old_revision = snapshot.header().revision();
    let window = &snapshot.windows()[0];
    let changed = WindowPlacement::new(
        WindowBounds::new(10, 20, 800, 600).unwrap(),
        WindowDisplayState::Normal,
        None,
        None,
    );
    execute(
        &fixture.store,
        session.update_placement(
            session.revision(&fixture.store).unwrap(),
            UpdateWindowPlacement::new(
                old_revision,
                window.window_id(),
                window.revision(),
                changed,
            ),
        ),
    );
    assert!(source.revalidate().is_err());
    assert!(
        attempt
            .begin_threadless(
                old_revision,
                window.window_id(),
                fixture.state.runtime_roots()
            )
            .is_err()
    );
    let (fresh_attempt, fresh_service) = fixture.attempt();
    let fresh = fixture.begin(&fresh_attempt).unwrap();
    drop(fresh_service);
    assert!(fresh.revalidate().is_err());
    let (another_attempt, _another_service) = fixture.attempt();
    let another = fixture.begin(&another_attempt).unwrap();
    drop(another_attempt);
    assert!(another.revalidate().is_err());
    drop((service, source, fresh, another));
}

#[test]
fn missing_member_and_runtime_backed_selection_are_rejected_without_mutation() {
    let fixture = ThreadlessFixture::new();
    let (attempt, _service) = fixture.attempt();
    let snapshot = fixture
        .state
        .session()
        .minimal_bootstrap(&fixture.store)
        .unwrap()
        .unwrap();
    let before = fixture.store.home_revision().unwrap();
    assert!(
        attempt
            .begin_threadless(
                snapshot.header().revision(),
                WindowId::from_bytes([1; 16]),
                fixture.state.runtime_roots()
            )
            .is_err()
    );
    assert_eq!(fixture.store.home_revision().unwrap(), before);
    let fixture = Fixture::new(91);
    drop(fixture.acquire(92));
    restoration_support::begin_restore(&fixture);
    let (attempt, _service) = restoration_support::attempt(&fixture);
    let snapshot = restoration_support::snapshot(&fixture);
    let before = fixture.store.home_revision().unwrap();
    assert!(
        attempt
            .begin_threadless(
                snapshot.header().revision(),
                snapshot.windows()[0].window_id(),
                fixture.state.runtime_roots()
            )
            .is_err()
    );
    assert_eq!(fixture.store.home_revision().unwrap(), before);
}
