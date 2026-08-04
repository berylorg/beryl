use std::{
    num::NonZeroUsize,
    sync::{Arc, Barrier},
    thread,
};

use super::*;

#[test]
fn service_config_rejects_invalid_capacity_boundaries() {
    assert_eq!(
        ProjectionServiceConfig::try_new(0, MINIMUM_WORKER_CAPACITY as u64),
        Err(ProjectionServiceConfigError::ZeroPreBindControlCapacity)
    );
    assert_eq!(
        ProjectionServiceConfig::try_new(1, 0),
        Err(ProjectionServiceConfigError::ZeroWorkerCapacity)
    );
    for capacity in 1..MINIMUM_WORKER_CAPACITY {
        assert_eq!(
            ProjectionServiceConfig::try_new(1, capacity as u64),
            Err(ProjectionServiceConfigError::InsufficientWorkerCapacity {
                capacity,
                required: MINIMUM_WORKER_CAPACITY,
            })
        );
    }
}

#[test]
fn service_config_preserves_valid_fixed_counts() {
    let config = ProjectionServiceConfig::try_new(17, MINIMUM_WORKER_CAPACITY as u64).unwrap();

    assert_eq!(config.foreground().pre_bind_control_capacity().get(), 17);
    assert_eq!(config.worker_capacity().get(), MINIMUM_WORKER_CAPACITY);
}

#[derive(Clone, Copy, Debug)]
enum TestWorkerRole {
    Connection,
    ScheduledOrdinary,
    SteeringCritical,
}

impl TestWorkerRole {
    const fn permits(self) -> usize {
        match self {
            Self::Connection => CONNECTION_WORKER_PERMITS,
            Self::ScheduledOrdinary => SCHEDULED_ORDINARY_WORKER_PERMITS,
            Self::SteeringCritical => 1,
        }
    }
}

const ROLE_ORDERS: [[TestWorkerRole; 3]; 6] = [
    [
        TestWorkerRole::Connection,
        TestWorkerRole::ScheduledOrdinary,
        TestWorkerRole::SteeringCritical,
    ],
    [
        TestWorkerRole::Connection,
        TestWorkerRole::SteeringCritical,
        TestWorkerRole::ScheduledOrdinary,
    ],
    [
        TestWorkerRole::ScheduledOrdinary,
        TestWorkerRole::Connection,
        TestWorkerRole::SteeringCritical,
    ],
    [
        TestWorkerRole::ScheduledOrdinary,
        TestWorkerRole::SteeringCritical,
        TestWorkerRole::Connection,
    ],
    [
        TestWorkerRole::SteeringCritical,
        TestWorkerRole::Connection,
        TestWorkerRole::ScheduledOrdinary,
    ],
    [
        TestWorkerRole::SteeringCritical,
        TestWorkerRole::ScheduledOrdinary,
        TestWorkerRole::Connection,
    ],
];

#[derive(Default)]
struct HeldRoles {
    connection: Option<ProjectionWorkerPermitPair>,
    scheduled_ordinary: Option<ProjectionWorkerPermit>,
    steering_critical: Option<ProjectionWorkerPermit>,
}

impl HeldRoles {
    fn acquire(&mut self, pool: &ProjectionWorkerPool, role: TestWorkerRole) {
        match role {
            TestWorkerRole::Connection => {
                self.connection = Some(pool.try_acquire_pair().unwrap());
            }
            TestWorkerRole::ScheduledOrdinary => {
                self.scheduled_ordinary =
                    Some(pool.try_acquire_scheduled_ordinary_or_arm().unwrap());
            }
            TestWorkerRole::SteeringCritical => {
                self.steering_critical = Some(pool.try_acquire_steering_critical().unwrap());
            }
        }
    }

    fn release(&mut self, role: TestWorkerRole) {
        match role {
            TestWorkerRole::Connection => drop(self.connection.take().unwrap()),
            TestWorkerRole::ScheduledOrdinary => {
                drop(self.scheduled_ordinary.take().unwrap());
            }
            TestWorkerRole::SteeringCritical => {
                drop(self.steering_critical.take().unwrap());
            }
        }
    }
}

#[test]
fn minimum_capacity_supports_every_role_acquisition_and_drop_order() {
    for acquisition_order in ROLE_ORDERS {
        for drop_order in ROLE_ORDERS {
            let pool =
                ProjectionWorkerPool::new(NonZeroUsize::new(MINIMUM_WORKER_CAPACITY).unwrap());
            let mut held = HeldRoles::default();
            let mut active = 0;

            for role in acquisition_order {
                held.acquire(&pool, role);
                active += role.permits();
                assert_eq!(pool.diagnostics().active(), active);
                assert_eq!(
                    pool.diagnostics().available(),
                    MINIMUM_WORKER_CAPACITY - active
                );
            }
            assert_eq!(pool.diagnostics().high_water(), MINIMUM_WORKER_CAPACITY);

            for role in drop_order {
                held.release(role);
                active -= role.permits();
                assert_eq!(pool.diagnostics().active(), active);
                assert_eq!(
                    pool.diagnostics().available(),
                    MINIMUM_WORKER_CAPACITY - active
                );
            }
            assert_eq!(
                pool.diagnostics(),
                ProjectionWorkerPoolDiagnostics {
                    capacity: MINIMUM_WORKER_CAPACITY,
                    available: MINIMUM_WORKER_CAPACITY,
                    active: 0,
                    high_water: MINIMUM_WORKER_CAPACITY,
                    denied_pairs: 0,
                    denied_singles: 0,
                }
            );
        }
    }
}

#[test]
fn connection_pair_denial_preserves_the_steering_reserve_atomically() {
    let pool = ProjectionWorkerPool::new(NonZeroUsize::new(MINIMUM_WORKER_CAPACITY).unwrap());
    let pair = pool.try_acquire_pair().unwrap();
    let before_denial = pool.diagnostics();

    assert_eq!(before_denial.available(), 2);
    assert_eq!(before_denial.active(), 2);
    assert_eq!(
        pool.try_acquire_pair().err(),
        Some(ProjectionWorkerPermitError::CapacityFull { available: 2 })
    );
    let denied = pool.diagnostics();
    assert_eq!(denied.available(), before_denial.available());
    assert_eq!(denied.active(), before_denial.active());
    assert_eq!(denied.high_water(), before_denial.high_water());
    assert_eq!(denied.denied_pairs(), before_denial.denied_pairs() + 1);

    drop(pair);
    let released = pool.diagnostics();
    assert_eq!(released.available(), MINIMUM_WORKER_CAPACITY);
    assert_eq!(released.active(), 0);
    assert_eq!(released.high_water(), 2);
}

#[test]
fn scheduled_ordinary_never_satisfies_the_steering_reserve() {
    let pool = ProjectionWorkerPool::new(NonZeroUsize::new(5).unwrap());
    let scheduled = pool.try_acquire_scheduled_ordinary_or_arm().unwrap();
    let pair = pool.try_acquire_pair().unwrap();

    assert_eq!(pool.diagnostics().available(), 2);
    assert_eq!(pool.diagnostics().active(), 3);
    assert_eq!(
        pool.try_acquire_pair().err(),
        Some(ProjectionWorkerPermitError::CapacityFull { available: 2 })
    );

    drop(pair);
    drop(scheduled);
    assert_eq!(pool.diagnostics().available(), 5);
}

#[test]
fn steering_critical_allows_noncritical_roles_to_consume_final_capacity() {
    let pool = ProjectionWorkerPool::new(NonZeroUsize::new(MINIMUM_WORKER_CAPACITY).unwrap());
    let steering = pool.try_acquire_steering_critical().unwrap();
    let scheduled = pool.try_acquire_scheduled_ordinary_or_arm().unwrap();
    let pair = pool.try_acquire_pair().unwrap();

    assert_eq!(pool.diagnostics().available(), 0);
    assert_eq!(pool.diagnostics().active(), MINIMUM_WORKER_CAPACITY);

    drop(steering);
    assert_eq!(pool.diagnostics().available(), 1);
    assert_eq!(
        pool.try_acquire_scheduled_ordinary_or_arm().err(),
        Some(ProjectionWorkerPermitError::CapacityFull { available: 1 })
    );

    drop(pair);
    let second_scheduled = pool.try_acquire_scheduled_ordinary_or_arm().unwrap();
    drop(second_scheduled);
    drop(scheduled);
    assert_eq!(pool.diagnostics().available(), MINIMUM_WORKER_CAPACITY);
}

#[test]
fn steering_progresses_while_scheduled_ordinary_workers_hold_capacity() {
    let pool = ProjectionWorkerPool::new(NonZeroUsize::new(MINIMUM_WORKER_CAPACITY).unwrap());
    let scheduled = (0..MINIMUM_WORKER_CAPACITY - STEERING_CRITICAL_WORKER_RESERVE)
        .map(|_| pool.try_acquire_scheduled_ordinary_or_arm().unwrap())
        .collect::<Vec<_>>();

    assert_eq!(pool.diagnostics().available(), 1);
    assert_eq!(
        pool.try_acquire_scheduled_ordinary_or_arm().err(),
        Some(ProjectionWorkerPermitError::CapacityFull { available: 1 })
    );
    let steering = pool.try_acquire_steering_critical().unwrap();
    assert_eq!(pool.diagnostics().available(), 0);
    assert_eq!(pool.diagnostics().denied_singles(), 1);

    drop(steering);
    assert_eq!(pool.diagnostics().available(), 1);
    drop(scheduled);
    assert_eq!(pool.diagnostics().available(), MINIMUM_WORKER_CAPACITY);
    assert_eq!(pool.diagnostics().high_water(), MINIMUM_WORKER_CAPACITY);
}

#[test]
fn connection_pair_capacity_is_reused_at_minimum_and_larger_limits() {
    for capacity in [4, 5, 6, 7] {
        let pool = ProjectionWorkerPool::new(NonZeroUsize::new(capacity).unwrap());
        let admitted_pairs =
            (capacity - STEERING_CRITICAL_WORKER_RESERVE) / CONNECTION_WORKER_PERMITS;
        let residual = capacity - admitted_pairs * CONNECTION_WORKER_PERMITS;

        for denial_count in 1..=3 {
            let pairs = (0..admitted_pairs)
                .map(|_| pool.try_acquire_pair().unwrap())
                .collect::<Vec<_>>();
            let saturated = pool.diagnostics();
            assert_eq!(saturated.available(), residual);
            assert_eq!(saturated.active(), capacity - residual);
            assert_eq!(saturated.high_water(), capacity - residual);

            assert_eq!(
                pool.try_acquire_pair().err(),
                Some(ProjectionWorkerPermitError::CapacityFull {
                    available: residual,
                })
            );
            assert_eq!(pool.diagnostics().denied_pairs(), denial_count);

            drop(pairs);
            let released = pool.diagnostics();
            assert_eq!(released.available(), capacity);
            assert_eq!(released.active(), 0);
            assert_eq!(released.high_water(), capacity - residual);
        }
    }
}

#[test]
fn concurrent_connection_pair_admission_never_strands_one_permit() {
    let pool = ProjectionWorkerPool::new(NonZeroUsize::new(MINIMUM_WORKER_CAPACITY).unwrap());
    let start = Arc::new(Barrier::new(3));
    let attempts = (0..2)
        .map(|_| {
            let pool = pool.clone();
            let start = Arc::clone(&start);
            thread::spawn(move || {
                start.wait();
                pool.try_acquire_pair()
            })
        })
        .collect::<Vec<_>>();

    start.wait();
    let mut admitted = Vec::new();
    let mut denied = Vec::new();
    for attempt in attempts {
        match attempt.join().unwrap() {
            Ok(pair) => admitted.push(pair),
            Err(error) => denied.push(error),
        }
    }

    assert_eq!(admitted.len(), 1);
    assert_eq!(
        denied,
        vec![ProjectionWorkerPermitError::CapacityFull { available: 2 }]
    );
    assert_eq!(pool.diagnostics().available(), 2);
    assert_eq!(pool.diagnostics().active(), 2);
    assert_eq!(pool.diagnostics().denied_pairs(), 1);

    drop(admitted);
    assert_eq!(pool.diagnostics().available(), MINIMUM_WORKER_CAPACITY);
}

#[test]
fn role_denials_share_one_coalesced_worker_release_waiter() {
    let signal =
        crate::cas_projection::accepted_input_scheduler::AcceptedInputSchedulerSignal::new();
    let pool = ProjectionWorkerPool::new_with_scheduler(
        NonZeroUsize::new(MINIMUM_WORKER_CAPACITY).unwrap(),
        signal.clone(),
    );
    let pair = pool.try_acquire_pair().unwrap();
    let scheduled = pool.try_acquire_scheduled_ordinary_or_arm().unwrap();

    assert_eq!(
        pool.try_acquire_scheduled_ordinary_or_arm().err(),
        Some(ProjectionWorkerPermitError::CapacityFull { available: 1 })
    );
    let steering = pool.try_acquire_steering_critical().unwrap();
    assert_eq!(
        pool.try_acquire_steering_critical_quiet_or_arm().err(),
        Some(ProjectionWorkerPermitError::CapacityFull { available: 0 })
    );

    drop(pair);
    drop(scheduled);
    drop(steering);
    assert_eq!(signal.diagnostics().wake_count(), 2);
    assert_eq!(signal.diagnostics().coalesced_wake_count(), 0);
    assert_eq!(pool.diagnostics().denied_singles(), 2);
    assert_eq!(pool.diagnostics().available(), MINIMUM_WORKER_CAPACITY);
}

#[test]
fn scheduled_release_cannot_satisfy_its_own_capacity_waiter() {
    let signal =
        crate::cas_projection::accepted_input_scheduler::AcceptedInputSchedulerSignal::new();
    let pool = ProjectionWorkerPool::new_with_scheduler(
        NonZeroUsize::new(MINIMUM_WORKER_CAPACITY).unwrap(),
        signal.clone(),
    );
    let pair = pool.try_acquire_pair().unwrap();
    let scheduled = pool.try_acquire_scheduled_ordinary_or_arm().unwrap();

    assert_eq!(
        pool.try_acquire_scheduled_ordinary_or_arm().err(),
        Some(ProjectionWorkerPermitError::CapacityFull { available: 1 })
    );
    drop(scheduled);
    assert_eq!(signal.diagnostics().wake_count(), 0);

    drop(pair);
    assert_eq!(signal.diagnostics().wake_count(), 1);
    assert_eq!(pool.diagnostics().available(), MINIMUM_WORKER_CAPACITY);
}

#[test]
fn provisional_steering_scan_cannot_satisfy_scheduled_capacity_demand() {
    let signal =
        crate::cas_projection::accepted_input_scheduler::AcceptedInputSchedulerSignal::new();
    let pool = ProjectionWorkerPool::new_with_scheduler(
        NonZeroUsize::new(MINIMUM_WORKER_CAPACITY).unwrap(),
        signal.clone(),
    );
    let pair = pool.try_acquire_pair().unwrap();
    let scheduled = pool.try_acquire_scheduled_ordinary_or_arm().unwrap();

    assert_eq!(
        pool.try_acquire_scheduled_ordinary_or_arm().err(),
        Some(ProjectionWorkerPermitError::CapacityFull { available: 1 })
    );
    let provisional_steering = pool.try_acquire_steering_critical().unwrap();
    drop(provisional_steering);
    assert_eq!(signal.diagnostics().wake_count(), 0);

    drop(scheduled);
    assert_eq!(signal.diagnostics().wake_count(), 0);
    drop(pair);
    assert_eq!(signal.diagnostics().wake_count(), 1);
    assert_eq!(pool.diagnostics().available(), MINIMUM_WORKER_CAPACITY);
}

#[test]
fn recovery_hold_restores_the_exact_scheduled_permit_without_reacquisition() {
    let pool = ProjectionWorkerPool::new(NonZeroUsize::new(MINIMUM_WORKER_CAPACITY).unwrap());
    let worker = pool.try_acquire_scheduled_ordinary_or_arm().unwrap();
    let admission_identity = worker.admission_identity_for_test();
    let admitted = pool.diagnostics();
    let hold = match worker.into_preactivation_recovery_hold() {
        Ok(hold) => hold,
        Err(_) => panic!("scheduled permit must convert into a recovery hold"),
    };

    assert_eq!(pool.diagnostics(), admitted);
    let restored = hold.restore_worker_for_test();
    assert_eq!(restored.admission_identity_for_test(), admission_identity);
    assert_eq!(pool.diagnostics(), admitted);

    let hold = match restored.into_preactivation_recovery_hold() {
        Ok(hold) => hold,
        Err(_) => panic!("restored scheduled permit must convert into a recovery hold"),
    };
    assert_eq!(pool.diagnostics(), admitted);
    drop(hold);
    let released = pool.diagnostics();
    assert_eq!(released.active(), 0);
    assert_eq!(released.available(), MINIMUM_WORKER_CAPACITY);
    assert_eq!(released.high_water(), 1);
    assert_eq!(released.denied_singles(), 0);
}

#[cfg(target_pointer_width = "32")]
#[test]
fn service_config_rejects_cross_platform_unrepresentable_counts() {
    assert!(matches!(
        ProjectionServiceConfig::try_new(u64::MAX, 2),
        Err(
            ProjectionServiceConfigError::UnrepresentablePreBindControlCapacity {
                capacity: u64::MAX
            }
        )
    ));
    assert!(matches!(
        ProjectionServiceConfig::try_new(1, u64::MAX),
        Err(ProjectionServiceConfigError::UnrepresentableWorkerCapacity { capacity: u64::MAX })
    ));
}
