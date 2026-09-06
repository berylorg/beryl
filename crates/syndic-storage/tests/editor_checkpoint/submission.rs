use super::{
    disposal::assert_receipt_replay,
    fixture::{Fixture, read_limit},
    publication_support::head,
    support::*,
};
use syndic_storage::{
    DraftEditorCandidateSessionDisposeRequestV1, DraftRootHistoryPairV1, FirstAcceptanceStatus,
};

#[test]
fn idle_submission_accepts_a_nonzero_reopened_checkpoint_and_preserves_exact_disposal() {
    accepts_opening(false);
}

#[test]
fn queued_submission_accepts_a_nonzero_reopened_checkpoint_and_preserves_exact_disposal() {
    accepts_opening(true);
}

fn accepts_opening(queued: bool) {
    let fixture = Fixture::opening("opening-submission", 150, queued, true);
    let durable = current(&fixture.storage, &fixture.store, fixture.thread);
    assert!(
        fixture
            .storage
            .draft_editor_candidate_is_saved(
                &fixture.store,
                DraftEditorCandidateActivationBindingV1::from_head(&fixture.source),
                selector(&durable),
            )
            .unwrap()
    );
    committed(fixture.accept());
    assert_eq!(
        fixture
            .storage
            .first_acceptance_status(&fixture.store, &fixture.acceptance, read_limit())
            .unwrap(),
        fixture.expected_status()
    );
    let terminal = head(&fixture.storage, &fixture.store, &fixture.source);
    assert!(matches!(
        fixture
            .storage
            .draft_editor_candidate_session(
                &fixture.store,
                fixture.source.draft_id(),
                fixture.source.session_id(),
            )
            .unwrap(),
        DraftEditorCandidateSessionReadOutcomeV1::Disposed(_)
    ));
    assert_eq!(
        terminal.session_generation(),
        fixture.source.session_generation() + 1
    );
    assert_eq!(terminal.newest_root(), fixture.source.published_root());
    assert_eq!(
        terminal.newest_history(),
        fixture.source.published_history()
    );
    assert_eq!(terminal.newest_root(), terminal.published_root());
    assert_eq!(terminal.newest_history(), terminal.published_history());
    assert_eq!(
        terminal.disposal_operation_id(),
        Some(fixture.acceptance.session_disposal_operation_id())
    );
    assert!(terminal.active_operation().is_none());
    let fixture = fixture.reopen();
    assert_eq!(
        head(&fixture.storage, &fixture.store, &fixture.source),
        terminal
    );
    assert_eq!(
        fixture
            .storage
            .first_acceptance_status(&fixture.store, &fixture.acceptance, read_limit())
            .unwrap(),
        fixture.expected_status()
    );
    assert_eq!(
        current(&fixture.storage, &fixture.store, fixture.thread)
            .draft()
            .id(),
        fixture.acceptance.next_draft_id()
    );
    assert_eq!(
        fixture
            .storage
            .accepted_input(
                &fixture.store,
                fixture.acceptance.accepted_input_id(),
                read_limit()
            )
            .unwrap()
            .is_some(),
        queued
    );
    assert_eq!(
        fixture
            .storage
            .canonical_item(
                &fixture.store,
                fixture.acceptance.idle_user_item_id(),
                read_limit()
            )
            .unwrap()
            .is_some(),
        !queued
    );
    let request = DraftEditorCandidateSessionDisposeRequestV1::new(
        fixture.source.draft_id(),
        fixture.source.session_id(),
        fixture.acceptance.session_disposal_operation_id(),
        fixture.source.session_generation(),
        DraftRootHistoryPairV1::new(
            fixture.source.published_root(),
            fixture.source.published_history(),
        ),
    );
    assert_receipt_replay(
        &fixture.storage,
        &fixture.store,
        request,
        &fixture.source,
        &terminal,
    );
    let before = fixture.store.home_revision().unwrap();
    assert!(matches!(
        fixture.accept(),
        CommandOutcome::NotCommitted { .. }
    ));
    assert_eq!(fixture.store.home_revision().unwrap(), before);
    assert_eq!(
        fixture
            .storage
            .first_acceptance_status(&fixture.store, &fixture.acceptance, read_limit())
            .unwrap(),
        fixture.expected_status()
    );
}

#[test]
fn saved_empty_openings_remain_unsubmittable_without_disposing_the_editor() {
    for queued in [false, true] {
        let fixture = Fixture::opening("empty-submission", 170, queued, false);
        let durable = current(&fixture.storage, &fixture.store, fixture.thread);
        assert!(
            fixture
                .storage
                .draft_editor_candidate_is_saved(
                    &fixture.store,
                    DraftEditorCandidateActivationBindingV1::from_head(&fixture.source),
                    selector(&durable),
                )
                .unwrap()
        );
        let gate = fixture
            .storage
            .input_gate(&fixture.store, fixture.thread, read_limit())
            .unwrap();
        let before = fixture.store.home_revision().unwrap();
        assert!(matches!(
            fixture.accept(),
            CommandOutcome::NotCommitted { .. }
        ));
        assert_eq!(fixture.store.home_revision().unwrap(), before);
        assert_eq!(
            current(&fixture.storage, &fixture.store, fixture.thread),
            durable
        );
        assert_eq!(
            head(&fixture.storage, &fixture.store, &fixture.source),
            fixture.source
        );
        assert_eq!(
            fixture
                .storage
                .input_gate(&fixture.store, fixture.thread, read_limit())
                .unwrap(),
            gate
        );
        assert_eq!(
            fixture
                .storage
                .first_acceptance_status(&fixture.store, &fixture.acceptance, read_limit())
                .unwrap(),
            FirstAcceptanceStatus::ExactOld
        );
    }
}
