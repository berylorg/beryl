use super::*;

#[test]
fn selection_save_yields_on_publication_convergence_race_before_exact_checkpoint() {
    selection_save_convergence_race(false);
}

#[test]
fn selection_save_yields_on_reconciliation_convergence_race_before_exact_checkpoint() {
    selection_save_convergence_race(true);
}

fn selection_save_convergence_race(reconcile: bool) {
    use crate::composer_host::{
        ComposerHostFlushCapture, ComposerHostFlushState,
        ComposerHostPublicationExecutionObservation,
    };
    use std::sync::atomic::Ordering;

    let fixture = support::Fixture::new("selection-save-convergence", 231);
    let (service, receipt) = edited_selection_service(&fixture, reconcile);
    let prior = service.selected_identity().unwrap();
    let target = service.pending_identity(receipt).unwrap();
    let observation = ComposerHostPublicationExecutionObservation::new();
    let raced = service.test_with_selected_host(|host| {
        host.test_observe_publication_execution(observation.clone())
            .unwrap();
        arm_publication_convergence_race(&fixture, host, 240)
    });
    let ComposerHostFlushAdmission::Started {
        ticket,
        state: ComposerHostFlushState::CaptureRequired,
    } = service.begin_claim_publication_save(receipt).unwrap()
    else {
        panic!("selection save did not start its original capture")
    };
    let cancellation = CommandCancellation::new();
    let capture = |selection| {
        service
            .capture_flush_publication(
                selection,
                ticket,
                fixture.assets(),
                &fixture.marker_seals(),
                support::operation_id(195),
                None,
                syndic_storage::SyndicTimestamp::from_unix_millis(999),
                &cancellation,
            )
            .unwrap()
    };
    assert!(matches!(
        capture(prior),
        ComposerHostFlushCapture::Captured(_)
    ));
    if reconcile {
        let advance = service.advance_claim_publication_source(receipt).unwrap();
        assert_eq!(
            advance.advance,
            MainWindowComposerPublishAdvance::ReconciliationPending
        );
        assert!(!raced.load(Ordering::SeqCst));
    }
    let advance = service.advance_claim_publication_source(receipt).unwrap();
    assert!(raced.load(Ordering::SeqCst));
    assert_eq!(
        advance.advance,
        MainWindowComposerPublishAdvance::Progress(ComposerHostFlushState::PublicationPending)
    );
    assert_eq!(advance.selected, prior);
    assert_eq!(advance.pending, target);
    assert_eq!(service.pending_receipt(), Some(receipt));
    assert_eq!(observation.snapshot().original_attempts, 1);
    service.test_with_selected_host(|host| {
        assert_eq!(host.publication_custody_count(), 1);
        assert!(!host.is_unavailable());
        assert_eq!(
            host.flush_state(ticket).unwrap(),
            ComposerHostFlushState::PublicationPending
        );
    });
    cancellation.cancel();

    let mut advance = advance;
    for _ in 0..8 {
        match advance.advance {
            MainWindowComposerPublishAdvance::WidgetReleaseRequired(_) => break,
            MainWindowComposerPublishAdvance::Progress(ComposerHostFlushState::CaptureRequired) => {
                assert!(matches!(
                    capture(advance.selected),
                    ComposerHostFlushCapture::State(ComposerHostFlushState::DisposalRequired)
                ));
            }
            MainWindowComposerPublishAdvance::Progress(
                ComposerHostFlushState::PublicationPending,
            ) => {}
            other => panic!("original selection save did not converge: {other:?}"),
        }
        advance = service.advance_claim_publication_source(receipt).unwrap();
    }
    assert_eq!(
        advance.advance,
        MainWindowComposerPublishAdvance::WidgetReleaseRequired(advance.selected)
    );
    assert_eq!(advance.pending, target);
    assert_eq!(service.pending_receipt(), Some(receipt));
    assert!(
        advance
            .selected
            .binding()
            .history()
            .availability()
            .undo_available()
    );
    let snapshot = observation.snapshot();
    assert_eq!(snapshot.original_attempts, 1);
    assert_eq!(snapshot.different_identity_attempts, 0);
    let original = snapshot.original.unwrap();
    assert_eq!(original.draft_id, prior.binding().candidate().draft_id());
    assert_eq!(
        original.session_id,
        prior.binding().candidate().session_id()
    );
    assert_eq!(original.operation_id, support::operation_id(195));
    service.test_with_selected_host(|host| {
        assert_eq!(host.publication_custody_count(), 0);
        assert!(!host.is_dirty());
        assert!(!host.is_unavailable());
        assert_eq!(
            host.flush_state(ticket).unwrap(),
            ComposerHostFlushState::DisposalRequired
        );
    });
    let binding = advance.selected.binding();
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
    assert_eq!(
        service
            .abort_claim_publication_before_release(receipt, advance.selected)
            .unwrap(),
        crate::main_window::MainWindowComposerRetirementAdvance::Retired
    );
    assert_eq!(service.selected_identity(), Some(advance.selected));
    assert_eq!(
        fixture
            .storage
            .draft_editor_candidate_session(
                &fixture.store,
                binding.candidate().draft_id(),
                binding.candidate().session_id(),
            )
            .unwrap(),
        head
    );
}

fn arm_publication_convergence_race(
    fixture: &support::Fixture,
    host: &mut crate::composer_host::SyndicComposerHost,
    seed: u8,
) -> Arc<std::sync::atomic::AtomicBool> {
    use std::sync::atomic::{AtomicBool, Ordering};

    let execution = fixture
        .storage
        .thread_execution(
            &fixture.store,
            fixture.selected_thread,
            syndic_storage::SyndicPointReadLimit::new(65_536).unwrap(),
        )
        .unwrap()
        .unwrap()
        .execution()
        .clone();
    let raced = Arc::new(AtomicBool::new(false));
    let race_observation = raced.clone();
    host.test_arm_publication_convergence_read_fault(move |store, storage| {
        let mut command = beryl_home_store::HomeCommand::new(store.home_revision().unwrap());
        command
            .add(storage.create_thread(
                storage.revision(store).unwrap(),
                syndic_storage::CreateThread::ordinary(
                    beryl_model::SyndicThreadId::from_bytes([seed; 16]),
                    beryl_model::SyndicDraftId::from_bytes([seed.wrapping_add(1); 16]),
                    execution,
                    syndic_storage::SyndicTimestamp::from_unix_millis(1000),
                    syndic_storage::DraftEditHistoryPolicyV1::new(65_536, 1).unwrap(),
                ),
            ))
            .unwrap();
        assert!(matches!(
            store.execute(command),
            beryl_home_store::CommandOutcome::Committed {
                later_failure: None,
                ..
            }
        ));
        race_observation.store(true, Ordering::SeqCst);
    });
    raced
}

#[test]
fn autosave_settlement_preserves_original_outcome_after_convergence_yield_and_cancellation() {
    use crate::composer_host::{
        ComposerHostAutosaveAdvance, ComposerHostAutosaveCapture,
        ComposerHostPublicationExecutionObservation,
    };
    use std::sync::atomic::Ordering;

    for reconcile in [false, true] {
        let fixture = support::Fixture::new("autosave-convergence", 231);
        let (service, _) = edited_selection_service(&fixture, reconcile);
        let observation = ComposerHostPublicationExecutionObservation::new();
        let cancellation = CommandCancellation::new();
        service.test_with_selected_host(|host| {
            host.test_observe_publication_execution(observation.clone())
                .unwrap();
            let raced = arm_publication_convergence_race(&fixture, host, 240);
            let ComposerHostAutosaveCapture::Captured(ticket) = host
                .fire_autosave(
                    &fixture.store,
                    host.autosave_timer().unwrap(),
                    fixture.assets(),
                    &fixture.marker_seals(),
                    support::operation_id(195),
                    None,
                    syndic_storage::SyndicTimestamp::from_unix_millis(999),
                    &cancellation,
                )
                .unwrap()
            else {
                panic!("autosave did not capture the original publication")
            };
            if reconcile {
                assert_eq!(
                    host.advance_autosave(&fixture.store, ticket).unwrap(),
                    ComposerHostAutosaveAdvance::ReconciliationPending
                );
            }
            assert_eq!(
                host.advance_autosave(&fixture.store, ticket).unwrap(),
                ComposerHostAutosaveAdvance::Progress
            );
            assert!(raced.load(Ordering::SeqCst));
            assert_eq!(host.publication_custody_count(), 1);
            cancellation.cancel();
            assert_eq!(
                host.advance_autosave(&fixture.store, ticket).unwrap(),
                ComposerHostAutosaveAdvance::Saved {
                    dirty_successor: false
                }
            );
            assert!(!host.is_dirty());
            assert!(host.autosave_timer().is_none());
            assert_eq!(host.publication_custody_count(), 0);
            assert_eq!(observation.snapshot().original_attempts, 1);
            assert_eq!(observation.snapshot().different_identity_attempts, 0);
        });
    }
}

#[test]
fn service_disposal_resumes_original_settlement_after_repeated_convergence_yields() {
    use crate::composer_host::{
        ComposerHostAutosaveAdvance, ComposerHostAutosaveCapture,
        ComposerHostPublicationExecutionObservation, ComposerHostServiceDisposalCompletion,
    };
    use std::sync::atomic::Ordering;

    for reconcile in [false, true] {
        let fixture = support::Fixture::new("service-disposal-convergence", 231);
        let (service, _) = edited_selection_service(&fixture, reconcile);
        let observation = ComposerHostPublicationExecutionObservation::new();
        let cancellation = CommandCancellation::new();
        service.test_with_selected_host(|host| {
            host.test_observe_publication_execution(observation.clone())
                .unwrap();
            let raced = arm_publication_convergence_race(&fixture, host, 240);
            let ComposerHostAutosaveCapture::Captured(ticket) = host
                .fire_autosave(
                    &fixture.store,
                    host.autosave_timer().unwrap(),
                    fixture.assets(),
                    &fixture.marker_seals(),
                    support::operation_id(195),
                    None,
                    syndic_storage::SyndicTimestamp::from_unix_millis(999),
                    &cancellation,
                )
                .unwrap()
            else {
                panic!("disposal did not retain an original autosave")
            };
            if reconcile {
                assert_eq!(
                    host.advance_autosave(&fixture.store, ticket).unwrap(),
                    ComposerHostAutosaveAdvance::ReconciliationPending
                );
                cancellation.cancel();
                assert_eq!(
                    host.dispose_composer_service(&fixture.store).unwrap(),
                    ComposerHostServiceDisposalCompletion::Pending
                );
            } else {
                assert_eq!(
                    host.advance_autosave(&fixture.store, ticket).unwrap(),
                    ComposerHostAutosaveAdvance::Progress
                );
            }
            assert!(raced.load(Ordering::SeqCst));
            let raced_again = arm_publication_convergence_race(&fixture, host, 242);
            cancellation.cancel();
            assert_eq!(
                host.dispose_composer_service(&fixture.store).unwrap(),
                ComposerHostServiceDisposalCompletion::Pending
            );
            assert!(raced_again.load(Ordering::SeqCst));
            assert_eq!(host.publication_custody_count(), 1);
            assert_eq!(observation.snapshot().original_attempts, 1);
            assert_eq!(
                host.dispose_composer_service(&fixture.store).unwrap(),
                ComposerHostServiceDisposalCompletion::Disposed
            );
            assert_eq!(host.publication_custody_count(), 0);
            assert!(host.binding().is_none());
            assert_eq!(observation.snapshot().original_attempts, 1);
            assert_eq!(observation.snapshot().different_identity_attempts, 0);
        });
    }
}
