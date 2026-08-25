use beryl_app::composer_host::{
    ComposerHostFlushAdvance, ComposerHostFlushFailure, ComposerHostFlushPurpose,
};
use beryl_home_store::{CommandCancellation, test_faults::FaultPoint};
use beryl_state::BerylState;
use syndic_storage::{
    DraftEditorCandidateActivationBindingV1, DraftHistoricalRootDirectionV1,
    DraftHistoricalRootSelectionIntentV1,
};

use super::{
    base, captured_flush, captured_flush_with_cancellation, composer, lifecycle_common as common,
    publication, started_flush,
};

#[test]
fn publication_not_committed_cuts_once_and_rearms_for_an_explicit_attempt() {
    let (_home, mut store, storage, thread) = base::fixture("phase173-not-committed", 51);
    let assets = BerylState::register(&mut store).unwrap().assets();
    let seals = publication::service(&store, storage, assets, 1, 1);
    let (mut host, empty) = composer::activated(storage, &store, thread, 52, 53);
    let _ = composer::commit_text(&mut host, &store, empty, 1, 0, 0, "a", 1, 1);
    let (flush, _) = started_flush(&mut host, ComposerHostFlushPurpose::Submission);
    let _ = captured_flush(&mut host, &store, assets, &seals, flush, 2);
    host.test_arm_publication_before_execute_fault(move |store, storage| {
        base::bump_home_revision(storage, store, 54);
    });

    assert_eq!(
        host.advance_flush(&store, flush).unwrap(),
        ComposerHostFlushAdvance::Unsatisfied(ComposerHostFlushFailure::NotCommitted)
    );
    assert_eq!(host.lifecycle_diagnostics().barriers(), 0);
    assert_eq!(host.publication_custody_count(), 0);
    assert!(host.autosave_timer().is_some());
    assert!(matches!(
        host.begin_flush(ComposerHostFlushPurpose::Submission)
            .unwrap(),
        beryl_app::composer_host::ComposerHostFlushAdmission::Started { .. }
    ));
}

#[test]
fn durable_base_conflict_is_an_exact_terminal_cut() {
    let (_home, mut store, storage, thread) = base::fixture("phase173-durable-conflict", 61);
    let assets = BerylState::register(&mut store).unwrap().assets();
    let seals = publication::service(&store, storage, assets, 1, 1);
    let selector = common::selector(storage, &store, thread);
    let competing = {
        let (mut competitor, empty) = composer::activated(storage, &store, thread, 64, 65);
        composer::commit_text(&mut competitor, &store, empty, 3, 0, 0, "z", 1, 1)
    };
    let (mut host, empty) = composer::activated(storage, &store, thread, 62, 63);
    let _ = composer::commit_text(&mut host, &store, empty, 1, 0, 0, "a", 1, 1);
    let (flush, _) = started_flush(&mut host, ComposerHostFlushPurpose::Submission);
    let _ = captured_flush(&mut host, &store, assets, &seals, flush, 2);
    host.test_arm_publication_before_execute_fault(move |store, storage| {
        common::publish_binding(store, storage, selector, competing, 66, 66);
    });

    assert_eq!(
        host.advance_flush(&store, flush).unwrap(),
        ComposerHostFlushAdvance::Unsatisfied(ComposerHostFlushFailure::DurableBaseConflict)
    );
    assert_eq!(host.lifecycle_diagnostics().barriers(), 0);
    assert_eq!(host.publication_custody_count(), 1);
    assert!(host.is_unavailable());
}

#[test]
fn session_disposal_is_an_exact_terminal_cut() {
    let (_home, mut store, storage, thread) = base::fixture("phase173-session-disposed", 71);
    let assets = BerylState::register(&mut store).unwrap().assets();
    let seals = publication::service(&store, storage, assets, 1, 1);
    let selector = common::selector(storage, &store, thread);
    let (mut host, empty) = composer::activated(storage, &store, thread, 72, 73);
    let dirty = composer::commit_text(&mut host, &store, empty, 1, 0, 0, "a", 1, 1);
    let (flush, _) = started_flush(&mut host, ComposerHostFlushPurpose::Submission);
    let _ = captured_flush(&mut host, &store, assets, &seals, flush, 2);
    host.test_arm_publication_before_execute_fault(move |store, storage| {
        common::publish_binding(store, storage, selector, dirty, 80, 80);
        common::dispose_binding(store, storage, dirty, 81);
    });

    assert_eq!(
        host.advance_flush(&store, flush).unwrap(),
        ComposerHostFlushAdvance::Unsatisfied(ComposerHostFlushFailure::SessionDisposed)
    );
    assert_eq!(host.lifecycle_diagnostics().barriers(), 0);
    assert_eq!(host.publication_custody_count(), 1);
    assert!(host.is_unavailable());
}

#[test]
fn identity_and_reconciliation_collisions_preserve_exact_failure_identity() {
    let (_home, mut store, storage, thread) = base::fixture("phase173-identity", 81);
    let assets = BerylState::register(&mut store).unwrap().assets();
    let seals = publication::service(&store, storage, assets, 1, 1);
    let selector = common::selector(storage, &store, thread);
    let (mut host, empty) = composer::activated(storage, &store, thread, 82, 83);
    let dirty = composer::commit_text(&mut host, &store, empty, 1, 0, 0, "a", 1, 1);
    let (flush, _) = started_flush(&mut host, ComposerHostFlushPurpose::Submission);
    let _ = captured_flush(&mut host, &store, assets, &seals, flush, 84);
    let competing = common::publication_request(selector, dirty, 84, 85);
    let source = common::capture_source(&store, storage, competing);
    host.test_arm_publication_before_execute_fault(move |store, storage| {
        let _ = common::publish_source(store, storage, source, competing);
    });
    assert_eq!(
        host.advance_flush(&store, flush).unwrap(),
        ComposerHostFlushAdvance::Unsatisfied(ComposerHostFlushFailure::IdentityCollision)
    );

    let (_home, mut store, storage, thread, faults) =
        base::fault_fixture("phase173-reconciliation-collision", 91);
    let assets = BerylState::register(&mut store).unwrap().assets();
    let seals = publication::service(&store, storage, assets, 1, 1);
    let (mut host, empty) = composer::activated(storage, &store, thread, 92, 93);
    let dirty = composer::commit_text(&mut host, &store, empty, 1, 0, 0, "a", 1, 1);
    let (flush, _) = started_flush(&mut host, ComposerHostFlushPurpose::Submission);
    let cancellation = CommandCancellation::new();
    let _ = captured_flush_with_cancellation(
        &mut host,
        &store,
        assets,
        &seals,
        flush,
        94,
        &cancellation,
    );
    host.test_arm_publication_before_execute_fault(move |_, _| {
        faults.fail_next(FaultPoint::AfterCommitBeforePersist);
    });
    assert_eq!(
        host.advance_flush(&store, flush).unwrap(),
        ComposerHostFlushAdvance::ReconciliationPending
    );
    cancellation.cancel();
    let session = storage
        .draft_editor_candidate_session(
            &store,
            dirty.candidate().draft_id(),
            dirty.candidate().session_id(),
        )
        .unwrap();
    let syndic_storage::DraftEditorCandidateSessionReadOutcomeV1::Active(session) = session else {
        panic!("candidate session was not active")
    };
    composer::direct_adopt(
        &store,
        storage,
        DraftHistoricalRootSelectionIntentV1::new(
            DraftEditorCandidateActivationBindingV1::from_head(&session),
            composer::operation_id(95),
            DraftHistoricalRootDirectionV1::Undo,
        ),
    );
    assert_eq!(
        host.advance_flush(&store, flush).unwrap(),
        ComposerHostFlushAdvance::Unsatisfied(ComposerHostFlushFailure::ReconciliationCollision)
    );
    assert_eq!(host.lifecycle_diagnostics().barriers(), 0);
    assert_eq!(host.publication_custody_count(), 1);
}

#[test]
fn ambiguous_publication_reconciliation_stays_pending_then_cuts_exactly() {
    let (_home, mut store, storage, thread, faults) =
        base::fault_fixture("phase173-reconciliation-exact", 101);
    let assets = BerylState::register(&mut store).unwrap().assets();
    let seals = publication::service(&store, storage, assets, 1, 1);
    let (mut host, empty) = composer::activated(storage, &store, thread, 102, 103);
    let _ = composer::commit_text(&mut host, &store, empty, 1, 0, 0, "a", 1, 1);
    let (flush, _) = started_flush(&mut host, ComposerHostFlushPurpose::Submission);
    let _ = captured_flush(&mut host, &store, assets, &seals, flush, 104);
    host.test_arm_publication_before_execute_fault(move |_, _| {
        faults.fail_next(FaultPoint::AfterCommitBeforePersist);
    });
    assert_eq!(
        host.advance_flush(&store, flush).unwrap(),
        ComposerHostFlushAdvance::ReconciliationPending
    );
    assert_eq!(host.lifecycle_diagnostics().barriers(), 1);
    assert_eq!(host.publication_custody_count(), 1);
    assert_eq!(
        host.advance_flush(&store, flush).unwrap(),
        ComposerHostFlushAdvance::Satisfied(ComposerHostFlushPurpose::Submission)
    );
    assert_eq!(host.lifecycle_diagnostics().barriers(), 0);
    assert_eq!(host.publication_custody_count(), 0);
    assert!(!host.is_dirty());
}
