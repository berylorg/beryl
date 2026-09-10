#![cfg(all(feature = "test-faults", target_os = "windows"))]
#[path = "runtime_session_preparation/idle_maintenance.rs"]
mod idle_maintenance;
#[path = "runtime_session_preparation/work_facts.rs"]
mod work_facts;

#[allow(dead_code)]
#[path = "runtime_session_preparation/submission.rs"]
mod submission;
#[allow(dead_code)]
#[path = "support/managed_runtime.rs"]
mod support;

use std::{fs, path::Path, sync::Arc, thread, time::Duration};

use beryl_app::{
    cas_projection::{
        OrdinaryTurnExecutionRequest, ProcessScheduledExecutionProvider,
        ProjectionConnectionServiceCloseOutcome, RuntimeSessionPreparationConfig,
        RuntimeTokenDirectories, ScheduledExecutionSessions, ScheduledOrdinaryAdmissionResult,
        ScheduledOrdinaryRequestPolicy,
    },
    lifecycle_attention::ProcessLifecycleAttentionPool,
};
use beryl_backend::{ThreadStartOptions, TurnStartOptions};
use beryl_home_store::{CommandOutcome, HomeCommand};
use beryl_model::{
    AdmittedHostPath, ExecutionBinding, PathFlavor, RootId, RuntimeId, RuntimeMode, SyndicDraftId,
    SyndicThreadId,
};
use beryl_state::{
    AddConfiguredRoot, AvailabilitySnapshot, CreateRuntimeWithHomeRoot, RootRegistration,
    RuntimeRegistration, UnixMillis,
};
use support::{Fixture, ProcessWitness, TIMEOUT, canonical_path, native, wait_until};
use syndic_storage::{CreateThread, DraftEditHistoryPolicyV1, SyndicTimestamp};

fn host(path: &str) -> AdmittedHostPath {
    AdmittedHostPath::from_admitted(PathFlavor::Windows, path).unwrap()
}

fn thread_id(root: u8) -> SyndicThreadId {
    SyndicThreadId::from_bytes([100 + root; 16])
}

fn binding(fixture: &Fixture, root: u8) -> ExecutionBinding {
    ExecutionBinding::new(
        RuntimeId::from_bytes([99; 16]),
        RootId::from_bytes([root; 16]),
        native(&canonical_path(&fixture.root(root))),
    )
}

fn fixture(
    capacity: u64,
) -> (
    Fixture,
    ScheduledExecutionSessions,
    Arc<ProcessLifecycleAttentionPool>,
) {
    let (provider, sessions) = ProcessScheduledExecutionProvider::new();
    let fixture = Fixture::with_capacity(Box::new(provider), capacity);
    let live = fixture.service().live_home_command().unwrap();
    let home = live.home();
    let executable = canonical_path(Path::new(env!("CARGO_BIN_EXE_managed-runtime-fixture")));
    let runtime = RuntimeRegistration::new(
        RuntimeId::from_bytes([99; 16]),
        host(&executable),
        RuntimeMode::Host,
        native(&executable),
        UnixMillis::new(1),
        AvailabilitySnapshot::unknown(),
    )
    .unwrap();
    for root in 1..=2 {
        let root_path = canonical_path(&fixture.root(root));
        let registration = RootRegistration::new(
            RootId::from_bytes([root; 16]),
            native(&root_path),
            host(&root_path),
            UnixMillis::new(1),
            AvailabilitySnapshot::unknown(),
        );
        let mut command = HomeCommand::new(home.home_revision().unwrap());
        if root == 1 {
            command
                .add(fixture.state.runtime_roots().create_runtime_with_home_root(
                    fixture.state.runtime_roots().revision(home).unwrap(),
                    CreateRuntimeWithHomeRoot::new(runtime.clone(), registration).unwrap(),
                ))
                .unwrap();
        } else {
            command
                .add(fixture.state.runtime_roots().add_root(
                    fixture.state.runtime_roots().revision(home).unwrap(),
                    AddConfiguredRoot::new(RuntimeId::from_bytes([99; 16]), registration),
                ))
                .unwrap();
        }
        assert!(matches!(
            home.execute(command),
            CommandOutcome::Committed {
                later_failure: None,
                ..
            }
        ));
        let mut command = HomeCommand::new(home.home_revision().unwrap());
        command
            .add(fixture.storage.create_thread(
                fixture.storage.revision(home).unwrap(),
                CreateThread::ordinary(
                    thread_id(root),
                    SyndicDraftId::from_bytes([110 + root; 16]),
                    binding(&fixture, root),
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
    }
    drop(live);
    let attention = Arc::new(ProcessLifecycleAttentionPool::new());
    configure(&fixture, &sessions, &attention);
    (fixture, sessions, attention)
}

fn configure(
    fixture: &Fixture,
    sessions: &ScheduledExecutionSessions,
    attention: &Arc<ProcessLifecycleAttentionPool>,
) {
    let tokens = canonical_path(&fixture.tokens());
    fixture
        .service()
        .configure_runtime_session_preparation(
            &sessions,
            RuntimeSessionPreparationConfig {
                runtime_roots: fixture.state.runtime_roots(),
                assets: fixture.state.assets(),
                policy: ScheduledOrdinaryRequestPolicy::new(
                    ThreadStartOptions::persistent(),
                    Some(2_000_000),
                    TIMEOUT,
                    OrdinaryTurnExecutionRequest::new(TurnStartOptions::default(), TIMEOUT),
                ),
                token_directories: vec![RuntimeTokenDirectories {
                    runtime_id: RuntimeId::from_bytes([99; 16]),
                    host: host(&tokens),
                    runtime: native(&tokens),
                }],
            },
            &attention,
        )
        .unwrap();
}

fn begin(fixture: &Fixture, root: u8) {
    assert!(matches!(
        fixture
            .service()
            .checkout_scheduled_session_for_test(thread_id(root), binding(fixture, root))
            .unwrap(),
        ScheduledOrdinaryAdmissionResult::Unavailable(_)
    ));
}

fn checkout(
    fixture: &Fixture,
    root: u8,
) -> beryl_app::cas_projection::ScheduledOrdinaryExecutionLease {
    use beryl_app::cas_projection::{ProjectionCoordinatorError, ScheduledOrdinaryAdmissionError};
    let deadline = std::time::Instant::now() + TIMEOUT;
    loop {
        match fixture
            .service()
            .checkout_scheduled_session_for_test(thread_id(root), binding(fixture, root))
        {
            Ok(ScheduledOrdinaryAdmissionResult::Issued(lease)) => return lease,
            Err(ScheduledOrdinaryAdmissionError::Authority(
                ProjectionCoordinatorError::ProjectionWorkerCapacityFull { .. }
                | ProjectionCoordinatorError::ProjectionInFlight { .. },
            )) => {}
            Ok(ScheduledOrdinaryAdmissionResult::Unavailable(_)) => {}
            Err(error) => panic!("prepared checkout lost authority: {error}"),
        }
        assert!(
            std::time::Instant::now() < deadline,
            "prepared session did not become eligible for checkout"
        );
        thread::sleep(Duration::from_millis(2));
    }
}

fn close(fixture: &mut Fixture, sessions: &ScheduledExecutionSessions) {
    assert!(matches!(
        fixture.service.take().unwrap().close().unwrap(),
        ProjectionConnectionServiceCloseOutcome::Closed
    ));
    assert_eq!(sessions.diagnostics().retained, 0);
    assert_eq!(fixture.token_count(), 0);
}

#[test]
fn eligible_candidate_launches_and_registers_a_real_managed_session() {
    let (mut fixture, sessions, _attention) = fixture(8);
    assert_eq!(fixture.token_count(), 0);
    fs::write(fixture.root(1).join("fixture-mode"), "pause-config").unwrap();
    begin(&fixture, 1);
    wait_until(|| fixture.root(1).join("runtime-evidence.json").exists());
    let view = fixture
        .acquire(1, beryl_app::cas_projection::RuntimeInterestKind::View)
        .unwrap();
    fs::write(fixture.root(1).join("release-config"), "ready").unwrap();
    wait_until(|| sessions.diagnostics().available == 1);
    let process = ProcessWitness::open(fixture.evidence(1)["pid"].as_u64().unwrap() as u32);
    let lease = checkout(&fixture, 1);
    assert_eq!(lease.execution_binding(), &binding(&fixture, 1));
    assert_eq!(sessions.diagnostics().checked_out, 1);
    assert!(process.running());
    wait_until(|| fixture.service().worker_pool_diagnostics().active() == 5);
    drop(lease);
    drop(view);
    close(&mut fixture, &sessions);
    process.assert_exited();
}

#[test]
fn competing_exact_roots_coalesce_one_starting_runtime() {
    let (mut fixture, sessions, _attention) = fixture(10);
    fs::write(fixture.root(1).join("fixture-mode"), "pause-config").unwrap();
    begin(&fixture, 1);
    wait_until(|| fixture.root(1).join("runtime-evidence.json").exists());
    begin(&fixture, 2);
    wait_until(|| fixture.service().worker_pool_diagnostics().active() == 8);
    assert!(!fixture.root(2).join("runtime-evidence.json").exists());
    let views = [
        fixture
            .acquire(1, beryl_app::cas_projection::RuntimeInterestKind::View)
            .unwrap(),
        fixture
            .acquire(2, beryl_app::cas_projection::RuntimeInterestKind::View)
            .unwrap(),
    ];
    fs::write(fixture.root(1).join("release-config"), "ready").unwrap();
    wait_until(|| sessions.diagnostics().available == 2);
    let first = checkout(&fixture, 1);
    let second = checkout(&fixture, 2);
    assert_eq!(first.process_generation(), second.process_generation());
    assert_eq!(second.execution_binding(), &binding(&fixture, 2));
    drop((first, second));
    drop(views);
    close(&mut fixture, &sessions);
}

#[test]
fn failure_stays_unavailable_until_exact_explicit_retry() {
    let (mut fixture, sessions, _attention) = fixture(6);
    let runtime_id = binding(&fixture, 1).runtime_id();
    fs::write(fixture.root(1).join("fixture-mode"), "reject-config").unwrap();
    begin(&fixture, 1);
    wait_until(|| {
        sessions
            .runtime_failure(runtime_id)
            .is_some_and(|failure| failure.retry_ready())
    });
    wait_until(|| fixture.service().worker_pool_diagnostics().active() == 0);
    let failure = sessions.runtime_failure(runtime_id).unwrap();
    let failed_pid = fixture.evidence(1)["pid"].clone();
    fs::write(fixture.root(1).join("fixture-mode"), "").unwrap();
    for _ in 0..3 {
        begin(&fixture, 1);
        wait_until(|| fixture.service().worker_pool_diagnostics().active() == 0);
    }
    assert_eq!(fixture.evidence(1)["pid"], failed_pid);
    assert_eq!(fixture.token_count(), 0);
    assert!(
        sessions
            .retry_runtime_session(failure, thread_id(2), binding(&fixture, 1))
            .is_err()
    );
    sessions
        .retry_runtime_session(failure, thread_id(1), binding(&fixture, 1))
        .unwrap();
    assert!(!sessions.runtime_failure(runtime_id).unwrap().retry_ready());
    let release_worker = fixture
        .service()
        .reserve_scheduled_ordinary_worker_for_test();
    begin(&fixture, 1);
    wait_until(|| fixture.service().worker_pool_diagnostics().active() == 1);
    assert_eq!(fixture.evidence(1)["pid"], failed_pid);
    assert!(!sessions.runtime_failure(runtime_id).unwrap().retry_ready());
    release_worker();
    begin(&fixture, 2);
    wait_until(|| fixture.service().worker_pool_diagnostics().active() == 0);
    assert!(!fixture.root(2).join("runtime-evidence.json").exists());
    fs::write(fixture.root(1).join("fixture-mode"), "pause-config").unwrap();
    begin(&fixture, 1);
    wait_until(|| fixture.evidence(1)["pid"] != failed_pid);
    let view = fixture
        .acquire(1, beryl_app::cas_projection::RuntimeInterestKind::View)
        .unwrap();
    fs::write(fixture.root(1).join("release-config"), "ready").unwrap();
    wait_until(|| sessions.diagnostics().available == 1);
    drop(checkout(&fixture, 1));
    assert!(
        sessions
            .retry_runtime_session(failure, thread_id(1), binding(&fixture, 1))
            .is_err()
    );
    drop(view);
    close(&mut fixture, &sessions);
}

#[test]
fn insufficient_cold_capacity_stays_quiet_without_launch_or_failure() {
    let (mut fixture, sessions, _attention) = fixture(5);
    begin(&fixture, 1);
    wait_until(|| fixture.service().worker_pool_diagnostics().active() == 0);
    let before = fixture.service().worker_pool_diagnostics();
    thread::sleep(Duration::from_millis(150));
    assert_eq!(fixture.service().worker_pool_diagnostics(), before);
    assert_eq!(fixture.token_count(), 0);
    assert!(!fixture.root(1).join("runtime-evidence.json").exists());
    assert!(
        sessions
            .runtime_failure(binding(&fixture, 1).runtime_id())
            .is_none()
    );
    close(&mut fixture, &sessions);
}

#[test]
fn shutdown_joins_preparation_and_rejects_late_registration() {
    let (mut fixture, sessions, _attention) = fixture(8);
    fs::write(fixture.root(1).join("fixture-mode"), "pause-config").unwrap();
    begin(&fixture, 1);
    wait_until(|| fixture.root(1).join("runtime-evidence.json").exists());
    let process = ProcessWitness::open(fixture.evidence(1)["pid"].as_u64().unwrap() as u32);
    let service = fixture.service.take().unwrap();
    let closed = thread::spawn(move || service.close());
    wait_until(|| sessions.diagnostics().closed);
    fs::write(fixture.root(1).join("release-config"), "ready").unwrap();
    assert!(matches!(
        closed.join().unwrap().unwrap(),
        ProjectionConnectionServiceCloseOutcome::Closed
    ));
    assert_eq!(sessions.diagnostics().retained, 0);
    assert_eq!(fixture.token_count(), 0);
    process.assert_exited();
}

#[test]
fn recovered_durable_submission_waits_quietly_for_capacity_then_checks_out() {
    let (mut fixture, old_sessions, attention) = fixture(8);
    submission::submit(&fixture, thread_id(1));
    fs::write(fixture.root(1).join("fixture-mode"), "pause-projection").unwrap();
    let (provider, sessions) = ProcessScheduledExecutionProvider::new();
    fixture.reopen(Box::new(provider), 6);
    assert!(old_sessions.diagnostics().closed);
    let release_worker = fixture
        .service()
        .reserve_scheduled_ordinary_worker_for_test();
    configure(&fixture, &sessions, &attention);
    wait_until(|| {
        fixture.service().worker_pool_diagnostics().denied_pairs() > 0
            && fixture.service().worker_pool_diagnostics().active() == 1
    });
    let before = fixture.service().worker_pool_diagnostics();
    thread::sleep(Duration::from_millis(150));
    assert_eq!(fixture.service().worker_pool_diagnostics(), before);
    assert!(!fixture.root(1).join("runtime-evidence.json").exists());
    release_worker();
    wait_until(|| {
        fixture
            .root(1)
            .join("runtime-projection-evidence.json")
            .exists()
    });
    assert_eq!(sessions.diagnostics().checked_out, 1);
    let evidence: serde_json::Value = serde_json::from_slice(
        &fs::read(fixture.root(1).join("runtime-projection-evidence.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(evidence["method"], "thread/start");
    assert_eq!(evidence["cwd"], binding(&fixture, 1).root_path().as_str());
    let service = fixture.service.take().unwrap();
    let closed = thread::spawn(move || service.close());
    wait_until(|| sessions.diagnostics().closed);
    fs::write(fixture.root(1).join("release-projection"), "complete").unwrap();
    assert!(matches!(
        closed.join().unwrap().unwrap(),
        ProjectionConnectionServiceCloseOutcome::Closed
    ));
    assert_eq!(sessions.diagnostics().retained, 0);
    assert_eq!(fixture.token_count(), 0);
}

#[test]
fn persistent_home_failure_joins_paused_preparation_and_releases_runtime_resources() {
    let (mut fixture, sessions, _attention) = fixture(8);
    fs::write(fixture.root(1).join("fixture-mode"), "pause-config").unwrap();
    begin(&fixture, 1);
    wait_until(|| fixture.root(1).join("runtime-evidence.json").exists());
    let process = ProcessWitness::open(fixture.evidence(1)["pid"].as_u64().unwrap() as u32);
    let workers = fixture.service().worker_pool_observer_for_test();
    fixture.fail_home();
    wait_until(|| sessions.diagnostics().closed);
    fs::write(fixture.root(1).join("release-config"), "ready").unwrap();
    let outcome = fixture.service.take().unwrap().close().unwrap();
    let ProjectionConnectionServiceCloseOutcome::PersistentFailure(evidence) = outcome else {
        panic!("home failure must own terminal disposal")
    };
    assert_eq!(
        evidence.completion(),
        beryl_app::cas_projection::PersistentFailureCutCompletion::Finished
    );
    assert_eq!(workers().active(), 0);
    assert_eq!(sessions.diagnostics().retained, 0);
    assert_eq!(fixture.token_count(), 0);
    process.assert_exited();
}

#[test]
fn ordinary_service_drop_requests_preparation_cancellation_without_waiting_for_network() {
    for mode in ["pause-config", "pause-session-config"] {
        let (mut fixture, sessions, _attention) = fixture(8);
        fs::write(fixture.root(1).join("fixture-mode"), mode).unwrap();
        begin(&fixture, 1);
        let evidence = if mode == "pause-config" {
            "runtime-evidence.json"
        } else {
            "runtime-session-evidence-1.json"
        };
        wait_until(|| fixture.root(1).join(evidence).exists());
        let process = ProcessWitness::open(fixture.evidence(1)["pid"].as_u64().unwrap() as u32);
        let workers = fixture.service().worker_pool_observer_for_test();
        let service = fixture.service.take().unwrap();
        let (dropped, observed) = std::sync::mpsc::sync_channel(1);
        let dropper = thread::spawn(move || {
            drop(service);
            dropped.send(()).unwrap();
        });
        observed
            .recv_timeout(Duration::from_secs(1))
            .expect("ordinary Drop must only request preparation cancellation");
        dropper.join().unwrap();
        wait_until(|| sessions.diagnostics().closed);
        fs::write(fixture.root(1).join("release-config"), "ready").unwrap();
        process.assert_exited();
        wait_until(|| workers().active() == 0 && fixture.token_count() == 0);
        assert_eq!(sessions.diagnostics().retained, 0);
    }
}

#[test]
fn durable_candidate_resumes_after_runtime_retirement_or_interest_capacity_release() {
    for retiring in [true, false] {
        let (mut fixture, old_sessions, attention) = fixture(8);
        submission::submit(&fixture, thread_id(1));
        let (provider, sessions) = ProcessScheduledExecutionProvider::new();
        fixture.reopen(Box::new(provider), 8);
        assert!(old_sessions.diagnostics().closed);
        fs::write(fixture.root(1).join("fixture-mode"), "pause-config").unwrap();
        let mut views = Vec::new();
        for _ in 0..if retiring { 1 } else { 4 } {
            views.push(
                fixture
                    .acquire(1, beryl_app::cas_projection::RuntimeInterestKind::View)
                    .unwrap(),
            );
        }
        wait_until(|| fixture.root(1).join("runtime-evidence.json").exists());
        if retiring {
            views.clear();
        }
        configure(&fixture, &sessions, &attention);
        wait_until(|| {
            let waits = fixture
                .service()
                .runtime_preparation_waits_for_test(binding(&fixture, 1).runtime_id());
            if retiring { waits.2 } else { waits.0 }
        });
        fs::write(fixture.root(1).join("fixture-mode"), "pause-projection").unwrap();
        if !retiring {
            drop(views.pop());
        }
        fs::write(fixture.root(1).join("release-config"), "ready").unwrap();
        wait_until(|| {
            fixture
                .root(1)
                .join("runtime-projection-evidence.json")
                .exists()
        });
        assert_eq!(sessions.diagnostics().checked_out, 1);
        drop(views);
        let service = fixture.service.take().unwrap();
        let closed = thread::spawn(move || service.close());
        wait_until(|| sessions.diagnostics().closed);
        fs::write(fixture.root(1).join("release-projection"), "complete").unwrap();
        assert!(matches!(
            closed.join().unwrap().unwrap(),
            ProjectionConnectionServiceCloseOutcome::Closed
        ));
        assert_eq!(sessions.diagnostics().retained, 0);
        assert_eq!(fixture.token_count(), 0);
    }
}
