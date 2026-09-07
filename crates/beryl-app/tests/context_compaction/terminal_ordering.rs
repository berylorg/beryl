use beryl_home_store::{
    HomeHealthState, ReconciliationResolution,
    test_faults::{FaultController, FaultPoint},
};
use syndic_storage::CompactionRequestTransitionStatus;

use super::*;

#[test]
fn acknowledgement_before_terminal_publishes_once_and_preserves_successful_settlement() {
    let fixture = LifecycleFixture::new(181, 201);
    let before = fixture.operation();
    assert_eq!(
        fixture
            .harness
            .observe_response(
                fixture.operation_id,
                before.attempt(),
                CompactionRequestDisposition::Accepted,
            )
            .unwrap(),
        CompactionRequestTransitionStatus::Exact
    );
    let acknowledged = fixture.operation();
    assert_eq!(
        acknowledged.revision(),
        before.revision().checked_next().unwrap()
    );
    assert_eq!(
        acknowledged.request().unwrap().disposition(),
        CompactionRequestDisposition::Accepted
    );
    assert!(acknowledged.state().is_live());
    assert_eq!(acknowledged.dispatch_claim(), before.dispatch_claim());

    fixture.publish_success_prefix();
    fixture.publish_success_terminal();
    let settled = fixture.operation();
    assert!(matches!(
        settled.state(),
        CompactionOperationState::Consumed(_)
    ));
    assert_eq!(settled.request(), acknowledged.request());
    assert_eq!(settled.dispatch_claim(), before.dispatch_claim());
    assert_eq!(
        fixture
            .service
            .context_compaction_diagnostics()
            .retained_operations(),
        0
    );
    fixture.close();
}

#[test]
fn late_response_matrix_preserves_terminal_successor_and_all_durable_revisions() {
    for accepted_next in [false, true] {
        let fixture = if accepted_next {
            LifecycleFixture::with_accepted_next(182, 203)
        } else {
            LifecycleFixture::new(183, 205)
        };
        fixture.publish_success_prefix();
        fixture.publish_success_terminal();
        let before = fixture.operation();
        let gate = fixture.input_gate();
        let tail = fixture.committed_tail();
        let command = fixture.service.live_home_command().unwrap();
        let home = command.home();
        let home_revision = home.home_revision().unwrap();
        let domain_revision = fixture.storage.revision(home).unwrap();

        for (disposition, unbind_failed, expected) in [
            (
                CompactionRequestDisposition::Accepted,
                false,
                ContextCompactionTerminalResponseTestOutcome::AwaitRouterTerminal,
            ),
            (
                CompactionRequestDisposition::Accepted,
                true,
                ContextCompactionTerminalResponseTestOutcome::RetireConnection,
            ),
            (
                CompactionRequestDisposition::CompletionUnknown,
                false,
                ContextCompactionTerminalResponseTestOutcome::RetireConnection,
            ),
            (
                CompactionRequestDisposition::RejectedBeforeCore,
                false,
                ContextCompactionTerminalResponseTestOutcome::InvariantFailure,
            ),
            (
                CompactionRequestDisposition::ProvenLocalNondispatch,
                false,
                ContextCompactionTerminalResponseTestOutcome::InvariantFailure,
            ),
        ] {
            assert_eq!(
                fixture
                    .harness
                    .reconcile_settled_response(
                        fixture.operation_id,
                        before.attempt(),
                        disposition,
                        unbind_failed,
                    )
                    .unwrap(),
                expected,
            );
            assert_eq!(fixture.operation(), before);
            assert_eq!(fixture.input_gate(), gate);
            assert_eq!(fixture.committed_tail(), tail);
            assert_eq!(home.home_revision().unwrap(), home_revision);
            assert_eq!(fixture.storage.revision(home).unwrap(), domain_revision);
            assert!(home.pending_reconciliations().is_empty());
        }
        drop(command);
        fixture.close();
    }
}

#[test]
fn wrong_attempt_cannot_publish_a_live_or_terminal_disposition() {
    let fixture = LifecycleFixture::new(184, 207);
    for terminal in [false, true] {
        if terminal {
            fixture.publish_success_prefix();
            fixture.publish_success_terminal();
        }
        let before = fixture.operation();
        let command = fixture.service.live_home_command().unwrap();
        let revision = command.home().home_revision().unwrap();
        assert!(matches!(
            fixture.harness.observe_response(
                fixture.operation_id,
                CompactionAttemptNonce::from_bytes([254; 16]),
                CompactionRequestDisposition::Accepted,
            ),
            Err(ContextCompactionError::AuthorityMismatch)
        ));
        assert_eq!(fixture.operation(), before);
        assert_eq!(command.home().home_revision().unwrap(), revision);
        assert!(command.home().pending_reconciliations().is_empty());
    }
    fixture.close();
}

#[test]
fn indeterminate_live_disposition_keeps_exact_custody_through_coordinator_shutdown() {
    let faults = FaultController::new();
    let fixture = LifecycleFixture::with_faults(185, 209, faults.clone());
    let before = fixture.operation();
    let command = fixture.service.live_home_command().unwrap();
    let home = command.home();
    let revision = home.home_revision().unwrap();
    faults.fail_next(FaultPoint::AfterCommitBeforePersist);
    assert!(matches!(
        fixture.harness.observe_response(
            fixture.operation_id,
            before.attempt(),
            CompactionRequestDisposition::Accepted,
        ),
        Err(ContextCompactionError::CommandIndeterminate { .. })
    ));
    let scopes = home.pending_reconciliations();
    assert_eq!(scopes.len(), 1);
    assert_eq!(home.health().state(), HomeHealthState::Healthy);
    fixture.harness.request_shutdown().unwrap();
    assert_eq!(home.pending_reconciliations().len(), 1);
    match home.reconcile(&scopes[0]).unwrap() {
        ReconciliationResolution::ExactNew { receipt } => {
            assert_eq!(receipt.home_revision(), revision.checked_next().unwrap());
        }
        other => panic!("request disposition lost its exact reconciliation: {other:?}"),
    }
    assert!(home.pending_reconciliations().is_empty());
    let after = fixture.operation();
    assert_eq!(after.revision(), before.revision().checked_next().unwrap());
    assert_eq!(after.dispatch_claim(), before.dispatch_claim());
    assert_eq!(
        after.request().unwrap().disposition(),
        CompactionRequestDisposition::Accepted
    );
    assert!(after.state().is_live());
    assert!(after.terminal().is_none());
    assert!(matches!(
        fixture.harness.observe_response(
            fixture.operation_id,
            before.attempt(),
            CompactionRequestDisposition::Accepted,
        ),
        Err(ContextCompactionError::Unavailable)
    ));
    assert_eq!(fixture.operation(), after);
    drop(command);
    fixture.close();
}
