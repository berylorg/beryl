use super::*;

fn separate_budgets(runtimes: usize, work: usize, views: usize) -> RuntimeInterestTestHarness {
    RuntimeInterestTestHarness::new(
        RuntimeInterestConfig::new(
            NonZeroUsize::new(runtimes).unwrap(),
            NonZeroUsize::new(work).unwrap(),
            TIMEOUT,
        )
        .unwrap()
        .with_view_interest_capacity(NonZeroUsize::new(views).unwrap()),
    )
}

#[test]
fn selected_view_budget_leaves_required_work_budget_and_original_runtime_owned() {
    let mut owner = separate_budgets(1, 1, 2);
    let launch = probe(620);
    launch.allow_launch(false);
    let (spec, binding) = demand(1, 1);
    let first = owner
        .acquire(
            spec.clone(),
            binding.clone(),
            RuntimeInterestKind::View,
            launch.clone(),
        )
        .unwrap();
    assert!(launch.wait_for_launch(TIMEOUT));
    let second = owner
        .acquire(
            spec.clone(),
            binding.clone(),
            RuntimeInterestKind::View,
            probe(621),
        )
        .unwrap();
    assert!(matches!(
        owner.acquire(
            spec.clone(),
            binding.clone(),
            RuntimeInterestKind::View,
            probe(622)
        ),
        Err(RuntimeInterestError::InterestCapacity)
    ));
    let work = owner
        .acquire_scheduled(spec.clone(), binding.clone(), probe(623))
        .unwrap();
    assert!(matches!(
        owner.acquire_scheduled(spec, binding, probe(624)),
        Err(RuntimeInterestError::InterestCapacity)
    ));
    assert_eq!(owner.retained_counts(), (1, 3));
    assert_eq!(launch.counts().0, 1);
    drop((first, second));
    assert_eq!(
        work.status(),
        RuntimeInterestStatus::Starting,
        "dropping both views keeps the original required launch"
    );
    launch.allow_launch(true);
    ready(&work);
    drop(work);
    assert!(owner.shutdown());
    assert_eq!(owner.retained_counts(), (0, 0));
}

#[test]
fn view_runtime_capacity_wait_preserves_original_cleanup_and_required_work_wakeup() {
    let mut owner = separate_budgets(1, 1, 2);
    let launch = probe(625);
    launch.allow_retirement(false);
    let (spec, binding) = demand(1, 1);
    let view = owner
        .acquire(spec, binding, RuntimeInterestKind::View, launch.clone())
        .unwrap();
    ready(&view);
    let next = probe(626);
    let (spec, binding) = demand(2, 1);
    assert!(matches!(
        owner.acquire_scheduled(spec.clone(), binding.clone(), next.clone()),
        Err(RuntimeInterestError::RuntimeCapacity)
    ));
    assert_eq!(next.counts(), (0, 0, 0));
    assert!(
        owner.failure_snapshot(binding.runtime_id()).is_none(),
        "capacity denial is not a launch failure"
    );
    drop(view);
    assert!(launch.wait_for_retirement(TIMEOUT));
    assert!(matches!(
        owner.acquire_scheduled(spec.clone(), binding.clone(), next.clone()),
        Err(RuntimeInterestError::RuntimeCapacity)
    ));
    assert!(!owner.preparation_wake_pending());
    launch.allow_retirement(true);
    wait_for_preparation_wake(&owner);
    let work = owner.acquire_scheduled(spec, binding, next).unwrap();
    ready(&work);
    drop(work);
    assert!(owner.shutdown());
}

#[test]
fn retired_original_view_token_remains_charged_until_its_own_release() {
    let mut owner = separate_budgets(1, 1, 1);
    let original = probe(627);
    original.fail_launch_with_cleanup(RuntimeFailure::Launch);
    let (spec, binding) = demand(1, 1);
    let view = owner
        .acquire(
            spec.clone(),
            binding.clone(),
            RuntimeInterestKind::View,
            original.clone(),
        )
        .unwrap();
    assert_eq!(
        view.wait_for_change(RuntimeInterestStatus::Starting, TIMEOUT),
        RuntimeInterestStatus::Unavailable(RuntimeFailure::Launch)
    );
    assert!(original.wait_for_disposal(TIMEOUT));
    let failure = owner.failure_snapshot(binding.runtime_id()).unwrap();
    let deadline = Instant::now() + TIMEOUT;
    let work = loop {
        match owner.retry_failed(spec.clone(), binding.clone(), failure, probe(628)) {
            Ok(work) => break work,
            Err(RuntimeInterestError::Retiring) if Instant::now() < deadline => thread::yield_now(),
            Err(error) => panic!("original cleanup did not release the failed slot: {error}"),
        }
    };
    ready(&work);
    assert_eq!(view.status(), RuntimeInterestStatus::Retired);
    assert!(
        matches!(
            owner.acquire(
                spec.clone(),
                binding.clone(),
                RuntimeInterestKind::View,
                probe(629)
            ),
            Err(RuntimeInterestError::InterestCapacity)
        ),
        "the original stale view still spends its own bounded slot"
    );
    drop(view);
    let replacement = owner
        .acquire(spec, binding, RuntimeInterestKind::View, probe(630))
        .unwrap();
    assert_eq!(
        ready(&replacement).process_generation(),
        ready(&work).process_generation()
    );
    drop((replacement, work));
    assert!(owner.shutdown());
}
