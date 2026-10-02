use super::{support, widget_support};
use beryl_app::main_window::{
    MainWindowConversationComposerService, MainWindowFailedResidentCandidateSource,
};
use beryl_home_store::{CommandOutcome, HomeCommand, test_faults::FaultPoint};
use beryl_state::{BerylState, SessionWindowRemovalState};
use gpui::px;
use gpui_text_input::{RangeRestorationScrollAnchor, RangeRestorationSeed, RangeSourceSelection};
use std::sync::Arc;
use syndic_storage::SyndicStorage;

#[test]
fn removed_and_restored_window_reconstructs_resident_with_exact_renewed_claim() {
    let fixture = widget_support::fixture::Fixture::new("failed-renewed-window-claim", 181);
    let slot = support::slot_close::slot(&fixture);
    let selection = slot.selected_identity().unwrap();
    let state = BerylState::reacquire(&fixture.store).unwrap();
    let session = state.session();
    let evidence = session
        .capture_window_removal(&fixture.store, selection.window_id())
        .unwrap();
    let original_window = evidence.window().clone();
    let mut removal = HomeCommand::new(fixture.store.home_revision().unwrap());
    removal
        .add(
            session
                .remove_captured_window(
                    &fixture.store,
                    session.revision(&fixture.store).unwrap(),
                    &evidence,
                )
                .unwrap(),
        )
        .unwrap();
    let service = Arc::new(MainWindowConversationComposerService::new(
        fixture.store.service_reference(),
        *slot,
    ));
    fixture.faults.fail_next(FaultPoint::AfterPersist);
    let original_outcome = fixture.store.execute(removal);
    assert!(matches!(original_outcome, CommandOutcome::Committed { .. }));
    let retained = service.retire_failed_resident().ok().unwrap();
    let (directory, store, _) = fixture.into_store();
    let mut candidate = store.recover_same_home().unwrap();
    let state = BerylState::reacquire_candidate(&candidate).unwrap();
    let storage = SyndicStorage::reacquire_candidate(&candidate).unwrap();
    assert!(
        retained
            .qualify_member(&mut candidate, &state, None)
            .is_err()
    );
    assert!(
        retained
            .qualify_member(&mut candidate, &state, Some(&evidence))
            .is_err()
    );
    let access = candidate.recovery_access().unwrap();
    assert_eq!(
        state
            .session()
            .classify_window_removal_candidate(&access, &evidence)
            .unwrap(),
        SessionWindowRemovalState::Removed
    );
    let mut restoration = HomeCommand::new(access.home_revision().unwrap());
    restoration
        .add(
            state
                .session()
                .recover_removed_window_candidate(
                    &access,
                    state.session().revision_candidate(&access).unwrap(),
                    &evidence,
                )
                .unwrap(),
        )
        .unwrap();
    let restoration_outcome = access.execute(restoration);
    assert!(matches!(
        restoration_outcome,
        CommandOutcome::Committed { .. }
    ));
    assert!(
        retained
            .qualify_member(&mut candidate, &state, None)
            .is_err()
    );
    let restored = retained
        .qualify_member(&mut candidate, &state, Some(&evidence))
        .unwrap();
    let fresh_claim = restored.selected_thread().unwrap();
    assert_eq!(fresh_claim.thread_id(), selection.claim().thread_id());
    assert_eq!(fresh_claim.generation(), selection.claim().generation());
    assert_eq!(
        fresh_claim.revision().get(),
        selection.claim().revision().get() + 1
    );
    assert_eq!(
        restored.revision().get(),
        original_window.revision().get() + 1
    );
    assert_eq!(restored.placement(), original_window.placement());
    let caret = super::composer::position(0);
    let seed = RangeRestorationSeed {
        binding: selection.binding().range_binding(),
        history: Some(selection.binding().range_history_frontier()),
        caret,
        selection: RangeSourceSelection::caret(caret),
        scroll: RangeRestorationScrollAnchor {
            position: caret,
            intra_anchor: px(0.),
        },
    };
    let source = MainWindowFailedResidentCandidateSource::new(
        &mut candidate,
        retained,
        storage,
        &state,
        seed,
        Some(&evidence),
    )
    .ok()
    .unwrap();
    assert_eq!(source.selection().claim(), fresh_claim);
    assert_eq!(
        source.selection().binding().candidate().session_id(),
        selection.binding().candidate().session_id()
    );
    assert_eq!(source.window(), &restored);
    assert_eq!(source.predecessor_selection(), selection);
    assert!(matches!(original_outcome, CommandOutcome::Committed { .. }));
    assert!(matches!(
        restoration_outcome,
        CommandOutcome::Committed { .. }
    ));
    drop(source);
    candidate.publish().unwrap().close().unwrap();
    drop(directory);
}
