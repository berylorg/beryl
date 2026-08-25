use std::num::NonZeroU64;

use beryl_app::composer_host::{
    ComposerHostActivationRequest, ComposerHostError, ComposerHostFlushAdvance,
    ComposerHostFlushCapture, ComposerHostFlushFailure, ComposerHostFlushPurpose,
    ComposerHostFlushState,
};
use beryl_home_store::{CommandCancellation, test_faults::FaultPoint};
use beryl_state::BerylState;
use gpui_text_input::MutationKind;
use syndic_storage::{DraftEditorCandidateSessionIdV1, SyndicTimestamp};

use super::{base, captured_flush, composer, publication, started_flush};

#[test]
fn activation_cannot_erase_an_admitted_publication_or_disposal_barrier() {
    let (_home, store, storage, thread) = base::fixture("phase173-barrier-fence", 111);
    let (mut host, empty) = composer::activated(storage, &store, thread, 112, 113);
    let _ = composer::commit_text(&mut host, &store, empty, 1, 0, 0, "a", 1, 1);
    let (flush, state) = started_flush(&mut host, ComposerHostFlushPurpose::ThreadSwitch);
    assert_eq!(state, ComposerHostFlushState::CaptureRequired);
    assert!(matches!(
        host.activate(
            &store,
            activation_request(thread, 114, 115),
            &CommandCancellation::new(),
        ),
        Err(ComposerHostError::LifecycleBlocked)
    ));
    assert_eq!(host.lifecycle_diagnostics().barriers(), 1);
    assert_eq!(host.flush_state(flush).unwrap(), state);
    host.dispose_composer_service(&store).unwrap();

    let (_home, mut store, storage, thread) = base::fixture("phase173-disposal-fence", 117);
    let assets = BerylState::register(&mut store).unwrap().assets();
    let seals = publication::service(&store, storage, assets, 1, 1);
    let (mut host, empty) = composer::activated(storage, &store, thread, 118, 119);
    let _ = composer::commit_text(&mut host, &store, empty, 1, 0, 0, "a", 1, 1);
    let (submission, _) = started_flush(&mut host, ComposerHostFlushPurpose::Submission);
    let _ = captured_flush(&mut host, &store, assets, &seals, submission, 2);
    assert_eq!(
        host.advance_flush(&store, submission).unwrap(),
        ComposerHostFlushAdvance::Satisfied(ComposerHostFlushPurpose::Submission)
    );
    let (flush, state) = started_flush(&mut host, ComposerHostFlushPurpose::Release);
    assert_eq!(state, ComposerHostFlushState::DisposalRequired);
    assert!(matches!(
        host.activate(
            &store,
            activation_request(thread, 120, 121),
            &CommandCancellation::new(),
        ),
        Err(ComposerHostError::LifecycleBlocked)
    ));
    assert_eq!(host.lifecycle_diagnostics().barriers(), 1);
    assert_eq!(host.flush_state(flush).unwrap(), state);
}

#[test]
fn activation_requires_an_empty_host_in_every_live_lifecycle_state() {
    let (_home, store, storage, thread) = base::fixture("phase173-activate-clean", 201);
    let (mut host, clean) = composer::activated(storage, &store, thread, 202, 203);
    assert_activation_blocked(&mut host, &store, thread, clean, 204, 205);

    let (_home, store, storage, thread) = base::fixture("phase173-activate-dirty", 206);
    let (mut host, clean) = composer::activated(storage, &store, thread, 207, 208);
    let dirty = composer::commit_text(&mut host, &store, clean, 1, 0, 0, "a", 1, 1);
    assert_activation_blocked(&mut host, &store, thread, dirty, 209, 210);

    let (_home, store, storage, thread) = base::fixture("phase173-activate-mutation", 211);
    let (mut host, clean) = composer::activated(storage, &store, thread, 212, 213);
    composer::begin_text(&mut host, &store, clean, 1, 0).unwrap();
    assert_activation_blocked(&mut host, &store, thread, clean, 214, 215);

    let (_home, store, storage, thread) = base::fixture("phase173-activate-history", 216);
    let (mut host, clean) = composer::activated(storage, &store, thread, 217, 218);
    let dirty = composer::commit_text(&mut host, &store, clean, 1, 0, 0, "a", 1, 1);
    let intent = composer::history_intent(
        dirty,
        2,
        MutationKind::Undo,
        composer::position(dirty.logical_extent().logical_utf8_bytes()),
    );
    host.begin_history_selection(&store, dirty, intent).unwrap();
    assert_activation_blocked(&mut host, &store, thread, dirty, 219, 220);

    let (_home, mut store, storage, thread) = base::fixture("phase173-activate-barriers", 221);
    let assets = BerylState::register(&mut store).unwrap().assets();
    let seals = publication::service(&store, storage, assets, 1, 1);
    let (mut host, clean) = composer::activated(storage, &store, thread, 222, 223);
    let dirty = composer::commit_text(&mut host, &store, clean, 1, 0, 0, "a", 1, 1);
    let (flush, state) = started_flush(&mut host, ComposerHostFlushPurpose::Submission);
    assert_eq!(state, ComposerHostFlushState::CaptureRequired);
    assert_activation_blocked(&mut host, &store, thread, dirty, 224, 225);
    let _ = captured_flush(&mut host, &store, assets, &seals, flush, 226);
    assert_eq!(
        host.flush_state(flush).unwrap(),
        ComposerHostFlushState::PublicationPending
    );
    assert_activation_blocked(&mut host, &store, thread, dirty, 227, 228);

    let (_home, mut store, storage, thread) = base::fixture("phase173-activate-disposal", 229);
    let assets = BerylState::register(&mut store).unwrap().assets();
    let seals = publication::service(&store, storage, assets, 1, 1);
    let (mut host, clean) = composer::activated(storage, &store, thread, 230, 231);
    let _ = composer::commit_text(&mut host, &store, clean, 1, 0, 0, "a", 1, 1);
    let (submission, _) = started_flush(&mut host, ComposerHostFlushPurpose::Submission);
    let _ = captured_flush(&mut host, &store, assets, &seals, submission, 232);
    assert_eq!(
        host.advance_flush(&store, submission).unwrap(),
        ComposerHostFlushAdvance::Satisfied(ComposerHostFlushPurpose::Submission)
    );
    let clean = host.binding().unwrap();
    let (flush, state) = started_flush(&mut host, ComposerHostFlushPurpose::Release);
    assert_eq!(state, ComposerHostFlushState::DisposalRequired);
    assert_activation_blocked(&mut host, &store, thread, clean, 233, 234);
    assert_eq!(
        host.flush_state(flush).unwrap(),
        ComposerHostFlushState::DisposalRequired
    );
}

#[test]
fn disposal_dirty_conflict_cuts_the_barrier_and_retains_exact_terminal_custody() {
    let (_home, mut store, storage, thread) = base::fixture("phase173-disposal-dirty", 121);
    let assets = BerylState::register(&mut store).unwrap().assets();
    let seals = publication::service(&store, storage, assets, 1, 1);
    let (mut host, empty) = composer::activated(storage, &store, thread, 122, 123);
    let _ = composer::commit_text(&mut host, &store, empty, 1, 0, 0, "a", 1, 1);
    let (submission, _) = started_flush(&mut host, ComposerHostFlushPurpose::Submission);
    let _ = captured_flush(&mut host, &store, assets, &seals, submission, 2);
    assert_eq!(
        host.advance_flush(&store, submission).unwrap(),
        ComposerHostFlushAdvance::Satisfied(ComposerHostFlushPurpose::Submission)
    );
    let binding = host.binding().unwrap();
    let session = storage
        .draft_editor_candidate_session(
            &store,
            binding.candidate().draft_id(),
            binding.candidate().session_id(),
        )
        .unwrap();
    let syndic_storage::DraftEditorCandidateSessionReadOutcomeV1::Active(session) = session else {
        panic!("candidate session was not active")
    };
    let (release, _) = started_flush(&mut host, ComposerHostFlushPurpose::Release);
    assert!(matches!(
        host.capture_flush_disposal(
            &store,
            release,
            composer::operation_id(124),
            &CommandCancellation::new(),
        )
        .unwrap(),
        ComposerHostFlushCapture::State(ComposerHostFlushState::DisposalRequired)
    ));
    host.test_arm_publication_before_execute_fault(move |store, storage| {
        composer::direct_adopt(
            store,
            storage,
            syndic_storage::DraftHistoricalRootSelectionIntentV1::new(
                syndic_storage::DraftEditorCandidateActivationBindingV1::from_head(&session),
                composer::operation_id(125),
                syndic_storage::DraftHistoricalRootDirectionV1::Undo,
            ),
        );
    });
    assert_eq!(
        host.advance_flush(&store, release).unwrap(),
        ComposerHostFlushAdvance::Unsatisfied(ComposerHostFlushFailure::DisposalDirtyConflict)
    );
    assert_eq!(host.lifecycle_diagnostics().barriers(), 0);
    assert_eq!(host.publication_custody_count(), 1);
    assert!(host.is_unavailable());
}

#[test]
fn foreign_disposal_execution_is_stale_and_preserves_the_joined_barrier() {
    let (_home, mut store, storage, thread) = base::fixture("phase173-disposal-generation", 131);
    let (_other_home, other_store, _, _) = base::fixture("phase173-disposal-foreign", 132);
    let assets = BerylState::register(&mut store).unwrap().assets();
    let seals = publication::service(&store, storage, assets, 1, 1);
    let (mut host, empty) = composer::activated(storage, &store, thread, 133, 134);
    let _ = composer::commit_text(&mut host, &store, empty, 1, 0, 0, "a", 1, 1);
    let (submission, _) = started_flush(&mut host, ComposerHostFlushPurpose::Submission);
    let _ = captured_flush(&mut host, &store, assets, &seals, submission, 2);
    assert_eq!(
        host.advance_flush(&store, submission).unwrap(),
        ComposerHostFlushAdvance::Satisfied(ComposerHostFlushPurpose::Submission)
    );
    let (release, _) = started_flush(&mut host, ComposerHostFlushPurpose::Release);
    assert!(matches!(
        host.capture_flush_disposal(
            &store,
            release,
            composer::operation_id(135),
            &CommandCancellation::new(),
        )
        .unwrap(),
        ComposerHostFlushCapture::State(ComposerHostFlushState::DisposalRequired)
    ));
    let diagnostics = host.lifecycle_diagnostics();
    let custody = host.publication_custody_count();
    assert_eq!(
        host.advance_flush(&other_store, release).unwrap(),
        ComposerHostFlushAdvance::Stale
    );
    assert_eq!(host.lifecycle_diagnostics(), diagnostics);
    assert_eq!(host.publication_custody_count(), custody);
    assert_eq!(host.lifecycle_diagnostics().barriers(), 1);
    assert_eq!(host.publication_custody_count(), 1);
    assert_eq!(
        host.advance_flush(&store, release).unwrap(),
        ComposerHostFlushAdvance::Satisfied(ComposerHostFlushPurpose::Release)
    );
    assert_eq!(host.publication_custody_count(), 0);
    assert!(host.binding().is_none());
}

#[test]
fn foreign_autosave_publication_callback_is_inert() {
    let (_home, mut store, storage, thread) = base::fixture("phase173-stale-autosave", 181);
    let (_other_home, other_store, _, _) = base::fixture("phase173-stale-autosave-other", 182);
    let assets = BerylState::register(&mut store).unwrap().assets();
    let seals = publication::service(&store, storage, assets, 1, 1);
    let (mut host, empty) = composer::activated(storage, &store, thread, 183, 184);
    let _ = composer::commit_text(&mut host, &store, empty, 1, 0, 0, "a", 1, 1);
    let timer = host.autosave_timer().unwrap();
    let publication = match host
        .fire_autosave(
            &store,
            timer,
            assets,
            &seals,
            composer::operation_id(185),
            None,
            SyndicTimestamp::from_unix_millis(185),
            &CommandCancellation::new(),
        )
        .unwrap()
    {
        beryl_app::composer_host::ComposerHostAutosaveCapture::Captured(ticket) => ticket,
        other => panic!("autosave was not captured: {other:?}"),
    };
    let diagnostics = host.lifecycle_diagnostics();
    let custody = host.publication_custody_count();
    assert_eq!(
        host.advance_autosave(&other_store, publication).unwrap(),
        beryl_app::composer_host::ComposerHostAutosaveAdvance::Stale
    );
    assert_eq!(host.lifecycle_diagnostics(), diagnostics);
    assert_eq!(host.publication_custody_count(), custody);
    assert!(matches!(
        host.advance_autosave(&store, publication).unwrap(),
        beryl_app::composer_host::ComposerHostAutosaveAdvance::Saved { .. }
    ));
}

#[test]
fn foreign_flush_publication_callback_is_inert() {
    let (_home, mut store, storage, thread) = base::fixture("phase173-stale-flush", 186);
    let (_other_home, other_store, _, _) = base::fixture("phase173-stale-flush-other", 187);
    let assets = BerylState::register(&mut store).unwrap().assets();
    let seals = publication::service(&store, storage, assets, 1, 1);
    let (mut host, empty) = composer::activated(storage, &store, thread, 188, 189);
    composer::commit_text(&mut host, &store, empty, 1, 0, 0, "a", 1, 1);
    let (old_flush, _) = started_flush(&mut host, ComposerHostFlushPurpose::Submission);
    let _ = captured_flush(&mut host, &store, assets, &seals, old_flush, 190);
    let diagnostics = host.lifecycle_diagnostics();
    let custody = host.publication_custody_count();
    assert_eq!(
        host.advance_flush(&other_store, old_flush).unwrap(),
        ComposerHostFlushAdvance::Stale
    );
    assert_eq!(host.lifecycle_diagnostics(), diagnostics);
    assert_eq!(host.publication_custody_count(), custody);
    assert_eq!(
        host.advance_flush(&store, old_flush).unwrap(),
        ComposerHostFlushAdvance::Satisfied(ComposerHostFlushPurpose::Submission)
    );
}

#[test]
fn stale_barrier_ticket_is_inert_while_current_barrier_remains_owned() {
    let (_home, mut store, storage, thread) = base::fixture("phase173-stale-barrier", 197);
    let assets = BerylState::register(&mut store).unwrap().assets();
    let seals = publication::service(&store, storage, assets, 1, 1);
    let (mut host, empty) = composer::activated(storage, &store, thread, 198, 199);
    let current_binding = composer::commit_text(&mut host, &store, empty, 1, 0, 0, "a", 1, 1);
    let (old_flush, _) = started_flush(&mut host, ComposerHostFlushPurpose::Submission);
    let cancellation = CommandCancellation::new();
    cancellation.cancel();
    assert_eq!(
        host.capture_flush_publication(
            &store,
            old_flush,
            assets,
            &seals,
            composer::operation_id(200),
            None,
            SyndicTimestamp::from_unix_millis(200),
            &cancellation,
        )
        .unwrap(),
        ComposerHostFlushCapture::Unsatisfied(ComposerHostFlushFailure::Cancelled)
    );
    let (current_flush, _) = started_flush(&mut host, ComposerHostFlushPurpose::Submission);
    let diagnostics = host.lifecycle_diagnostics();
    assert_eq!(
        host.advance_flush(&store, old_flush).unwrap(),
        ComposerHostFlushAdvance::Stale
    );
    assert_eq!(host.lifecycle_diagnostics(), diagnostics);
    assert_eq!(host.binding(), Some(current_binding));
    assert_eq!(
        host.flush_state(current_flush).unwrap(),
        ComposerHostFlushState::CaptureRequired
    );
}

#[test]
fn foreign_disposal_reconciliation_is_stale_and_preserves_exact_custody() {
    let (_home, mut store, storage, thread, faults) =
        base::fault_fixture("phase173-stale-disposal-reconcile", 191);
    let (_other_home, other_store, _, _) =
        base::fixture("phase173-stale-disposal-reconcile-other", 192);
    let assets = BerylState::register(&mut store).unwrap().assets();
    let seals = publication::service(&store, storage, assets, 1, 1);
    let (mut host, clean) = composer::activated(storage, &store, thread, 193, 194);
    let _ = composer::commit_text(&mut host, &store, clean, 1, 0, 0, "a", 1, 1);
    let (submission, _) = started_flush(&mut host, ComposerHostFlushPurpose::Submission);
    let _ = captured_flush(&mut host, &store, assets, &seals, submission, 195);
    assert_eq!(
        host.advance_flush(&store, submission).unwrap(),
        ComposerHostFlushAdvance::Satisfied(ComposerHostFlushPurpose::Submission)
    );
    let (release, _) = started_flush(&mut host, ComposerHostFlushPurpose::Release);
    assert!(matches!(
        host.capture_flush_disposal(
            &store,
            release,
            composer::operation_id(196),
            &CommandCancellation::new(),
        )
        .unwrap(),
        ComposerHostFlushCapture::State(ComposerHostFlushState::DisposalRequired)
    ));
    host.test_arm_publication_before_execute_fault(move |_, _| {
        faults.fail_next(FaultPoint::AfterCommitBeforePersist);
    });
    assert_eq!(
        host.advance_flush(&store, release).unwrap(),
        ComposerHostFlushAdvance::ReconciliationPending
    );
    let diagnostics = host.lifecycle_diagnostics();
    let custody = host.publication_custody_count();
    assert_eq!(
        host.advance_flush(&other_store, release).unwrap(),
        ComposerHostFlushAdvance::Stale
    );
    assert_eq!(host.lifecycle_diagnostics(), diagnostics);
    assert_eq!(host.publication_custody_count(), custody);
    assert_eq!(
        host.advance_flush(&store, release).unwrap(),
        ComposerHostFlushAdvance::Satisfied(ComposerHostFlushPurpose::Release)
    );
}

#[test]
fn recoverable_capture_failure_cuts_once_without_retaining_source_custody() {
    let (_home, mut store, storage, thread) = base::fixture("phase173-recoverable", 136);
    let assets = BerylState::register(&mut store).unwrap().assets();
    let seals = publication::service(&store, storage, assets, 1, 1);
    let (mut host, empty) = composer::activated(storage, &store, thread, 137, 138);
    let marker_assets = [
        publication::publish_image_asset(&store, assets, b"phase173-recoverable-a"),
        publication::publish_image_asset(&store, assets, b"phase173-recoverable-b"),
    ];
    let _ = publication::insert_two_markers(&mut host, &store, empty, 1, marker_assets);
    let (flush, _) = started_flush(&mut host, ComposerHostFlushPurpose::Submission);
    assert!(matches!(
        host.capture_flush_publication(
            &store,
            flush,
            assets,
            &seals,
            composer::operation_id(139),
            None,
            SyndicTimestamp::from_unix_millis(139),
            &CommandCancellation::new(),
        )
        .unwrap(),
        ComposerHostFlushCapture::Unsatisfied(ComposerHostFlushFailure::Recoverable)
    ));
    assert_eq!(host.lifecycle_diagnostics().barriers(), 0);
    assert_eq!(host.publication_custody_count(), 0);
    assert!(host.autosave_timer().is_some());
}

#[test]
fn undo_and_redo_during_one_flush_repeat_to_the_newest_frontier() {
    let (_home, mut store, storage, thread) = base::fixture("phase173-history-flush", 141);
    let assets = BerylState::register(&mut store).unwrap().assets();
    let seals = publication::service(&store, storage, assets, 1, 1);
    let (mut host, empty) = composer::activated(storage, &store, thread, 142, 143);
    let a = composer::commit_text(&mut host, &store, empty, 1, 0, 0, "a", 1, 1);
    let ab = composer::commit_text(&mut host, &store, a, 2, 1, 1, "b", 2, 1);
    let (flush, _) = started_flush(&mut host, ComposerHostFlushPurpose::Submission);
    let _ = captured_flush(&mut host, &store, assets, &seals, flush, 3);
    let undone = composer::select_history(&mut host, &store, ab, 4, MutationKind::Undo);
    assert_ne!(undone.root(), ab.root());
    assert_eq!(
        host.advance_flush(&store, flush).unwrap(),
        ComposerHostFlushAdvance::Progress(ComposerHostFlushState::CaptureRequired)
    );
    let current_undone = host.binding().unwrap();
    assert_eq!(current_undone.root(), undone.root());
    let redone = composer::select_history(&mut host, &store, current_undone, 5, MutationKind::Redo);
    assert_eq!(redone.root(), ab.root());
    let _ = captured_flush(&mut host, &store, assets, &seals, flush, 6);
    assert_eq!(
        host.advance_flush(&store, flush).unwrap(),
        ComposerHostFlushAdvance::Satisfied(ComposerHostFlushPurpose::Submission)
    );
    assert_eq!(host.binding().unwrap().root(), redone.root());
    assert!(!host.is_dirty());
    assert_eq!(host.lifecycle_diagnostics().barriers(), 0);
}

fn activation_request(
    thread: beryl_model::SyndicThreadId,
    session: u8,
    operation: u8,
) -> ComposerHostActivationRequest {
    ComposerHostActivationRequest::new(
        thread,
        DraftEditorCandidateSessionIdV1::from_bytes([session; 16]),
        composer::operation_id(u64::from(operation)),
        NonZeroU64::MIN,
        None,
        Box::new([]),
    )
}

fn assert_activation_blocked(
    host: &mut beryl_app::composer_host::SyndicComposerHost,
    store: &beryl_home_store::HomeStore,
    thread: beryl_model::SyndicThreadId,
    binding: beryl_app::composer_host::ComposerHostBinding,
    session: u8,
    operation: u8,
) {
    let diagnostics = host.lifecycle_diagnostics();
    assert!(matches!(
        host.activate(
            store,
            activation_request(thread, session, operation),
            &CommandCancellation::new(),
        ),
        Err(ComposerHostError::LifecycleBlocked)
    ));
    assert_eq!(host.binding(), Some(binding));
    assert_eq!(host.lifecycle_diagnostics(), diagnostics);
}
