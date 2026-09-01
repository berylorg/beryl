mod support;

use beryl_home_store::{CommandOutcome, CursorReadLimits, HomeCommand, HomeStore};
use beryl_model::{
    AdmittedHostPath, Availability, PathFlavor, ProjectionRevision, RootId, RuntimeId,
    SyndicThreadId, WindowBounds, WindowDisplayState, WindowId, WindowPlacement,
};
use beryl_state::{
    BerylState, CatalogArchiveSummary, CatalogAvailabilitySummary, CatalogClaimSummary,
    CatalogExecutionSummary, CatalogFacts, CatalogLineageSummary, CatalogPointReadLimit,
    CatalogResolvedTitle, CatalogRowExpectation, CatalogSourceRevisions, CreateClaimedWindow,
    InitializeThreadlessWindow, MarkCatalogRowStale, PublishCatalogClaim, PublishCatalogRow,
    RecordRevision, RememberedTarget, RemoveSessionWindow, ReplaceWindowClaim, UnixMillis,
    WindowAbandonmentNaturalState, WindowAcquisitionCommittedFacts, WindowAcquisitionNaturalState,
    WindowAcquisitionThreadOrigin,
};
use tempfile::TempDir;

struct Fixture {
    _directory: TempDir,
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
        let (store, state) = support::open(directory.path());
        initialize_empty_session(&store, &state);
        Self {
            _directory: directory,
            store,
            state,
            window_id: WindowId::from_bytes([2; 16]),
            thread_id: SyndicThreadId::from_bytes([3; 16]),
            target: RememberedTarget::new(
                RuntimeId::from_bytes([4; 16]),
                RootId::from_bytes([5; 16]),
            ),
            placement: placement(2),
        }
    }

    fn publish_acquired(&self) -> WindowAcquisitionCommittedFacts {
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
        assert_committed(self.store.execute(command));
        let WindowAcquisitionNaturalState::Committed(facts) = self
            .state
            .audit_window_acquisition(&self.store, self.window_id)
            .unwrap()
        else {
            panic!("acquired window was not exact")
        };
        facts
    }

    fn abandon(&self, facts: &WindowAcquisitionCommittedFacts) -> CommandOutcome {
        let mut command = HomeCommand::new(self.store.home_revision().unwrap());
        command
            .add(self.state.session().abandon_window(
                facts.session_domain_revision(),
                facts.abandon_session_window(),
            ))
            .unwrap();
        match facts.origin() {
            WindowAcquisitionThreadOrigin::Reused => command
                .add(self.state.catalog().release_claim(
                    facts.catalog_domain_revision(),
                    facts.release_catalog_claim(),
                ))
                .unwrap(),
            WindowAcquisitionThreadOrigin::CreatedFallback => command
                .add(self.state.catalog().delete_claimed_row(
                    facts.catalog_domain_revision(),
                    facts.delete_catalog_claimed_row(),
                ))
                .unwrap(),
        };
        self.store.execute(command)
    }

    fn publish_reused(&self) -> WindowAcquisitionCommittedFacts {
        let mut catalog_command = HomeCommand::new(self.store.home_revision().unwrap());
        catalog_command
            .add(
                self.state.catalog().publish(
                    self.state.catalog().revision(&self.store).unwrap(),
                    PublishCatalogRow::new(
                        self.thread_id,
                        CatalogRowExpectation::Missing,
                        catalog_sources(),
                        catalog_facts(self.target),
                    )
                    .unwrap(),
                ),
            )
            .unwrap();
        assert_committed(self.store.execute(catalog_command));

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
        let current = self
            .state
            .catalog()
            .current_row_source(
                &self.store,
                self.thread_id,
                CatalogPointReadLimit::schema_maximum(),
            )
            .unwrap()
            .unwrap();
        let mut command = HomeCommand::new(self.store.home_revision().unwrap());
        command
            .add(session.create_claimed_window(session.revision(&self.store).unwrap(), create))
            .unwrap();
        command
            .add(self.state.catalog().publish_claim(
                self.state.catalog().revision(&self.store).unwrap(),
                PublishCatalogClaim::current(current, claim),
            ))
            .unwrap();
        assert_committed(self.store.execute(command));
        let WindowAcquisitionNaturalState::Committed(facts) = self
            .state
            .audit_window_acquisition(&self.store, self.window_id)
            .unwrap()
        else {
            panic!("reused acquired window was not exact")
        };
        assert_eq!(facts.origin(), WindowAcquisitionThreadOrigin::Reused);
        facts
    }
}

#[test]
fn created_abandonment_deletes_exact_catalog_row_and_index_together() {
    let fixture = Fixture::new();
    let facts = fixture.publish_acquired();

    assert_committed(fixture.abandon(&facts));
    assert_eq!(
        fixture
            .state
            .audit_window_abandonment(&fixture.store, &facts)
            .unwrap(),
        WindowAbandonmentNaturalState::ExactAbandoned
    );
    let bootstrap = fixture
        .state
        .session()
        .minimal_bootstrap(&fixture.store)
        .unwrap()
        .unwrap();
    assert!(bootstrap.windows().is_empty());
    let row = fixture
        .state
        .catalog()
        .row(
            &fixture.store,
            fixture.thread_id,
            CatalogPointReadLimit::schema_maximum(),
        )
        .unwrap();
    assert!(row.is_none());
    let page = fixture
        .state
        .catalog()
        .recency_page(
            &fixture.store,
            None,
            CursorReadLimits::new(8, 32 * 1024).unwrap(),
        )
        .unwrap();
    assert!(page.rows().is_empty());
    assert_not_committed(fixture.abandon(&facts));
}

#[test]
fn reused_abandonment_keeps_the_exact_unclaimed_catalog_successor() {
    let fixture = Fixture::new();
    let facts = fixture.publish_reused();

    assert_committed(fixture.abandon(&facts));
    assert_eq!(
        fixture
            .state
            .audit_window_abandonment(&fixture.store, &facts)
            .unwrap(),
        WindowAbandonmentNaturalState::ExactAbandoned
    );
    let row = fixture
        .state
        .catalog()
        .row(
            &fixture.store,
            fixture.thread_id,
            CatalogPointReadLimit::schema_maximum(),
        )
        .unwrap()
        .unwrap();
    assert_eq!(row.facts().claim(), CatalogClaimSummary::Unclaimed);
    assert_eq!(row.sources().claim(), None);
}

#[test]
fn stale_or_replayed_abandonment_is_not_committed_and_audit_reports_collision() {
    let fixture = Fixture::new();
    let facts = fixture.publish_acquired();
    let session = fixture.state.session();
    let bootstrap = session.minimal_bootstrap(&fixture.store).unwrap().unwrap();
    let window = bootstrap.windows()[0].clone();
    assert_committed(support::execute(
        &fixture.store,
        session.update_placement(
            session.revision(&fixture.store).unwrap(),
            beryl_state::UpdateWindowPlacement::new(
                bootstrap.header().revision(),
                fixture.window_id,
                window.revision(),
                placement(9),
            ),
        ),
    ));

    assert_not_committed(fixture.abandon(&facts));
    assert_eq!(
        fixture
            .state
            .audit_window_abandonment(&fixture.store, &facts)
            .unwrap(),
        WindowAbandonmentNaturalState::Collision
    );
}

#[test]
fn session_only_partial_abandonment_is_a_collision() {
    let fixture = Fixture::new();
    let facts = fixture.publish_acquired();
    let mut partial = HomeCommand::new(fixture.store.home_revision().unwrap());
    partial
        .add(fixture.state.session().abandon_window(
            facts.session_domain_revision(),
            facts.abandon_session_window(),
        ))
        .unwrap();
    assert_committed(fixture.store.execute(partial));

    assert_eq!(
        fixture
            .state
            .audit_window_abandonment(&fixture.store, &facts)
            .unwrap(),
        WindowAbandonmentNaturalState::Collision
    );
}

#[test]
fn cross_origin_catalog_participants_reject_without_mutating_the_claimed_row() {
    let created = Fixture::new();
    let created_facts = created.publish_acquired();
    let mut release = HomeCommand::new(created.store.home_revision().unwrap());
    release
        .add(created.state.catalog().release_claim(
            created_facts.catalog_domain_revision(),
            created_facts.release_catalog_claim(),
        ))
        .unwrap();
    assert_not_committed(created.store.execute(release));
    assert_eq!(
        created
            .state
            .audit_window_abandonment(&created.store, &created_facts)
            .unwrap(),
        WindowAbandonmentNaturalState::ExactAcquired(created_facts.clone())
    );

    let reused = Fixture::new();
    let reused_facts = reused.publish_reused();
    let mut delete = HomeCommand::new(reused.store.home_revision().unwrap());
    delete
        .add(reused.state.catalog().delete_claimed_row(
            reused_facts.catalog_domain_revision(),
            reused_facts.delete_catalog_claimed_row(),
        ))
        .unwrap();
    assert_not_committed(reused.store.execute(delete));
    assert_eq!(
        reused
            .state
            .audit_window_abandonment(&reused.store, &reused_facts)
            .unwrap(),
        WindowAbandonmentNaturalState::ExactAcquired(reused_facts.clone())
    );
}

#[test]
fn unrelated_catalog_progress_refreshes_but_closure_drift_collides() {
    let refreshed = Fixture::new();
    let facts = refreshed.publish_acquired();
    let unrelated_thread = SyndicThreadId::from_bytes([9; 16]);
    let mut unrelated = HomeCommand::new(refreshed.store.home_revision().unwrap());
    unrelated
        .add(
            refreshed.state.catalog().publish(
                refreshed
                    .state
                    .catalog()
                    .revision(&refreshed.store)
                    .unwrap(),
                PublishCatalogRow::new(
                    unrelated_thread,
                    CatalogRowExpectation::Missing,
                    catalog_sources(),
                    catalog_facts(refreshed.target),
                )
                .unwrap(),
            ),
        )
        .unwrap();
    assert_committed(refreshed.store.execute(unrelated));
    let WindowAbandonmentNaturalState::ExactAcquired(replacement) = refreshed
        .state
        .audit_window_abandonment(&refreshed.store, &facts)
        .unwrap()
    else {
        panic!("unrelated catalog publication did not refresh exact acquisition facts")
    };
    assert!(replacement.catalog_domain_revision() > facts.catalog_domain_revision());
    assert_eq!(replacement.window_id(), facts.window_id());
    assert_eq!(replacement.thread_id(), facts.thread_id());
    assert_eq!(replacement.session_revision(), facts.session_revision());
    assert_eq!(replacement.claim_revision(), facts.claim_revision());

    let unrelated = refreshed
        .state
        .catalog()
        .row(
            &refreshed.store,
            unrelated_thread,
            CatalogPointReadLimit::schema_maximum(),
        )
        .unwrap()
        .unwrap();
    assert_committed(support::execute(
        &refreshed.store,
        refreshed.state.catalog().mark_stale(
            refreshed
                .state
                .catalog()
                .revision(&refreshed.store)
                .unwrap(),
            MarkCatalogRowStale::new(unrelated_thread, unrelated.revision()),
        ),
    ));
    assert!(matches!(
        refreshed
            .state
            .audit_window_abandonment(&refreshed.store, &replacement)
            .unwrap(),
        WindowAbandonmentNaturalState::ExactAcquired(_)
    ));

    let session_refresh = Fixture::new();
    let session_facts = session_refresh.publish_acquired();
    let session = session_refresh.state.session();
    let bootstrap = session
        .minimal_bootstrap(&session_refresh.store)
        .unwrap()
        .unwrap();
    assert_committed(support::execute(
        &session_refresh.store,
        session.create_claimed_window(
            session.revision(&session_refresh.store).unwrap(),
            CreateClaimedWindow::new(
                bootstrap.header().revision(),
                WindowId::from_bytes([8; 16]),
                session_refresh.target,
                SyndicThreadId::from_bytes([11; 16]),
                placement(8),
            ),
        ),
    ));
    let WindowAbandonmentNaturalState::ExactAcquired(session_replacement) = session_refresh
        .state
        .audit_window_abandonment(&session_refresh.store, &session_facts)
        .unwrap()
    else {
        panic!("unrelated session publication did not refresh exact acquisition facts")
    };
    assert!(session_replacement.session_revision() > session_facts.session_revision());
    assert_eq!(
        session_replacement.claim_generation(),
        session_facts.claim_generation()
    );
    assert_committed(session_refresh.abandon(&session_replacement));

    let row_drift = Fixture::new();
    let row_facts = row_drift.publish_acquired();
    let current = row_drift
        .state
        .catalog()
        .current_row_source(
            &row_drift.store,
            row_drift.thread_id,
            CatalogPointReadLimit::schema_maximum(),
        )
        .unwrap()
        .unwrap();
    let mut update_row = HomeCommand::new(row_drift.store.home_revision().unwrap());
    update_row
        .add(
            row_drift.state.catalog().publish(
                row_drift
                    .state
                    .catalog()
                    .revision(&row_drift.store)
                    .unwrap(),
                PublishCatalogRow::new(
                    row_drift.thread_id,
                    CatalogRowExpectation::Revision(current.row().revision()),
                    current.row().sources(),
                    current.row().facts().clone(),
                )
                .unwrap(),
            ),
        )
        .unwrap();
    assert_committed(row_drift.store.execute(update_row));
    assert_eq!(
        row_drift
            .state
            .audit_window_abandonment(&row_drift.store, &row_facts)
            .unwrap(),
        WindowAbandonmentNaturalState::Collision
    );

    let reverse_drift = Fixture::new();
    let reverse_facts = reverse_drift.publish_acquired();
    let session = reverse_drift.state.session();
    let bootstrap = session
        .minimal_bootstrap(&reverse_drift.store)
        .unwrap()
        .unwrap();
    let window = &bootstrap.windows()[0];
    assert_committed(support::execute(
        &reverse_drift.store,
        session.replace_claim(
            session.revision(&reverse_drift.store).unwrap(),
            ReplaceWindowClaim::new(
                bootstrap.header().revision(),
                reverse_drift.window_id,
                window.revision(),
                window.selected_thread(),
                reverse_drift.target,
                SyndicThreadId::from_bytes([10; 16]),
            ),
        ),
    ));
    assert_eq!(
        reverse_drift
            .state
            .audit_window_abandonment(&reverse_drift.store, &reverse_facts)
            .unwrap(),
        WindowAbandonmentNaturalState::Collision
    );
}

#[test]
fn successful_abandonment_releases_one_session_capacity_slot() {
    let fixture = Fixture::new();
    let facts = fixture.publish_acquired();
    assert_committed(fixture.abandon(&facts));

    let session = fixture.state.session();
    let bootstrap = session.minimal_bootstrap(&fixture.store).unwrap().unwrap();
    assert_committed(support::execute(
        &fixture.store,
        session.create_claimed_window(
            session.revision(&fixture.store).unwrap(),
            CreateClaimedWindow::new(
                bootstrap.header().revision(),
                WindowId::from_bytes([6; 16]),
                fixture.target,
                SyndicThreadId::from_bytes([7; 16]),
                placement(6),
            ),
        ),
    ));
}

fn initialize_empty_session(store: &HomeStore, state: &BerylState) {
    let session = state.session();
    let window_id = WindowId::from_bytes([1; 16]);
    assert_committed(support::execute(
        store,
        session.initialize_threadless(
            session.revision(store).unwrap(),
            InitializeThreadlessWindow::new(window_id, placement(1)),
        ),
    ));
    let bootstrap = session.minimal_bootstrap(store).unwrap().unwrap();
    let window = &bootstrap.windows()[0];
    assert_committed(support::execute(
        store,
        session.remove_window(
            session.revision(store).unwrap(),
            RemoveSessionWindow::new(
                bootstrap.header().revision(),
                window_id,
                window.revision(),
                window.selected_thread(),
            ),
        ),
    ));
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

fn placement(seed: i32) -> WindowPlacement {
    WindowPlacement::new(
        WindowBounds::new(seed, seed + 1, 900, 700).unwrap(),
        WindowDisplayState::Normal,
        None,
        None,
    )
}

fn assert_committed(outcome: CommandOutcome) {
    match outcome {
        CommandOutcome::Committed {
            later_failure: None,
            ..
        } => {}
        outcome => panic!("expected committed outcome, got {outcome:?}"),
    }
}

fn assert_not_committed(outcome: CommandOutcome) {
    assert!(matches!(outcome, CommandOutcome::NotCommitted { .. }));
}
