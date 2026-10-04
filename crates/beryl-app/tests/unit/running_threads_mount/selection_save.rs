use super::*;

#[path = "../../syndic_composer_history/support.rs"]
mod edit_support;

fn edited_selection_service(
    fixture: &support::Fixture,
    fault: bool,
) -> (
    Arc<MainWindowConversationComposerService>,
    MainWindowComposerActivationReceipt,
) {
    let (selected_claim, target_claim) = fixture.claims();
    let mut host = fixture.activated_host(fixture.selected_thread, 190, 191, 1);
    let binding = host.binding().unwrap();
    edit_support::commit_text(
        &mut host,
        &fixture.store,
        binding,
        1,
        0,
        0,
        "saved selection payload",
        23,
        1,
    );
    if fault {
        let faults = fixture.faults.clone();
        host.test_arm_publication_before_execute_fault(move |_, _| {
            faults.fail_next(beryl_home_store::test_faults::FaultPoint::AfterCommitBeforePersist)
        });
    }
    let slot = MainWindowComposerSlot::new(
        fixture.window_id,
        selected_claim,
        host,
        fixture.storage.clone(),
        MainWindowComposerMarkerMetadataAuthority::new(fixture.assets()),
    )
    .unwrap();
    let service = Arc::new(MainWindowConversationComposerService::new(
        fixture.store.service_reference(),
        slot,
    ));
    let MainWindowComposerActivationAdvance::Ready(receipt) = service
        .begin_activation(
            target_claim,
            support::activation(fixture.target_thread, 192, 193, 2),
            support::operation_id(194),
            &CommandCancellation::new(),
        )
        .unwrap()
    else {
        panic!("target not ready")
    };
    (service, receipt)
}

fn settle_selection_save(
    fixture: &support::Fixture,
    service: &Arc<MainWindowConversationComposerService>,
    receipt: MainWindowComposerActivationReceipt,
    cancellation: &CommandCancellation,
) -> (MainWindowComposerClaimAdvance, bool) {
    let mut admission = service.begin_claim_publication_save(receipt).unwrap();
    let mut pending = false;
    for _ in 0..32 {
        let ticket = match admission {
            ComposerHostFlushAdmission::Started { ticket, state }
            | ComposerHostFlushAdmission::Joined { ticket, state } => {
                if state == crate::composer_host::ComposerHostFlushState::CaptureRequired {
                    let captured = service
                        .capture_flush_publication(
                            service.selected_identity().unwrap(),
                            ticket,
                            fixture.assets(),
                            &fixture.marker_seals(),
                            support::operation_id(195),
                            None,
                            syndic_storage::SyndicTimestamp::from_unix_millis(999),
                            cancellation,
                        )
                        .unwrap();
                    if matches!(
                        captured,
                        crate::composer_host::ComposerHostFlushCapture::Unsatisfied(_)
                    ) {
                        assert_eq!(
                            service
                                .abort_claim_publication_before_release(
                                    receipt,
                                    service.selected_identity().unwrap()
                                )
                                .is_err(),
                            true
                        );
                        assert_eq!(
                            service.release_failed_pending(receipt).unwrap(),
                            crate::main_window::MainWindowComposerRetirementAdvance::Retired
                        );
                        panic!("save unexpectedly refused")
                    }
                }
                ticket
            }
            _ => panic!("selection save skipped checkpoint"),
        };
        let advance = service.advance_claim_publication_source(receipt).unwrap();
        match advance.advance {
            MainWindowComposerPublishAdvance::WidgetReleaseRequired(_) => {
                return (advance, pending);
            }
            MainWindowComposerPublishAdvance::Progress(state) => {
                admission = ComposerHostFlushAdmission::Joined { ticket, state }
            }
            MainWindowComposerPublishAdvance::ReconciliationPending => pending = true,
            _ => panic!("selection save failed"),
        }
    }
    panic!("selection save did not settle")
}

#[test]
fn saved_selection_proof_preserves_exact_active_candidate_and_history_on_abort() {
    let fixture = support::Fixture::new("selection-save-proof", 201);
    let (service, receipt) = edited_selection_service(&fixture, false);
    let (saved, pending) =
        settle_selection_save(&fixture, &service, receipt, &CommandCancellation::new());
    assert!(!pending);
    let binding = saved.selected.binding();
    let head = fixture
        .storage
        .draft_editor_candidate_session(
            &fixture.store,
            binding.candidate().draft_id(),
            binding.candidate().session_id(),
        )
        .unwrap();
    assert!(matches!(
        head,
        syndic_storage::DraftEditorCandidateSessionReadOutcomeV1::Active(_)
    ));
    assert!(binding.history().availability().undo_available());
    assert_eq!(
        binding.logical_extent().logical_utf8_bytes(),
        "saved selection payload".len() as u64
    );
    assert!(
        service
            .abort_claim_publication_before_release(receipt, saved.pending)
            .is_err()
    );
    assert_eq!(service.selected_identity(), Some(saved.selected));
    assert_eq!(
        service
            .abort_claim_publication_before_release(receipt, saved.selected)
            .unwrap(),
        crate::main_window::MainWindowComposerRetirementAdvance::Retired
    );
    assert_eq!(service.selected_identity(), Some(saved.selected));
    assert_eq!(
        fixture
            .storage
            .draft_editor_candidate_session(
                &fixture.store,
                binding.candidate().draft_id(),
                binding.candidate().session_id()
            )
            .unwrap(),
        head
    );
}

#[test]
fn indeterminate_selection_save_reconciles_before_sealed_checkpoint_abort() {
    let fixture = support::Fixture::new("selection-save-reconcile", 211);
    let (service, receipt) = edited_selection_service(&fixture, true);
    let (saved, pending) =
        settle_selection_save(&fixture, &service, receipt, &CommandCancellation::new());
    assert!(pending);
    assert_eq!(
        service
            .abort_claim_publication_before_release(receipt, saved.selected)
            .unwrap(),
        crate::main_window::MainWindowComposerRetirementAdvance::Retired
    );
    assert_eq!(service.selected_identity(), Some(saved.selected));
}

#[test]
fn refused_selection_save_retires_only_target_and_preserves_dirty_candidate() {
    let fixture = support::Fixture::new("selection-save-refused", 221);
    let (service, receipt) = edited_selection_service(&fixture, false);
    let prior = service.selected_identity().unwrap();
    let cancellation = CommandCancellation::new();
    cancellation.cancel();
    let ComposerHostFlushAdmission::Started { ticket, .. } =
        service.begin_claim_publication_save(receipt).unwrap()
    else {
        panic!("save not started")
    };
    assert!(matches!(
        service
            .capture_flush_publication(
                prior,
                ticket,
                fixture.assets(),
                &fixture.marker_seals(),
                support::operation_id(195),
                None,
                syndic_storage::SyndicTimestamp::from_unix_millis(999),
                &cancellation
            )
            .unwrap(),
        crate::composer_host::ComposerHostFlushCapture::Unsatisfied(_)
    ));
    assert_eq!(
        service.release_failed_pending(receipt).unwrap(),
        crate::main_window::MainWindowComposerRetirementAdvance::Retired
    );
    assert_eq!(service.selected_identity(), Some(prior));
    let head = fixture
        .storage
        .draft_editor_candidate_session(
            &fixture.store,
            prior.binding().candidate().draft_id(),
            prior.binding().candidate().session_id(),
        )
        .unwrap();
    let syndic_storage::DraftEditorCandidateSessionReadOutcomeV1::Active(head) = head else {
        panic!("prior candidate was disposed")
    };
    assert_eq!(
        syndic_storage::DraftEditorCandidateActivationBindingV1::from_head(&head),
        prior.binding().candidate()
    );
}
