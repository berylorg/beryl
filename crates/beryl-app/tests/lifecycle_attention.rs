use std::sync::{Arc, Barrier};

use beryl_app::{
    LifecycleYieldOutcome,
    lifecycle_attention::{
        LifecycleAttentionAdmission as Admission, LifecycleAttentionAttempt,
        LifecycleAttentionKind as Kind, LifecycleAttentionRejection as Rejection,
        LifecycleAttentionToken, ProcessLifecycleAttentionPool,
    },
    main_window::NOTICE_RECORD_CAPACITY,
};
use beryl_model::{BerylHomeId, SyndicThreadId, SyndicTurnId};

fn attempt(
    pool: &ProcessLifecycleAttentionPool,
    turn: u8,
    outcome: LifecycleYieldOutcome,
) -> LifecycleAttentionAttempt {
    pool.track_accepted_yield(
        BerylHomeId::from_bytes([1; 16]),
        SyndicThreadId::from_bytes([2; 16]),
        SyndicTurnId::from_bytes([turn; 16]),
        outcome,
    )
    .unwrap()
}

fn admitted(result: Admission) -> LifecycleAttentionToken {
    let Admission::Admitted(token) = result else {
        panic!("expected admission, got {result:?}");
    };
    token
}

#[test]
fn terminal_outcomes_retain_exact_host_facts_in_fifo_order() {
    let pool = ProcessLifecycleAttentionPool::new();
    let cases = [
        (LifecycleYieldOutcome::PhaseNeedsReview, Kind::ReviewReady),
        (
            LifecycleYieldOutcome::BlockedNeedsOperator,
            Kind::OperatorAttention,
        ),
        (LifecycleYieldOutcome::PlanComplete, Kind::PlanComplete),
    ];
    for (index, (outcome, _)) in cases.iter().enumerate() {
        admitted(pool.report_terminal(&attempt(&pool, index as u8, *outcome)));
    }
    let records = pool.snapshot();
    assert_eq!(records.len(), cases.len());
    for (index, (record, (outcome, kind))) in records.iter().zip(cases).enumerate() {
        assert_eq!(record.home_id(), BerylHomeId::from_bytes([1; 16]));
        assert_eq!(record.thread_id(), SyndicThreadId::from_bytes([2; 16]));
        assert_eq!(
            record.turn_id(),
            SyndicTurnId::from_bytes([index as u8; 16])
        );
        assert_eq!(record.outcome(), outcome);
        assert_eq!(record.kind(), kind);
        assert_eq!(record.report_count(), 1);
    }
}

#[test]
fn repeated_observation_updates_then_cannot_recreate_acknowledged_record() {
    let pool = ProcessLifecycleAttentionPool::new();
    let yielding = attempt(&pool, 3, LifecycleYieldOutcome::PhaseNeedsReview);
    let token = admitted(pool.report_terminal(&yielding));
    assert_eq!(
        pool.report_terminal(&yielding),
        Admission::Updated(token.clone())
    );
    assert_eq!(pool.snapshot()[0].report_count(), 2);
    assert!(pool.acknowledge(&token));
    assert_eq!(pool.report_terminal(&yielding), Admission::AlreadyReported);
    assert!(pool.snapshot().is_empty());
    let later = attempt(&pool, 4, LifecycleYieldOutcome::PhaseNeedsReview);
    let later_token = admitted(pool.report_terminal(&later));
    assert!(!pool.acknowledge(&token));
    assert_eq!(pool.snapshot()[0].token(), &later_token);
}

#[test]
fn continuation_only_reports_explicit_failure_and_invalid_event_does_not_latch() {
    let pool = ProcessLifecycleAttentionPool::new();
    let continuing = attempt(&pool, 3, LifecycleYieldOutcome::PhaseContinue);
    assert_eq!(pool.report_terminal(&continuing), Admission::NotRequested);
    assert!(pool.snapshot().is_empty());
    let token = admitted(pool.report_continuation_failure(&continuing));
    assert_eq!(pool.snapshot()[0].kind(), Kind::ContinuationFailed);
    assert_eq!(
        pool.report_continuation_failure(&continuing),
        Admission::Updated(token.clone())
    );
    assert!(pool.acknowledge(&token));
    assert_eq!(
        pool.report_continuation_failure(&continuing),
        Admission::AlreadyReported
    );
    let review = attempt(&pool, 4, LifecycleYieldOutcome::PhaseNeedsReview);
    assert_eq!(
        pool.report_continuation_failure(&review),
        Admission::NotRequested
    );
    admitted(pool.report_terminal(&review));
}

#[test]
fn capacity_omits_newest_without_overflow_or_delayed_retry() {
    let pool = ProcessLifecycleAttentionPool::new();
    for index in 0..NOTICE_RECORD_CAPACITY {
        admitted(pool.report_terminal(&attempt(
            &pool,
            index as u8,
            LifecycleYieldOutcome::PhaseNeedsReview,
        )));
    }
    let snapshot = pool.snapshot();
    let omitted = attempt(&pool, 100, LifecycleYieldOutcome::PlanComplete);
    assert_eq!(pool.report_terminal(&omitted), Admission::Omitted);
    assert_eq!(pool.snapshot().len(), NOTICE_RECORD_CAPACITY);
    assert_eq!(pool.snapshot()[0].token(), snapshot[0].token());
    assert!(pool.acknowledge(snapshot[0].token()));
    assert_eq!(pool.report_terminal(&omitted), Admission::AlreadyReported);
    assert_eq!(pool.snapshot().len(), NOTICE_RECORD_CAPACITY - 1);
    admitted(pool.report_terminal(&attempt(&pool, 101, LifecycleYieldOutcome::PlanComplete)));
    assert_eq!(
        pool.snapshot().last().unwrap().turn_id(),
        SyndicTurnId::from_bytes([101; 16])
    );
    assert_eq!(pool.diagnostics().omitted, 1);
}

#[test]
fn foreign_pool_cannot_consume_attempt_or_acknowledge_record() {
    let origin = ProcessLifecycleAttentionPool::new();
    let foreign = ProcessLifecycleAttentionPool::new();
    let yielding = attempt(&origin, 3, LifecycleYieldOutcome::PlanComplete);
    assert_eq!(
        foreign.report_terminal(&yielding),
        Admission::Rejected(Rejection::ForeignAttempt)
    );
    let token = admitted(origin.report_terminal(&yielding));
    assert!(!foreign.acknowledge(&token));
    assert_eq!(origin.snapshot().len(), 1);
    assert!(origin.acknowledge(&token));
}

#[test]
fn close_discards_records_and_fences_outstanding_attempts_and_tokens() {
    let pool = ProcessLifecycleAttentionPool::new();
    let yielding = attempt(&pool, 3, LifecycleYieldOutcome::PlanComplete);
    let token = admitted(pool.report_terminal(&yielding));
    let pending = attempt(&pool, 4, LifecycleYieldOutcome::PhaseContinue);
    pool.close();
    pool.close();
    assert!(pool.snapshot().is_empty());
    assert!(!pool.acknowledge(&token));
    assert_eq!(
        pool.report_terminal(&yielding),
        Admission::Rejected(Rejection::Closed)
    );
    assert_eq!(
        pool.report_continuation_failure(&pending),
        Admission::Rejected(Rejection::Closed)
    );
    assert!(
        pool.track_accepted_yield(
            BerylHomeId::from_bytes([1; 16]),
            SyndicThreadId::from_bytes([2; 16]),
            SyndicTurnId::from_bytes([5; 16]),
            LifecycleYieldOutcome::PlanComplete,
        )
        .is_none()
    );
}

#[test]
fn concurrent_reports_admit_once_and_close_remains_final() {
    let pool = Arc::new(ProcessLifecycleAttentionPool::new());
    let yielding = attempt(&pool, 3, LifecycleYieldOutcome::PhaseNeedsReview);
    let barrier = Arc::new(Barrier::new(9));
    let results = std::thread::scope(|scope| {
        let mut workers = Vec::new();
        for _ in 0..8 {
            workers.push(scope.spawn(|| {
                barrier.wait();
                pool.report_terminal(&yielding)
            }));
        }
        barrier.wait();
        workers
            .into_iter()
            .map(|worker| worker.join().unwrap())
            .collect::<Vec<_>>()
    });
    assert_eq!(
        results
            .iter()
            .filter(|result| matches!(result, Admission::Admitted(_)))
            .count(),
        1
    );
    assert_eq!(pool.snapshot()[0].report_count(), 8);
    let barrier = Barrier::new(3);
    std::thread::scope(|scope| {
        scope.spawn(|| {
            barrier.wait();
            pool.report_terminal(&yielding);
        });
        scope.spawn(|| {
            barrier.wait();
            pool.close();
        });
        barrier.wait();
    });
    assert!(pool.snapshot().is_empty());
    assert_eq!(
        pool.report_terminal(&yielding),
        Admission::Rejected(Rejection::Closed)
    );
}

#[test]
fn retained_snapshots_and_attempts_do_not_keep_process_pool_alive() {
    let pool = Arc::new(ProcessLifecycleAttentionPool::new());
    let weak = Arc::downgrade(&pool);
    let yielding = attempt(&pool, 3, LifecycleYieldOutcome::PlanComplete);
    let token = admitted(pool.report_terminal(&yielding));
    let snapshot = pool.snapshot();
    drop(pool);
    assert!(weak.upgrade().is_none());
    assert_eq!(snapshot[0].token(), &token);
    let replacement = ProcessLifecycleAttentionPool::new();
    assert!(!replacement.acknowledge(&token));
    assert_eq!(
        replacement.report_terminal(&yielding),
        Admission::Rejected(Rejection::ForeignAttempt)
    );
    assert!(replacement.snapshot().is_empty());
}
