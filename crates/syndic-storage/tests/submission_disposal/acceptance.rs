use super::{
    fixture::{Fixture, read_limit},
    support::*,
};
use syndic_storage::{
    DraftEditorCandidateSessionDisposeOutcomeV1, DraftEditorCandidateSessionDisposeRequestV1,
    DraftRootHistoryPairV1,
};

#[test]
fn idle_submission_leaves_a_readable_disposed_session_and_exact_replay_after_reopen() {
    successful_submission(false);
}

#[test]
fn queued_submission_leaves_a_readable_disposed_session_and_exact_replay_after_reopen() {
    successful_submission(true);
}

fn successful_submission(queued: bool) {
    let fixture = Fixture::new("success", 10, queued);
    committed(fixture.accept());
    let terminal = assert_exact_acceptance(&fixture);
    assert_exact_receipt_replay(&fixture, &terminal);
    let fixture = fixture.reopen();
    assert_eq!(assert_exact_acceptance(&fixture), terminal);
    let before = fixture.store.home_revision().unwrap();
    assert!(matches!(
        fixture.accept(),
        CommandOutcome::NotCommitted { .. }
    ));
    assert_eq!(fixture.store.home_revision().unwrap(), before);
    assert_eq!(assert_exact_acceptance(&fixture), terminal);
    assert_exact_receipt_replay(&fixture, &terminal);
}

pub(super) fn assert_exact_acceptance(fixture: &Fixture) -> DraftEditorCandidateSessionV1 {
    let Fixture {
        store,
        storage,
        thread,
        source,
        acceptance,
        queued,
        ..
    } = fixture;
    let gate = storage
        .input_gate(store, *thread, read_limit())
        .unwrap()
        .unwrap();
    assert_eq!(
        storage
            .non_idle_gate_source(store, *thread, read_limit())
            .unwrap(),
        Some(syndic_storage::NonIdleGateSourceRecord::new(
            *thread,
            gate.revision()
        ))
    );
    assert_eq!(
        storage
            .first_acceptance_status(store, acceptance, read_limit())
            .unwrap(),
        fixture.expected_status()
    );
    let selected = current(storage, store, *thread);
    assert_eq!(selected.draft().id(), acceptance.next_draft_id());
    assert_ne!(selected.draft().id(), source.draft_id());
    let terminal = match storage
        .draft_editor_candidate_session(store, source.draft_id(), source.session_id())
        .unwrap()
    {
        DraftEditorCandidateSessionReadOutcomeV1::Disposed(head) => head,
        other => panic!("accepted submission left an invalid terminal session: {other:?}"),
    };
    assert_eq!(
        terminal.session_generation(),
        source.session_generation() + 1
    );
    assert_eq!(terminal.newest_root(), source.newest_root());
    assert_eq!(terminal.newest_history(), source.newest_history());
    assert_eq!(terminal.newest_root(), terminal.published_root());
    assert_eq!(terminal.newest_history(), terminal.published_history());
    assert_eq!(
        terminal.disposal_operation_id(),
        Some(acceptance.session_disposal_operation_id())
    );
    assert!(terminal.active_operation().is_none());
    assert_eq!(
        storage
            .accepted_input(store, acceptance.accepted_input_id(), read_limit())
            .unwrap()
            .is_some(),
        *queued
    );
    assert_eq!(
        storage
            .canonical_item(store, acceptance.idle_user_item_id(), read_limit())
            .unwrap()
            .is_some(),
        !*queued
    );
    terminal
}

#[test]
fn acceptance_reconciliation_requires_its_current_non_idle_source() {
    let fixture = Fixture::new("acceptance-source", 31, false);
    committed(fixture.accept());
    assert_exact_acceptance(&fixture);
    let mut corruption = syndic_storage::test_faults::FixtureBatch::new();
    corruption
        .delete(syndic_storage::test_faults::FixtureDelete::NonIdleGateSource(fixture.thread))
        .unwrap();
    committed(execute(
        &fixture.store,
        fixture.storage.fixture_contribution(
            fixture.storage.revision(&fixture.store).unwrap(),
            corruption,
        ),
    ));
    assert!(matches!(
        fixture
            .storage
            .first_acceptance_status(&fixture.store, &fixture.acceptance, read_limit()),
        Err(syndic_storage::SyndicReadError::Invariant(
            "current input gate and non-idle source disagree"
        ))
    ));
}

pub(super) fn assert_exact_receipt_replay(
    fixture: &Fixture,
    terminal: &DraftEditorCandidateSessionV1,
) {
    let request = DraftEditorCandidateSessionDisposeRequestV1::new(
        fixture.source.draft_id(),
        fixture.source.session_id(),
        fixture.acceptance.session_disposal_operation_id(),
        fixture.source.session_generation(),
        DraftRootHistoryPairV1::new(
            fixture.source.newest_root(),
            fixture.source.newest_history(),
        ),
    );
    let prepared = fixture
        .storage
        .prepare_dispose_draft_editor_candidate_session(&fixture.store, request)
        .unwrap();
    let before = fixture.store.home_revision().unwrap();
    let outcome = execute(
        &fixture.store,
        fixture.storage.dispose_draft_editor_candidate_session(
            fixture.storage.revision(&fixture.store).unwrap(),
            prepared.clone(),
        ),
    );
    assert!(matches!(&outcome, CommandOutcome::NotCommitted { .. }));
    let replay = fixture
        .storage
        .reconcile_draft_editor_candidate_session_disposal(&fixture.store, &prepared, outcome)
        .unwrap();
    let DraftEditorCandidateSessionDisposeOutcomeV1::ExactReplay(receipt) = replay else {
        panic!("accepted disposal did not have an exact ordinary receipt: {replay:?}");
    };
    assert_eq!(receipt.before_head(), &fixture.source);
    assert_eq!(receipt.after_head(), terminal);
    assert_eq!(fixture.store.home_revision().unwrap(), before);
}
