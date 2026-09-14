use super::*;

#[test]
fn exact_provider_stop_keeps_status_and_terminal_publication_authorized() {
    let fixture = LifecycleFixture::new(193, 236);
    let original_tail = fixture.committed_tail();
    fixture.publish_success_prefix();
    fixture
        .service
        .cancel_selected_continuation_for_window_close(fixture.thread_id)
        .unwrap();
    let stop_id = {
        let live = fixture.service.live_home_command().unwrap();
        let StopAdmissionRead::Admissible(candidate) = fixture
            .storage
            .stop_admission_read(live.home(), fixture.thread_id, point_limit())
            .unwrap()
        else {
            panic!("known exact compaction must admit stop");
        };
        let admission = candidate.admission(
            StopOperationNonce::from_bytes([238; 16]),
            StopCauseSet::from(StopCause::HealthyHomeWindowClose),
        );
        let stop_id = admission.operation_id();
        assert!(matches!(
            live.home()
                .execute_current(fixture.storage.current_admit_stop_operation(admission)),
            CommandOutcome::Committed {
                later_failure: None,
                ..
            }
        ));
        stop_id
    };
    assert!(matches!(
        fixture.operation().state(),
        CompactionOperationState::Stopping(_)
    ));
    fixture
        .harness
        .publish_provider_event(
            fixture.operation_id,
            CompactionProviderEvent::ThreadStatus(syndic_storage::CompactionThreadStatus::Idle),
            SyndicTimestamp::from_unix_millis(72_019),
        )
        .unwrap();
    assert!(fixture.operation().terminal().is_none());
    fixture.publish_success_terminal();
    assert_eq!(fixture.committed_tail(), original_tail);
    assert!(matches!(
        fixture.operation().state(),
        CompactionOperationState::Consumed(witness)
            if witness.settlement() == &CompactionSettlement::ManualSuccess
    ));
    assert_eq!(
        fixture.input_gate().state(),
        &syndic_storage::InputGateState::Idle
    );
    {
        let live = fixture.service.live_home_command().unwrap();
        let stop = fixture
            .storage
            .stop_operation(live.home(), stop_id, point_limit())
            .unwrap()
            .unwrap();
        assert!(matches!(
            stop.state(),
            StopOperationState::MatchingTerminal(_)
        ));
    }
    assert!(matches!(
        fixture.harness.publish_provider_event(
            fixture.operation_id,
            CompactionProviderEvent::ThreadStatus(syndic_storage::CompactionThreadStatus::Idle),
            SyndicTimestamp::from_unix_millis(72_021),
        ),
        Err(ContextCompactionError::AuthorityMismatch)
    ));
    fixture.close();
}
