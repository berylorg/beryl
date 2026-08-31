#![cfg(feature = "test-faults")]

#[path = "phase166_syndic_composer_history/support.rs"]
mod composer;
#[path = "phase141_syndic_composer_host/support.rs"]
mod composer_base;
#[path = "phase177_main_window_composer_slot/support.rs"]
mod support;

use beryl_app::composer_host::{
    ComposerHostFlushAdmission, ComposerHostFlushCapture, ComposerHostFlushState,
};
use beryl_app::main_window::{
    MainWindowComposerActivationAdvance, MainWindowComposerDisposalAdvance,
    MainWindowComposerMarkerMetadataAuthority, MainWindowComposerPublishAdvance,
    MainWindowComposerRetirementAdvance, MainWindowComposerSlot, MainWindowComposerSlotError,
    MainWindowComposerWidgetRelease,
};
use beryl_home_store::{CommandCancellation, test_faults::FaultPoint};
use syndic_storage::{
    DraftEditorCandidateSessionLifecycleV1, DraftEditorCandidateSessionReadOutcomeV1,
    DraftPieceReplacementV1, DraftPieceV1, SyndicTimestamp,
};

use support::{Fixture, activation, operation_id};

#[test]
fn prior_remains_authoritative_until_exact_target_publication() {
    let fixture = Fixture::new("publish", 11);
    let (selected_claim, target_claim) = fixture.claims();
    let mut selected = fixture.activated_host(fixture.selected_thread, 21, 22, 1);
    let clean = selected.binding().unwrap();
    let _ = composer::commit_text(&mut selected, &fixture.store, clean, 20, 0, 0, "a", 1, 1);
    let selected_binding = selected.binding().unwrap();
    let mut slot = MainWindowComposerSlot::new(
        fixture.window_id,
        selected_claim,
        selected,
        fixture.storage.clone(),
        MainWindowComposerMarkerMetadataAuthority::new(fixture.assets()),
    )
    .unwrap();

    let ready = slot
        .begin_activation(
            &fixture.store,
            target_claim,
            activation(fixture.target_thread, 23, 24, 2),
            operation_id(25),
            &CommandCancellation::new(),
        )
        .unwrap();
    let MainWindowComposerActivationAdvance::Ready(receipt) = ready else {
        panic!("target did not become ready: {ready:?}")
    };
    assert_eq!(
        slot.selected_identity().unwrap().binding(),
        selected_binding
    );
    assert_eq!(slot.pending_receipt(), Some(receipt));

    let flush = match slot.begin_publish(&fixture.store, receipt).unwrap() {
        ComposerHostFlushAdmission::Started { ticket, state } => {
            assert_eq!(state, ComposerHostFlushState::CaptureRequired);
            ticket
        }
        other => panic!("unexpected prior flush admission: {other:?}"),
    };
    assert_eq!(
        slot.selected_identity().unwrap().binding(),
        selected_binding
    );
    prepare_disposal(&fixture, &mut slot, receipt, flush, 26);
    assert!(matches!(
        slot.test_selected_host_mut()
            .unwrap()
            .capture_flush_disposal(
                &fixture.store,
                flush,
                operation_id(27),
                &CommandCancellation::new(),
            )
            .unwrap(),
        ComposerHostFlushCapture::State(ComposerHostFlushState::DisposalRequired)
    ));
    let release_required = slot.advance_publish(&fixture.store, receipt).unwrap();
    assert_eq!(
        release_required,
        MainWindowComposerPublishAdvance::WidgetReleaseRequired(slot.selected_identity().unwrap())
    );
    assert_eq!(
        slot.selected_identity().unwrap().binding(),
        selected_binding
    );
    let expected = slot.selected_identity().unwrap();
    slot.begin_final_publish(&fixture.store, receipt, expected)
        .unwrap();
    let release = MainWindowComposerWidgetRelease::for_test(expected);
    let published = slot
        .complete_publish_after_widget_release(&fixture.store, receipt, &release)
        .unwrap();
    let MainWindowComposerPublishAdvance::Published(identity) = published else {
        panic!("target was not published: {published:?}")
    };
    assert_eq!(identity.claim(), target_claim);
    assert_eq!(identity.binding().presentation_generation().get(), 2);
    assert!(slot.pending_receipt().is_none());
    assert!(matches!(
        slot.advance_publish(&fixture.store, receipt),
        Err(MainWindowComposerSlotError::StaleActivationReceipt)
    ));
    assert!(matches!(
        slot.retire_pending(&fixture.store, receipt),
        Err(MainWindowComposerSlotError::StaleActivationReceipt)
    ));
}

#[test]
fn retirement_abandons_only_the_exact_pristine_pending_target() {
    let fixture = Fixture::new("retire", 41);
    let (selected_claim, target_claim) = fixture.claims();
    let selected = fixture.activated_host(fixture.selected_thread, 42, 43, 1);
    let mut slot = MainWindowComposerSlot::new(
        fixture.window_id,
        selected_claim,
        selected,
        fixture.storage.clone(),
        MainWindowComposerMarkerMetadataAuthority::new(fixture.assets()),
    )
    .unwrap();
    let MainWindowComposerActivationAdvance::Ready(receipt) = slot
        .begin_activation(
            &fixture.store,
            target_claim,
            activation(fixture.target_thread, 44, 45, 2),
            operation_id(46),
            &CommandCancellation::new(),
        )
        .unwrap()
    else {
        panic!("target did not become ready")
    };
    let candidate = slot.pending_receipt().unwrap().session_id();
    assert!(matches!(
        slot.begin_activation(
            &fixture.store,
            target_claim,
            activation(fixture.target_thread, 47, 48, 3),
            operation_id(49),
            &CommandCancellation::new(),
        ),
        Err(MainWindowComposerSlotError::ActivationPending)
    ));
    assert_eq!(
        slot.retire_pending(&fixture.store, receipt).unwrap(),
        MainWindowComposerRetirementAdvance::Retired
    );
    assert!(slot.pending_receipt().is_none());
    assert_eq!(slot.selected_identity().unwrap().claim(), selected_claim);
    let draft = fixture.current_draft(fixture.target_thread).draft().id();
    let DraftEditorCandidateSessionReadOutcomeV1::Disposed(head) = fixture
        .storage
        .draft_editor_candidate_session(&fixture.store, draft, candidate)
        .unwrap()
    else {
        panic!("retired target was not disposed")
    };
    assert_eq!(
        head.lifecycle(),
        DraftEditorCandidateSessionLifecycleV1::Disposed
    );
    assert!(matches!(
        slot.retire_pending(&fixture.store, receipt),
        Err(MainWindowComposerSlotError::StaleActivationReceipt)
    ));
    let MainWindowComposerActivationAdvance::Ready(successor) = slot
        .begin_activation(
            &fixture.store,
            target_claim,
            activation(fixture.target_thread, 50, 51, 3),
            operation_id(52),
            &CommandCancellation::new(),
        )
        .unwrap()
    else {
        panic!("successor target did not become ready")
    };
    assert!(matches!(
        slot.retire_pending(&fixture.store, receipt),
        Err(MainWindowComposerSlotError::StaleActivationReceipt)
    ));
    assert_eq!(slot.pending_receipt(), Some(successor));
    assert_eq!(
        slot.retire_pending(&fixture.store, successor).unwrap(),
        MainWindowComposerRetirementAdvance::Retired
    );
}

#[test]
fn post_open_cancellation_abandons_the_exact_target_before_returning() {
    let fixture = Fixture::new("post-open-cancel", 81);
    let (selected_claim, target_claim) = fixture.claims();
    let selected = fixture.activated_host(fixture.selected_thread, 82, 83, 1);
    let mut slot = MainWindowComposerSlot::new(
        fixture.window_id,
        selected_claim,
        selected,
        fixture.storage.clone(),
        MainWindowComposerMarkerMetadataAuthority::new(fixture.assets()),
    )
    .unwrap();
    let cancellation = CommandCancellation::new();
    let cut = cancellation.clone();
    slot.test_arm_activation_after_open_fault(move |_, _| cut.cancel());
    assert!(matches!(
        slot.begin_activation(
            &fixture.store,
            target_claim,
            activation(fixture.target_thread, 84, 85, 2),
            operation_id(86),
            &cancellation,
        )
        .unwrap(),
        MainWindowComposerActivationAdvance::Cancelled
    ));
    assert!(slot.pending_receipt().is_none());
    let draft = fixture.current_draft(fixture.target_thread).draft().id();
    assert!(matches!(
        fixture
            .storage
            .draft_editor_candidate_session(
                &fixture.store,
                draft,
                syndic_storage::DraftEditorCandidateSessionIdV1::from_bytes([84; 16]),
            )
            .unwrap(),
        DraftEditorCandidateSessionReadOutcomeV1::Disposed(_)
    ));
}

#[test]
fn indeterminate_abandonment_retains_one_prepared_custody_then_reconciles() {
    let fixture = Fixture::new("retained-retry", 91);
    let (selected_claim, target_claim) = fixture.claims();
    let selected = fixture.activated_host(fixture.selected_thread, 92, 93, 1);
    let mut slot = MainWindowComposerSlot::new(
        fixture.window_id,
        selected_claim,
        selected,
        fixture.storage.clone(),
        MainWindowComposerMarkerMetadataAuthority::new(fixture.assets()),
    )
    .unwrap();
    let MainWindowComposerActivationAdvance::Ready(receipt) = slot
        .begin_activation(
            &fixture.store,
            target_claim,
            activation(fixture.target_thread, 94, 95, 2),
            operation_id(96),
            &CommandCancellation::new(),
        )
        .unwrap()
    else {
        panic!("target did not become ready")
    };
    let faults = fixture.faults.clone();
    slot.test_arm_abandonment_before_execute_fault(move |_, _| {
        faults.fail_next(FaultPoint::AfterCommitBeforePersist);
        faults.fail_next(FaultPoint::BeforeReadConfirmation);
    });
    assert_eq!(
        slot.retire_pending(&fixture.store, receipt).unwrap(),
        MainWindowComposerRetirementAdvance::Pending
    );
    assert_eq!(
        slot.pending_status(),
        Some(beryl_app::main_window::MainWindowComposerPendingStatus::ReconciliationPending)
    );
    let stale_storage = fixture.storage.clone();
    let (_directory, store, recovered_storage) = fixture.recover_same_home();
    assert!(matches!(
        slot.reconcile_pending_after_recovery(&store, stale_storage, receipt),
        Err(MainWindowComposerSlotError::RecoveryHandleMismatch)
    ));
    assert_eq!(
        slot.reconcile_pending_after_recovery(&store, recovered_storage, receipt)
            .unwrap(),
        MainWindowComposerRetirementAdvance::Retired
    );
    assert!(slot.pending_receipt().is_none());
}

#[test]
fn target_departing_fresh_state_after_prepare_fails_closed() {
    let fixture = Fixture::new("not-fresh", 101);
    let (selected_claim, target_claim) = fixture.claims();
    let selected = fixture.activated_host(fixture.selected_thread, 102, 103, 1);
    let mut slot = MainWindowComposerSlot::new(
        fixture.window_id,
        selected_claim,
        selected,
        fixture.storage.clone(),
        MainWindowComposerMarkerMetadataAuthority::new(fixture.assets()),
    )
    .unwrap();
    let MainWindowComposerActivationAdvance::Ready(receipt) = slot
        .begin_activation(
            &fixture.store,
            target_claim,
            activation(fixture.target_thread, 104, 105, 2),
            operation_id(106),
            &CommandCancellation::new(),
        )
        .unwrap()
    else {
        panic!("target did not become ready")
    };
    let binding = slot.test_pending_binding().unwrap();
    slot.test_arm_abandonment_before_execute_fault(move |store, storage| {
        let DraftEditorCandidateSessionReadOutcomeV1::Active(head) = storage
            .draft_editor_candidate_session(
                store,
                binding.candidate().draft_id(),
                binding.candidate().session_id(),
            )
            .unwrap()
        else {
            panic!("pending target was not active")
        };
        let transaction = composer_base::transaction_for_session(
            storage.clone(),
            store,
            head,
            107,
            vec![DraftPieceReplacementV1::new(
                composer_base::point(0),
                composer_base::point(0),
                vec![DraftPieceV1::Text("x".into())],
            )],
            composer_base::point(1),
        );
        composer_base::run_transaction(storage, store, &transaction, 1);
    });
    assert_eq!(
        slot.retire_pending(&fixture.store, receipt).unwrap(),
        MainWindowComposerRetirementAdvance::DepartedFreshBoundary
    );
    assert_eq!(slot.selected_identity().unwrap().claim(), selected_claim);
    assert_eq!(slot.pending_receipt(), Some(receipt));
    assert!(matches!(
        slot.begin_publish(&fixture.store, receipt),
        Err(MainWindowComposerSlotError::TargetNotReady)
    ));
}

#[test]
fn abandonment_winning_serial_order_rejects_late_publication() {
    let fixture = Fixture::new("abandon-first", 111);
    let (selected_claim, target_claim) = fixture.claims();
    let selected = fixture.activated_host(fixture.selected_thread, 112, 113, 1);
    let mut slot = MainWindowComposerSlot::new(
        fixture.window_id,
        selected_claim,
        selected,
        fixture.storage.clone(),
        MainWindowComposerMarkerMetadataAuthority::new(fixture.assets()),
    )
    .unwrap();
    let MainWindowComposerActivationAdvance::Ready(receipt) = slot
        .begin_activation(
            &fixture.store,
            target_claim,
            activation(fixture.target_thread, 114, 115, 2),
            operation_id(116),
            &CommandCancellation::new(),
        )
        .unwrap()
    else {
        panic!("target did not become ready")
    };
    assert_eq!(
        slot.retire_pending(&fixture.store, receipt).unwrap(),
        MainWindowComposerRetirementAdvance::Retired
    );
    assert!(matches!(
        slot.begin_publish(&fixture.store, receipt),
        Err(MainWindowComposerSlotError::StaleActivationReceipt)
    ));
    assert_eq!(slot.selected_identity().unwrap().claim(), selected_claim);
}

#[test]
fn slot_disposal_retires_pending_then_exactly_releases_selected() {
    let fixture = Fixture::new("dispose", 71);
    let (selected_claim, target_claim) = fixture.claims();
    let mut selected = fixture.activated_host(fixture.selected_thread, 72, 73, 1);
    let clean = selected.binding().unwrap();
    let _ = composer::commit_text(&mut selected, &fixture.store, clean, 70, 0, 0, "a", 1, 1);
    let mut slot = MainWindowComposerSlot::new(
        fixture.window_id,
        selected_claim,
        selected,
        fixture.storage.clone(),
        MainWindowComposerMarkerMetadataAuthority::new(fixture.assets()),
    )
    .unwrap();
    assert!(matches!(
        slot.begin_activation(
            &fixture.store,
            target_claim,
            activation(fixture.target_thread, 74, 75, 2),
            operation_id(76),
            &CommandCancellation::new(),
        )
        .unwrap(),
        MainWindowComposerActivationAdvance::Ready(_)
    ));
    let flush = match slot.begin_disposal(&fixture.store).unwrap() {
        ComposerHostFlushAdmission::Started { ticket, state } => {
            assert_eq!(state, ComposerHostFlushState::CaptureRequired);
            ticket
        }
        other => panic!("unexpected disposal admission: {other:?}"),
    };
    assert!(slot.pending_receipt().is_none());
    prepare_slot_disposal(&fixture, &mut slot, flush, 77);
    assert!(matches!(
        slot.test_selected_host_mut()
            .unwrap()
            .capture_flush_disposal(
                &fixture.store,
                flush,
                operation_id(78),
                &CommandCancellation::new(),
            )
            .unwrap(),
        ComposerHostFlushCapture::State(ComposerHostFlushState::DisposalRequired)
    ));
    let advance = slot.advance_disposal(&fixture.store).unwrap();
    let MainWindowComposerDisposalAdvance::WidgetReleaseRequired(selected_identity) = advance
    else {
        panic!("widget release was not required: {advance:?}")
    };
    assert_eq!(slot.selected_identity(), Some(selected_identity));
    let release = MainWindowComposerWidgetRelease::for_test(selected_identity);
    assert_eq!(
        slot.complete_disposal_after_widget_release(&fixture.store, &release)
            .unwrap(),
        MainWindowComposerDisposalAdvance::Disposed
    );
    assert!(slot.selected_identity().is_none());
    assert!(matches!(
        slot.begin_disposal(&fixture.store),
        Err(MainWindowComposerSlotError::Disposed)
    ));
}

fn prepare_disposal(
    fixture: &Fixture,
    slot: &mut MainWindowComposerSlot,
    receipt: beryl_app::main_window::MainWindowComposerActivationReceipt,
    flush: beryl_app::composer_host::ComposerHostFlushTicket,
    operation: u8,
) {
    let seals = fixture.marker_seals();
    match slot
        .test_selected_host_mut()
        .unwrap()
        .capture_flush_publication(
            &fixture.store,
            flush,
            fixture.assets(),
            &seals,
            operation_id(operation),
            None,
            SyndicTimestamp::from_unix_millis(u64::from(operation)),
            &CommandCancellation::new(),
        )
        .unwrap()
    {
        ComposerHostFlushCapture::Captured(_) => {}
        ComposerHostFlushCapture::State(ComposerHostFlushState::DisposalRequired) => return,
        other => panic!("prior publication was not captured: {other:?}"),
    }
    for _ in 0..16 {
        if matches!(
            slot.advance_publish(&fixture.store, receipt).unwrap(),
            MainWindowComposerPublishAdvance::Progress(ComposerHostFlushState::DisposalRequired)
        ) {
            return;
        }
    }
    panic!("prior publication did not reach disposal")
}

fn prepare_slot_disposal(
    fixture: &Fixture,
    slot: &mut MainWindowComposerSlot,
    flush: beryl_app::composer_host::ComposerHostFlushTicket,
    operation: u8,
) {
    let seals = fixture.marker_seals();
    match slot
        .test_selected_host_mut()
        .unwrap()
        .capture_flush_publication(
            &fixture.store,
            flush,
            fixture.assets(),
            &seals,
            operation_id(operation),
            None,
            SyndicTimestamp::from_unix_millis(u64::from(operation)),
            &CommandCancellation::new(),
        )
        .unwrap()
    {
        ComposerHostFlushCapture::Captured(_) => {}
        ComposerHostFlushCapture::State(ComposerHostFlushState::DisposalRequired) => return,
        other => panic!("selected publication was not captured: {other:?}"),
    }
    for _ in 0..16 {
        if matches!(
            slot.advance_disposal(&fixture.store).unwrap(),
            MainWindowComposerDisposalAdvance::Progress(ComposerHostFlushState::DisposalRequired)
        ) {
            return;
        }
    }
    panic!("selected publication did not reach disposal")
}
