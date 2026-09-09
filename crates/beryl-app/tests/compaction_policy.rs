#![cfg(feature = "test-faults")]

#[allow(dead_code)]
#[path = "accepted_next_scheduler/support.rs"]
mod scheduler_support;
#[path = "support/compaction_server.rs"]
mod server;
#[path = "compaction_policy/support.rs"]
mod support;
#[path = "projection/syndic.rs"]
mod syndic;

use std::{
    thread,
    time::{Duration, Instant},
};

use beryl_app::{
    LifecycleYieldOutcome,
    cas_projection::{
        ContextCompactionError, ContextCompactionOutcome, ContextCompactionRequest,
        ContextCompactionTimeoutPolicy, ContextCompactionTimeoutSource as Source,
        OrdinaryTurnExecutionError, OrdinaryTurnExecutionFailure, OrdinaryTurnExecutionOutcome,
        OrdinaryTurnExecutionRequest,
    },
};
use beryl_backend::TurnStartOptions;
use syndic_storage::CompactionAdmissionRead;

use server::{CompactionServer, SUBMITTED_TEXT, TIMEOUT};
use support::{apply_timeout, execute, obtain};
use syndic::Fixture;

const EXECUTION_ROOT: &str = r"C:\work\beryl";

#[test]
fn settings_resolution_distinguishes_absent_applied_rejected_and_foreign_home() {
    let fixture = Fixture::new(201);
    let policy = ContextCompactionTimeoutPolicy::applied_settings(fixture.state.settings());
    let resolved = policy.resolve(&fixture.home()).unwrap();
    assert_eq!(resolved.duration(), Duration::from_secs(180));
    assert_eq!(resolved.source(), Source::AbsentDefault);
    for (millis, seconds, source) in [
        (1_000, 1, Source::Applied),
        (86_400_000, 86_400, Source::Applied),
        (0, 180, Source::RejectedSavedValue),
        (999, 180, Source::RejectedSavedValue),
        (1_001, 180, Source::RejectedSavedValue),
        (86_400_001, 180, Source::RejectedSavedValue),
        (u64::MAX, 180, Source::RejectedSavedValue),
        (2_000, 2, Source::Applied),
    ] {
        apply_timeout(&fixture, millis);
        let resolved = policy.resolve(&fixture.home()).unwrap();
        assert_eq!(resolved.duration(), Duration::from_secs(seconds));
        assert_eq!(resolved.source(), source);
    }
    let foreign = Fixture::new(202);
    assert!(matches!(
        policy.resolve(&foreign.home()),
        Err(ContextCompactionError::HomeRead(_))
    ));
    let fixed = ContextCompactionTimeoutPolicy::fixed(Duration::from_secs(7));
    let resolved = fixed.resolve(&fixture.home()).unwrap();
    assert_eq!(resolved.duration(), Duration::from_secs(7));
    assert_eq!(resolved.source(), Source::Fixed);
}

#[test]
fn manual_admission_reads_latest_settings_and_join_retains_original_deadline() {
    let mut fixture = Fixture::new(203);
    let foreign = Fixture::new(204);
    fixture.submit_text(SUBMITTED_TEXT);
    apply_timeout(&fixture, 20_000);
    let manual =
        ContextCompactionRequest::applied_settings(fixture.thread, fixture.state.settings());
    let server = CompactionServer::spawn(true);
    let (session, projection) = obtain(&fixture, &server);
    let request = OrdinaryTurnExecutionRequest::backend_defaults(fixture.state.settings(), TIMEOUT);
    let outcome = thread::scope(|scope| {
        let worker = scope.spawn(|| execute(&fixture, projection, &request));
        server.wait_started();
        server.finish_turn();
        worker.join().unwrap().unwrap()
    });
    assert!(
        matches!(outcome, OrdinaryTurnExecutionOutcome::Terminal { .. }),
        "unexpected ordinary outcome: {outcome:?}"
    );
    let OrdinaryTurnExecutionOutcome::Terminal { projection, .. } = outcome else {
        unreachable!()
    };
    let revision = fixture.home().home_revision().unwrap();
    assert!(matches!(
        fixture
            .store
            .compact_thread(ContextCompactionRequest::applied_settings(
                fixture.thread,
                foreign.state.settings()
            )),
        Err(ContextCompactionError::HomeRead(_))
    ));
    assert_eq!(fixture.home().home_revision().unwrap(), revision);
    apply_timeout(&fixture, 1_000);
    thread::scope(|scope| {
        let worker = scope.spawn(|| fixture.store.compact_thread(manual));
        server.wait_compaction();
        let harness = fixture
            .store
            .context_compaction_lifecycle_test_harness()
            .unwrap();
        let resolution = harness.timeout_resolution(fixture.thread).unwrap().unwrap();
        assert_eq!(resolution.duration(), Duration::from_secs(1));
        assert_eq!(resolution.source(), Source::Applied);
        apply_timeout(&fixture, 20_000);
        let start = Instant::now();
        let joined = fixture
            .store
            .compact_thread(ContextCompactionRequest::applied_settings(
                fixture.thread,
                foreign.state.settings(),
            ))
            .unwrap();
        assert_eq!(joined, ContextCompactionOutcome::StillRunning);
        assert!(start.elapsed() < Duration::from_secs(5));
        assert_eq!(
            worker.join().unwrap().unwrap(),
            ContextCompactionOutcome::StillRunning
        );
        assert_eq!(
            harness.timeout_resolution(fixture.thread).unwrap(),
            Some(resolution)
        );
        assert!(matches!(
            fixture
                .storage
                .compaction_admission_read(
                    &fixture.home(),
                    fixture.thread,
                    scheduler_support::point_limit()
                )
                .unwrap(),
            CompactionAdmissionRead::Existing(_)
        ));
    });
    session.invalidate_connection();
    drop(projection);
    drop(session);
    server.join();
}

#[test]
fn apply_during_ordinary_turn_is_used_only_when_lifecycle_compaction_is_admitted() {
    let mut fixture = Fixture::new(205);
    let submitted = fixture.submit_text(SUBMITTED_TEXT);
    apply_timeout(&fixture, 20_000);
    let request = OrdinaryTurnExecutionRequest::backend_defaults(fixture.state.settings(), TIMEOUT);
    let server = CompactionServer::spawn(true);
    let (session, projection) = obtain(&fixture, &server);
    thread::scope(|scope| {
        let worker = scope.spawn(|| execute(&fixture, projection, &request));
        server.wait_started();
        scheduler_support::wait_until("register lifecycle yield", || {
            fixture
                .store
                .record_lifecycle_yield_outcome(
                    fixture.thread,
                    submitted.turn,
                    LifecycleYieldOutcome::PhaseContinue,
                )
                .unwrap()
                .then_some(())
        });
        apply_timeout(&fixture, 1_000);
        server.finish_turn();
        let result = worker.join().unwrap();
        assert!(
            matches!(
                &result,
                Ok(OrdinaryTurnExecutionOutcome::LifecycleContinuationScheduled { .. })
            ),
            "expected admitted lifecycle compaction: {result:?}"
        );
        server.wait_compaction();
        let harness = fixture
            .store
            .context_compaction_lifecycle_test_harness()
            .unwrap();
        let resolution = harness.timeout_resolution(fixture.thread).unwrap().unwrap();
        assert_eq!(resolution.duration(), Duration::from_secs(1));
        assert_eq!(resolution.source(), Source::Applied);
        apply_timeout(&fixture, 20_000);
        let start = Instant::now();
        assert_eq!(
            fixture
                .store
                .compact_thread(ContextCompactionRequest::applied_settings(
                    fixture.thread,
                    fixture.state.settings()
                ))
                .unwrap(),
            ContextCompactionOutcome::StillRunning
        );
        assert!(start.elapsed() < Duration::from_secs(5));
        assert_eq!(
            harness.timeout_resolution(fixture.thread).unwrap(),
            Some(resolution)
        );
    });
    session.invalidate_connection();
    drop(session);
    server.join();
}

#[test]
fn lifecycle_settings_failure_keeps_typed_cause_and_admits_no_compaction() {
    let mut fixture = Fixture::new(206);
    let foreign = Fixture::new(207);
    let submitted = fixture.submit_text(SUBMITTED_TEXT);
    let request = OrdinaryTurnExecutionRequest::new(TurnStartOptions::default(), TIMEOUT)
        .with_context_compaction_timeout_policy(ContextCompactionTimeoutPolicy::applied_settings(
            foreign.state.settings(),
        ));
    let server = CompactionServer::spawn(false);
    let (session, projection) = obtain(&fixture, &server);
    thread::scope(|scope| {
        let worker = scope.spawn(|| execute(&fixture, projection, &request));
        server.wait_started();
        scheduler_support::wait_until("register lifecycle yield", || {
            fixture
                .store
                .record_lifecycle_yield_outcome(
                    fixture.thread,
                    submitted.turn,
                    LifecycleYieldOutcome::PhaseContinue,
                )
                .unwrap()
                .then_some(())
        });
        server.finish_turn();
        let result = worker.join().unwrap();
        assert!(
            matches!(
                &result,
                Err(OrdinaryTurnExecutionFailure::AfterActivation {
                    source: OrdinaryTurnExecutionError::ContextCompaction(
                        ContextCompactionError::HomeRead(_)
                    )
                })
            ),
            "expected typed settings failure at lifecycle admission: {result:?}"
        );
    });
    assert!(
        fixture
            .store
            .context_compaction_lifecycle_test_harness()
            .unwrap()
            .timeout_resolution(fixture.thread)
            .unwrap()
            .is_none()
    );
    assert!(matches!(
        fixture
            .storage
            .compaction_admission_read(
                &fixture.home(),
                fixture.thread,
                scheduler_support::point_limit()
            )
            .unwrap(),
        CompactionAdmissionRead::Admissible(_)
    ));
    session.invalidate_connection();
    drop(session);
    server.join();
}
