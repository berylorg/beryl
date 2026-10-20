#![cfg(feature = "test-faults")]

#[path = "support/syndic_command.rs"]
mod command;
#[path = "discussion_context_execution/server.rs"]
mod server;
#[path = "discussion_context_execution/settlement.rs"]
mod settlement;
#[path = "discussion_context_execution/source_drift.rs"]
mod source_drift;
#[path = "discussion_context_execution/submission.rs"]
mod submission;
#[path = "discussion_context_execution/support.rs"]
mod support;
#[path = "projection/syndic.rs"]
mod syndic;
#[allow(dead_code)]
#[path = "normal_terminal/server.rs"]
mod wire;

use beryl_app::cas_projection::{
    CasProjectionCoordinator, OrdinaryDynamicToolHandlers, OrdinaryTurnExecutionOutcome,
    OrdinaryTurnExecutionRequest, ProjectionExecutionError,
    test_faults::install_recovery_source_barrier,
};
use beryl_backend::{ManagedBackendError, ThreadInjectionSourceError, TurnStartOptions};
use syndic_storage::{
    BindingState, CasLineageProof, NativeProjectionError, RecoveryProjectionError,
};

use server::{Reply, Step, WireServer};
use support::*;
use syndic::Fixture;

const EXECUTION_ROOT: &str = r"C:\work\beryl";
const TARGET: &str = "discussion-target";

fn fork(source: String) -> Step {
    Step::Load {
        method: "thread/fork",
        source: Some(source),
        target: TARGET,
    }
}

fn seed(items: Vec<serde_json::Value>) -> Step {
    Step::Inject {
        target: TARGET,
        items,
        reply: Reply::Success,
    }
}

#[test]
fn native_discussion_seeds_one_frame_then_executes_unchanged_user_input() {
    let mut fixture = Fixture::new(181);
    let child = create_discussion(&mut fixture, "selected source passage");
    let frame = frame(&fixture, child);
    let server = WireServer::spawn(vec![
        fork(source_thread(&fixture, child)),
        seed(vec![assistant(&frame)]),
        Step::Turn {
            target: TARGET,
            text: " unchanged local input",
        },
        Step::Turn {
            target: TARGET,
            text: " second unchanged discussion input",
        },
        Step::Load {
            method: "thread/fork",
            source: Some(TARGET.into()),
            target: "inherited-discussion",
        },
        Step::Turn {
            target: "inherited-discussion",
            text: "ordinary descendant input",
        },
    ]);
    let mut session = admit(&fixture, &server, 92_181);
    let projection = obtain(&fixture, &mut session, child, 1_000_000).unwrap();
    assert!(matches!(
        projection.lineage_proof(),
        CasLineageProof::ContextAware(_)
    ));
    let floor = projection.lineage_proof().recovered_completed_at().unwrap();
    let mut lifecycle = UnusedLifecycle;
    let mut branch = UnusedBranch;
    let home = fixture.home();
    let outcome = CasProjectionCoordinator::for_healthy_home(&home)
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
        .unwrap();
    let OrdinaryTurnExecutionOutcome::Terminal { projection, status } = outcome else {
        panic!("context execution failed: {outcome:?}")
    };
    assert_eq!(status, syndic_storage::TurnEndStatus::complete());
    assert_eq!(
        projection.lineage_proof().recovered_completed_at(),
        Some(floor)
    );
    let state = fixture
        .storage
        .turn_state(&home, fixture.selected_path(child).tail().unwrap(), point())
        .unwrap()
        .unwrap();
    let syndic_storage::TurnDispatchProvenance::Activated(anchor) = state.dispatch_provenance()
    else {
        panic!("completed user turn must retain exact activation provenance")
    };
    let snapshot = fixture
        .storage
        .execution_snapshot(&home, anchor.snapshot_id(), point())
        .unwrap()
        .unwrap();
    assert!(snapshot.started_at() >= floor);
    drop(home);
    fixture.thread = child;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64;
    fixture.advance_clock_to(now + 1);
    let projection = execute_same_discussion_continuation(&mut fixture, &mut session, projection);
    let inherited_child = fixture.create_child_pending("ordinary descendant input");
    let inherited = obtain(&fixture, &mut session, inherited_child, 1_000_000).unwrap();
    assert_eq!(inherited.cas_thread_id().as_str(), "inherited-discussion");
    let CasLineageProof::ContextAware(inherited_proof) = inherited.lineage_proof() else {
        panic!("ordinary descendant lost inherited context")
    };
    assert!(inherited_proof.injection().is_none());
    assert_eq!(inherited_proof.ordering_floor(), floor);
    assert_eq!(
        server
            .requests()
            .iter()
            .filter(|request| request["method"] == "thread/inject_items")
            .count(),
        1
    );
    let home = fixture.home();
    assert!(
        fixture
            .storage
            .thread(&home, inherited_child, point())
            .unwrap()
            .unwrap()
            .context_owner_id()
            .is_none()
    );
    let inherited_outcome = CasProjectionCoordinator::for_healthy_home(&home)
        .unwrap()
        .execute_ordinary_turn(
            &home,
            &fixture.storage,
            &fixture.state.assets(),
            None,
            inherited,
            &fixture.cancellation,
            &OrdinaryTurnExecutionRequest::new(TurnStartOptions::default(), wire::TIMEOUT),
            OrdinaryDynamicToolHandlers::new(&mut lifecycle, &mut branch),
        )
        .unwrap();
    let OrdinaryTurnExecutionOutcome::Terminal {
        projection: inherited,
        status,
    } = inherited_outcome
    else {
        panic!("inherited ordinary execution failed: {inherited_outcome:?}")
    };
    assert_eq!(status, syndic_storage::TurnEndStatus::complete());
    assert_eq!(
        inherited.lineage_proof().recovered_completed_at(),
        Some(floor)
    );
    drop(home);
    close(session, server);
    drop((projection, inherited));
}

#[test]
fn maximum_context_seed_crosses_fixed_pages_with_one_resident_page() {
    let mut fixture = Fixture::new(182);
    let child = create_code_discussion(&mut fixture, &"🦀".repeat(16_384));
    let expected = frame(&fixture, child);
    assert_eq!(expected.len(), 65_997);
    let server = WireServer::spawn(vec![
        fork(source_thread(&fixture, child)),
        seed(vec![assistant(&expected)]),
    ]);
    let mut session = admit(&fixture, &server, 92_182);
    let observer = session.recovery_replay_diagnostics_observer();
    let projection = obtain(&fixture, &mut session, child, 131_994).unwrap();
    let snapshot = observer.snapshot().unwrap();
    assert_eq!(snapshot.logical_items(), 1);
    assert_eq!(snapshot.logical_pages(), 2);
    assert_eq!(snapshot.logical_utf8_bytes(), 65_997);
    assert!(snapshot.released());
    let pages = snapshot.final_capacity().unwrap().pages();
    assert_eq!(
        (
            pages.page_count,
            pages.page_capacity,
            pages.high_water,
            pages.leased
        ),
        (1, 65_536, 1, 0)
    );
    close(session, server);
    drop(projection);
}

#[test]
fn dropped_local_context_lease_resumes_same_process_without_reinjection() {
    let mut fixture = Fixture::new(183);
    let child = create_discussion(&mut fixture, "resume source");
    let expected = frame(&fixture, child);
    let server = WireServer::spawn(vec![
        fork(source_thread(&fixture, child)),
        seed(vec![assistant(&expected)]),
        Step::Unsubscribe(TARGET),
        Step::Load {
            method: "thread/resume",
            source: Some(TARGET.into()),
            target: TARGET,
        },
    ]);
    let mut session = admit(&fixture, &server, 92_183);
    let first = obtain(&fixture, &mut session, child, 1_000_000).unwrap();
    let original = first.lineage_proof();
    let generation = first.loaded_session_generation();
    first.release().unwrap();
    let resumed = obtain(&fixture, &mut session, child, 1_000_000).unwrap();
    assert_eq!(resumed.lineage_proof(), original);
    assert_ne!(resumed.loaded_session_generation(), generation);
    assert_eq!(
        resumed.loaded_session_generation().process(),
        generation.process()
    );
    let reused = obtain(&fixture, &mut session, child, 1_000_000).unwrap();
    assert_eq!(
        reused.loaded_session_generation(),
        resumed.loaded_session_generation()
    );
    close(session, server);
    drop((resumed, reused));
}

#[test]
fn a_new_process_replays_canonical_prefix_and_context_into_a_new_target() {
    let mut fixture = Fixture::new(184);
    let child = create_discussion(&mut fixture, "process source");
    let expected = frame(&fixture, child);
    let server = WireServer::spawn(vec![
        fork(source_thread(&fixture, child)),
        seed(vec![assistant(&expected)]),
    ]);
    let mut old_session = admit(&fixture, &server, 92_184);
    let old = obtain(&fixture, &mut old_session, child, 1_000_000).unwrap();
    close(old_session, server);
    drop(old);
    fixture.submit_text("unrelated parent pending");
    fixture.retire_current_binding(fixture.thread);
    let server = WireServer::spawn(vec![
        Step::Load {
            method: "thread/start",
            source: None,
            target: "replayed-context",
        },
        Step::Inject {
            target: "replayed-context",
            items: vec![
                user("source question"),
                assistant("process source"),
                assistant(&expected),
            ],
            reply: Reply::Success,
        },
    ]);
    let mut new_session = admit(&fixture, &server, 92_185);
    let projection = obtain(&fixture, &mut new_session, child, 1_000_000).unwrap();
    assert_eq!(projection.cas_thread_id().as_str(), "replayed-context");
    assert_eq!(
        projection
            .lineage_proof()
            .recovered_injection_generation()
            .unwrap()
            .process(),
        projection.loaded_session_generation().process()
    );
    assert_eq!(
        server
            .requests()
            .iter()
            .filter(|request| request["method"] == "thread/resume")
            .count(),
        0
    );
    close(new_session, server);
    drop(projection);
}

#[test]
fn seed_budget_refusal_precedes_any_fork_or_injection() {
    let mut fixture = Fixture::new(185);
    let child = create_discussion(&mut fixture, "budget source");
    let server = WireServer::spawn(Vec::new());
    let mut session = admit(&fixture, &server, 92_186);
    let error = obtain(&fixture, &mut session, child, 100).unwrap_err();
    assert!(matches!(
        error,
        ProjectionExecutionError::RecoveryProjection(
            RecoveryProjectionError::BudgetOverflow { .. }
        ) | ProjectionExecutionError::NativePlanning(NativeProjectionError::Context(
            RecoveryProjectionError::BudgetOverflow { .. }
        ))
    ));
    assert!(server.requests().is_empty());
    close(session, server);
}

#[test]
fn seed_cancelled_before_transport_retires_consumed_target_and_releases_page() {
    let mut fixture = Fixture::new(186);
    let child = create_discussion(&mut fixture, "cancel source");
    let server = WireServer::spawn(vec![
        fork(source_thread(&fixture, child)),
        Step::Unsubscribe(TARGET),
    ]);
    let mut session = admit(&fixture, &server, 92_187);
    let observer = session.recovery_replay_diagnostics_observer();
    let barrier = install_recovery_source_barrier(child, 0);
    let error = std::thread::scope(|scope| {
        let worker = scope.spawn(|| obtain(&fixture, &mut session, child, 1_000_000));
        barrier.wait();
        fixture.cancellation.cancel();
        barrier.release();
        worker.join().unwrap().unwrap_err()
    });
    assert!(
        matches!(error,ProjectionExecutionError::InjectionNotDispatched{source,..}
        if matches!(*source,ManagedBackendError::ThreadInjectionSource{source:ThreadInjectionSourceError::Cancelled,transport_bytes_written:false,..}))
    );
    let home = fixture.home();
    assert!(matches!(
        fixture
            .storage
            .current_binding(&home, child, point())
            .unwrap()
            .unwrap()
            .binding()
            .state(),
        BindingState::Stale(_)
    ));
    drop(home);
    let snapshot = observer.snapshot().unwrap();
    assert!(snapshot.released());
    assert_eq!(snapshot.logical_items(), 0);
    assert_eq!(snapshot.final_capacity().unwrap().pages().leased, 0);
    close(session, server);
}

#[test]
fn rejected_seed_retires_target_and_retry_uses_a_different_idle_thread() {
    let mut fixture = Fixture::new(187);
    let child = create_discussion(&mut fixture, "rejected source");
    let source = source_thread(&fixture, child);
    let frame = frame(&fixture, child);
    let server = WireServer::spawn(vec![
        fork(source.clone()),
        Step::Inject {
            target: TARGET,
            items: vec![assistant(&frame)],
            reply: Reply::Rejected,
        },
        Step::Unsubscribe(TARGET),
        Step::Load {
            method: "thread/fork",
            source: Some(source),
            target: "context-retry",
        },
        Step::Inject {
            target: "context-retry",
            items: vec![assistant(&frame)],
            reply: Reply::Success,
        },
    ]);
    let mut session = admit(&fixture, &server, 92_188);
    let error = obtain(&fixture, &mut session, child, 1_000_000).unwrap_err();
    assert!(
        matches!(
            &error,
            ProjectionExecutionError::InjectionRejected { code: -32602, .. }
        ),
        "exact injection rejection classification: {error:?}"
    );
    assert_target_retired(&fixture, child);
    let projection = obtain(&fixture, &mut session, child, 1_000_000).unwrap();
    assert_eq!(projection.cas_thread_id().as_str(), "context-retry");
    let requests = server.requests();
    let targets = requests
        .iter()
        .filter(|request| request["method"] == "thread/inject_items")
        .map(|request| request["params"]["threadId"].as_str().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(targets, vec![TARGET, "context-retry"]);
    close(session, server);
    drop(projection);
}

#[test]
fn lost_seed_completion_keeps_uncertainty_and_never_publishes_a_usable_binding() {
    let mut fixture = Fixture::new(188);
    let child = create_discussion(&mut fixture, "uncertain source");
    let server = WireServer::spawn(vec![
        fork(source_thread(&fixture, child)),
        Step::Inject {
            target: TARGET,
            items: vec![assistant(&frame(&fixture, child))],
            reply: Reply::Lost,
        },
    ]);
    let mut session = admit(&fixture, &server, 92_189);
    let error = obtain(&fixture, &mut session, child, 1_000_000).unwrap_err();
    let primary = match &error {
        ProjectionExecutionError::AbandonmentFailed { primary, .. } => primary.as_ref(),
        error => error,
    };
    assert!(
        matches!(
            primary,
            ProjectionExecutionError::InjectionTransportLost { .. }
                | ProjectionExecutionError::InjectionCompletionUnknown { .. }
        ),
        "original seed uncertainty must survive cleanup: {error:?}"
    );
    assert_target_retired(&fixture, child);
    assert_eq!(
        server
            .requests()
            .iter()
            .filter(|request| request["method"] == "thread/inject_items")
            .count(),
        1
    );
    close(session, server);
}

fn assert_target_retired(fixture: &Fixture, child: beryl_model::SyndicThreadId) {
    let home = fixture.home();
    let binding = fixture
        .storage
        .current_binding(&home, child, point())
        .unwrap()
        .unwrap();
    let BindingState::Stale(stale) = binding.binding().state() else {
        panic!("consumed idle target remained usable")
    };
    assert_eq!(stale.cas_thread_id().as_str(), TARGET);
    let owner = fixture
        .storage
        .cas_thread_owner(&home, stale.cas_thread_id().clone(), point())
        .unwrap()
        .unwrap();
    assert!(owner.retired_binding_revision().is_some());
}

struct UnusedLifecycle;
impl beryl_app::LifecycleYieldRequestHandler for UnusedLifecycle {
    fn respond_lifecycle_yield(
        &mut self,
        _: beryl_app::cas_projection::OrdinaryDynamicToolContext,
        _: beryl_app::LifecycleYieldRequest,
    ) -> beryl_backend::DynamicToolCallResponse {
        panic!("context execution must not invoke lifecycle tools")
    }
}
struct UnusedBranch;
impl beryl_app::BranchDiscussionResolutionRequestHandler for UnusedBranch {
    fn respond_branch_discussion_resolution(
        &mut self,
        _: beryl_app::cas_projection::BranchDiscussionResolutionContext,
        _: beryl_app::BranchDiscussionResolutionRequest,
    ) -> beryl_backend::DynamicToolCallResponse {
        panic!("context execution must not invoke resolution tools")
    }
}
