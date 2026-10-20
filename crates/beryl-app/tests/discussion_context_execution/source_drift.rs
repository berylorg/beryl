use super::*;
use beryl_app::cas_projection::test_faults::install_recovery_cursor_open_barrier;
use beryl_home_store::test_faults::{FaultController, FaultPoint};
use beryl_model::{SyndicDraftId, SyndicThreadId};
use syndic_storage::{
    CreateThread, DraftEditHistoryPolicyV1, PendingDispatchEvidence, SyndicTimestamp,
};

const FRESH_TARGET: &str = "fresh-context-after-source-advance";

#[derive(Clone, Copy)]
enum RefusalBoundary {
    CursorOpen,
    SourceCallback,
}

fn pending(fixture: &Fixture, child: SyndicThreadId) -> PendingDispatchEvidence {
    fixture
        .storage
        .pending_dispatch_evidence(&fixture.home(), child, point())
        .unwrap()
        .unwrap()
}

fn unrelated_write(fixture: &Fixture) {
    let home = fixture.home();
    command::execute_exact_contribution(
        &home,
        fixture.storage.create_thread(
            fixture.storage.revision(&home).unwrap(),
            CreateThread::ordinary(
                SyndicThreadId::from_bytes([251; 16]),
                SyndicDraftId::from_bytes([252; 16]),
                syndic::execution_binding(),
                SyndicTimestamp::from_unix_millis(100_000),
                DraftEditHistoryPolicyV1::new(65_536, 1).unwrap(),
            ),
        ),
    );
}

fn refuse_after_fork(
    fixture: &Fixture,
    session: &mut beryl_app::cas_projection::AdmittedProjectionSession,
    child: SyndicThreadId,
    boundary: RefusalBoundary,
) -> ProjectionExecutionError {
    let barrier = match boundary {
        RefusalBoundary::CursorOpen => install_recovery_cursor_open_barrier(child),
        RefusalBoundary::SourceCallback => install_recovery_source_barrier(child, 0),
    };
    std::thread::scope(|scope| {
        let worker = scope.spawn(|| obtain(fixture, session, child, 1_000_000));
        barrier.wait();
        unrelated_write(fixture);
        barrier.release();
        worker.join().unwrap().unwrap_err()
    })
}

fn assert_original_pending(before: PendingDispatchEvidence, after: PendingDispatchEvidence) {
    assert_eq!(before.turn_id(), after.turn_id());
    assert_eq!(before.item_id(), after.item_id());
    assert_eq!(before.input(), after.input());
    assert_eq!(before.gate_revision(), after.gate_revision());
    assert_eq!(before.state_revision(), after.state_revision());
    assert_eq!(before.context_owner_id(), after.context_owner_id());
}

fn assert_no_injection_or_turn(server: &WireServer) {
    let requests = server.requests();
    assert_eq!(requests.len(), 2);
    assert_eq!(requests[0]["method"], "thread/fork");
    assert_eq!(requests[1]["method"], "thread/unsubscribe");
    assert_eq!(requests[1]["params"]["threadId"], TARGET);
}

#[test]
fn cursor_source_advance_abandons_the_unused_fork_then_executes_once_on_a_fresh_target() {
    source_advance_abandons_then_executes(RefusalBoundary::CursorOpen, 210, 92_210);
}

#[test]
fn nondispatched_source_callback_drift_abandons_then_executes_once_on_a_fresh_target() {
    source_advance_abandons_then_executes(RefusalBoundary::SourceCallback, 213, 92_214);
}

fn source_advance_abandons_then_executes(boundary: RefusalBoundary, seed: u8, process: u64) {
    let mut fixture = Fixture::new(seed);
    let child = create_discussion(&mut fixture, "same immutable selected café 🌸 passage");
    let source = source_thread(&fixture, child);
    let original_frame = frame(&fixture, child);
    let before = pending(&fixture, child);
    let original_gate = fixture
        .storage
        .discussion_handoff_gate(&fixture.home(), child, point())
        .unwrap();
    let server = WireServer::spawn(vec![
        fork(source.clone()),
        Step::Unsubscribe(TARGET),
        Step::Load {
            method: "thread/fork",
            source: Some(source),
            target: FRESH_TARGET,
        },
        Step::Inject {
            target: FRESH_TARGET,
            items: vec![assistant(&original_frame)],
            reply: Reply::Success,
        },
        Step::Turn {
            target: FRESH_TARGET,
            text: " unchanged local input",
        },
    ]);
    let mut session = admit(&fixture, &server, process);
    let observer = session.recovery_replay_diagnostics_observer();
    let error = refuse_after_fork(&fixture, &mut session, child, boundary);
    let ProjectionExecutionError::ProjectionSourceAdvancedBeforeInjection {
        thread_id,
        expected,
        actual,
    } = error
    else {
        panic!("pre-injection drift did not prove successful abandonment: {error:?}");
    };
    assert_eq!(thread_id, child);
    assert_ne!(expected, actual);
    assert!(fixture.storage.revision(&fixture.home()).unwrap() > actual);
    assert_no_injection_or_turn(&server);
    assert_target_retired(&fixture, child);
    assert_original_pending(before, pending(&fixture, child));
    assert_eq!(
        fixture
            .storage
            .discussion_handoff_gate(&fixture.home(), child, point())
            .unwrap(),
        original_gate
    );
    assert_eq!(frame(&fixture, child), original_frame);
    let resources = observer.snapshot().unwrap();
    assert!(resources.released());
    assert_eq!(resources.logical_pages(), 0);
    assert_eq!(resources.logical_items(), 0);
    assert_eq!(resources.logical_utf8_bytes(), 0);
    assert!(resources.live_capacity().is_none());
    match boundary {
        RefusalBoundary::CursorOpen => assert!(resources.final_capacity().is_none()),
        RefusalBoundary::SourceCallback => {
            let capacity = resources.final_capacity().unwrap();
            let pages = capacity.pages();
            assert_eq!(pages.page_capacity, 65_536);
            assert_eq!(pages.page_count, 1);
            assert_eq!(pages.available, 1);
            assert_eq!(pages.leased, 0);
            assert_eq!(pages.high_water, 1);
            assert_eq!(pages.total_leases, 1);
            assert_eq!(pages.exhausted, 0);
            for channel in [capacity.requests(), capacity.replies()] {
                assert_eq!(channel.capacity, 1);
                assert_eq!(channel.len, 0);
                assert_eq!(channel.high_water, 1);
            }
        }
    }
    let projection = obtain(&fixture, &mut session, child, 1_000_000).unwrap();
    assert_eq!(projection.cas_thread_id().as_str(), FRESH_TARGET);
    let fresh_resources = observer.snapshot().unwrap();
    assert!(fresh_resources.released());
    assert!(fresh_resources.live_capacity().is_none());
    assert_eq!(fresh_resources.logical_items(), 1);
    assert_eq!(
        fresh_resources.logical_utf8_bytes(),
        original_frame.len() as u64
    );
    let fresh_pages = fresh_resources.final_capacity().unwrap().pages();
    assert_eq!(fresh_pages.page_capacity, 65_536);
    assert_eq!(fresh_pages.page_count, 1);
    assert_eq!(fresh_pages.high_water, 1);
    assert_eq!(fresh_pages.leased, 0);
    let mut lifecycle = UnusedLifecycle;
    let mut branch = UnusedBranch;
    let outcome = {
        let home = fixture.home();
        CasProjectionCoordinator::for_healthy_home(&home)
            .unwrap()
            .execute_ordinary_turn(
                &home,
                &fixture.storage,
                &fixture.state.assets(),
                None,
                projection,
                &fixture.cancellation,
                &OrdinaryTurnExecutionRequest::new(TurnStartOptions::default(), wire::TIMEOUT),
                OrdinaryDynamicToolHandlers::new(&mut lifecycle, &mut branch),
            )
            .unwrap()
    };
    let OrdinaryTurnExecutionOutcome::Terminal { projection, status } = outcome else {
        panic!("fresh target did not execute exact original pending input: {outcome:?}");
    };
    assert_eq!(status, syndic_storage::TurnEndStatus::complete());
    let requests = server.requests();
    let injections: Vec<_> = requests
        .iter()
        .filter(|request| request["method"] == "thread/inject_items")
        .collect();
    assert_eq!(injections.len(), 1);
    assert_eq!(injections[0]["params"]["threadId"], FRESH_TARGET);
    assert_eq!(
        injections[0]["params"]["items"],
        serde_json::json!([assistant(&original_frame)])
    );
    assert_eq!(
        requests
            .iter()
            .filter(|request| request["method"] == "turn/start")
            .count(),
        1
    );
    close(session, server);
    drop(projection);
}

#[test]
fn failed_or_uncertain_unused_fork_abandonment_never_exposes_source_progress() {
    failed_or_uncertain_abandonment(RefusalBoundary::CursorOpen, 211, 92_211);
}

#[test]
fn failed_or_uncertain_nondispatched_source_cleanup_never_exposes_source_progress() {
    failed_or_uncertain_abandonment(RefusalBoundary::SourceCallback, 214, 92_215);
}

fn failed_or_uncertain_abandonment(boundary: RefusalBoundary, seed: u8, process: u64) {
    for uncertain_publication in [false, true] {
        let faults = FaultController::new();
        let mut fixture = Fixture::with_faults(seed, faults.clone());
        let child = create_discussion(&mut fixture, "cleanup refusal source");
        let release = if uncertain_publication {
            Step::UnsubscribeAfter {
                target: TARGET,
                before_ack: Box::new(move || {
                    faults.fail_next(FaultPoint::AfterCommitBeforePersist);
                }),
            }
        } else {
            Step::UnsubscribeLost(TARGET)
        };
        let server = WireServer::spawn(vec![fork(source_thread(&fixture, child)), release]);
        let mut session = admit(
            &fixture,
            &server,
            process + u64::from(uncertain_publication),
        );
        let observer = session.recovery_replay_diagnostics_observer();
        let original_home = fixture.home();
        let error = refuse_after_fork(&fixture, &mut session, child, boundary);
        let ProjectionExecutionError::AbandonmentFailed {
            primary,
            release,
            publication,
        } = error
        else {
            panic!("failed cleanup incorrectly exposed source progress: {error:?}");
        };
        assert!(matches!(
            *primary,
            ProjectionExecutionError::ProjectionSourceAdvancedBeforeInjection { .. }
        ));
        if uncertain_publication {
            assert!(publication.is_some());
            let pending = original_home.pending_reconciliations();
            assert_eq!(pending.len(), 1);
            assert!(matches!(
                original_home.reconcile(&pending[0]).unwrap(),
                beryl_home_store::ReconciliationResolution::ExactNew { .. }
            ));
            let binding = fixture
                .storage
                .current_binding(&original_home, child, point())
                .unwrap()
                .unwrap();
            assert!(
                matches!(binding.binding().state(), BindingState::Stale(stale) if stale.cas_thread_id().as_str() == TARGET)
            );
        } else {
            assert!(release.is_some());
        }
        assert_no_injection_or_turn(&server);
        assert!(observer.snapshot().unwrap().released());
        drop(original_home);
        close(session, server);
    }
}

#[test]
fn cancellation_during_unused_fork_cleanup_suppresses_the_captured_source_advance() {
    cancellation_during_cleanup(RefusalBoundary::CursorOpen, 212, 92_213);
}

#[test]
fn cancellation_during_nondispatched_source_cleanup_suppresses_source_progress() {
    cancellation_during_cleanup(RefusalBoundary::SourceCallback, 215, 92_217);
}

fn cancellation_during_cleanup(boundary: RefusalBoundary, seed: u8, process: u64) {
    let mut fixture = Fixture::new(seed);
    let child = create_discussion(&mut fixture, "cancellation after source witness");
    let before = pending(&fixture, child);
    let cancellation = fixture.cancellation.clone();
    let server = WireServer::spawn(vec![
        fork(source_thread(&fixture, child)),
        Step::UnsubscribeAfter {
            target: TARGET,
            before_ack: Box::new(move || cancellation.cancel()),
        },
    ]);
    let mut session = admit(&fixture, &server, process);
    let error = refuse_after_fork(&fixture, &mut session, child, boundary);
    assert!(matches!(error, ProjectionExecutionError::Cancelled));
    assert_target_retired(&fixture, child);
    assert_original_pending(before, pending(&fixture, child));
    assert_no_injection_or_turn(&server);
    close(session, server);
}

#[test]
fn nondrift_source_callback_cancellation_cannot_grant_source_progress() {
    let mut fixture = Fixture::new(216);
    let child = create_discussion(&mut fixture, "cancelled source callback");
    let before = pending(&fixture, child);
    let server = WireServer::spawn(vec![
        fork(source_thread(&fixture, child)),
        Step::Unsubscribe(TARGET),
    ]);
    let mut session = admit(&fixture, &server, 92_218);
    let barrier = install_recovery_source_barrier(child, 0);
    let error = std::thread::scope(|scope| {
        let worker = scope.spawn(|| obtain(&fixture, &mut session, child, 1_000_000));
        barrier.wait();
        fixture.cancellation.cancel();
        barrier.release();
        worker.join().unwrap().unwrap_err()
    });
    assert!(matches!(
        error,
        ProjectionExecutionError::InjectionNotDispatched { source, .. }
            if matches!(source.as_ref(), ManagedBackendError::ThreadInjectionSource {
                source: ThreadInjectionSourceError::Cancelled,
                transport_bytes_written: false,
                ..
            })
    ));
    assert_no_injection_or_turn(&server);
    assert_target_retired(&fixture, child);
    assert_original_pending(before, pending(&fixture, child));
    close(session, server);
}
