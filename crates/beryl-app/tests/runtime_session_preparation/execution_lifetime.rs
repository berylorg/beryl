use super::*;
use beryl_app::cas_projection::RuntimeInterestKind;
use beryl_model::SyndicTurnId;
use serde_json::Value;
use syndic_storage::{FirstAcceptanceKind, SyndicPointReadLimit, TurnLifecycle};

pub(super) fn started(
    fixture: &Fixture,
    sessions: &ScheduledExecutionSessions,
    ordinal: u8,
) -> Value {
    let path = fixture
        .root(1)
        .join(format!("execution-started-{ordinal}.json"));
    let deadline = std::time::Instant::now() + TIMEOUT;
    loop {
        let evidence = fs::read(&path)
            .ok()
            .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok());
        if let Some(evidence) = evidence {
            return evidence;
        }
        if std::time::Instant::now() >= deadline {
            let gate = fixture.service().live_home_command().map(|live| {
                fixture.storage.input_gate(
                    live.home(),
                    thread_id(1),
                    SyndicPointReadLimit::new(1_000_000).unwrap(),
                )
            });
            panic!(
                "managed execution did not start: sessions {:?}, scheduler {:?}, gate {:?}, failure {:?}, projection {:?}, request {:?}",
                sessions.diagnostics(),
                fixture.service().accepted_input_scheduler_diagnostics(),
                gate,
                fixture.service().persistent_failure_cut_snapshot(),
                fs::read_to_string(
                    fixture
                        .root(1)
                        .join(format!("execution-projection-{ordinal}.json"))
                ),
                fs::read_to_string(
                    fixture
                        .root(1)
                        .join(format!("execution-request-{ordinal}.json"))
                ),
            );
        }
        thread::sleep(Duration::from_millis(2));
    }
}

pub(super) fn turn(fixture: &Fixture, expected: TurnLifecycle) -> SyndicTurnId {
    let mut selected = None;
    wait_until(|| {
        let live = fixture.service().live_home_command().unwrap();
        let limit = SyndicPointReadLimit::new(1_000_000).unwrap();
        let Some(record) = fixture
            .storage
            .thread(live.home(), thread_id(1), limit)
            .unwrap()
        else {
            return false;
        };
        let Some(id) = record.committed_tail() else {
            return false;
        };
        let state = fixture
            .storage
            .turn_state(live.home(), id, limit)
            .unwrap()
            .unwrap();
        if state.lifecycle() == expected
            && (expected != TurnLifecycle::Active || state.source_event_count() >= 3)
        {
            selected = Some(id);
            true
        } else {
            false
        }
    });
    selected.unwrap()
}

pub(super) fn release(fixture: &Fixture, ordinal: u8) {
    fs::write(
        fixture.root(1).join(format!("execution-release-{ordinal}")),
        "complete",
    )
    .unwrap();
}

#[test]
fn managed_execution_retains_session_through_terminal_history_without_a_view() {
    use beryl_app::cas_projection::test_faults::{
        TerminalHistoryBarrierStage, install_terminal_history_barrier,
    };
    for stage in [
        TerminalHistoryBarrierStage::BeforeGateRelease,
        TerminalHistoryBarrierStage::AfterGateRelease,
    ] {
        let (mut fixture, sessions, attention) = fixture(8);
        fs::write(fixture.root(1).join("fixture-mode"), "execution-lifetime").unwrap();
        let view = fixture.acquire(1, RuntimeInterestKind::View).unwrap();
        let readiness = support::ready(&view);
        submission::submit(&fixture, thread_id(1));
        let evidence = started(&fixture, &sessions, 0);
        let process = ProcessWitness::open(evidence["pid"].as_u64().unwrap() as u32);
        let active = turn(&fixture, TurnLifecycle::Active);
        let barrier = install_terminal_history_barrier(thread_id(1), stage);
        drop(view);
        release(&fixture, 0);
        barrier.wait();
        assert_eq!(turn(&fixture, TurnLifecycle::Complete), active);
        assert_eq!(sessions.diagnostics().checked_out, 1);
        assert!(process.running());
        assert!(!fixture.root(1).join("execution-unsubscribed-0").exists());
        let live = fixture.service().live_home_command().unwrap();
        let gate = fixture
            .storage
            .input_gate(
                live.home(),
                thread_id(1),
                SyndicPointReadLimit::new(1_000_000).unwrap(),
            )
            .unwrap()
            .unwrap();
        assert_eq!(
            gate.state(),
            &match stage {
                TerminalHistoryBarrierStage::BeforeGateRelease =>
                    syndic_storage::InputGateState::FinalizingHistory(active),
                TerminalHistoryBarrierStage::AfterGateRelease =>
                    syndic_storage::InputGateState::Idle,
                _ => unreachable!(),
            }
        );
        drop(live);
        let inventory = fixture
            .service()
            .process_work_inventory(&sessions, &attention);
        let mut captured = None;
        wait_until(|| {
            match inventory.revision().and_then(|revision| {
                inventory.page(
                    &revision,
                    None,
                    beryl_app::cas_projection::ProcessWorkPageLimits::new(256, 65_536).unwrap(),
                    &beryl_app::cas_projection::ProjectionCancellationToken::new(),
                )
            }) {
                Ok(page) => {
                    captured = Some(page);
                    true
                }
                Err(
                    beryl_app::cas_projection::ProcessWorkError::StaleRevision
                    | beryl_app::cas_projection::ProcessWorkError::Connections(
                        beryl_app::cas_projection::ConnectionWorkError::StaleRevision,
                    ),
                ) => false,
                Err(error) => panic!("terminal work inventory failed: {error:?}"),
            }
        });
        let page = captured.unwrap();
        assert_eq!(page.total_threads(), 1);
        drop(inventory);
        let reattached = fixture.acquire(1, RuntimeInterestKind::View).unwrap();
        assert_eq!(support::ready(&reattached), readiness);
        assert_eq!(sessions.diagnostics().checked_out, 1);
        drop(reattached);
        barrier.release();
        process.assert_exited();
        wait_until(|| {
            sessions.diagnostics().retained == 0
                && fixture.token_count() == 0
                && fixture.service().worker_pool_diagnostics().active() == 0
        });
        close(&mut fixture, &sessions);
    }
}

#[test]
fn managed_direct_execution_survives_last_view_detach_and_immediate_reattach() {
    let (mut fixture, sessions, _attention) = fixture(8);
    fs::write(fixture.root(1).join("fixture-mode"), "execution-lifetime").unwrap();
    let view = fixture.acquire(1, RuntimeInterestKind::View).unwrap();
    let readiness = support::ready(&view);
    submission::submit(&fixture, thread_id(1));
    let evidence = started(&fixture, &sessions, 0);
    assert_eq!(evidence["text"], "continue durable work");
    let process = ProcessWitness::open(evidence["pid"].as_u64().unwrap() as u32);
    let active = turn(&fixture, TurnLifecycle::Active);
    assert_eq!(sessions.diagnostics().checked_out, 1);
    drop(view);
    let reattached = fixture.acquire(1, RuntimeInterestKind::View).unwrap();
    assert_eq!(support::ready(&reattached), readiness);
    assert_eq!(sessions.diagnostics().checked_out, 1);
    assert!(process.running());
    assert!(!fixture.root(1).join("execution-unsubscribed-0").exists());
    assert!(
        !fixture
            .root(1)
            .join("runtime-session-evidence-2.json")
            .exists()
    );
    drop(reattached);
    release(&fixture, 0);
    assert_eq!(turn(&fixture, TurnLifecycle::Complete), active);
    process.assert_exited();
    wait_until(|| {
        sessions.diagnostics().retained == 0
            && fixture.token_count() == 0
            && fixture.service().worker_pool_diagnostics().active() == 0
    });
    assert!(!fixture.root(1).join("execution-started-1.json").exists());
    close(&mut fixture, &sessions);
}

#[test]
fn terminal_completion_survives_winning_successor_admission_during_cleanup() {
    use beryl_app::cas_projection::test_faults::TerminalHistoryBarrierStage;
    for stage in [
        TerminalHistoryBarrierStage::AfterGateCommit,
        TerminalHistoryBarrierStage::AfterGateRelease,
    ] {
        terminal_completion_successor_race(stage);
    }
}

fn terminal_completion_successor_race(
    stage: beryl_app::cas_projection::test_faults::TerminalHistoryBarrierStage,
) {
    use beryl_app::cas_projection::test_faults::{
        TerminalHistoryBarrierStage, install_terminal_history_barrier,
    };
    let (mut fixture, sessions, _attention) = fixture(8);
    fs::write(fixture.root(1).join("fixture-mode"), "execution-lifetime").unwrap();
    let view = fixture.acquire(1, RuntimeInterestKind::View).unwrap();
    support::ready(&view);
    submission::submit(&fixture, thread_id(1));
    let evidence = started(&fixture, &sessions, 0);
    let process = ProcessWitness::open(evidence["pid"].as_u64().unwrap() as u32);
    let first = turn(&fixture, TurnLifecycle::Active);
    let mut captured = None;
    wait_until(|| {
        match fixture
            .service()
            .terminal_completion_for_test(&sessions, thread_id(1))
        {
            Ok(Some(probe)) => {
                captured = Some(probe);
                true
            }
            Ok(None)
            | Err(beryl_app::cas_projection::ProcessWorkError::StaleRevision)
            | Err(beryl_app::cas_projection::ProcessWorkError::Connections(
                beryl_app::cas_projection::ConnectionWorkError::StaleRevision,
            )) => false,
            Err(error) => panic!("terminal completion capture failed: {error:?}"),
        }
    });
    let captured = captured.unwrap();
    assert_eq!(captured.turn_id(), first);
    assert_eq!(captured.lifecycle(), None);
    let barrier = install_terminal_history_barrier(thread_id(1), stage);
    drop(view);
    release(&fixture, 0);
    barrier.wait();
    let before_release =
        (stage == TerminalHistoryBarrierStage::AfterGateRelease).then_some(TurnLifecycle::Complete);
    assert_eq!(captured.lifecycle(), before_release);
    assert_eq!(sessions.diagnostics().checked_out, 1);
    assert!(process.running());
    let admission = fixture.process_admission.clone();
    let (send_fence, receive_fence) = std::sync::mpsc::sync_channel(1);
    let acceptance = submission::submit_text_after_admission(
        &fixture,
        thread_id(1),
        "preserve the admitted successor",
        170,
        SyndicTimestamp::from_unix_millis(6),
        move |_, _| {
            send_fence.send(admission.test_fence().unwrap()).unwrap();
        },
    );
    assert!(matches!(
        acceptance,
        syndic_storage::FirstAcceptanceKind::Idle { .. }
    ));
    let fence = receive_fence.recv().unwrap();
    let mut execution_capture =
        beryl_app::cas_projection::test_faults::ShutdownExecutionCaptureProbe::new(
            fixture.service(),
            &fence,
        )
        .unwrap();
    execution_capture.refresh(fixture.service()).unwrap();
    assert_eq!(execution_capture.counts(), (1, 0));
    let retained_completion = execution_capture.ordinary(thread_id(1), first).unwrap();
    assert_eq!(retained_completion.lifecycle(), before_release);
    let pending = turn(&fixture, TurnLifecycle::Pending);
    assert_ne!(pending, first);
    let live = fixture.service().live_home_command().unwrap();
    assert!(
        fixture
            .storage
            .terminal_history_evidence(
                live.home(),
                thread_id(1),
                first,
                SyndicPointReadLimit::new(1_000_000).unwrap(),
            )
            .unwrap()
            .is_none()
    );
    drop(live);
    assert_eq!(captured.turn_id(), first);
    assert_eq!(captured.lifecycle(), before_release);
    assert_eq!(sessions.diagnostics().checked_out, 1);
    assert_eq!(
        fence.try_reopen(true),
        Err(beryl_app::process_admission::ProcessAdmissionError::Unsettled)
    );
    barrier.release();
    wait_until(|| sessions.diagnostics().checked_out == 0);
    execution_capture.refresh(fixture.service()).unwrap();
    assert_eq!(execution_capture.counts(), (1, 0));
    assert_eq!(
        retained_completion.lifecycle(),
        Some(TurnLifecycle::Complete)
    );
    assert_eq!(turn(&fixture, TurnLifecycle::Pending), pending);
    assert!(!fixture.root(1).join("execution-started-1.json").exists());
    close(&mut fixture, &sessions);
    process.assert_exited();
    assert_eq!(captured.lifecycle(), Some(TurnLifecycle::Complete));
}

#[test]
fn managed_accepted_successor_dispatches_and_captures_without_a_view() {
    let (mut fixture, sessions, _attention) = fixture(8);
    fs::write(fixture.root(1).join("fixture-mode"), "execution-next").unwrap();
    let view = fixture.acquire(1, RuntimeInterestKind::View).unwrap();
    support::ready(&view);
    submission::submit(&fixture, thread_id(1));
    let first = started(&fixture, &sessions, 0);
    let first_turn = turn(&fixture, TurnLifecycle::Active);
    let process = ProcessWitness::open(first["pid"].as_u64().unwrap() as u32);
    drop(view);
    assert_eq!(
        submission::submit_text(
            &fixture,
            thread_id(1),
            "queued without a view",
            180,
            SyndicTimestamp::from_unix_millis(37_003)
        ),
        FirstAcceptanceKind::Accepted
    );
    wait_until(|| fixture.root(1).join("execution-steering-denied").exists());
    release(&fixture, 0);
    let second = started(&fixture, &sessions, 1);
    let successor_process = ProcessWitness::open(second["pid"].as_u64().unwrap() as u32);
    if second["pid"] != first["pid"] {
        process.assert_exited();
    }
    assert_eq!(second["thread"], first["thread"]);
    assert_eq!(second["text"], "queued without a view");
    let second_turn = turn(&fixture, TurnLifecycle::Active);
    assert_ne!(second_turn, first_turn);
    assert_eq!(sessions.diagnostics().checked_out, 1);
    release(&fixture, 1);
    assert_eq!(turn(&fixture, TurnLifecycle::Complete), second_turn);
    process.assert_exited();
    successor_process.assert_exited();
    wait_until(|| {
        sessions.diagnostics().retained == 0
            && fixture.token_count() == 0
            && fixture.service().worker_pool_diagnostics().active() == 0
    });
    assert!(
        !fixture
            .root(1)
            .join("runtime-session-evidence-3.json")
            .exists()
    );
    close(&mut fixture, &sessions);
}
