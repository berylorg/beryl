#![cfg(feature = "test-faults")]

#[path = "same_window_thread_acquisition/candidate_recovery.rs"]
mod candidate_recovery;

#[path = "same_window_thread_acquisition/canonical_election.rs"]
mod canonical_election;

#[path = "acquisition_support/empty_after_edit.rs"]
mod empty_after_edit;

include!("same_window_thread_acquisition/support.rs");
use beryl_app::catalog_projection::{
    ThreadCatalogProjectionPreparation, prepare_thread_catalog_projection,
};
use beryl_app::same_window_thread_acquisition::*;
use beryl_model::InputGateRevision;
use syndic_storage::test_faults::{FixtureBatch, FixtureRecord};
use syndic_storage::{InputGateRecord, InputGateState};

fn request(
    fixture: &Fixture,
    selected: Option<WindowClaimSelection>,
    seed: u8,
) -> SameWindowThreadRequest {
    SameWindowThreadRequest::new(
        fixture.window,
        selected,
        fixture.target,
        thread(seed),
        SyndicDraftId::from_bytes([seed; 16]),
        ExecutionBinding::new(
            fixture.target.runtime_id(),
            fixture.target.root_id(),
            native(r"C:\Work\Beryl"),
        ),
        SyndicTimestamp::from_unix_millis(100),
        DraftEditHistoryPolicyV1::new(65_536, 1).unwrap(),
    )
    .unwrap()
}
fn project(fixture: &Fixture, seed: u8) {
    match prepare_thread_catalog_projection(
        &fixture.store,
        &fixture.syndic,
        &fixture.state,
        thread(seed),
    )
    .unwrap()
    {
        ThreadCatalogProjectionPreparation::Publish(command) => assert!(matches!(
            fixture.store.execute(command),
            CommandOutcome::Committed { .. }
        )),
        ThreadCatalogProjectionPreparation::ExactCurrent => {}
        ThreadCatalogProjectionPreparation::ThreadMissing => panic!("missing projection source"),
    }
}
fn prepared(
    fixture: &Fixture,
    selected: Option<WindowClaimSelection>,
    seed: u8,
) -> SameWindowThreadAcquisition {
    match request(fixture, selected, seed)
        .prepare(
            &fixture.store,
            &fixture.state,
            &fixture.syndic,
            CommandCancellation::new(),
        )
        .unwrap()
    {
        SameWindowThreadPreparation::Prepared(prepared) => prepared,
        SameWindowThreadPreparation::Current { .. } => panic!("unexpected current no-op"),
    }
}
fn settled(fixture: &Fixture, prepared: SameWindowThreadAcquisition) -> SameWindowThreadCommit {
    match prepared.commit(&fixture.store, &fixture.state) {
        SameWindowThreadOutcome::Settled(commit) => commit,
        _ => panic!("expected exact committed result"),
    }
}
fn disqualify_submission(fixture: &Fixture, seed: u8) {
    let mut batch = FixtureBatch::new();
    batch
        .put(FixtureRecord::InputGate(
            InputGateRecord::new(
                thread(seed),
                InputGateRevision::new(2).unwrap(),
                InputGateState::Idle,
                1,
                None,
                None,
                0,
                0,
                0,
            )
            .unwrap(),
        ))
        .unwrap();
    execute(
        &fixture.store,
        fixture
            .syndic
            .fixture_contribution(fixture.syndic.revision(&fixture.store).unwrap(), batch),
    );
}

fn recover_fixture(fixture: Fixture) -> Fixture {
    let Fixture {
        store,
        faults,
        window,
        target,
        _directory,
        ..
    } = fixture;
    let candidate = store.recover_same_home().unwrap();
    let state = BerylState::reacquire_candidate(&candidate).unwrap();
    let syndic = SyndicStorage::reacquire_candidate(&candidate).unwrap();
    let store = candidate.publish().unwrap();
    Fixture {
        store,
        state,
        syndic,
        faults,
        window,
        target,
        _directory,
    }
}

#[test]
fn deterministic_reuse_replaces_existing_claim_without_creating_window_or_fallback() {
    let fixture = Fixture::new();
    project(&fixture, 3);
    project(&fixture, 4);
    let prior = fixture.commit(fixture.prepare(None, 4));
    disqualify_submission(&fixture, 4);
    let before = fixture.store.home_revision().unwrap();
    let prepared = prepared(&fixture, Some(prior.selection), 9);
    assert_eq!(prepared.disposition(), SameWindowThreadDisposition::Reused);
    assert_eq!(prepared.future_selection().thread_id(), thread(3));
    let commit = settled(&fixture, prepared);
    assert!(fixture.store.home_revision().unwrap() > before);
    assert_eq!(commit.window.remembered_target(), Some(fixture.target));
    let bootstrap = fixture
        .state
        .session()
        .minimal_bootstrap(&fixture.store)
        .unwrap()
        .unwrap();
    assert_eq!(bootstrap.windows().len(), 1);
    assert_eq!(bootstrap.header().fallback(), Some(fixture.target));
    assert_eq!(
        fixture
            .state
            .session()
            .thread_claim_catalog_source(&fixture.store, thread(4))
            .unwrap()
            .claim(),
        None
    );
    assert!(
        fixture
            .syndic
            .thread(
                &fixture.store,
                thread(9),
                syndic_storage::SyndicPointReadLimit::new(65536).unwrap()
            )
            .unwrap()
            .is_none()
    );
    assert_eq!(
        fixture
            .state
            .catalog()
            .current_row_source(
                &fixture.store,
                thread(4),
                CatalogPointReadLimit::schema_maximum()
            )
            .unwrap()
            .unwrap()
            .row()
            .facts()
            .claim(),
        CatalogClaimSummary::Unclaimed
    );
}

#[test]
fn submitted_empty_source_and_candidate_force_atomic_fresh_creation() {
    let fixture = Fixture::new();
    project(&fixture, 3);
    project(&fixture, 4);
    let prior = fixture.commit(fixture.prepare(None, 3));
    disqualify_submission(&fixture, 3);
    disqualify_submission(&fixture, 4);
    let commit = settled(&fixture, prepared(&fixture, Some(prior.selection), 9));
    assert_eq!(commit.disposition, SameWindowThreadDisposition::Created);
    assert_eq!(commit.selection.thread_id(), thread(9));
    assert_eq!(commit.draft, SyndicDraftId::from_bytes([9; 16]));
    assert_eq!(
        fixture
            .state
            .session()
            .minimal_bootstrap(&fixture.store)
            .unwrap()
            .unwrap()
            .windows()
            .len(),
        1
    );
    assert_eq!(
        fixture
            .syndic
            .current_draft(
                &fixture.store,
                thread(9),
                syndic_storage::SyndicPointReadLimit::new(65536).unwrap()
            )
            .unwrap()
            .unwrap()
            .draft()
            .id(),
        commit.draft
    );
}

#[test]
fn current_pristine_source_is_noop_even_when_older_unclaimed_thread_exists() {
    let fixture = Fixture::new();
    project(&fixture, 3);
    project(&fixture, 4);
    let prior = fixture.commit(fixture.prepare(None, 4));
    let before = fixture.store.home_revision().unwrap();
    let SameWindowThreadPreparation::Current {
        window,
        claim,
        draft,
    } = request(&fixture, Some(prior.selection), 9)
        .prepare(
            &fixture.store,
            &fixture.state,
            &fixture.syndic,
            CommandCancellation::new(),
        )
        .unwrap()
    else {
        panic!("current should remain selected");
    };
    assert_eq!(window.window_id(), fixture.window);
    assert_eq!(claim, prior.claim);
    assert_eq!(draft, SyndicDraftId::from_bytes([4; 16]));
    assert_eq!(fixture.store.home_revision().unwrap(), before);
}

#[test]
fn cancellation_and_writer_race_leave_original_selection_and_no_fallback() {
    let fixture = Fixture::new();
    let cancellation = CommandCancellation::new();
    let SameWindowThreadPreparation::Prepared(cancelled_acquisition) = request(&fixture, None, 9)
        .prepare(
            &fixture.store,
            &fixture.state,
            &fixture.syndic,
            cancellation.clone(),
        )
        .unwrap()
    else {
        panic!();
    };
    cancellation.cancel();
    let before = fixture.store.home_revision().unwrap();
    assert!(matches!(
        cancelled_acquisition.commit(&fixture.store, &fixture.state),
        SameWindowThreadOutcome::NotCommitted(_)
    ));
    assert_eq!(fixture.store.home_revision().unwrap(), before);
    let raced_acquisition = prepared(&fixture, None, 10);
    fixture.commit(fixture.prepare(None, 3));
    let before = fixture.store.home_revision().unwrap();
    assert!(matches!(
        raced_acquisition.commit(&fixture.store, &fixture.state),
        SameWindowThreadOutcome::NotCommitted(_)
    ));
    assert_eq!(fixture.store.home_revision().unwrap(), before);
    assert!(
        fixture
            .syndic
            .thread(
                &fixture.store,
                thread(10),
                syndic_storage::SyndicPointReadLimit::new(65536).unwrap()
            )
            .unwrap()
            .is_none()
    );
}

#[test]
fn acknowledgement_loss_reconciles_original_fresh_creation_without_another_command() {
    let fixture = Fixture::new();
    let prepared = prepared(&fixture, None, 9);
    let expected = prepared.future_selection();
    fixture
        .faults
        .fail_next(FaultPoint::AfterCommitBeforePersist);
    let SameWindowThreadOutcome::Pending(pending) = prepared.commit(&fixture.store, &fixture.state)
    else {
        panic!("expected uncertainty");
    };
    assert_eq!(pending.future_selection(), expected);
    let before = fixture.store.home_revision().unwrap();
    let SameWindowThreadOutcome::Settled(commit) =
        pending.reconcile(&fixture.store, &fixture.state)
    else {
        panic!("expected exact-new reconciliation");
    };
    assert_eq!(commit.selection, expected);
    assert_eq!(fixture.store.home_revision().unwrap(), before);
}

#[test]
fn occupied_pristine_candidate_is_excluded() {
    let fixture = Fixture::new();
    project(&fixture, 3);
    project(&fixture, 4);
    let source = fixture.commit(fixture.prepare(None, 4));
    disqualify_submission(&fixture, 4);
    let header = fixture
        .state
        .session()
        .minimal_bootstrap(&fixture.store)
        .unwrap()
        .unwrap()
        .header()
        .revision();
    execute(
        &fixture.store,
        fixture.state.session().create_claimed_window(
            fixture.state.session().revision(&fixture.store).unwrap(),
            beryl_state::CreateClaimedWindow::new(
                header,
                WindowId::from_bytes([7; 16]),
                fixture.target,
                thread(3),
                placement(),
            ),
        ),
    );
    project(&fixture, 3);
    let commit = settled(&fixture, prepared(&fixture, Some(source.selection), 9));
    assert_eq!(commit.disposition, SameWindowThreadDisposition::Created);
    assert_eq!(
        fixture
            .state
            .session()
            .thread_claim_catalog_source(&fixture.store, thread(3))
            .unwrap()
            .claim()
            .unwrap()
            .window_id(),
        WindowId::from_bytes([7; 16])
    );
}

#[test]
fn fresh_creation_joins_stale_predecessor_summary_and_new_remembered_scope() {
    let fixture = Fixture::new();
    let prior = fixture.commit(fixture.prepare(None, 3));
    let attributes = fixture
        .syndic
        .thread_attributes(
            &fixture.store,
            thread(3),
            syndic_storage::SyndicPointReadLimit::new(65536).unwrap(),
        )
        .unwrap()
        .unwrap();
    let mut batch = FixtureBatch::new();
    batch
        .put(FixtureRecord::ThreadAttributes(
            syndic_storage::test_faults::thread_attributes_with_revision(
                &attributes,
                attributes.revision().checked_next().unwrap(),
            ),
        ))
        .unwrap();
    execute(
        &fixture.store,
        fixture
            .syndic
            .fixture_contribution(fixture.syndic.revision(&fixture.store).unwrap(), batch),
    );
    let target = RememberedTarget::new(fixture.target.runtime_id(), RootId::from_bytes([8; 16]));
    execute(
        &fixture.store,
        fixture.state.runtime_roots().add_root(
            fixture
                .state
                .runtime_roots()
                .revision(&fixture.store)
                .unwrap(),
            beryl_state::AddConfiguredRoot::new(
                target.runtime_id(),
                RootRegistration::new(
                    target.root_id(),
                    native(r"C:\Other"),
                    host(r"C:\Other"),
                    UnixMillis::new(2),
                    AvailabilitySnapshot::unknown(),
                ),
            ),
        ),
    );
    let request = SameWindowThreadRequest::new(
        fixture.window,
        Some(prior.selection),
        target,
        thread(9),
        SyndicDraftId::from_bytes([9; 16]),
        ExecutionBinding::new(target.runtime_id(), target.root_id(), native(r"C:\Other")),
        SyndicTimestamp::from_unix_millis(100),
        DraftEditHistoryPolicyV1::new(65536, 1).unwrap(),
    )
    .unwrap();
    let SameWindowThreadPreparation::Prepared(prepared) = request
        .prepare(
            &fixture.store,
            &fixture.state,
            &fixture.syndic,
            CommandCancellation::new(),
        )
        .unwrap()
    else {
        panic!();
    };
    let before = fixture.syndic.revision(&fixture.store).unwrap();
    let commit = settled(&fixture, prepared);
    assert_eq!(
        fixture.syndic.revision(&fixture.store).unwrap(),
        before.checked_next().unwrap()
    );
    assert_eq!(commit.window.remembered_target(), Some(target));
    assert_eq!(
        fixture
            .state
            .session()
            .minimal_bootstrap(&fixture.store)
            .unwrap()
            .unwrap()
            .header()
            .fallback(),
        Some(target)
    );
    assert!(matches!(
        fixture
            .syndic
            .prepare_thread_catalog_summary(&fixture.store, thread(3))
            .unwrap()
            .unwrap(),
        syndic_storage::ThreadCatalogSummaryPreparation::ExactCurrent(_)
    ));
    assert_eq!(
        fixture
            .state
            .catalog()
            .current_row_source(
                &fixture.store,
                thread(3),
                CatalogPointReadLimit::schema_maximum()
            )
            .unwrap()
            .unwrap()
            .row()
            .facts()
            .claim(),
        CatalogClaimSummary::Unclaimed
    );
}

#[test]
fn reused_target_and_stale_predecessor_summary_share_one_atomic_participant() {
    for reject in [true, false] {
        let mut fixture = Fixture::new();
        project(&fixture, 3);
        let prior = fixture.commit(fixture.prepare(None, 4));
        disqualify_submission(&fixture, 4);
        let mut batch = FixtureBatch::new();
        for seed in [3, 4] {
            let attributes = fixture
                .syndic
                .thread_attributes(
                    &fixture.store,
                    thread(seed),
                    syndic_storage::SyndicPointReadLimit::new(65536).unwrap(),
                )
                .unwrap()
                .unwrap();
            batch
                .put(FixtureRecord::ThreadAttributes(
                    syndic_storage::test_faults::thread_attributes_with_revision(
                        &attributes,
                        attributes.revision().checked_next().unwrap(),
                    ),
                ))
                .unwrap();
        }
        execute(
            &fixture.store,
            fixture
                .syndic
                .fixture_contribution(fixture.syndic.revision(&fixture.store).unwrap(), batch),
        );
        assert!(matches!(
            fixture
                .syndic
                .prepare_thread_catalog_summary(&fixture.store, thread(4))
                .unwrap()
                .unwrap(),
            syndic_storage::ThreadCatalogSummaryPreparation::PreparedReplacement(_)
        ));
        assert!(matches!(
            fixture
                .syndic
                .prepare_thread_catalog_summary(&fixture.store, thread(3))
                .unwrap()
                .unwrap(),
            syndic_storage::ThreadCatalogSummaryPreparation::PreparedReplacement(_)
        ));
        let prepared = prepared(&fixture, Some(prior.selection), 9);
        assert_eq!(prepared.disposition(), SameWindowThreadDisposition::Reused);
        let home_revision = fixture.store.home_revision().unwrap();
        let domain_revision = fixture.syndic.revision(&fixture.store).unwrap();
        if reject {
            fixture.faults.fail_next(FaultPoint::BeforeCommit);
        }
        let outcome = prepared.commit(&fixture.store, &fixture.state);
        if reject {
            assert!(matches!(outcome, SameWindowThreadOutcome::NotCommitted(_)));
            fixture = recover_fixture(fixture);
            assert_eq!(fixture.store.home_revision().unwrap(), home_revision);
            assert_eq!(
                fixture.syndic.revision(&fixture.store).unwrap(),
                domain_revision
            );
            assert_eq!(
                fixture
                    .state
                    .session()
                    .capture_window_removal(&fixture.store, fixture.window)
                    .unwrap()
                    .window()
                    .selected_thread(),
                Some(prior.selection)
            );
            assert!(matches!(
                fixture
                    .syndic
                    .prepare_thread_catalog_summary(&fixture.store, thread(4))
                    .unwrap()
                    .unwrap(),
                syndic_storage::ThreadCatalogSummaryPreparation::PreparedReplacement(_)
            ));
            assert!(matches!(
                fixture
                    .syndic
                    .prepare_thread_catalog_summary(&fixture.store, thread(3))
                    .unwrap()
                    .unwrap(),
                syndic_storage::ThreadCatalogSummaryPreparation::PreparedReplacement(_)
            ));
        } else {
            let SameWindowThreadOutcome::Settled(commit) = outcome else {
                panic!();
            };
            assert_eq!(commit.selection.thread_id(), thread(3));
            assert_eq!(
                fixture.syndic.revision(&fixture.store).unwrap(),
                domain_revision.checked_next().unwrap()
            );
            assert!(matches!(
                fixture
                    .syndic
                    .prepare_thread_catalog_summary(&fixture.store, thread(4))
                    .unwrap()
                    .unwrap(),
                syndic_storage::ThreadCatalogSummaryPreparation::ExactCurrent(_)
            ));
            assert!(matches!(
                fixture
                    .syndic
                    .prepare_thread_catalog_summary(&fixture.store, thread(3))
                    .unwrap()
                    .unwrap(),
                syndic_storage::ThreadCatalogSummaryPreparation::ExactCurrent(_)
            ));
        }
        assert!(
            fixture
                .syndic
                .thread(
                    &fixture.store,
                    thread(9),
                    syndic_storage::SyndicPointReadLimit::new(65536).unwrap()
                )
                .unwrap()
                .is_none()
        );
    }
}

#[test]
fn writer_failure_preserves_old_claim_and_absent_fresh_closure() {
    let fixture = Fixture::new();
    let prepared = prepared(&fixture, None, 9);
    fixture.faults.fail_next(FaultPoint::BeforeCommit);
    assert!(matches!(
        prepared.commit(&fixture.store, &fixture.state),
        SameWindowThreadOutcome::NotCommitted(_)
    ));
    let fixture = recover_fixture(fixture);
    assert_eq!(
        fixture
            .state
            .session()
            .capture_window_removal(&fixture.store, fixture.window)
            .unwrap()
            .window()
            .selected_thread(),
        None
    );
    assert!(
        fixture
            .syndic
            .thread(
                &fixture.store,
                thread(9),
                syndic_storage::SyndicPointReadLimit::new(65536).unwrap()
            )
            .unwrap()
            .is_none()
    );
}

#[test]
fn coherent_idle_noninitial_bindings_preserve_current_and_unclaimed_reuse_eligibility() {
    let fixture = Fixture::new();
    project(&fixture, 3);
    let prior = fixture.commit(fixture.prepare(None, 4));
    let mut batch = FixtureBatch::new();
    for seed in [3, 4] {
        let current = fixture
            .syndic
            .current_binding(
                &fixture.store,
                thread(seed),
                syndic_storage::SyndicPointReadLimit::new(65536).unwrap(),
            )
            .unwrap()
            .unwrap();
        let revision = beryl_model::BindingRevision::new(2).unwrap();
        batch
            .put(FixtureRecord::Binding(syndic_storage::BindingRecord::new(
                thread(seed),
                revision,
                current.binding().selected_path(),
                syndic_storage::BindingState::unbound("advanced binding").unwrap(),
            )))
            .unwrap();
        batch
            .put(FixtureRecord::BindingHead(
                syndic_storage::BindingHeadRecord::new(
                    thread(seed),
                    revision,
                    syndic_storage::BindingLifecycle::Unbound,
                    syndic_storage::empty_selected_path_digest(),
                ),
            ))
            .unwrap();
    }
    execute(
        &fixture.store,
        fixture
            .syndic
            .fixture_contribution(fixture.syndic.revision(&fixture.store).unwrap(), batch),
    );
    assert!(matches!(
        request(&fixture, Some(prior.selection), 9)
            .prepare(
                &fixture.store,
                &fixture.state,
                &fixture.syndic,
                CommandCancellation::new()
            )
            .unwrap(),
        SameWindowThreadPreparation::Current { .. }
    ));
    disqualify_submission(&fixture, 4);
    let execution = request(&fixture, Some(prior.selection), 9);
    let SameWindowThreadPreparation::Prepared(prepared) = execution
        .prepare(
            &fixture.store,
            &fixture.state,
            &fixture.syndic,
            CommandCancellation::new(),
        )
        .unwrap()
    else {
        panic!("submitted source acquired Current authority");
    };
    assert_eq!(prepared.disposition(), SameWindowThreadDisposition::Reused);
    assert_eq!(settled(&fixture, prepared).selection.thread_id(), thread(3));
}

#[path = "same_window_thread_acquisition/binding_eligibility.rs"]
mod binding_eligibility;

#[test]
fn retired_preparation_cannot_commit_into_same_home_replacement_generation() {
    let fixture = Fixture::new();
    let prepared = prepared(&fixture, None, 9);
    let original = fixture
        .state
        .session()
        .capture_window_removal(&fixture.store, fixture.window)
        .unwrap();
    let mut command = HomeCommand::new(fixture.store.home_revision().unwrap());
    command
        .add(fixture.state.session().update_placement(
            fixture.state.session().revision(&fixture.store).unwrap(),
            UpdateWindowPlacement::new(
                original.header().revision(),
                fixture.window,
                original.window().revision(),
                WindowPlacement::new(
                    WindowBounds::new(20, 0, 900, 700).unwrap(),
                    WindowDisplayState::Normal,
                    None,
                    None,
                ),
            ),
        ))
        .unwrap();
    fixture.faults.fail_next(FaultPoint::BeforeCommit);
    assert!(matches!(
        fixture.store.execute(command),
        CommandOutcome::NotCommitted { .. }
    ));
    let Fixture {
        store, _directory, ..
    } = fixture;
    let candidate = store.recover_same_home().unwrap();
    let state = BerylState::reacquire_candidate(&candidate).unwrap();
    let replacement = candidate.publish().unwrap();
    let before = replacement.home_revision().unwrap();
    assert!(matches!(
        prepared.commit(&replacement, &state),
        SameWindowThreadOutcome::NotCommitted(SameWindowThreadError::SourceChanged)
    ));
    assert_eq!(replacement.home_revision().unwrap(), before);
}
