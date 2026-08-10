use std::{
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    thread,
    time::Duration,
};

use beryl_app::cas_projection::{
    CasProjectionCoordinator, CasProjectionRequest, OrdinaryDynamicToolHandlers,
    OrdinaryTurnExecutionOutcome, OrdinaryTurnExecutionRequest,
    test_faults::{TerminalHistoryBarrierStage, install_terminal_history_barrier},
};
use beryl_backend::{ManagedBackendClientConnector, ThreadStartOptions, TurnStartOptions};
use beryl_model::CasProcessGeneration;
use syndic_storage::{
    AcceptedRouteEffectiveState, HistorySummaryRecord, InputGateRecord, InputGateState,
    NextTurnReason, ProjectionLifecycle, StartTranscriptBuild, ThreadRecord, TranscriptBuildPhase,
    TranscriptBuildRecord, TranscriptViewHeadRecord, TurnEndStatus,
};

use super::{
    live_support::{NoopBranch, NoopLifecycle, TerminalHistoryReleaseGuard},
    support::{CountingUnavailableProvider, admit_successor},
};
use crate::{
    app_support::point_limit,
    phase62_support::{
        AUTHORIZATION, NormalTerminalServer, SUBMITTED_TEXT, TIMEOUT, accepted_route_state,
        wait_until,
    },
};

#[derive(Clone, Debug, Eq, PartialEq)]
struct TranscriptAdmissionSnapshot {
    thread: ThreadRecord,
    summary: HistorySummaryRecord,
    head: TranscriptViewHeadRecord,
    build: TranscriptBuildRecord,
    gate: InputGateRecord,
}

fn snapshot(fixture: &crate::syndic::Fixture) -> TranscriptAdmissionSnapshot {
    let command_home = fixture.store.live_home_command().unwrap();
    let home = command_home.home();
    let thread = fixture
        .storage
        .thread(home, fixture.thread, point_limit())
        .unwrap()
        .unwrap();
    let summary = fixture
        .storage
        .history_summary(home, fixture.thread, point_limit())
        .unwrap()
        .unwrap();
    let head = fixture
        .storage
        .transcript_view_head(home, fixture.thread, point_limit())
        .unwrap()
        .unwrap();
    let build = fixture
        .storage
        .transcript_build(home, fixture.thread, head.generation(), point_limit())
        .unwrap()
        .unwrap();
    let gate = fixture
        .storage
        .input_gate(home, fixture.thread, point_limit())
        .unwrap()
        .unwrap();
    TranscriptAdmissionSnapshot {
        thread,
        summary,
        head,
        build,
        gate,
    }
}

fn assert_compatible_admission(
    before: &TranscriptAdmissionSnapshot,
    after: &TranscriptAdmissionSnapshot,
) {
    assert_eq!(
        after.thread.revision(),
        before.thread.revision().checked_next().unwrap()
    );
    assert_eq!(
        after.thread.committed_tail(),
        before.thread.committed_tail()
    );
    assert_eq!(
        after.thread.selected_path_digest(),
        before.thread.selected_path_digest()
    );
    assert_eq!(after.summary.thread_revision(), after.thread.revision());
    assert_eq!(
        after.summary.committed_tail(),
        before.summary.committed_tail()
    );
    assert_eq!(
        after.summary.selected_path_digest(),
        before.summary.selected_path_digest()
    );
    assert_eq!(after.summary.complete(), before.summary.complete());
    assert_eq!(after.head, before.head);
    assert_eq!(after.build, before.build);
    assert_eq!(
        after.gate.revision(),
        before.gate.revision().checked_next().unwrap()
    );
    assert_eq!(after.gate.state(), before.gate.state());
    assert_eq!(
        after.gate.accepted_high_water(),
        before.gate.accepted_high_water() + 1
    );
    assert_eq!(
        after.gate.live_next_turn_count(),
        before.gate.live_next_turn_count() + 1
    );
}

fn run_successful_terminal_cut<R>(
    seed: u8,
    process_generation: u64,
    stage: TerminalHistoryBarrierStage,
    during_pause: impl FnOnce(&crate::syndic::Fixture, crate::syndic::SubmittedTurn) -> R,
    after_release: impl FnOnce(&crate::syndic::Fixture, R),
) {
    let attempts = Arc::new(AtomicUsize::new(0));
    let provider_attempts = Arc::clone(&attempts);
    let mut fixture = crate::syndic::Fixture::new_with_scheduled_provider(seed, move |_| {
        Box::new(CountingUnavailableProvider {
            attempts: provider_attempts,
        })
    });
    let submitted = fixture.submit_text(SUBMITTED_TEXT);
    fixture.store.notify_scheduled_ordinary_execution_ready();
    wait_until("manual terminal fixture scheduler becomes idle", || {
        let diagnostics = fixture.store.accepted_input_scheduler_diagnostics();
        (attempts.load(Ordering::SeqCst) == 1
            && diagnostics.recovered_pending_execution_unavailable() == 1
            && diagnostics.workers_active() == 0)
            .then_some(())
    });

    let server = NormalTerminalServer::spawn();
    let connector =
        ManagedBackendClientConnector::for_lifecycle_test(server.endpoint(), AUTHORIZATION);
    let mut session = fixture
        .store
        .admit_lifecycle_test_candidate(
            &connector,
            crate::syndic::execution_binding().runtime_id(),
            CasProcessGeneration::new(process_generation).unwrap(),
            Path::new(crate::EXECUTION_ROOT),
            TIMEOUT,
        )
        .unwrap();
    let (coordinator, projection) = {
        let command_home = fixture.store.live_home_command().unwrap();
        let home = command_home.home();
        let coordinator = CasProjectionCoordinator::for_healthy_home(home).unwrap();
        let projection = coordinator
            .obtain_projection(
                home,
                fixture.storage,
                &mut session,
                &CasProjectionRequest::new(
                    fixture.thread,
                    fixture.selected_path(fixture.thread),
                    crate::syndic::execution_binding(),
                    ThreadStartOptions::persistent(),
                    Some(2_000_000),
                    syndic_storage::SyndicTimestamp::from_unix_millis(65_200 + u64::from(seed)),
                    TIMEOUT,
                ),
                &fixture.cancellation,
            )
            .unwrap();
        (coordinator, projection)
    };
    server.wait_for_projection();

    let execution_request = OrdinaryTurnExecutionRequest::new(TurnStartOptions::default(), TIMEOUT);
    thread::scope(|scope| {
        let mut release_guard = TerminalHistoryReleaseGuard::new(install_terminal_history_barrier(
            fixture.thread,
            stage,
        ));
        let capture = scope.spawn(|| {
            let command_home = fixture.store.live_home_command().unwrap();
            let home = command_home.home();
            let mut lifecycle = NoopLifecycle;
            let mut branch = NoopBranch;
            coordinator
                .execute_ordinary_turn(
                    home,
                    fixture.storage,
                    fixture.state.assets(),
                    projection,
                    &fixture.cancellation,
                    &execution_request,
                    OrdinaryDynamicToolHandlers::new(&mut lifecycle, &mut branch),
                )
                .unwrap()
        });
        release_guard.wait();

        let paused_gate = {
            let command_home = fixture.store.live_home_command().unwrap();
            fixture
                .storage
                .input_gate(command_home.home(), fixture.thread, point_limit())
                .unwrap()
                .unwrap()
        };
        assert_eq!(
            paused_gate.state(),
            &InputGateState::FinalizingHistory(submitted.turn)
        );
        let paused_attempts = attempts.load(Ordering::SeqCst);
        let during = during_pause(&fixture, submitted);
        thread::sleep(Duration::from_millis(100));
        assert_eq!(
            attempts.load(Ordering::SeqCst),
            paused_attempts,
            "FinalizingHistory must fence queued execution while capture is paused"
        );
        let admitted_gate = {
            let command_home = fixture.store.live_home_command().unwrap();
            fixture
                .storage
                .input_gate(command_home.home(), fixture.thread, point_limit())
                .unwrap()
                .unwrap()
        };
        assert_eq!(
            admitted_gate.state(),
            &InputGateState::FinalizingHistory(submitted.turn)
        );
        let before_release = fixture.store.accepted_input_scheduler_diagnostics();

        release_guard.release();
        let outcome = capture.join().unwrap();
        assert!(matches!(
            outcome,
            OrdinaryTurnExecutionOutcome::Terminal {
                status,
                ..
            } if status == TurnEndStatus::complete()
        ));
        let released = wait_until("successful capture releases queued successor once", || {
            let command_home = fixture.store.live_home_command().ok()?;
            let gate = fixture
                .storage
                .input_gate(command_home.home(), fixture.thread, point_limit())
                .ok()
                .flatten()?;
            let diagnostics = fixture.store.accepted_input_scheduler_diagnostics();
            (gate.state() == &InputGateState::Idle
                && attempts.load(Ordering::SeqCst) == paused_attempts + 1
                && diagnostics.next_execution_unavailable()
                    == before_release.next_execution_unavailable() + 1)
                .then_some((gate, diagnostics))
        });
        assert_eq!(
            released.0.revision(),
            admitted_gate.revision().checked_next().unwrap()
        );
        assert_eq!(
            released.1.next_execution_unavailable(),
            before_release.next_execution_unavailable() + 1
        );
        after_release(&fixture, during);
        thread::sleep(Duration::from_millis(100));
        assert_eq!(attempts.load(Ordering::SeqCst), paused_attempts + 1);
    });

    session.invalidate_connection();
    drop(session);
    server.join();
    let (directory, service) = fixture.into_service();
    service.close().unwrap();
    drop(directory);
}

#[test]
fn successful_terminal_queued_admission_preserves_completed_history_authority() {
    run_successful_terminal_cut(
        199,
        63_799,
        TerminalHistoryBarrierStage::BeforeGateRelease,
        |fixture, submitted| {
            let before = snapshot(fixture);
            assert_eq!(before.head.lifecycle(), ProjectionLifecycle::Current);
            assert_eq!(before.build.phase(), TranscriptBuildPhase::Complete);
            assert!(before.build.history_complete());
            assert!(before.summary.complete());

            let successor = {
                let command_home = fixture.store.live_home_command().unwrap();
                admit_successor(
                    command_home.home(),
                    fixture.storage,
                    fixture.thread,
                    submitted.turn,
                )
            };
            let after = snapshot(fixture);
            assert_compatible_admission(&before, &after);
            assert!(after.summary.complete());
            assert!(after.build.history_complete());
            (after, successor)
        },
        |fixture, (after, successor)| {
            let released = snapshot(fixture);
            assert_eq!(released.thread, after.thread);
            assert_eq!(released.summary, after.summary);
            assert_eq!(released.head, after.head);
            assert_eq!(released.build, after.build);
            assert!(released.summary.complete());
            assert_eq!(
                {
                    let command_home = fixture.store.live_home_command().unwrap();
                    accepted_route_state(command_home.home(), fixture.storage, &successor)
                },
                AcceptedRouteEffectiveState::NextTurn(NextTurnReason::TerminalHistory)
            );
        },
    );
}

#[test]
fn successful_terminal_queued_admission_preserves_and_resumes_active_transcript_generation() {
    run_successful_terminal_cut(
        200,
        63_800,
        TerminalHistoryBarrierStage::AfterItems,
        |fixture, submitted| {
            let (thread, head) = {
                let command_home = fixture.store.live_home_command().unwrap();
                let home = command_home.home();
                let thread = fixture
                    .storage
                    .thread(home, fixture.thread, point_limit())
                    .unwrap()
                    .unwrap();
                let head = fixture
                    .storage
                    .transcript_view_head(home, fixture.thread, point_limit())
                    .unwrap()
                    .unwrap();
                (thread, head)
            };
            assert_eq!(head.lifecycle(), ProjectionLifecycle::Stale);
            {
                let command_home = fixture.store.live_home_command().unwrap();
                command_home
                    .home()
                    .execute_current(fixture.storage.current_start_transcript_build(
                        StartTranscriptBuild::new(
                            fixture.thread,
                            thread.revision(),
                            head.revision(),
                        ),
                    ))
                    .unwrap();
            }
            let active = snapshot(fixture);
            assert!(matches!(
                active.build.phase(),
                TranscriptBuildPhase::Collecting { .. } | TranscriptBuildPhase::Publishing { .. }
            ));

            let successor = {
                let command_home = fixture.store.live_home_command().unwrap();
                admit_successor(
                    command_home.home(),
                    fixture.storage,
                    fixture.thread,
                    submitted.turn,
                )
            };
            let admitted = snapshot(fixture);
            assert_compatible_admission(&active, &admitted);
            assert_eq!(admitted.build.generation(), active.build.generation());
            (active, admitted, successor)
        },
        |fixture, (active, admitted, successor)| {
            let released = snapshot(fixture);
            assert_eq!(released.head.generation(), active.head.generation());
            assert_eq!(released.build.generation(), active.build.generation());
            assert!(released.build.revision() > active.build.revision());
            assert_eq!(released.build.phase(), TranscriptBuildPhase::Complete);
            assert_eq!(released.head.lifecycle(), ProjectionLifecycle::Current);
            assert!(released.summary.complete());
            assert_eq!(released.thread.revision(), admitted.thread.revision());
            assert_eq!(
                {
                    let command_home = fixture.store.live_home_command().unwrap();
                    accepted_route_state(command_home.home(), fixture.storage, &successor)
                },
                AcceptedRouteEffectiveState::NextTurn(NextTurnReason::TerminalHistory)
            );
        },
    );
}
