use beryl_home_store::{
    CommandCancellation, CommandOutcome, HomeCommand, HomeOpenOptions, HomeSchemaVersion,
    HomeStore, MutationContribution,
};
use beryl_model::{
    AdmittedHostPath, Availability, ClaimRevision, PathFlavor, ProjectionRevision, RootId,
    RuntimeId, SyndicThreadId, WindowBounds, WindowDisplayState, WindowId, WindowPlacement,
};

use super::{
    WindowAcquisitionNaturalState, WindowAcquisitionThreadOrigin,
    catalog_scan_max_retained_rows_for_test, reset_catalog_scan_test_state,
    set_catalog_scan_page_hook_for_test,
};
use crate::{
    BerylState, CatalogArchiveSummary, CatalogAvailabilitySummary, CatalogClaimSummary,
    CatalogExecutionSummary, CatalogFacts, CatalogLineageSummary, CatalogPointReadLimit,
    CatalogResolvedTitle, CatalogRowExpectation, CatalogSourceRevisions, CatalogWindowClaim,
    CreateClaimedWindow, InitializeThreadlessWindow, MarkCatalogRowStale, PublishCatalogClaim,
    PublishCatalogRow, RecordRevision, RememberedTarget, RemoveSessionWindow, UnixMillis,
    WindowAcquisitionAuditError,
};

struct Fixture {
    directory: tempfile::TempDir,
    store: HomeStore,
    state: BerylState,
    window_id: WindowId,
    thread_id: SyndicThreadId,
    target: RememberedTarget,
    placement: WindowPlacement,
}

impl Fixture {
    fn new() -> Self {
        let directory = tempfile::tempdir().unwrap();
        let mut store = HomeStore::open(HomeOpenOptions::new(
            directory.path(),
            HomeSchemaVersion::CURRENT,
        ))
        .unwrap();
        let state = BerylState::register(&mut store).unwrap();
        initialize_empty_session(&store, &state);
        Self {
            directory,
            store,
            state,
            window_id: WindowId::from_bytes([2; 16]),
            thread_id: SyndicThreadId::from_bytes([3; 16]),
            target: RememberedTarget::new(
                RuntimeId::from_bytes([4; 16]),
                RootId::from_bytes([5; 16]),
            ),
            placement: placement(),
        }
    }

    fn publish(&self, include_catalog: bool) {
        let session = self.state.session();
        let bootstrap = session.minimal_bootstrap(&self.store).unwrap().unwrap();
        let create = CreateClaimedWindow::new(
            bootstrap.header().revision(),
            self.window_id,
            self.target,
            self.thread_id,
            self.placement.clone(),
        );
        let claim = create.catalog_claim();
        let mut command = HomeCommand::new(self.store.home_revision().unwrap());
        command
            .add(session.create_claimed_window(session.revision(&self.store).unwrap(), create))
            .unwrap();
        if include_catalog {
            command
                .add(self.state.catalog().publish_claim(
                    self.state.catalog().revision(&self.store).unwrap(),
                    PublishCatalogClaim::initial(
                        self.thread_id,
                        catalog_sources(),
                        catalog_facts(self.target),
                        claim,
                    ),
                ))
                .unwrap();
        }
        assert_committed(self.store.execute(command));
    }
}

#[test]
fn missing_requires_no_session_or_catalog_natural_record() {
    let fixture = Fixture::new();
    assert_eq!(
        fixture
            .state
            .audit_window_acquisition(&fixture.store, fixture.window_id)
            .unwrap(),
        WindowAcquisitionNaturalState::Missing
    );
}

#[test]
fn exact_initial_claim_publication_returns_opaque_committed_facts() {
    let fixture = Fixture::new();
    fixture.publish(true);
    let WindowAcquisitionNaturalState::Committed(facts) = fixture
        .state
        .audit_window_acquisition(&fixture.store, fixture.window_id)
        .unwrap()
    else {
        panic!("exact publication was not committed")
    };
    assert_eq!(facts.window_id(), fixture.window_id);
    assert_eq!(facts.thread_id(), fixture.thread_id);
    assert_eq!(facts.target(), fixture.target);
    assert_eq!(facts.placement(), &fixture.placement);
    assert_eq!(facts.session_revision(), facts.claim_generation());
    assert_eq!(facts.fallback_target(), fixture.target);
    assert_eq!(
        facts.origin(),
        WindowAcquisitionThreadOrigin::CreatedFallback
    );
}

#[test]
fn partial_session_only_publication_is_a_collision() {
    let fixture = Fixture::new();
    fixture.publish(false);
    assert_eq!(
        fixture
            .state
            .audit_window_acquisition(&fixture.store, fixture.window_id)
            .unwrap(),
        WindowAcquisitionNaturalState::Collision
    );
}

#[test]
fn missing_reverse_claim_copy_is_a_collision() {
    let fixture = Fixture::new();
    fixture.publish(true);
    #[cfg(feature = "test-faults")]
    let deletion = fixture.state.session().delete_thread_claim_copy_for_test(
        fixture.state.session().revision(&fixture.store).unwrap(),
        fixture.window_id,
        fixture.thread_id,
    );
    #[cfg(not(feature = "test-faults"))]
    let deletion = fixture.state.session().delete_thread_claim_for_test(
        fixture.state.session().revision(&fixture.store).unwrap(),
        fixture.window_id,
        fixture.thread_id,
    );
    execute_contribution(&fixture.store, deletion);
    assert_eq!(
        fixture
            .state
            .audit_window_acquisition(&fixture.store, fixture.window_id)
            .unwrap(),
        WindowAcquisitionNaturalState::Collision
    );
}

#[cfg(feature = "test-faults")]
#[test]
fn missing_window_claim_copy_from_public_fault_seam_is_a_collision() {
    let fixture = Fixture::new();
    fixture.publish(true);
    execute_contribution(
        &fixture.store,
        fixture.state.session().delete_window_claim_copy_for_test(
            fixture.state.session().revision(&fixture.store).unwrap(),
            fixture.window_id,
            fixture.thread_id,
        ),
    );
    assert_eq!(
        fixture
            .state
            .audit_window_acquisition(&fixture.store, fixture.window_id)
            .unwrap(),
        WindowAcquisitionNaturalState::Collision
    );
}

#[test]
fn unrelated_stale_catalog_row_requests_typed_repair_before_missing() {
    let fixture = Fixture::new();
    let stale_thread = SyndicThreadId::from_bytes([9; 16]);
    publish_unclaimed_catalog_row(&fixture, stale_thread);
    mark_catalog_stale(&fixture, stale_thread);
    assert!(matches!(
        fixture
            .state
            .audit_window_acquisition(&fixture.store, fixture.window_id),
        Err(WindowAcquisitionAuditError::RepairNeeded { thread_id })
            if thread_id == stale_thread
    ));
}

#[test]
fn stale_claimed_catalog_row_requests_typed_repair() {
    let fixture = Fixture::new();
    fixture.publish(true);
    mark_catalog_stale(&fixture, fixture.thread_id);
    assert!(matches!(
        fixture
            .state
            .audit_window_acquisition(&fixture.store, fixture.window_id),
        Err(WindowAcquisitionAuditError::RepairNeeded { thread_id })
            if thread_id == fixture.thread_id
    ));
}

#[test]
fn multi_page_duplicate_claim_scan_retains_at_most_one_row() {
    let fixture = Fixture::new();
    reset_catalog_scan_test_state();
    for seed in 20_u8..38 {
        publish_claimed_catalog_row(
            &fixture,
            SyndicThreadId::from_bytes([seed; 16]),
            fixture.window_id,
        );
    }
    assert_eq!(
        fixture
            .state
            .audit_window_acquisition(&fixture.store, fixture.window_id)
            .unwrap(),
        WindowAcquisitionNaturalState::Collision
    );
    assert_eq!(catalog_scan_max_retained_rows_for_test(), 1);
    reset_catalog_scan_test_state();
}

#[test]
fn cancellation_between_catalog_pages_is_typed() {
    let fixture = Fixture::new();
    for seed in 40_u8..57 {
        publish_unclaimed_catalog_row(&fixture, SyndicThreadId::from_bytes([seed; 16]));
    }
    let cancellation = CommandCancellation::new();
    let cancel_after_page = cancellation.clone();
    reset_catalog_scan_test_state();
    set_catalog_scan_page_hook_for_test(move || cancel_after_page.cancel());
    assert!(matches!(
        fixture.state.audit_window_acquisition_with_cancellation(
            &fixture.store,
            fixture.window_id,
            &cancellation,
        ),
        Err(WindowAcquisitionAuditError::Cancelled)
    ));
    reset_catalog_scan_test_state();
}

#[test]
fn abandonment_audit_uses_no_catalog_scan() {
    let fixture = Fixture::new();
    fixture.publish(true);
    let WindowAcquisitionNaturalState::Committed(facts) = fixture
        .state
        .audit_window_acquisition(&fixture.store, fixture.window_id)
        .unwrap()
    else {
        panic!("exact publication was not committed")
    };
    let cancellation = CommandCancellation::new();
    reset_catalog_scan_test_state();
    set_catalog_scan_page_hook_for_test(|| panic!("abandonment audit scanned catalog rows"));
    assert!(matches!(
        fixture
            .state
            .audit_window_abandonment_with_cancellation(&fixture.store, &facts, &cancellation,)
            .unwrap(),
        super::WindowAbandonmentNaturalState::ExactAcquired(_)
    ));
    reset_catalog_scan_test_state();
}

#[test]
fn created_abandonment_with_retained_expected_recency_copy_is_a_collision() {
    let fixture = Fixture::new();
    fixture.publish(true);
    let WindowAcquisitionNaturalState::Committed(facts) = fixture
        .state
        .audit_window_acquisition(&fixture.store, fixture.window_id)
        .unwrap()
    else {
        panic!("exact publication was not committed")
    };
    let retained_row = facts.catalog_current.row().clone();
    let retained_cursor = retained_row.recency_cursor();
    let mut abandonment = HomeCommand::new(fixture.store.home_revision().unwrap());
    abandonment
        .add(fixture.state.session().abandon_window(
            facts.session_domain_revision(),
            facts.abandon_session_window(),
        ))
        .unwrap();
    abandonment
        .add(fixture.state.catalog().delete_claimed_row(
            facts.catalog_domain_revision(),
            facts.delete_catalog_claimed_row(),
        ))
        .unwrap();
    assert_committed(fixture.store.execute(abandonment));
    execute_contribution(
        &fixture.store,
        fixture.state.catalog().corrupt_recency_copy_for_test(
            fixture.state.catalog().revision(&fixture.store).unwrap(),
            retained_cursor,
            retained_row,
        ),
    );
    assert_eq!(
        fixture
            .state
            .audit_window_abandonment(&fixture.store, &facts)
            .unwrap(),
        super::WindowAbandonmentNaturalState::Collision
    );
}

#[test]
fn known_thread_acquisition_audit_uses_no_catalog_scan() {
    let fixture = Fixture::new();
    fixture.publish(true);
    let cancellation = CommandCancellation::new();
    reset_catalog_scan_test_state();
    set_catalog_scan_page_hook_for_test(|| panic!("known-thread audit scanned catalog rows"));
    assert!(matches!(
        fixture
            .state
            .audit_window_acquisition_for_thread_with_cancellation(
                &fixture.store,
                fixture.window_id,
                fixture.thread_id,
                &cancellation,
            )
            .unwrap(),
        WindowAcquisitionNaturalState::Committed(_)
    ));
    reset_catalog_scan_test_state();
}

#[test]
fn abandonment_audit_honors_initial_cancellation() {
    let fixture = Fixture::new();
    fixture.publish(true);
    let WindowAcquisitionNaturalState::Committed(facts) = fixture
        .state
        .audit_window_acquisition(&fixture.store, fixture.window_id)
        .unwrap()
    else {
        panic!("exact publication was not committed")
    };
    let cancellation = CommandCancellation::new();
    cancellation.cancel();
    assert!(matches!(
        fixture.state.audit_window_abandonment_with_cancellation(
            &fixture.store,
            &facts,
            &cancellation,
        ),
        Err(WindowAcquisitionAuditError::Cancelled)
    ));
}

#[test]
fn exact_committed_facts_survive_close_and_reopen() {
    let fixture = Fixture::new();
    fixture.publish(true);
    let Fixture {
        directory,
        store,
        window_id,
        thread_id,
        target,
        placement,
        ..
    } = fixture;
    store.close().unwrap();
    let mut reopened = HomeStore::open(HomeOpenOptions::new(
        directory.path(),
        HomeSchemaVersion::CURRENT,
    ))
    .unwrap();
    let state = BerylState::register(&mut reopened).unwrap();
    let WindowAcquisitionNaturalState::Committed(facts) = state
        .audit_window_acquisition(&reopened, window_id)
        .unwrap()
    else {
        panic!("reopened publication was not committed")
    };
    assert_eq!(facts.thread_id(), thread_id);
    assert_eq!(facts.target(), target);
    assert_eq!(facts.placement(), &placement);
    assert_eq!(
        facts.origin(),
        WindowAcquisitionThreadOrigin::CreatedFallback
    );
    reopened.close().unwrap();
}

fn initialize_empty_session(store: &HomeStore, state: &BerylState) {
    let session = state.session();
    let initial_window = WindowId::from_bytes([1; 16]);
    execute_contribution(
        store,
        session.initialize_threadless(
            session.revision(store).unwrap(),
            InitializeThreadlessWindow::new(initial_window, placement()),
        ),
    );
    let bootstrap = session.minimal_bootstrap(store).unwrap().unwrap();
    let window = &bootstrap.windows()[0];
    execute_contribution(
        store,
        session.remove_window(
            session.revision(store).unwrap(),
            RemoveSessionWindow::new(
                bootstrap.header().revision(),
                initial_window,
                window.revision(),
                window.selected_thread(),
            ),
        ),
    );
}

fn execute_contribution(store: &HomeStore, contribution: MutationContribution) {
    let mut command = HomeCommand::new(store.home_revision().unwrap());
    command.add(contribution).unwrap();
    assert_committed(store.execute(command));
}

fn assert_committed(outcome: CommandOutcome) {
    assert!(matches!(
        outcome,
        CommandOutcome::Committed {
            later_failure: None,
            ..
        }
    ));
}

fn placement() -> WindowPlacement {
    WindowPlacement::new(
        WindowBounds::new(10, 20, 900, 700).unwrap(),
        WindowDisplayState::Normal,
        None,
        None,
    )
}

fn catalog_sources() -> CatalogSourceRevisions {
    CatalogSourceRevisions::new(
        ProjectionRevision::new(1).unwrap(),
        RecordRevision::INITIAL,
        RecordRevision::INITIAL,
        None,
    )
}

fn catalog_facts(target: RememberedTarget) -> CatalogFacts {
    let execution = CatalogExecutionSummary::new(
        target.runtime_id(),
        target.root_id(),
        "Host",
        AdmittedHostPath::from_admitted(PathFlavor::Windows, r"C:\Codex\codex.exe").unwrap(),
        AdmittedHostPath::from_admitted(PathFlavor::Windows, r"C:\Work\beryl").unwrap(),
        CatalogAvailabilitySummary::new(Availability::Available, Availability::Available),
    )
    .unwrap();
    CatalogFacts::new(
        CatalogResolvedTitle::absent(),
        execution,
        CatalogArchiveSummary::Ordinary,
        UnixMillis::new(1),
        true,
        CatalogClaimSummary::Unclaimed,
        CatalogLineageSummary::TopLevel,
    )
    .unwrap()
}

fn publish_unclaimed_catalog_row(fixture: &Fixture, thread_id: SyndicThreadId) {
    let publish = PublishCatalogRow::new(
        thread_id,
        CatalogRowExpectation::Missing,
        catalog_sources(),
        catalog_facts(fixture.target),
    )
    .unwrap();
    execute_contribution(
        &fixture.store,
        fixture.state.catalog().publish(
            fixture.state.catalog().revision(&fixture.store).unwrap(),
            publish,
        ),
    );
}

fn publish_claimed_catalog_row(fixture: &Fixture, thread_id: SyndicThreadId, window_id: WindowId) {
    let claim = CatalogWindowClaim::active(thread_id, window_id, ClaimRevision::new(1).unwrap());
    execute_contribution(
        &fixture.store,
        fixture.state.catalog().publish_claim(
            fixture.state.catalog().revision(&fixture.store).unwrap(),
            PublishCatalogClaim::initial(
                thread_id,
                catalog_sources(),
                catalog_facts(fixture.target),
                claim,
            ),
        ),
    );
}

fn mark_catalog_stale(fixture: &Fixture, thread_id: SyndicThreadId) {
    let row = fixture
        .state
        .catalog()
        .row(
            &fixture.store,
            thread_id,
            CatalogPointReadLimit::schema_maximum(),
        )
        .unwrap()
        .unwrap();
    execute_contribution(
        &fixture.store,
        fixture.state.catalog().mark_stale(
            fixture.state.catalog().revision(&fixture.store).unwrap(),
            MarkCatalogRowStale::new(thread_id, row.revision()),
        ),
    );
}
