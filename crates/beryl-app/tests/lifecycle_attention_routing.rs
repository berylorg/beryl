use beryl_app::{LifecycleYieldOutcome, lifecycle_attention::*};
use beryl_model::{BerylHomeId, SyndicThreadId, SyndicTurnId, WindowId};

fn pool_record(
    pool: &ProcessLifecycleAttentionPool,
    thread: u8,
    turn: u8,
) -> LifecycleAttentionRecord {
    let attempt = pool
        .track_accepted_yield(
            BerylHomeId::from_bytes([1; 16]),
            SyndicThreadId::from_bytes([thread; 16]),
            SyndicTurnId::from_bytes([turn; 16]),
            LifecycleYieldOutcome::PhaseNeedsReview,
        )
        .unwrap();
    assert!(matches!(
        pool.report_terminal(&attempt),
        LifecycleAttentionAdmission::Admitted(_)
    ));
    pool.snapshot()
        .into_iter()
        .find(|record| record.turn_id() == SyndicTurnId::from_bytes([turn; 16]))
        .unwrap()
}

fn window(id: u8, thread: Option<u8>) -> LifecycleAttentionWindow {
    LifecycleAttentionWindow {
        window_id: WindowId::from_bytes([id; 16]),
        viewed_thread: thread.map(|thread| SyndicThreadId::from_bytes([thread; 16])),
    }
}

#[test]
fn viewed_thread_routes_to_its_exact_window_and_unviewed_uses_smallest_identity() {
    let pool = ProcessLifecycleAttentionPool::new();
    let first = pool_record(&pool, 9, 1);
    let second = pool_record(&pool, 8, 2);
    let mut router = LifecycleAttentionRouter::default();
    let changes = router
        .reconcile(
            &pool.snapshot(),
            &[window(7, Some(9)), window(2, None), window(5, Some(3))],
        )
        .unwrap();
    assert_eq!(
        changes,
        vec![
            LifecycleAttentionRouteChange::Offer {
                record: first,
                window_id: WindowId::from_bytes([7; 16])
            },
            LifecycleAttentionRouteChange::Offer {
                record: second,
                window_id: WindowId::from_bytes([2; 16])
            }
        ]
    );
    assert_eq!(router.diagnostics().retained_routes, 2);
}

#[test]
fn every_previous_destination_is_removed_before_any_new_offer() {
    let pool = ProcessLifecycleAttentionPool::new();
    let first = pool_record(&pool, 9, 1);
    let second = pool_record(&pool, 8, 2);
    let mut router = LifecycleAttentionRouter::default();
    router
        .reconcile(&pool.snapshot(), &[window(7, Some(9)), window(2, Some(8))])
        .unwrap();
    let changes = router
        .reconcile(&pool.snapshot(), &[window(7, Some(8)), window(2, Some(9))])
        .unwrap();
    assert_eq!(
        changes,
        vec![
            LifecycleAttentionRouteChange::Remove {
                token: first.token().clone(),
                window_id: WindowId::from_bytes([7; 16])
            },
            LifecycleAttentionRouteChange::Remove {
                token: second.token().clone(),
                window_id: WindowId::from_bytes([2; 16])
            },
            LifecycleAttentionRouteChange::Offer {
                record: first,
                window_id: WindowId::from_bytes([2; 16])
            },
            LifecycleAttentionRouteChange::Offer {
                record: second,
                window_id: WindowId::from_bytes([7; 16])
            },
        ]
    );
    assert_eq!(pool.snapshot().len(), 2);
}

#[test]
fn detachment_and_no_windows_preserve_pool_records_without_acknowledgement() {
    let pool = ProcessLifecycleAttentionPool::new();
    let record = pool_record(&pool, 9, 1);
    let mut router = LifecycleAttentionRouter::default();
    router
        .reconcile(&pool.snapshot(), &[window(7, Some(9)), window(2, None)])
        .unwrap();
    let detached = router
        .reconcile(&pool.snapshot(), &[window(7, None), window(2, None)])
        .unwrap();
    assert!(
        matches!(&detached[0], LifecycleAttentionRouteChange::Remove { window_id, .. } if *window_id == WindowId::from_bytes([7; 16]))
    );
    let removed = router.reconcile(&pool.snapshot(), &[]).unwrap();
    assert_eq!(
        removed,
        vec![LifecycleAttentionRouteChange::Remove {
            token: record.token().clone(),
            window_id: WindowId::from_bytes([2; 16])
        }]
    );
    assert_eq!(pool.snapshot(), vec![record]);
    assert_eq!(router.diagnostics().retained_routes, 0);
}

#[test]
fn successor_pool_and_acknowledgement_clear_only_exact_previous_routes() {
    let previous = ProcessLifecycleAttentionPool::new();
    let old = pool_record(&previous, 9, 1);
    let successor = ProcessLifecycleAttentionPool::new();
    let new = pool_record(&successor, 9, 1);
    let mut router = LifecycleAttentionRouter::default();
    router
        .reconcile(&previous.snapshot(), &[window(2, None)])
        .unwrap();
    let changes = router
        .reconcile(&successor.snapshot(), &[window(2, None)])
        .unwrap();
    assert_eq!(
        changes,
        vec![
            LifecycleAttentionRouteChange::Remove {
                token: old.token().clone(),
                window_id: WindowId::from_bytes([2; 16])
            },
            LifecycleAttentionRouteChange::Offer {
                record: new.clone(),
                window_id: WindowId::from_bytes([2; 16])
            }
        ]
    );
    assert!(!successor.acknowledge(old.token()));
    assert!(successor.acknowledge(new.token()));
    assert_eq!(
        router
            .reconcile(&successor.snapshot(), &[window(2, None)])
            .unwrap()
            .len(),
        1
    );
    assert_eq!(router.diagnostics().retained_routes, 0);
}

#[test]
fn route_capacity_is_fixed_and_invalid_snapshots_preserve_existing_routes() {
    let pool = ProcessLifecycleAttentionPool::new();
    for id in 1..=beryl_app::main_window::NOTICE_RECORD_CAPACITY {
        pool_record(&pool, id as u8, id as u8);
    }
    let records = pool.try_snapshot().unwrap();
    let mut router = LifecycleAttentionRouter::default();
    router.reconcile(&records, &[window(1, None)]).unwrap();
    assert_eq!(
        router.diagnostics().retained_routes,
        beryl_app::main_window::NOTICE_RECORD_CAPACITY
    );
    assert!(
        router
            .reconcile(
                &vec![records[0].clone(); beryl_app::main_window::NOTICE_RECORD_CAPACITY + 1],
                &[window(2, None)]
            )
            .is_none()
    );
    assert_eq!(
        router.diagnostics().retained_routes,
        beryl_app::main_window::NOTICE_RECORD_CAPACITY
    );
    assert_eq!(
        router.clear().len(),
        beryl_app::main_window::NOTICE_RECORD_CAPACITY
    );
    assert_eq!(
        pool.try_snapshot().unwrap().len(),
        beryl_app::main_window::NOTICE_RECORD_CAPACITY
    );
    pool.close();
    assert!(pool.try_snapshot().is_none());
}
