use super::*;
use crate::cas_projection::stop::{WindowCloseStopBarrier, WindowCloseStopBarrierStatus};
use syndic_storage::{
    CompactionOperationState, CompactionSettlement, PublishCompactionProviderEvent,
    SettleCompactionOperation, StopAdmissionRead, StopCause, StopCauseSet, StopOperationNonce,
};

#[test]
fn stop_barrier_waits_after_compaction_terminal_until_exact_gate_release() {
    let mut fixture = Fixture::new(186);
    fixture.publish_marker(
        191,
        ProviderItemLifecycle::Started,
        ProviderItemKind::ContextCompaction,
    );
    fixture.publish_marker(
        192,
        ProviderItemLifecycle::Completed,
        ProviderItemKind::ContextCompaction,
    );
    let StopAdmissionRead::Admissible(candidate) = fixture
        .storage
        .stop_admission_read(
            &fixture.home,
            fixture.operation_id.thread_id(),
            point_limit(),
        )
        .unwrap()
    else {
        panic!("published compaction must admit exact stop");
    };
    let request = candidate.admission(
        StopOperationNonce::from_bytes([187; 16]),
        StopCauseSet::from(StopCause::HealthyHomeWindowClose),
    );
    let stop_id = request.operation_id();
    assert!(matches!(
        fixture
            .home
            .execute_current(fixture.storage.current_admit_stop_operation(request)),
        CommandOutcome::Committed {
            later_failure: None,
            ..
        }
    ));
    let mut barrier = WindowCloseStopBarrier::new(
        Arc::clone(&fixture.stop_coordinator),
        stop_id,
        fixture.operation_id.provider_turn_id(),
        false,
    );
    assert_eq!(
        barrier.poll().unwrap(),
        WindowCloseStopBarrierStatus::Waiting
    );
    for (event, at) in [
        (
            CompactionProviderEvent::ThreadStatus(CompactionThreadStatus::Idle),
            193,
        ),
        (
            CompactionProviderEvent::Terminal(syndic_storage::TurnEndStatus::complete()),
            194,
        ),
    ] {
        let operation = fixture.operation();
        assert!(matches!(
            fixture.home.execute_current(
                fixture.storage.current_publish_compaction_provider_event(
                    PublishCompactionProviderEvent::new(
                        fixture.operation_id,
                        operation.revision(),
                        operation
                            .provider_frontier()
                            .unwrap()
                            .checked_next()
                            .unwrap(),
                        event,
                        SyndicTimestamp::from_unix_millis(at)
                    ),
                )
            ),
            CommandOutcome::Committed {
                later_failure: None,
                ..
            }
        ));
    }
    assert_eq!(
        fixture.operation().state(),
        &CompactionOperationState::Finalizing
    );
    assert_eq!(
        barrier.poll().unwrap(),
        WindowCloseStopBarrierStatus::Waiting
    );
    assert_eq!(
        barrier.poll().unwrap(),
        WindowCloseStopBarrierStatus::Waiting
    );
    let operation = fixture.operation();
    assert!(matches!(
        fixture
            .home
            .execute_current(fixture.storage.current_settle_compaction_operation(
                SettleCompactionOperation::new(
                    fixture.operation_id,
                    operation.revision(),
                    CompactionSettlement::ManualSuccess
                ),
            )),
        CommandOutcome::Committed {
            later_failure: None,
            ..
        }
    ));
    assert_eq!(
        barrier.poll().unwrap(),
        WindowCloseStopBarrierStatus::Converged
    );
}
