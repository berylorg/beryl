use super::*;
use beryl_app::cas_projection::{RuntimeInterestKind, test_faults::GracefulShutdownProbe};
use syndic_storage::{SyndicPointReadLimit, TurnLifecycle};

#[test]
fn shutdown_soft_stops_hidden_execution_and_waits_for_terminal_and_cleanup() {
    let (mut fixture, sessions, _) = fixture(8);
    fs::write(fixture.root(1).join("fixture-mode"), "execution-shutdown").unwrap();
    let view = fixture.acquire(1, RuntimeInterestKind::View).unwrap();
    support::ready(&view);
    submission::submit(&fixture, thread_id(1));
    execution_lifetime::started(&fixture, &sessions, 0);
    let active = execution_lifetime::turn(&fixture, TurnLifecycle::Active);
    drop(view);
    let fence = fixture.process_admission.test_fence().unwrap();
    let shutdown = GracefulShutdownProbe::begin(fixture.service(), &fence).unwrap();
    let joined = GracefulShutdownProbe::begin(fixture.service(), &fence).unwrap();
    wait_until(|| {
        assert!(!shutdown.poll(fixture.service(), &sessions).unwrap());
        fixture.root(1).join("execution-interrupted").exists()
    });
    assert_eq!(shutdown.retained_counts(fixture.service()), (1, 0, 1));
    assert!(!joined.poll(fixture.service(), &sessions).unwrap());
    assert_eq!(
        execution_lifetime::turn(&fixture, TurnLifecycle::Active),
        active
    );
    execution_lifetime::release(&fixture, 0);
    wait_until(|| shutdown.poll(fixture.service(), &sessions).unwrap());
    assert_eq!(shutdown.retained_counts(fixture.service()), (1, 0, 1));
    let live = fixture.service().live_home_command().unwrap();
    assert!(
        fixture
            .storage
            .terminal_history_evidence(
                live.home(),
                thread_id(1),
                active,
                SyndicPointReadLimit::new(65_536).unwrap()
            )
            .unwrap()
            .is_some()
    );
    drop(live);
    assert!(!fixture.root(1).join("execution-started-1.json").exists());
    assert_eq!(sessions.diagnostics().retained, 0);
    assert!(matches!(
        fixture.service.take().unwrap().close().unwrap(),
        ProjectionConnectionServiceCloseOutcome::Closed
    ));
}

#[test]
fn shutdown_keeps_predecessor_completion_separate_from_a_winning_pending_successor() {
    use beryl_app::cas_projection::test_faults::{
        TerminalHistoryBarrierStage, install_terminal_history_barrier,
    };
    for stage in [
        TerminalHistoryBarrierStage::AfterGateCommit,
        TerminalHistoryBarrierStage::AfterGateRelease,
    ] {
        let (mut fixture, sessions, _) = fixture(8);
        fs::write(fixture.root(1).join("fixture-mode"), "execution-lifetime").unwrap();
        let view = fixture.acquire(1, RuntimeInterestKind::View).unwrap();
        support::ready(&view);
        submission::submit(&fixture, thread_id(1));
        execution_lifetime::started(&fixture, &sessions, 0);
        let predecessor = execution_lifetime::turn(&fixture, TurnLifecycle::Active);
        let barrier = install_terminal_history_barrier(thread_id(1), stage);
        drop(view);
        execution_lifetime::release(&fixture, 0);
        barrier.wait();
        let admission = fixture.process_admission.clone();
        let (send, receive) = std::sync::mpsc::sync_channel(1);
        let acceptance = submission::submit_text_after_admission(
            &fixture,
            thread_id(1),
            "preserve the winning pending successor",
            170,
            SyndicTimestamp::from_unix_millis(6),
            move |_, _| send.send(admission.test_fence().unwrap()).unwrap(),
        );
        assert!(matches!(
            acceptance,
            syndic_storage::FirstAcceptanceKind::Idle { .. }
        ));
        let fence = receive.recv().unwrap();
        let pending = execution_lifetime::turn(&fixture, TurnLifecycle::Pending);
        assert_ne!(pending, predecessor);
        let shutdown = GracefulShutdownProbe::begin(fixture.service(), &fence).unwrap();
        assert!(!shutdown.poll(fixture.service(), &sessions).unwrap());
        assert_eq!(shutdown.retained_counts(fixture.service()), (1, 0, 0));
        assert_eq!(sessions.diagnostics().checked_out, 1);
        barrier.release();
        wait_until(|| shutdown.poll(fixture.service(), &sessions).unwrap());
        assert_eq!(
            execution_lifetime::turn(&fixture, TurnLifecycle::Pending),
            pending
        );
        assert_eq!(shutdown.retained_counts(fixture.service()), (1, 0, 0));
        assert!(!fixture.root(1).join("execution-started-1.json").exists());
        assert_eq!(sessions.diagnostics().retained, 0);
        assert!(matches!(
            fixture.service.take().unwrap().close().unwrap(),
            ProjectionConnectionServiceCloseOutcome::Closed
        ));
    }
}

#[test]
fn shutdown_waits_for_noninterruptible_compaction_and_cancels_its_continuation() {
    let (mut fixture, sessions, _) = fixture(8);
    fs::write(
        fixture.root(1).join("fixture-mode"),
        "execution-compaction-shutdown",
    )
    .unwrap();
    let view = fixture.acquire(1, RuntimeInterestKind::View).unwrap();
    support::ready(&view);
    submission::submit(&fixture, thread_id(1));
    execution_lifetime::started(&fixture, &sessions, 0);
    let yielding = execution_lifetime::turn(&fixture, TurnLifecycle::Active);
    assert!(
        fixture
            .service()
            .record_lifecycle_yield_outcome(
                thread_id(1),
                yielding,
                beryl_app::LifecycleYieldOutcome::PhaseContinue,
            )
            .unwrap()
    );
    drop(view);
    execution_lifetime::release(&fixture, 0);
    wait_until(|| fixture.root(1).join("compaction-requested").exists());
    let operation = {
        let live = fixture.service().live_home_command().unwrap();
        let syndic_storage::CompactionAdmissionRead::Existing(operation) = fixture
            .storage
            .compaction_admission_read(
                live.home(),
                thread_id(1),
                SyndicPointReadLimit::new(65_536).unwrap(),
            )
            .unwrap()
        else {
            panic!("compaction must remain admitted");
        };
        operation.id()
    };
    let fence = fixture.process_admission.test_fence().unwrap();
    let shutdown = GracefulShutdownProbe::begin(fixture.service(), &fence).unwrap();
    wait_until(|| {
        assert!(!shutdown.poll(fixture.service(), &sessions).unwrap());
        shutdown.retained_counts(fixture.service()).1 == 1
    });
    assert_eq!(shutdown.retained_counts(fixture.service()).2, 0);
    fs::write(fixture.root(1).join("compaction-begin"), "ready").unwrap();
    wait_until(|| fixture.root(1).join("compaction-started").exists());
    wait_until(|| {
        let live = fixture.service().live_home_command().unwrap();
        matches!(
            fixture.storage.stop_admission_read(
                live.home(),
                thread_id(1),
                SyndicPointReadLimit::new(65_536).unwrap(),
            ),
            Ok(syndic_storage::StopAdmissionRead::Admissible(_))
        )
    });
    wait_until(|| {
        assert!(!shutdown.poll(fixture.service(), &sessions).unwrap());
        fixture.root(1).join("compaction-interrupted").exists()
    });
    assert_eq!(shutdown.retained_counts(fixture.service()).2, 1);
    assert!(!shutdown.poll(fixture.service(), &sessions).unwrap());
    assert!(!fixture.root(1).join("execution-started-1.json").exists());
    fs::write(fixture.root(1).join("compaction-release"), "ready").unwrap();
    wait_until(|| shutdown.poll(fixture.service(), &sessions).unwrap());
    let live = fixture.service().live_home_command().unwrap();
    let settled = fixture
        .storage
        .compaction_operation(
            live.home(),
            operation,
            SyndicPointReadLimit::new(65_536).unwrap(),
        )
        .unwrap()
        .unwrap();
    assert!(matches!(
        settled.state(),
        syndic_storage::CompactionOperationState::Consumed(witness)
            if witness.settlement() == &syndic_storage::CompactionSettlement::ManualSuccess
    ));
    drop(live);
    assert!(!fixture.root(1).join("execution-started-1.json").exists());
    assert_eq!(sessions.diagnostics().retained, 0);
    assert_eq!(shutdown.retained_counts(fixture.service()).1, 1);
    assert!(matches!(
        fixture.service.take().unwrap().close().unwrap(),
        ProjectionConnectionServiceCloseOutcome::Closed
    ));
}
