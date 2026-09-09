use super::*;

fn attempt(pool: &ProcessLifecycleAttentionPool, seed: u8) -> LifecycleAttentionAttempt {
    pool.track_accepted_yield(
        BerylHomeId::from_bytes([1; 16]),
        SyndicThreadId::from_bytes([2; 16]),
        SyndicTurnId::from_bytes([seed; 16]),
        LifecycleYieldOutcome::PlanComplete,
    )
    .unwrap()
}

#[test]
fn exhausted_work_revision_never_suppresses_attention_admission_or_acknowledgement() {
    let pool = ProcessLifecycleAttentionPool::new();
    pool.state.lock().unwrap().revision = Some(u64::MAX);
    let revision = pool.work_revision().unwrap();
    let LifecycleAttentionAdmission::Admitted(token) = pool.report_terminal(&attempt(&pool, 1))
    else {
        panic!("attention admission must survive revision exhaustion");
    };
    assert_eq!(
        pool.work_revision(),
        Err(LifecycleAttentionWorkError::RevisionUnavailable)
    );
    assert_eq!(
        pool.validate_work_revision(&revision),
        Err(LifecycleAttentionWorkError::RevisionUnavailable)
    );
    assert_eq!(
        pool.work_snapshot(&revision),
        Err(LifecycleAttentionWorkError::RevisionUnavailable)
    );
    assert_eq!(pool.snapshot().len(), 1);
    assert!(pool.acknowledge(&token));
    assert!(matches!(
        pool.report_terminal(&attempt(&pool, 2)),
        LifecycleAttentionAdmission::Admitted(_)
    ));
    assert_eq!(pool.state.lock().unwrap().revision, None);
    pool.close();
    assert!(pool.snapshot().is_empty());
    assert_eq!(
        pool.work_revision(),
        Err(LifecycleAttentionWorkError::Closed)
    );
}

#[test]
fn poisoned_work_observation_does_not_change_existing_attention_cleanup() {
    let pool = ProcessLifecycleAttentionPool::new();
    let revision = pool.work_revision().unwrap();
    assert!(
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _state = pool.state.lock().unwrap();
            panic!("injected attention source poison");
        }))
        .is_err()
    );
    assert_eq!(
        pool.work_revision(),
        Err(LifecycleAttentionWorkError::Poisoned)
    );
    assert_eq!(
        pool.validate_work_revision(&revision),
        Err(LifecycleAttentionWorkError::Poisoned)
    );
    assert_eq!(
        pool.work_snapshot(&revision),
        Err(LifecycleAttentionWorkError::Poisoned)
    );
    let LifecycleAttentionAdmission::Admitted(token) = pool.report_terminal(&attempt(&pool, 1))
    else {
        panic!("existing recovered attention admission must remain usable");
    };
    assert!(pool.acknowledge(&token));
    pool.close();
    assert!(pool.snapshot().is_empty());
    assert_eq!(
        pool.work_revision(),
        Err(LifecycleAttentionWorkError::Poisoned)
    );
}
