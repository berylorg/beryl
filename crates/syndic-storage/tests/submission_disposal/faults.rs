use super::{
    acceptance::*,
    fixture::{Fixture, read_limit},
    publication_support::head,
    support::*,
};
use beryl_home_store::ReconciliationResolution;
use syndic_storage::{
    DraftEditorCandidateSessionRecordKeyV1, FirstAcceptanceStatus,
    test_faults::{DraftCandidatePublicationFault, inject_draft_candidate_publication_fault},
};

#[test]
fn acceptance_and_disposal_receipt_reconcile_together_at_atomic_command_cuts() {
    for queued in [false, true] {
        for (fault, committed_at_cut) in [
            (FaultPoint::BeforeCommit, false),
            (FaultPoint::AfterCommitBeforePersist, true),
            (FaultPoint::AfterPersist, true),
            (FaultPoint::BeforeVerification, true),
        ] {
            let fixture = Fixture::new("atomic-cut", 30, queued);
            let prior = current(&fixture.storage, &fixture.store, fixture.thread);
            fixture.faults.fail_next(fault);
            let outcome = fixture.accept();
            let fixture = fixture.recover_if_failed();
            let did_commit = match outcome {
                CommandOutcome::Committed { .. } => true,
                CommandOutcome::NotCommitted { .. } => false,
                CommandOutcome::Indeterminate { reconciliation, .. } => {
                    assert!(matches!(
                        fixture
                            .store
                            .reconcile(&reconciliation.install_and_handle())
                            .unwrap(),
                        ReconciliationResolution::ExactNew { .. }
                    ));
                    true
                }
            };
            assert_eq!(did_commit, committed_at_cut);
            if committed_at_cut {
                let terminal = assert_exact_acceptance(&fixture);
                assert_exact_receipt_replay(&fixture, &terminal);
            } else {
                assert_eq!(
                    fixture
                        .storage
                        .first_acceptance_status(&fixture.store, &fixture.acceptance, read_limit())
                        .unwrap(),
                    FirstAcceptanceStatus::ExactOld
                );
                assert_eq!(
                    current(&fixture.storage, &fixture.store, fixture.thread),
                    prior
                );
                assert_eq!(
                    head(&fixture.storage, &fixture.store, &fixture.source),
                    fixture.source
                );
                committed(execute(
                    &fixture.store,
                    fixture.storage.first_acceptance(
                        fixture.storage.revision(&fixture.store).unwrap(),
                        fixture.acceptance.clone(),
                    ),
                ));
                let terminal = assert_exact_acceptance(&fixture);
                assert_exact_receipt_replay(&fixture, &terminal);
            }
        }
    }
}

#[test]
fn occupied_disposal_receipt_identity_rejects_acceptance_without_partial_clear() {
    for queued in [false, true] {
        let fixture = Fixture::new("receipt-occupied", 50, queued);
        let receipt_key = DraftEditorCandidateSessionRecordKeyV1::disposal_receipt(
            fixture.source.draft_id(),
            fixture.source.session_id(),
            fixture.acceptance.session_disposal_operation_id(),
        );
        committed(execute(
            &fixture.store,
            inject_draft_candidate_publication_fault(
                &fixture.store,
                fixture.storage.clone(),
                DraftCandidatePublicationFault::OccupyReceiptWithHead {
                    receipt_key,
                    draft_id: fixture.source.draft_id(),
                    session_id: fixture.source.session_id(),
                },
            ),
        ));
        let prior = current(&fixture.storage, &fixture.store, fixture.thread);
        let gate = fixture
            .storage
            .input_gate(&fixture.store, fixture.thread, read_limit())
            .unwrap();
        let revision = fixture.store.home_revision().unwrap();
        assert!(matches!(
            fixture.accept(),
            CommandOutcome::NotCommitted { .. }
        ));
        assert_eq!(fixture.store.home_revision().unwrap(), revision);
        assert_eq!(
            current(&fixture.storage, &fixture.store, fixture.thread),
            prior
        );
        assert_eq!(
            fixture
                .storage
                .input_gate(&fixture.store, fixture.thread, read_limit())
                .unwrap(),
            gate
        );
        assert_eq!(
            head(&fixture.storage, &fixture.store, &fixture.source),
            fixture.source
        );
        assert!(
            fixture
                .storage
                .accepted_input(
                    &fixture.store,
                    fixture.acceptance.accepted_input_id(),
                    read_limit()
                )
                .unwrap()
                .is_none()
        );
        assert!(
            fixture
                .storage
                .canonical_item(
                    &fixture.store,
                    fixture.acceptance.idle_user_item_id(),
                    read_limit()
                )
                .unwrap()
                .is_none()
        );
    }
}
