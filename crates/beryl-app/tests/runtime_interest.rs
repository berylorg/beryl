#![cfg(feature = "test-faults")]

use std::{
    num::NonZeroUsize,
    thread,
    time::{Duration, Instant},
};

use beryl_app::cas_projection::{
    RuntimeFailure, RuntimeInterest, RuntimeInterestConfig, RuntimeInterestError,
    RuntimeInterestKind, RuntimeInterestStatus, RuntimeInterestTestHarness,
    RuntimeInterestTestProbe, RuntimeReadiness,
};
use beryl_backend::ManagedBackendLaunchSpec;
use beryl_model::{
    AdmittedHostPath, CasProcessGeneration, ExecutionBinding, PathFlavor, RootId, RuntimeId,
    RuntimeMode, RuntimeNativePath,
};

const TIMEOUT: Duration = Duration::from_secs(5);

fn harness(runtimes: usize, interests: usize) -> RuntimeInterestTestHarness {
    RuntimeInterestTestHarness::new(
        RuntimeInterestConfig::new(
            NonZeroUsize::new(runtimes).unwrap(),
            NonZeroUsize::new(interests).unwrap(),
            TIMEOUT,
        )
        .unwrap(),
    )
}

fn native(path: &str) -> RuntimeNativePath {
    RuntimeNativePath::from_admitted(RuntimeMode::Host, PathFlavor::Windows, path).unwrap()
}

fn demand(runtime: u8, root: u8) -> (ManagedBackendLaunchSpec, ExecutionBinding) {
    let runtime_id = RuntimeId::from_bytes([runtime; 16]);
    let executable = format!(r"C:\runtime-{runtime}\codex.exe");
    let directory = native(&format!(r"C:\root-{root}"));
    let spec = ManagedBackendLaunchSpec::new(
        runtime_id,
        AdmittedHostPath::from_admitted(PathFlavor::Windows, &executable).unwrap(),
        RuntimeMode::Host,
        native(&executable),
        directory.clone(),
        AdmittedHostPath::from_admitted(PathFlavor::Windows, r"C:\tokens").unwrap(),
        native(r"C:\tokens"),
    )
    .unwrap();
    (
        spec,
        ExecutionBinding::new(runtime_id, RootId::from_bytes([root; 16]), directory),
    )
}

fn probe(generation: u64) -> RuntimeInterestTestProbe {
    RuntimeInterestTestProbe::new(CasProcessGeneration::new(generation).unwrap())
}

fn ready(interest: &RuntimeInterest) -> RuntimeReadiness {
    match interest.wait_for_change(RuntimeInterestStatus::Starting, TIMEOUT) {
        RuntimeInterestStatus::Ready(ready) => ready,
        status => panic!("runtime failed to become ready: {status:?}"),
    }
}

fn acquire_after_retirement(
    owner: &RuntimeInterestTestHarness,
    runtime: u8,
    root: u8,
    probe: &RuntimeInterestTestProbe,
) -> RuntimeInterest {
    let deadline = Instant::now() + TIMEOUT;
    loop {
        let (spec, binding) = demand(runtime, root);
        match owner.acquire(spec, binding, RuntimeInterestKind::View, probe.clone()) {
            Ok(interest) => return interest,
            Err(RuntimeInterestError::Retiring) if Instant::now() < deadline => {
                thread::sleep(Duration::from_millis(1))
            }
            Err(error) => panic!("retired runtime could not be reacquired: {error}"),
        }
    }
}

#[test]
fn concurrent_root_qualified_demands_share_one_launch_and_activity_period() {
    let mut owner = harness(1, 8);
    let launch = probe(501);
    launch.allow_launch(false);
    let interests = thread::scope(|scope| {
        let joins = (1..=8)
            .map(|root| {
                let owner = &owner;
                let launch = launch.clone();
                scope.spawn(move || {
                    let (spec, binding) = demand(1, root);
                    owner
                        .acquire(spec, binding, RuntimeInterestKind::View, launch)
                        .unwrap()
                })
            })
            .collect::<Vec<_>>();
        joins
            .into_iter()
            .map(|join| join.join().unwrap())
            .collect::<Vec<_>>()
    });
    assert!(launch.wait_for_launch(TIMEOUT));
    assert_eq!(launch.counts(), (1, 0, 0));
    assert_eq!(owner.retained_counts(), (1, 8));
    launch.allow_launch(true);
    let first = ready(&interests[0]);
    for interest in &interests {
        assert_eq!(ready(interest), first);
        assert!(interest.is_current(first.activity_period()));
    }
    drop(interests);
    assert!(launch.wait_for_disposal(TIMEOUT));
    assert!(owner.shutdown());
}

#[test]
fn required_work_survives_view_cancellation_during_launch_and_after_readiness() {
    let mut owner = harness(1, 3);
    let launch = probe(502);
    launch.allow_launch(false);
    let (spec, binding) = demand(1, 1);
    let view = owner
        .acquire(spec, binding, RuntimeInterestKind::View, launch.clone())
        .unwrap();
    assert!(launch.wait_for_launch(TIMEOUT));
    let (spec, binding) = demand(1, 2);
    let work = owner
        .acquire(
            spec,
            binding.clone(),
            RuntimeInterestKind::RequiredWork,
            launch.clone(),
        )
        .unwrap();
    drop(view);
    assert_eq!(work.binding(), &binding);
    assert_eq!(work.kind(), RuntimeInterestKind::RequiredWork);
    launch.allow_launch(true);
    let first = ready(&work);
    let (spec, binding) = demand(1, 3);
    let view = owner
        .acquire(spec, binding, RuntimeInterestKind::View, launch.clone())
        .unwrap();
    assert_eq!(ready(&view), first);
    drop(view);
    assert!(work.is_current(first.activity_period()));
    assert_eq!(launch.counts(), (1, 0, 0));
    drop(work);
    assert!(launch.wait_for_disposal(TIMEOUT));
    assert!(owner.shutdown());
}

#[test]
fn independent_runtime_becomes_ready_while_another_launch_is_pending() {
    let mut owner = harness(2, 2);
    let slow = probe(503);
    slow.allow_launch(false);
    let fast = probe(504);
    let (spec, binding) = demand(1, 1);
    let pending = owner
        .acquire(spec, binding, RuntimeInterestKind::View, slow.clone())
        .unwrap();
    assert!(slow.wait_for_launch(TIMEOUT));
    let (spec, binding) = demand(2, 2);
    let usable = owner
        .acquire(
            spec,
            binding,
            RuntimeInterestKind::RequiredWork,
            fast.clone(),
        )
        .unwrap();
    assert_eq!(
        ready(&usable).process_generation(),
        CasProcessGeneration::new(504).unwrap()
    );
    assert_eq!(pending.status(), RuntimeInterestStatus::Starting);
    drop(pending);
    slow.allow_launch(true);
    assert!(slow.wait_for_disposal(TIMEOUT));
    assert!(matches!(usable.status(), RuntimeInterestStatus::Ready(_)));
    drop(usable);
    assert!(owner.shutdown());
}

#[test]
fn capacity_is_reserved_before_launch_and_retirement_keeps_its_slot() {
    let mut owner = harness(1, 1);
    let launch = probe(505);
    launch.allow_retirement(false);
    let unused = probe(506);
    let (spec, binding) = demand(1, 1);
    let view = owner
        .acquire(spec, binding, RuntimeInterestKind::View, launch.clone())
        .unwrap();
    ready(&view);
    let (spec, binding) = demand(2, 2);
    assert!(matches!(
        owner.acquire(spec, binding, RuntimeInterestKind::View, unused.clone()),
        Err(RuntimeInterestError::InterestCapacity)
    ));
    assert_eq!(unused.counts(), (0, 0, 0));
    drop(view);
    assert!(launch.wait_for_retirement(TIMEOUT));
    let (spec, binding) = demand(1, 1);
    assert!(matches!(
        owner.acquire(spec, binding, RuntimeInterestKind::View, unused.clone()),
        Err(RuntimeInterestError::Retiring)
    ));
    let (spec, binding) = demand(2, 2);
    assert!(matches!(
        owner.acquire(spec, binding, RuntimeInterestKind::View, unused.clone()),
        Err(RuntimeInterestError::RuntimeCapacity)
    ));
    assert_eq!(owner.retained_counts(), (1, 0));
    launch.allow_retirement(true);
    assert!(launch.wait_for_disposal(TIMEOUT));
    let replacement = acquire_after_retirement(&owner, 1, 1, &unused);
    ready(&replacement);
    drop(replacement);
    assert!(owner.shutdown());
}

#[test]
fn generation_loss_rejects_late_launch_completion_and_releases_the_candidate() {
    let mut owner = harness(1, 1);
    let launch = probe(507);
    launch.allow_launch(false);
    let (spec, binding) = demand(1, 1);
    let interest = owner
        .acquire(
            spec,
            binding,
            RuntimeInterestKind::RequiredWork,
            launch.clone(),
        )
        .unwrap();
    assert!(launch.wait_for_launch(TIMEOUT));
    owner.lose_generation();
    assert_eq!(interest.status(), RuntimeInterestStatus::Retired);
    launch.allow_launch(true);
    assert!(launch.wait_for_disposal(TIMEOUT));
    assert_eq!(interest.status(), RuntimeInterestStatus::Retired);
    let (spec, binding) = demand(1, 1);
    assert!(matches!(
        owner.acquire(spec, binding, RuntimeInterestKind::View, launch),
        Err(RuntimeInterestError::Closed)
    ));
    assert!(owner.shutdown());
}

#[test]
fn ended_activity_period_cannot_be_reused_by_a_new_service_generation() {
    let mut old = harness(1, 1);
    let (spec, binding) = demand(1, 1);
    let prior = old
        .acquire(spec, binding, RuntimeInterestKind::View, probe(508))
        .unwrap();
    let period = ready(&prior).activity_period();
    old.lose_generation();
    assert!(!prior.is_current(period));
    assert!(old.shutdown());
    let mut new = harness(1, 1);
    let (spec, binding) = demand(1, 1);
    let current = new
        .acquire(spec, binding, RuntimeInterestKind::View, probe(509))
        .unwrap();
    assert_ne!(ready(&current).activity_period(), period);
    assert!(!current.is_current(period));
    drop(prior);
    assert_eq!(new.retained_counts(), (1, 1));
    drop(current);
    assert!(new.shutdown());
}

#[test]
fn admission_failure_remains_unavailable_after_clean_interest_release() {
    let mut owner = harness(1, 1);
    let failed = probe(510);
    failed.fail_admission(RuntimeFailure::Admission);
    let (spec, binding) = demand(1, 1);
    let interest = owner
        .acquire(spec, binding, RuntimeInterestKind::View, failed)
        .unwrap();
    assert_eq!(
        interest.wait_for_change(RuntimeInterestStatus::Starting, TIMEOUT),
        RuntimeInterestStatus::Unavailable(RuntimeFailure::Admission)
    );
    drop(interest);
    let unused = probe(511);
    let (spec, binding) = demand(1, 1);
    assert!(matches!(
        owner.acquire(spec, binding, RuntimeInterestKind::View, unused.clone()),
        Err(RuntimeInterestError::Unavailable(RuntimeFailure::Admission))
    ));
    assert_eq!(unused.counts(), (0, 0, 0));
    assert_eq!(owner.retained_counts(), (1, 0));
    assert!(owner.shutdown());
}

#[test]
fn failed_disposal_quarantines_the_runtime_instead_of_overlapping_a_new_process() {
    let mut owner = harness(1, 1);
    let failed = probe(512);
    failed.fail_retirement(RuntimeFailure::BackendDisposal);
    let (spec, binding) = demand(1, 1);
    let interest = owner
        .acquire(spec, binding, RuntimeInterestKind::View, failed.clone())
        .unwrap();
    ready(&interest);
    drop(interest);
    assert!(failed.wait_for_disposal(TIMEOUT));
    let unused = probe(513);
    let (spec, binding) = demand(1, 1);
    assert!(matches!(
        owner.acquire(spec, binding, RuntimeInterestKind::View, unused.clone()),
        Err(RuntimeInterestError::Retiring
            | RuntimeInterestError::Unavailable(RuntimeFailure::BackendDisposal))
    ));
    assert_eq!(unused.counts(), (0, 0, 0));
    assert!(!owner.shutdown());
}

#[test]
fn foreground_loss_ends_only_the_matching_runtime_period() {
    let mut owner = harness(2, 2);
    let failed = probe(514);
    let (spec, binding) = demand(1, 1);
    let first = owner
        .acquire(spec, binding, RuntimeInterestKind::View, failed.clone())
        .unwrap();
    let old = ready(&first);
    let (spec, binding) = demand(2, 2);
    let second = owner
        .acquire(spec, binding, RuntimeInterestKind::View, probe(515))
        .unwrap();
    let healthy = ready(&second);
    failed.fail_health(RuntimeFailure::ConnectionLost);
    assert_eq!(
        first.wait_for_change(RuntimeInterestStatus::Ready(old), TIMEOUT),
        RuntimeInterestStatus::Unavailable(RuntimeFailure::ConnectionLost)
    );
    assert!(!first.is_current(old.activity_period()));
    assert!(second.is_current(healthy.activity_period()));
    assert!(failed.wait_for_disposal(TIMEOUT));
    drop((first, second));
    assert!(owner.shutdown());
}

#[test]
fn scheduled_interest_capacity_refusal_wakes_on_real_interest_release() {
    let mut owner = harness(2, 1);
    let current_probe = probe(520);
    current_probe.allow_retirement(false);
    let (spec, binding) = demand(1, 1);
    let view = owner
        .acquire(
            spec,
            binding,
            RuntimeInterestKind::View,
            current_probe.clone(),
        )
        .unwrap();
    ready(&view);
    let next = probe(521);
    for _ in 0..2 {
        let (spec, binding) = demand(2, 1);
        assert!(matches!(
            owner.acquire_scheduled(spec, binding, next.clone()),
            Err(RuntimeInterestError::InterestCapacity)
        ));
    }
    assert_eq!(owner.preparation_wake_count(), 0);
    assert_eq!(next.counts(), (0, 0, 0));
    drop(view);
    assert_eq!(owner.preparation_wake_count(), 1);
    let (spec, binding) = demand(2, 1);
    let work = owner.acquire_scheduled(spec, binding, next).unwrap();
    ready(&work);
    current_probe.allow_retirement(true);
    drop(work);
    assert!(owner.shutdown());
}

#[test]
fn scheduled_runtime_capacity_wait_ignores_unrelated_interest_release() {
    let mut owner = harness(1, 3);
    let current_probe = probe(522);
    current_probe.allow_retirement(false);
    let (spec, binding) = demand(1, 1);
    let view = owner
        .acquire(
            spec,
            binding,
            RuntimeInterestKind::View,
            current_probe.clone(),
        )
        .unwrap();
    ready(&view);
    let (spec, binding) = demand(1, 2);
    let extra = owner
        .acquire(
            spec,
            binding,
            RuntimeInterestKind::View,
            current_probe.clone(),
        )
        .unwrap();
    let next = probe(523);
    let (spec, binding) = demand(2, 1);
    assert!(matches!(
        owner.acquire_scheduled(spec, binding, next.clone()),
        Err(RuntimeInterestError::RuntimeCapacity)
    ));
    drop(extra);
    assert_eq!(owner.preparation_wake_count(), 0);
    drop(view);
    assert!(current_probe.wait_for_retirement(TIMEOUT));
    assert_eq!(owner.preparation_wake_count(), 0);
    current_probe.allow_retirement(true);
    wait_for_preparation_wake(&owner);
    let (spec, binding) = demand(2, 1);
    let work = owner.acquire_scheduled(spec, binding, next).unwrap();
    ready(&work);
    drop(work);
    assert!(owner.shutdown());
}

#[test]
fn scheduled_retiring_runtime_wait_wakes_after_terminal_cleanup_publication() {
    let mut owner = harness(1, 2);
    let current_probe = probe(524);
    current_probe.allow_retirement(false);
    let (spec, binding) = demand(1, 1);
    let view = owner
        .acquire(
            spec,
            binding,
            RuntimeInterestKind::View,
            current_probe.clone(),
        )
        .unwrap();
    ready(&view);
    drop(view);
    assert!(current_probe.wait_for_retirement(TIMEOUT));
    let next = probe(525);
    let (spec, binding) = demand(1, 2);
    assert!(matches!(
        owner.acquire_scheduled(spec, binding, next.clone()),
        Err(RuntimeInterestError::Retiring)
    ));
    assert_eq!(owner.preparation_wake_count(), 0);
    current_probe.allow_retirement(true);
    wait_for_preparation_wake(&owner);
    let (spec, binding) = demand(1, 2);
    let work = owner.acquire_scheduled(spec, binding, next).unwrap();
    assert_eq!(
        ready(&work).process_generation(),
        CasProcessGeneration::new(525).unwrap()
    );
    drop(work);
    assert!(owner.shutdown());
}

fn wait_for_preparation_wake(owner: &RuntimeInterestTestHarness) {
    let deadline = Instant::now() + TIMEOUT;
    while owner.preparation_wake_count() == 0 {
        assert!(
            Instant::now() < deadline,
            "real owner dependency release did not wake preparation"
        );
        thread::sleep(Duration::from_millis(1));
    }
    assert_eq!(owner.preparation_wake_count(), 1);
}

#[test]
fn mismatched_runtime_or_root_is_refused_without_starting_work() {
    let mut owner = harness(2, 2);
    let unused = probe(516);
    let (spec, _) = demand(1, 1);
    let (_, binding) = demand(1, 2);
    assert!(matches!(
        owner.acquire(spec, binding, RuntimeInterestKind::View, unused.clone()),
        Err(RuntimeInterestError::ConfigurationMismatch)
    ));
    let (spec, _) = demand(1, 1);
    let (_, binding) = demand(2, 1);
    assert!(matches!(
        owner.acquire(spec, binding, RuntimeInterestKind::View, unused.clone()),
        Err(RuntimeInterestError::ConfigurationMismatch)
    ));
    assert_eq!(unused.counts(), (0, 0, 0));
    assert_eq!(owner.retained_counts(), (0, 0));
    assert!(owner.shutdown());
}
