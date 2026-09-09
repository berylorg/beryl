use super::*;
use beryl_app::lifecycle_attention::LifecycleAttentionWorkError as WorkError;

#[test]
fn work_revisions_detect_record_updates_and_add_remove_cycles() {
    let pool = ProcessLifecycleAttentionPool::new();
    let empty = pool.work_revision().unwrap();
    assert!(pool.work_snapshot(&empty).unwrap().records().is_empty());
    let yielding = attempt(&pool, 1, LifecycleYieldOutcome::PhaseNeedsReview);
    assert_eq!(pool.work_revision().unwrap(), empty);
    let token = admitted(pool.report_terminal(&yielding));
    assert_eq!(
        pool.validate_work_revision(&empty),
        Err(WorkError::StaleRevision)
    );
    let admitted_revision = pool.work_revision().unwrap();
    let observation = pool.work_snapshot(&admitted_revision).unwrap();
    assert_eq!(observation.revision(), &admitted_revision);
    assert_eq!(observation.records(), pool.snapshot());
    assert_eq!(pool.work_snapshot(&admitted_revision).unwrap(), observation);
    assert_eq!(
        pool.report_terminal(&yielding),
        Admission::Updated(token.clone())
    );
    assert_eq!(
        pool.work_snapshot(&admitted_revision),
        Err(WorkError::StaleRevision)
    );
    let updated = pool.work_revision().unwrap();
    assert_eq!(
        pool.work_snapshot(&updated).unwrap().records()[0].report_count(),
        2
    );
    assert!(pool.acknowledge(&token));
    assert_eq!(
        pool.validate_work_revision(&updated),
        Err(WorkError::StaleRevision)
    );
    assert_eq!(pool.work_snapshot(&empty), Err(WorkError::StaleRevision));
    let acknowledged = pool.work_revision().unwrap();
    assert!(
        pool.work_snapshot(&acknowledged)
            .unwrap()
            .records()
            .is_empty()
    );
    assert!(!pool.acknowledge(&token));
    assert_eq!(pool.report_terminal(&yielding), Admission::AlreadyReported);
    assert_eq!(pool.work_revision().unwrap(), acknowledged);
    assert_eq!(observation.records()[0].report_count(), 1);
    let successor =
        admitted(pool.report_terminal(&attempt(&pool, 1, LifecycleYieldOutcome::PhaseNeedsReview)));
    assert_ne!(&successor, observation.records()[0].token());
    assert!(!pool.acknowledge(observation.records()[0].token()));
    assert_eq!(pool.snapshot()[0].token(), &successor);
}

#[test]
fn work_observation_preserves_capacity_and_ignores_diagnostics_only_outcomes() {
    let pool = ProcessLifecycleAttentionPool::new();
    let empty = pool.work_revision().unwrap();
    let continuing = attempt(&pool, 1, LifecycleYieldOutcome::PhaseContinue);
    assert_eq!(pool.report_terminal(&continuing), Admission::NotRequested);
    assert_eq!(pool.work_revision().unwrap(), empty);
    for seed in 0..NOTICE_RECORD_CAPACITY {
        admitted(pool.report_terminal(&attempt(
            &pool,
            seed as u8,
            LifecycleYieldOutcome::PlanComplete,
        )));
    }
    let full = pool.work_revision().unwrap();
    let observation = pool.work_snapshot(&full).unwrap();
    assert_eq!(observation.records().len(), NOTICE_RECORD_CAPACITY);
    assert_eq!(
        pool.report_terminal(&attempt(&pool, 99, LifecycleYieldOutcome::PlanComplete)),
        Admission::Omitted
    );
    let foreign_pool = ProcessLifecycleAttentionPool::new();
    assert_eq!(
        pool.report_terminal(&attempt(
            &foreign_pool,
            98,
            LifecycleYieldOutcome::PlanComplete
        )),
        Admission::Rejected(Rejection::ForeignAttempt)
    );
    assert_eq!(pool.work_revision().unwrap(), full);
    assert_eq!(pool.work_snapshot(&full).unwrap(), observation);
    assert_eq!(pool.diagnostics().omitted, 1);
    assert_eq!(pool.diagnostics().rejected, 1);
    assert_eq!(pool.snapshot().len(), NOTICE_RECORD_CAPACITY);
}

#[test]
fn work_observation_rejects_foreign_and_closed_pools_without_retaining_pool_lifetime() {
    let pool = Arc::new(ProcessLifecycleAttentionPool::new());
    admitted(pool.report_terminal(&attempt(&pool, 1, LifecycleYieldOutcome::PlanComplete)));
    let revision = pool.work_revision().unwrap();
    let observation = pool.work_snapshot(&revision).unwrap();
    let other = ProcessLifecycleAttentionPool::new();
    assert_eq!(
        other.validate_work_revision(&revision),
        Err(WorkError::ForeignRevision)
    );
    assert_eq!(
        other.work_snapshot(&revision),
        Err(WorkError::ForeignRevision)
    );
    assert!(!other.acknowledge(observation.records()[0].token()));
    assert_eq!(Arc::strong_count(&pool), 1);
    pool.close();
    assert_eq!(pool.work_revision(), Err(WorkError::Closed));
    assert_eq!(
        pool.validate_work_revision(&revision),
        Err(WorkError::Closed)
    );
    assert_eq!(pool.work_snapshot(&revision), Err(WorkError::Closed));
    assert!(!pool.acknowledge(observation.records()[0].token()));
    let weak_pool = Arc::downgrade(&pool);
    drop(pool);
    assert!(weak_pool.upgrade().is_none());
    assert_eq!(observation.records().len(), 1);
    assert!(other.snapshot().is_empty());
}

#[test]
fn work_snapshot_racing_acknowledgement_is_exact_or_stale() {
    let pool = ProcessLifecycleAttentionPool::new();
    for seed in 0..32 {
        let token = admitted(pool.report_terminal(&attempt(
            &pool,
            seed,
            LifecycleYieldOutcome::PhaseNeedsReview,
        )));
        let revision = pool.work_revision().unwrap();
        let barrier = Barrier::new(2);
        std::thread::scope(|scope| {
            let acknowledger = scope.spawn(|| {
                barrier.wait();
                assert!(pool.acknowledge(&token));
            });
            barrier.wait();
            match pool.work_snapshot(&revision) {
                Ok(snapshot) => {
                    assert_eq!(snapshot.records().len(), 1);
                    assert_eq!(snapshot.records()[0].token(), &token);
                }
                Err(error) => assert_eq!(error, WorkError::StaleRevision),
            }
            acknowledger.join().unwrap();
        });
        assert_eq!(
            pool.validate_work_revision(&revision),
            Err(WorkError::StaleRevision)
        );
        assert!(pool.snapshot().is_empty());
    }
}
