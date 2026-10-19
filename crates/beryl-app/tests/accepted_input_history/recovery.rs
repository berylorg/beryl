use super::*;
use beryl_app::main_window::{
    MainWindowComposerMarkerMetadataAuthority, MainWindowComposerSlot,
    MainWindowConversationComposerService, MainWindowFailedResidentCandidateSource,
};
use gpui_text_input::MutationPositions;
use std::sync::Arc;
#[path = "support.rs"]
mod recall_support;

#[test]
fn committed_recall_survives_failed_home_and_fresh_service_keeps_the_window_history_owner() {
    recover_recalled_input(false, false);
}

#[test]
fn ready_unacknowledged_recall_keeps_the_original_gui_binding_eligible_for_recovery() {
    recover_recalled_input(true, false);
}

#[test]
fn indeterminate_recall_keeps_its_original_receipt_until_fresh_candidate_authentication() {
    recover_recalled_input(false, true);
}

fn recover_recalled_input(after_ready: bool, indeterminate: bool) {
    let fixture = slot_support::Fixture::new("ordinary-recall-recovery", 111);
    let claim = fixture.claims().0;
    let thread = fixture.selected_thread;
    let owner = Arc::new(ComposerAcceptedInputHistoryOwner::default());
    let assets = fixture.assets();
    let seals = fixture.marker_seals();
    let mut source = fixture.activated_host(thread, 201, 202, 1);
    source.bind_accepted_input_history(owner.clone());
    let empty = source.binding().unwrap();
    history_support::commit_text(
        &mut source,
        &fixture.store,
        empty,
        1,
        0,
        0,
        "history recalled",
        16,
        1,
    );
    recall_support::submit(&mut source, &fixture.store, &assets, &seals, 210);
    let reference = owner.entry(fixture.store.home_id(), thread, 0).unwrap();
    let mut host = fixture.activated_host(thread, 220, 221, 2);
    let empty = host.binding().unwrap();
    let prior =
        history_support::commit_text(&mut host, &fixture.store, empty, 1, 0, 0, "unsent", 6, 1);
    let ticket = host
        .begin_accepted_input_recall(
            &fixture.store,
            prior,
            reference.clone(),
            MutationPositions::collapsed(history_support::position(6)),
            history_support::operation_id(4000),
            true,
        )
        .unwrap();
    for _ in 0..512 {
        if host.test_accepted_input_recall_stage() == Some("adopting") {
            break;
        }
        assert!(matches!(
            host.advance_accepted_input_recall(
                &fixture.store,
                ticket,
                &assets,
                &CommandCancellation::new()
            )
            .unwrap(),
            ComposerHostRecallAdvance::Progress
        ));
    }
    assert_eq!(host.test_accepted_input_recall_stage(), Some("adopting"));
    if after_ready {
        assert!(matches!(
            host.advance_accepted_input_recall(
                &fixture.store,
                ticket,
                &assets,
                &CommandCancellation::new()
            )
            .unwrap(),
            ComposerHostRecallAdvance::Ready(_)
        ));
        assert!(host.historical_recovery_accepts_binding(prior));
        assert_ne!(host.binding(), Some(prior));
        fail(&fixture.store, &fixture.faults);
    } else if indeterminate {
        fixture
            .faults
            .fail_next(FaultPoint::AfterCommitBeforePersist);
        let faults = fixture.faults.clone();
        host.test_arm_recall_after_execute_fault(move |_, outcome| {
            assert!(matches!(
                outcome,
                beryl_home_store::CommandOutcome::Indeterminate { .. }
            ));
            faults.fail_next(FaultPoint::BeforeReconciliationSnapshot);
        });
        assert!(
            host.advance_accepted_input_recall(
                &fixture.store,
                ticket,
                &assets,
                &CommandCancellation::new()
            )
            .is_err()
        );
        assert_eq!(host.binding(), Some(prior));
        fail(&fixture.store, &fixture.faults);
    } else {
        let faults = fixture.faults.clone();
        host.test_arm_recall_after_execute_fault(move |store, outcome| {
            assert!(matches!(
                outcome,
                beryl_home_store::CommandOutcome::Committed { .. }
            ));
            fail(store, &faults);
        });
        assert!(
            host.advance_accepted_input_recall(
                &fixture.store,
                ticket,
                &assets,
                &CommandCancellation::new()
            )
            .is_err()
        );
        assert_eq!(host.binding(), Some(prior));
    }
    let slot = MainWindowComposerSlot::new(
        fixture.window_id,
        claim,
        host,
        fixture.storage.clone(),
        MainWindowComposerMarkerMetadataAuthority::new(assets.clone()),
    )
    .unwrap();
    let service = Arc::new(MainWindowConversationComposerService::new(
        fixture.store.service_reference(),
        slot,
    ));
    service.bind_accepted_input_history(owner.clone()).unwrap();
    assert!(Arc::ptr_eq(&service.accepted_input_history(), &owner));
    let mut retired = service
        .retire_failed_resident()
        .ok()
        .expect("original recall command custody retires");
    assert_eq!(retired.recall_prior_binding(), Some(prior));
    drop((assets, seals, source));
    let (_directory, store, old_storage) = fixture.into_store();
    drop(old_storage);
    let mut candidate = store.recover_same_home().unwrap();
    let storage = SyndicStorage::reacquire_candidate(&candidate).unwrap();
    let state = BerylState::reacquire_candidate(&candidate).unwrap();
    retired
        .observe_historical_recovery(&mut candidate, &storage)
        .unwrap();
    assert!(
        !retired
            .qualify_saved(&mut candidate, &storage, &state, None)
            .unwrap()
    );
    let restored = retired.recall_restoration().unwrap();
    assert_eq!(
        restored.candidate().logical_extent().logical_utf8_bytes(),
        16
    );
    assert_eq!(restored.positions().unwrap().0.utf8_offset(), 16);
    retired
        .publish_candidate(
            &mut candidate,
            &storage,
            &state,
            None,
            history_support::operation_id(4001),
            SyndicTimestamp::from_unix_millis(100),
            DraftEditorCandidatePublicationEvidenceV1::UnchangedEmpty,
            CommandCancellation::new(),
        )
        .unwrap();
    assert!(
        retired
            .qualify_saved(&mut candidate, &storage, &state, None)
            .unwrap()
    );
    if after_ready {
        let caret = history_support::position(6);
        let predecessor = gpui_text_input::RangeRestorationSeed {
            binding: prior.range_binding(),
            history: Some(prior.range_history_frontier()),
            caret,
            selection: gpui_text_input::RangeSourceSelection::caret(caret),
            scroll: gpui_text_input::RangeRestorationScrollAnchor {
                position: caret,
                intra_anchor: gpui::px(0.),
            },
        };
        let source = MainWindowFailedResidentCandidateSource::new(
            &mut candidate,
            retired,
            storage.clone(),
            &state,
            predecessor,
            None,
        )
        .unwrap_or_else(|(_, error)| panic!("recalled GUI source conversion failed: {error}"));
        assert_eq!(source.predecessor(), predecessor);
        assert_eq!(source.seed().caret, history_support::position(16));
        assert_eq!(source.seed().selection.anchor, source.seed().caret);
        assert_eq!(source.seed().selection.head, source.seed().caret);
        assert_eq!(
            source.seed().binding,
            source.selection().binding().range_binding()
        );
        assert_ne!(source.seed().binding, prior.range_binding());
        drop((source, state));
        let store = candidate.publish().unwrap();
        storage
            .authenticate_composer_accepted_input(&store, &reference)
            .unwrap();
        drop(storage);
        store.close().unwrap();
        assert_eq!(owner.entry_count(prior.home_id(), thread), 1);
        return;
    }
    let (slot, _window, retired) = retired
        .reconstruct(&mut candidate, storage.clone(), &state, None)
        .ok()
        .unwrap();
    let fresh = Arc::new(MainWindowConversationComposerService::new(
        candidate.service_reference(),
        *slot,
    ));
    fresh.bind_accepted_input_history(owner.clone()).unwrap();
    assert!(Arc::ptr_eq(&fresh.accepted_input_history(), &owner));
    assert_eq!(
        owner.entry(store_home_id(&fresh), thread, 0),
        Some(reference.clone())
    );
    drop((fresh, retired, state));
    let store = candidate.publish().unwrap();
    storage
        .authenticate_composer_accepted_input(&store, &reference)
        .unwrap();
    drop(storage);
    store.close().unwrap();
    assert_eq!(owner.entry_count(prior.home_id(), thread), 1);
}

fn store_home_id(service: &MainWindowConversationComposerService) -> beryl_model::BerylHomeId {
    service.selected_identity().unwrap().binding().home_id()
}
