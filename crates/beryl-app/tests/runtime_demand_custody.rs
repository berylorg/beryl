#![cfg(all(feature = "test-faults", target_os = "windows"))]

#[allow(dead_code)]
#[path = "support/managed_runtime.rs"]
mod support;

#[allow(dead_code)]
#[path = "runtime_session_preparation/submission.rs"]
mod submission;

use std::fs;

use beryl_app::cas_projection::{
    AdmittedProjectionSession, CasProjectionCoordinator, CasProjectionRequest, LoadedCasProjection,
    OrdinaryDynamicToolAuthority, OrdinaryDynamicToolHandlers, OrdinaryTurnExecutionRequest,
    ProcessScheduledExecutionProvider, ProjectionCancellationToken,
    ProjectionConnectionServiceCloseOutcome, RuntimeInterestKind, ScheduledOrdinaryRequestPolicy,
};
use beryl_backend::{ThreadStartOptions, TurnStartOptions};
use beryl_home_store::{CommandOutcome, HomeCommand};
use beryl_model::{ExecutionBinding, RuntimeId, SyndicDraftId, SyndicThreadId};
use support::{Fixture, ProcessWitness, TIMEOUT, ready, wait_until};
use syndic_storage::{
    CreateThread, DraftEditHistoryPolicyV1, SelectedPathProof, SyndicPointReadLimit,
    SyndicTimestamp,
};

fn projection(
    fixture: &Fixture,
    session: &mut AdmittedProjectionSession,
    binding: ExecutionBinding,
) -> LoadedCasProjection {
    let live = fixture.service().live_home_command().unwrap();
    let home = live.home();
    let thread = SyndicThreadId::from_bytes([177; 16]);
    let mut command = HomeCommand::new(home.home_revision().unwrap());
    command
        .add(fixture.storage.create_thread(
            fixture.storage.revision(home).unwrap(),
            CreateThread::ordinary(
                thread,
                SyndicDraftId::from_bytes([178; 16]),
                binding.clone(),
                SyndicTimestamp::from_unix_millis(1),
                DraftEditHistoryPolicyV1::new(64 * 1024 * 1024, 1).unwrap(),
            ),
        ))
        .unwrap();
    assert!(matches!(
        home.execute(command),
        CommandOutcome::Committed {
            later_failure: None,
            ..
        }
    ));
    submission::submit(fixture, thread);
    let current = fixture
        .storage
        .thread(home, thread, SyndicPointReadLimit::new(1_000_000).unwrap())
        .unwrap()
        .unwrap();
    let request = CasProjectionRequest::new(
        thread,
        SelectedPathProof::new(
            current.committed_tail(),
            current.revision(),
            current.selected_path_digest(),
        ),
        binding,
        ThreadStartOptions::persistent(),
        Some(2_000_000),
        SyndicTimestamp::from_unix_millis(2),
        TIMEOUT,
    );
    CasProjectionCoordinator::for_healthy_home(home)
        .unwrap()
        .obtain_projection(
            home,
            &fixture.storage,
            session,
            &request,
            &ProjectionCancellationToken::new(),
        )
        .unwrap()
}

fn interest_count(fixture: &Fixture, kind: RuntimeInterestKind) -> usize {
    fixture
        .service()
        .runtime_interest_count_for_test(RuntimeId::from_bytes([99; 16]), kind)
}

fn finish(mut fixture: Fixture, process: ProcessWitness) {
    wait_until(|| {
        fixture.token_count() == 0 && fixture.service().worker_pool_diagnostics().active() == 0
    });
    process.assert_exited();
    assert_eq!(
        interest_count(&fixture, RuntimeInterestKind::RequiredWork),
        0
    );
    assert!(matches!(
        fixture.service.take().unwrap().close().unwrap(),
        ProjectionConnectionServiceCloseOutcome::Closed
    ));
}

#[test]
fn loaded_projection_preserves_the_same_runtime_after_session_and_view_release() {
    let fixture = Fixture::new();
    fs::write(fixture.root(1).join("fixture-mode"), "projection-lifetime").unwrap();
    let view = fixture.acquire(1, RuntimeInterestKind::View).unwrap();
    let initial = ready(&view);
    let process = ProcessWitness::open(fixture.evidence(1)["pid"].as_u64().unwrap() as u32);
    let work = fixture
        .acquire(2, RuntimeInterestKind::RequiredWork)
        .unwrap();
    let binding = work.binding().clone();
    let mut session = fixture
        .service()
        .admit_runtime_session(work, TIMEOUT)
        .unwrap();
    let retired = session.connection_retirement_handle_for_test();
    let loaded = projection(&fixture, &mut session, binding);
    assert_eq!(
        interest_count(&fixture, RuntimeInterestKind::RequiredWork),
        1
    );
    drop(session);
    drop(view);

    assert_eq!(interest_count(&fixture, RuntimeInterestKind::View), 0);
    assert_eq!(
        interest_count(&fixture, RuntimeInterestKind::RequiredWork),
        1
    );
    assert!(loaded.is_live().unwrap());
    assert!(process.running());
    let reattached = fixture.acquire(2, RuntimeInterestKind::View).unwrap();
    assert_eq!(ready(&reattached), initial);
    assert!(loaded.is_live().unwrap());
    drop(reattached);
    drop(loaded);
    finish(fixture, process);
    assert!(retired.is_detached());
}

struct NoToolInvocation;

impl OrdinaryDynamicToolAuthority for NoToolInvocation {
    fn handlers(&mut self) -> OrdinaryDynamicToolHandlers<'_> {
        panic!("runtime custody verification does not dispatch tools")
    }
}

#[test]
fn idle_retirement_preserves_a_loaded_projection_and_reclaims_the_final_runtime() {
    let (provider, sessions) = ProcessScheduledExecutionProvider::new();
    let fixture = Fixture::with_provider(Box::new(provider));
    fs::write(fixture.root(1).join("fixture-mode"), "projection-lifetime").unwrap();
    let view = fixture.acquire(1, RuntimeInterestKind::View).unwrap();
    let initial = ready(&view);
    let process = ProcessWitness::open(fixture.evidence(1)["pid"].as_u64().unwrap() as u32);
    let work = fixture
        .acquire(2, RuntimeInterestKind::RequiredWork)
        .unwrap();
    let binding = work.binding().clone();
    let mut session = fixture
        .service()
        .admit_runtime_session(work, TIMEOUT)
        .unwrap();
    let retired = session.connection_retirement_handle_for_test();
    let thread = SyndicThreadId::from_bytes([177; 16]);
    let loaded = projection(&fixture, &mut session, binding.clone());
    let flight = fixture
        .service()
        .hold_scheduled_flight_for_test(thread)
        .unwrap();
    let registration = sessions
        .register(
            thread,
            binding.clone(),
            session,
            ScheduledOrdinaryRequestPolicy::new(
                ThreadStartOptions::persistent(),
                Some(2_000_000),
                TIMEOUT,
                OrdinaryTurnExecutionRequest::new(TurnStartOptions::default(), TIMEOUT),
            ),
            fixture.state.assets(),
            Box::new(NoToolInvocation),
        )
        .unwrap();
    drop(view);
    assert!(!sessions.retire_if_idle(registration).unwrap());
    assert_eq!(sessions.diagnostics().available, 1);
    assert!(loaded.is_live().unwrap());
    let reattached = fixture.acquire(2, RuntimeInterestKind::View).unwrap();
    assert_eq!(ready(&reattached), initial);
    drop(reattached);
    loaded.release().unwrap();
    assert!(!retired.is_retired());
    assert_eq!(sessions.diagnostics().available, 1);
    wait_until(|| sessions.retire_if_idle(registration).unwrap());
    drop(flight);
    finish(fixture, process);
    assert!(retired.is_detached());
    assert_eq!(sessions.diagnostics().retained, 0);
}

#[test]
fn retained_worker_custody_preserves_demand_after_connection_workers_join() {
    let fixture = Fixture::new();
    let view = fixture.acquire(1, RuntimeInterestKind::View).unwrap();
    let initial = ready(&view);
    let process = ProcessWitness::open(fixture.evidence(1)["pid"].as_u64().unwrap() as u32);
    let work = fixture
        .acquire(2, RuntimeInterestKind::RequiredWork)
        .unwrap();
    let session = fixture
        .service()
        .admit_runtime_session(work, TIMEOUT)
        .unwrap();
    let retired = session.connection_retirement_handle_for_test();
    let custody = retired.retain_worker_custody();
    drop(session);
    drop(view);
    wait_until(|| retired.is_detached());

    assert_eq!(interest_count(&fixture, RuntimeInterestKind::View), 0);
    assert_eq!(
        interest_count(&fixture, RuntimeInterestKind::RequiredWork),
        1
    );
    assert_eq!(fixture.service().worker_pool_diagnostics().active(), 4);
    assert!(process.running());
    let reattached = fixture.acquire(2, RuntimeInterestKind::View).unwrap();
    assert_eq!(ready(&reattached), initial);
    drop(reattached);
    drop(custody);
    finish(fixture, process);
}
