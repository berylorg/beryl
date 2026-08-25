#![cfg(feature = "test-faults")]

#[path = "phase141_syndic_composer_host/support.rs"]
mod fixture_support;
#[path = "phase166_syndic_composer_history/support.rs"]
mod support;

use std::num::NonZeroUsize;

use beryl_app::composer_host::{ComposerHostError, ComposerHostHistoryStatus, SyndicComposerHost};
use beryl_home_store::{CommandCancellation, HomeCommand};
use gpui_text_input::{MutationKind, RangeHistoryOutcome};
use syndic_storage::test_faults::DraftPieceImmutableDeletion;
use syndic_storage::{
    DraftHistoricalRootAdoptionErrorReasonV1, DraftHistoricalRootSelectionIntentV1,
    DraftHistoricalRootSelectionV1, DraftPieceMarkerDemandV1, DraftPieceMarkerDirectionV1,
    DraftPieceMarkerScopeV1,
};

use fixture_support::{fixture, fixture_with_history_budget, reopen};
use support::*;

#[test]
fn sequential_undo_redo_branching_and_fresh_activation_use_compact_durable_authority() {
    let (home, store, storage, thread) = fixture("phase166_sequence", 41);
    let (mut host, empty) = activated(storage, &store, thread, 42, 43);
    let a = commit_text(&mut host, &store, empty, 1, 0, 0, "a", 1, 1);
    let ab = commit_text(&mut host, &store, a, 2, 1, 1, "b", 2, 1);

    let undone = select_history(&mut host, &store, ab, 3, MutationKind::Undo);
    assert_eq!(candidate_text(storage, &store, undone), b"a");
    assert!(undone.range_history_frontier().redo_available);
    let redone = select_history(&mut host, &store, undone, 4, MutationKind::Redo);
    assert_eq!(candidate_text(storage, &store, redone), b"ab");

    let undone = select_history(&mut host, &store, redone, 5, MutationKind::Undo);
    let branched = commit_text(&mut host, &store, undone, 6, 1, 1, "c", 2, 1);
    assert_eq!(candidate_text(storage, &store, branched), b"ac");
    assert!(!branched.range_history_frontier().redo_available);
    let rejected = history_intent(branched, 7, MutationKind::Redo, position(2));
    host.begin_history_selection(&store, branched, rejected)
        .unwrap();
    assert_eq!(
        host.execute_history_selection(&store, rejected.key(), &CommandCancellation::new())
            .unwrap(),
        RangeHistoryOutcome::Rejected
    );
    assert_eq!(host.binding(), Some(branched));

    host.dispose_composer_service(&store).unwrap();
    host = SyndicComposerHost::new(storage);
    let fresh = reactivate(&mut host, &store, thread, 44, 45);
    assert_eq!(candidate_text(storage, &store, fresh), b"");
    drop(store);
    let (store, storage) = reopen(&home);
    let (mut restarted, fresh) = activated(storage, &store, thread, 46, 47);
    assert_eq!(candidate_text(storage, &store, fresh), b"");
    assert!(!fresh.range_history_frontier().undo_available);
    restarted.dispose_composer_service(&store).unwrap();
    assert!(restarted.binding().is_none());
}

#[test]
fn five_outcomes_pre_admission_cancel_and_candidate_drift_preserve_exact_live_state() {
    let (_home, store, storage, thread) = fixture("phase166_outcomes", 51);
    let (mut host, empty) = activated(storage, &store, thread, 52, 53);
    let a = commit_text(&mut host, &store, empty, 1, 0, 0, "a", 1, 1);
    let ab = commit_text(&mut host, &store, a, 2, 1, 1, "b", 2, 1);

    let cancelled = history_intent(ab, 3, MutationKind::Undo, position(2));
    host.begin_history_selection(&store, ab, cancelled).unwrap();
    let token = CommandCancellation::new();
    token.cancel();
    assert_eq!(
        host.execute_history_selection(&store, cancelled.key(), &token)
            .unwrap(),
        RangeHistoryOutcome::Cancelled
    );
    assert_eq!(host.binding(), Some(ab));
    assert_eq!(host.settlement_custody_in_use(), 0);

    let conflict = history_intent(ab, 4, MutationKind::Undo, position(2));
    host.begin_history_selection(&store, ab, conflict).unwrap();
    host.test_arm_history_before_execute_fault(move |store, storage| {
        direct_adopt(
            store,
            storage,
            DraftHistoricalRootSelectionIntentV1::new(
                ab.candidate(),
                operation_id(400),
                syndic_storage::DraftHistoricalRootDirectionV1::Undo,
            ),
        );
    });
    assert_eq!(
        host.execute_history_selection(&store, conflict.key(), &CommandCancellation::new())
            .unwrap(),
        RangeHistoryOutcome::Conflict
    );
    assert_eq!(host.binding(), Some(ab));

    let (mut host, current) = activated(storage, &store, thread, 54, 55);
    let current = commit_text(&mut host, &store, current, 5, 0, 0, "x", 1, 1);
    let error = history_intent(current, 6, MutationKind::Undo, position(1));
    host.begin_history_selection(&store, current, error)
        .unwrap();
    host.test_arm_history_before_execute_fault(move |store, storage| {
        let DraftHistoricalRootSelectionV1::Prepared(prepared) = storage
            .prepare_draft_historical_root_selection(
                store,
                DraftHistoricalRootSelectionIntentV1::new(
                    current.candidate(),
                    operation_id(6),
                    syndic_storage::DraftHistoricalRootDirectionV1::Undo,
                ),
            )
            .unwrap()
        else {
            panic!("history unexpectedly unavailable")
        };
        let mut command = HomeCommand::new(store.home_revision().unwrap());
        command
            .add(storage.error_draft_historical_root_adoption(
                storage.revision(store).unwrap(),
                prepared,
                DraftHistoricalRootAdoptionErrorReasonV1::InvalidAuthority,
            ))
            .unwrap();
        assert!(matches!(
            store.execute(command),
            beryl_home_store::CommandOutcome::Committed { .. }
        ));
    });
    assert_eq!(
        host.execute_history_selection(&store, error.key(), &CommandCancellation::new())
            .unwrap(),
        RangeHistoryOutcome::Error
    );
    assert_eq!(host.binding(), Some(current));
}

#[test]
fn identical_retry_replays_pre_admission_cancel_without_reissuing_adoption() {
    let (_home, store, storage, thread) = fixture("phase166_cancel_replay", 131);
    let (mut host, empty) = activated(storage, &store, thread, 132, 133);
    let a = commit_text(&mut host, &store, empty, 1, 0, 0, "a", 1, 1);
    let ab = commit_text(&mut host, &store, a, 2, 1, 1, "b", 2, 1);
    let intent = history_intent(ab, 3, MutationKind::Undo, position(2));
    let frontier = ab.range_history_frontier();

    host.begin_history_selection(&store, ab, intent).unwrap();
    let cancellation = CommandCancellation::new();
    cancellation.cancel();
    assert_eq!(
        host.execute_history_selection(&store, intent.key(), &cancellation)
            .unwrap(),
        RangeHistoryOutcome::Cancelled
    );

    host.begin_history_selection(&store, ab, intent).unwrap();
    assert_eq!(
        host.execute_history_selection(&store, intent.key(), &CommandCancellation::new())
            .unwrap(),
        RangeHistoryOutcome::Cancelled
    );
    assert_eq!(host.binding(), Some(ab));
    assert_eq!(host.binding().unwrap().range_history_frontier(), frontier);
    assert_eq!(candidate_text(storage, &store, ab), b"ab");
    assert_eq!(host.settlement_custody_in_use(), 0);
}

#[test]
fn service_replacement_cuts_late_history_completion_and_custody_exactly() {
    let (_home, store, storage, thread) = fixture("phase166_lifecycle", 61);
    let mut host = SyndicComposerHost::with_settlement_custody_capacity(storage, NonZeroUsize::MIN);
    let empty = reactivate(&mut host, &store, thread, 62, 63);
    let a = commit_text(&mut host, &store, empty, 1, 0, 0, "a", 1, 1);
    let intent = history_intent(a, 2, MutationKind::Undo, position(1));
    host.begin_history_selection(&store, a, intent).unwrap();
    host.dispose_composer_service(&store).unwrap();
    assert_eq!(host.settlement_custody_in_use(), 0);
    assert!(matches!(
        host.execute_history_selection(&store, intent.key(), &CommandCancellation::new()),
        Err(ComposerHostError::HistoryNotPending)
    ));
    host = SyndicComposerHost::with_settlement_custody_capacity(storage, NonZeroUsize::MIN);
    let replacement = reactivate(&mut host, &store, thread, 64, 65);
    assert_eq!(host.binding(), Some(replacement));
    assert_eq!(host.settlement_custody_in_use(), 0);

    let pending = history_intent(replacement, 4, MutationKind::Undo, position(0));
    host.begin_history_selection(&store, replacement, pending)
        .unwrap();
    host.dispose_composer_service(&store).unwrap();
    assert_eq!(host.binding(), None);
    assert!(matches!(
        host.execute_history_selection(&store, pending.key(), &CommandCancellation::new()),
        Err(ComposerHostError::HistoryNotPending)
    ));
    assert_eq!(host.binding(), None);
    assert_eq!(host.settlement_custody_in_use(), 0);
}

#[test]
fn committed_postread_failure_rebinds_once_and_retains_fail_closed_custody() {
    let (_home, store, storage, thread) = fixture("phase166_postread", 71);
    let (mut host, empty) = activated(storage, &store, thread, 72, 73);
    let a = commit_text(&mut host, &store, empty, 1, 0, 0, "a", 1, 1);
    let ab = commit_text(&mut host, &store, a, 2, 1, 1, "b", 2, 1);
    let intent = history_intent(ab, 3, MutationKind::Undo, position(2));
    host.begin_history_selection(&store, ab, intent).unwrap();
    host.test_arm_history_after_commit_fault(move |store, storage| {
        let mut command = HomeCommand::new(store.home_revision().unwrap());
        command
            .add(
                syndic_storage::test_faults::delete_draft_piece_immutable_record(
                    store,
                    storage,
                    a.root(),
                    DraftPieceImmutableDeletion::Root,
                ),
            )
            .unwrap();
        assert!(matches!(
            store.execute(command),
            beryl_home_store::CommandOutcome::Committed { .. }
        ));
    });
    assert!(matches!(
        host.execute_history_selection(&store, intent.key(), &CommandCancellation::new()),
        Err(ComposerHostError::HistoryUnavailable)
    ));
    let successor = host.binding().expect("committed successor remains bound");
    assert_ne!(successor, ab);
    assert_eq!(successor.root(), a.root());
    assert!(host.is_unavailable());
    assert_eq!(
        host.history_status(),
        Some(ComposerHostHistoryStatus::Unavailable)
    );
    assert_eq!(host.settlement_custody_in_use(), 1);
    assert_eq!(host.retained_history_intent().unwrap().intent(), intent);
    assert!(matches!(
        begin_text(&mut host, &store, successor, 4, 0),
        Err(ComposerHostError::HistoryPending | ComposerHostError::HistoryUnavailable)
    ));
}

#[test]
fn historical_before_all_and_after_all_positions_hydrate_one_exact_neighbor() {
    let (_home, store, storage, thread) = fixture("phase166_before_all", 81);
    let (mut host, empty) = activated(storage, &store, thread, 82, 83);
    let (with_marker, before, after) = insert_marker(&mut host, &store, empty, 1, false);
    let without_marker = remove_marker(&mut host, &store, with_marker, 2, before, after, before);
    let page = storage
        .draft_piece_marker_demand(
            &store,
            with_marker.root(),
            DraftPieceMarkerDemandV1::new(
                DraftPieceMarkerScopeV1::ExactAnchor(0),
                DraftPieceMarkerDirectionV1::Forward,
                None,
                1,
                4_096,
            ),
        )
        .unwrap();
    assert_eq!(page.markers().len(), 1, "{page:?}");
    let intent = history_intent(without_marker, 3, MutationKind::Undo, position(0));
    host.begin_history_selection(&store, without_marker, intent)
        .unwrap();
    let RangeHistoryOutcome::Committed(commit) = host
        .execute_history_selection(&store, intent.key(), &CommandCancellation::new())
        .unwrap()
    else {
        panic!("before-all history did not commit")
    };
    assert_eq!(commit.caret(), before);
    assert_eq!(commit.selection().anchor, before);
    assert_eq!(commit.selection().head, before);

    let (_home, store, storage, thread) = fixture("phase166_after_all", 84);
    let (mut host, empty) = activated(storage, &store, thread, 85, 86);
    let (with_marker, before, after) = insert_marker(&mut host, &store, empty, 1, true);
    let without_marker = remove_marker(&mut host, &store, with_marker, 2, before, after, after);
    let intent = history_intent(without_marker, 3, MutationKind::Undo, position(0));
    host.begin_history_selection(&store, without_marker, intent)
        .unwrap();
    let RangeHistoryOutcome::Committed(commit) = host
        .execute_history_selection(&store, intent.key(), &CommandCancellation::new())
        .unwrap()
    else {
        panic!("after-all history did not commit")
    };
    assert_eq!(commit.caret(), after);
    assert_eq!(commit.selection().anchor, after);
    assert_eq!(commit.selection().head, after);
}

#[test]
fn post_admission_cancellation_cannot_override_the_committed_settlement() {
    let (_home, store, storage, thread) = fixture("phase166_late_cancel", 91);
    let (mut host, empty) = activated(storage, &store, thread, 92, 93);
    let a = commit_text(&mut host, &store, empty, 1, 0, 0, "a", 1, 1);
    let ab = commit_text(&mut host, &store, a, 2, 1, 1, "b", 2, 1);
    let intent = history_intent(ab, 3, MutationKind::Undo, position(2));
    host.begin_history_selection(&store, ab, intent).unwrap();
    let cancellation = CommandCancellation::new();
    let late = cancellation.clone();
    host.test_arm_history_after_commit_fault(move |_, _| late.cancel());
    assert!(matches!(
        host.execute_history_selection(&store, intent.key(), &cancellation)
            .unwrap(),
        RangeHistoryOutcome::Committed(_)
    ));
    assert!(cancellation.is_cancelled());
    assert_eq!(
        candidate_text(storage, &store, host.binding().unwrap()),
        b"a"
    );
}

#[test]
fn occupied_disagreeing_operation_is_terminal_unavailable_not_widget_error() {
    let (_home, store, storage, thread) = fixture("phase166_collision", 101);
    let (mut host, empty) = activated(storage, &store, thread, 102, 103);
    let a = commit_text(&mut host, &store, empty, 1, 0, 0, "a", 1, 1);
    let DraftHistoricalRootSelectionV1::Prepared(stale) = storage
        .prepare_draft_historical_root_selection(
            &store,
            DraftHistoricalRootSelectionIntentV1::new(
                a.candidate(),
                operation_id(3),
                syndic_storage::DraftHistoricalRootDirectionV1::Undo,
            ),
        )
        .unwrap()
    else {
        panic!("stale collision fixture was unavailable")
    };
    let ab = commit_text(&mut host, &store, a, 2, 1, 1, "b", 2, 1);
    let intent = history_intent(ab, 3, MutationKind::Undo, position(2));
    host.begin_history_selection(&store, ab, intent).unwrap();
    host.test_arm_history_before_execute_fault(move |store, storage| {
        let mut command = HomeCommand::new(store.home_revision().unwrap());
        command
            .add(storage.adopt_draft_historical_root(storage.revision(store).unwrap(), stale))
            .unwrap();
        assert!(matches!(
            store.execute(command),
            beryl_home_store::CommandOutcome::Committed { .. }
        ));
    });
    assert!(matches!(
        host.execute_history_selection(&store, intent.key(), &CommandCancellation::new()),
        Err(ComposerHostError::HistoryUnavailable)
    ));
    assert_eq!(host.binding(), Some(ab));
    assert!(host.is_unavailable());
    assert_eq!(
        host.history_status(),
        Some(ComposerHostHistoryStatus::Unavailable)
    );
    assert_eq!(host.settlement_custody_in_use(), 1);
}

#[test]
fn missing_transition_and_frontier_fail_without_mutating_the_live_binding() {
    let (_home, store, storage, thread) = fixture("phase166_missing_transition", 111);
    let (mut host, empty) = activated(storage, &store, thread, 112, 113);
    let a = commit_text(&mut host, &store, empty, 1, 0, 0, "a", 1, 1);
    let intent = history_intent(a, 2, MutationKind::Undo, position(1));
    let DraftHistoricalRootSelectionV1::Prepared(prepared) = storage
        .prepare_draft_historical_root_selection(
            &store,
            DraftHistoricalRootSelectionIntentV1::new(
                a.candidate(),
                operation_id(2),
                syndic_storage::DraftHistoricalRootDirectionV1::Undo,
            ),
        )
        .unwrap()
    else {
        panic!("transition fixture was unavailable")
    };
    let mut command = HomeCommand::new(store.home_revision().unwrap());
    command
        .add(
            syndic_storage::test_faults::delete_draft_edit_history_record(
                &store,
                storage,
                syndic_storage::test_faults::DraftEditHistoryRecordDeletion::Transition(
                    prepared.request().selected_transition().key(),
                ),
            ),
        )
        .unwrap();
    assert!(matches!(
        store.execute(command),
        beryl_home_store::CommandOutcome::Committed { .. }
    ));
    host.begin_history_selection(&store, a, intent).unwrap();
    assert_eq!(
        host.execute_history_selection(&store, intent.key(), &CommandCancellation::new())
            .unwrap(),
        RangeHistoryOutcome::Error
    );
    assert_eq!(host.binding(), Some(a));

    let (_home, store, storage, thread) = fixture("phase166_missing_frontier", 114);
    let (mut host, empty) = activated(storage, &store, thread, 115, 116);
    let a = commit_text(&mut host, &store, empty, 1, 0, 0, "a", 1, 1);
    let intent = history_intent(a, 2, MutationKind::Undo, position(1));
    let mut command = HomeCommand::new(store.home_revision().unwrap());
    command
        .add(
            syndic_storage::test_faults::delete_draft_edit_history_frontier(
                &store,
                storage,
                a.history().key(),
            ),
        )
        .unwrap();
    assert!(matches!(
        store.execute(command),
        beryl_home_store::CommandOutcome::Committed { .. }
    ));
    host.begin_history_selection(&store, a, intent).unwrap();
    assert_eq!(
        host.execute_history_selection(&store, intent.key(), &CommandCancellation::new())
            .unwrap(),
        RangeHistoryOutcome::Error
    );
    assert_eq!(host.binding(), Some(a));
}

#[test]
fn bounded_retention_evicts_old_undo_transitions_without_unbounded_host_custody() {
    let (_home, store, storage, thread) =
        fixture_with_history_budget("phase166_retention", 121, 4_096);
    let (mut host, mut binding) = activated(storage, &store, thread, 122, 123);
    const EDITS: u64 = 32;
    for operation in 1..=EDITS {
        let offset = binding.logical_extent().logical_utf8_bytes();
        binding = commit_text(
            &mut host,
            &store,
            binding,
            operation,
            offset,
            offset,
            "x",
            offset + 1,
            1,
        );
        assert_eq!(host.settlement_custody_in_use(), 0);
    }
    let mut undone = 0_u64;
    while undone < EDITS {
        let offset = binding.logical_extent().logical_utf8_bytes();
        let intent = history_intent(binding, 100 + undone, MutationKind::Undo, position(offset));
        host.begin_history_selection(&store, binding, intent)
            .unwrap();
        match host
            .execute_history_selection(&store, intent.key(), &CommandCancellation::new())
            .unwrap()
        {
            RangeHistoryOutcome::Committed(commit) => {
                binding = host.binding().unwrap();
                assert_eq!(commit.binding(), binding.range_binding());
                undone += 1;
            }
            RangeHistoryOutcome::Rejected => break,
            other => panic!("retained undo settled unexpectedly: {other:?}"),
        }
        assert_eq!(host.settlement_custody_in_use(), 0);
    }
    assert!(undone > 0);
    assert!(
        undone < EDITS,
        "the 4 KiB history retained every transition"
    );
}
