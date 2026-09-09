#![cfg(all(feature = "test-faults", target_os = "windows"))]

#[path = "support/managed_runtime.rs"]
mod support;

use beryl_app::cas_projection::{
    PersistentFailureCutCompletion, ProjectionConnectionServiceCloseOutcome, RuntimeFailure,
    RuntimeInterestError, RuntimeInterestKind, RuntimeInterestStatus,
};
use std::{fs, thread, time::Duration};
use support::{Fixture, ProcessWitness, TIMEOUT, ready, wait_until};

#[test]
fn real_managed_launch_admits_once_and_required_work_keeps_the_process_alive() {
    let mut fixture = Fixture::new();
    let view = fixture.acquire(1, RuntimeInterestKind::View).unwrap();
    let first = ready(&view);
    let evidence = fixture.evidence(1);
    assert_eq!(evidence["authenticated"], true);
    assert_eq!(evidence["foreground"], true);
    assert_eq!(
        evidence["methods"],
        serde_json::json!(["initialize", "initialized", "config/read"])
    );
    let process = ProcessWitness::open(evidence["pid"].as_u64().unwrap() as u32);
    assert!(process.running());
    assert_eq!(fixture.token_count(), 1);
    assert_eq!(fixture.service().worker_pool_diagnostics().active(), 2);
    let work = fixture
        .acquire(2, RuntimeInterestKind::RequiredWork)
        .unwrap();
    assert_eq!(ready(&work), first);
    drop(view);
    assert!(work.is_current(first.activity_period()));
    assert!(process.running());
    drop(work);
    wait_until(|| {
        fixture.token_count() == 0 && fixture.service().worker_pool_diagnostics().active() == 0
    });
    process.assert_exited();
    let mut replacement = None;
    wait_until(|| match fixture.acquire(2, RuntimeInterestKind::View) {
        Ok(interest) => {
            replacement = Some(interest);
            true
        }
        Err(RuntimeInterestError::Retiring) => false,
        Err(error) => panic!("fresh runtime failed: {error}"),
    });
    let replacement = replacement.unwrap();
    assert_ne!(
        ready(&replacement).activity_period(),
        first.activity_period()
    );
    drop(replacement);
    assert!(matches!(
        fixture.service.take().unwrap().close().unwrap(),
        ProjectionConnectionServiceCloseOutcome::Closed
    ));
    assert_eq!(fixture.token_count(), 0);
}

#[test]
fn real_admission_failure_removes_launch_token_and_releases_app_workers() {
    let mut fixture = Fixture::new();
    fs::write(fixture.root(1).join("fixture-mode"), "reject-config").unwrap();
    let interest = fixture.acquire(1, RuntimeInterestKind::View).unwrap();
    assert_eq!(
        interest.wait_for_change(RuntimeInterestStatus::Starting, TIMEOUT),
        RuntimeInterestStatus::Unavailable(RuntimeFailure::Admission)
    );
    assert_eq!(fixture.evidence(1)["rejected_config"], true);
    assert_eq!(fixture.token_count(), 0);
    assert_eq!(fixture.service().worker_pool_diagnostics().active(), 0);
    assert!(matches!(
        fixture.service.take().unwrap().close().unwrap(),
        ProjectionConnectionServiceCloseOutcome::Closed
    ));
    assert_eq!(interest.status(), RuntimeInterestStatus::Retired);
}

#[test]
fn service_close_rejects_a_late_real_admission_and_joins_registered_resources() {
    let mut fixture = Fixture::new();
    fs::write(fixture.root(1).join("fixture-mode"), "pause-config").unwrap();
    let interest = fixture
        .acquire(1, RuntimeInterestKind::RequiredWork)
        .unwrap();
    wait_until(|| fixture.root(1).join("runtime-evidence.json").exists());
    let process = ProcessWitness::open(fixture.evidence(1)["pid"].as_u64().unwrap() as u32);
    let service = fixture.service.take().unwrap();
    let close = thread::spawn(move || service.close());
    wait_until(|| interest.status() == RuntimeInterestStatus::Retired);
    fs::write(fixture.root(1).join("release-config"), "release").unwrap();
    assert!(matches!(
        close.join().unwrap().unwrap(),
        ProjectionConnectionServiceCloseOutcome::Closed
    ));
    process.assert_exited();
    assert_eq!(fixture.token_count(), 0);
}

#[test]
fn persistent_failure_cut_finishes_before_runtime_retirement_detaches_its_connection() {
    let mut fixture = Fixture::new();
    let interest = fixture
        .acquire(1, RuntimeInterestKind::RequiredWork)
        .unwrap();
    let prior = ready(&interest);
    let process = ProcessWitness::open(fixture.evidence(1)["pid"].as_u64().unwrap() as u32);
    let admitted = fixture.service().live_home_command().unwrap();
    fixture.fail_home();
    wait_until(|| fixture.service().runtime_retirement_waiters_for_test() == 1);
    assert!(!interest.is_current(prior.activity_period()));
    assert!(process.running());
    assert_eq!(fixture.service().worker_pool_diagnostics().active(), 2);
    drop(admitted);
    let outcome = fixture.service.take().unwrap().close().unwrap();
    match outcome {
        ProjectionConnectionServiceCloseOutcome::PersistentFailure(evidence) => {
            assert_eq!(
                evidence.completion(),
                PersistentFailureCutCompletion::Finished
            );
        }
        ProjectionConnectionServiceCloseOutcome::Closed => {
            panic!("persistent failure lost its close election")
        }
    }
    process.assert_exited();
    assert_eq!(fixture.token_count(), 0);
}

#[test]
fn ordinary_service_drop_does_not_wait_for_a_pending_managed_admission() {
    let mut fixture = Fixture::new();
    fs::write(fixture.root(1).join("fixture-mode"), "pause-config").unwrap();
    let interest = fixture.acquire(1, RuntimeInterestKind::View).unwrap();
    wait_until(|| fixture.root(1).join("runtime-evidence.json").exists());
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
        .expect("ordinary Drop must not wait for admission");
    dropper.join().unwrap();
    assert_eq!(interest.status(), RuntimeInterestStatus::Retired);
    assert!(process.running());
    fs::write(fixture.root(1).join("release-config"), "release").unwrap();
    process.assert_exited();
    wait_until(|| fixture.token_count() == 0);
    assert_eq!(workers().active(), 0);
}

#[test]
fn ordinary_service_drop_preserves_ready_connection_custody_until_joined_disposal() {
    let mut fixture = Fixture::new();
    let interest = fixture.acquire(1, RuntimeInterestKind::View).unwrap();
    ready(&interest);
    let process = ProcessWitness::open(fixture.evidence(1)["pid"].as_u64().unwrap() as u32);
    let workers = fixture.service().worker_pool_observer_for_test();
    assert_eq!(workers().active(), 2);
    drop(fixture.service.take());
    assert_eq!(interest.status(), RuntimeInterestStatus::Retired);
    process.assert_exited();
    wait_until(|| fixture.token_count() == 0);
    assert_eq!(workers().active(), 0);
}
