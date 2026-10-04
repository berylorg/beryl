#![cfg(feature = "test-faults")]

use beryl_app::{
    process_admission::ProcessAdmissionGate,
    window_acquisition::{
        RuntimeBackedWindowMainWindowReservationError, RuntimeBackedWindowProcessRegistry,
    },
};
use beryl_model::WindowId;
use std::{sync::mpsc, time::Duration};

fn window(seed: u8) -> WindowId {
    WindowId::from_bytes([seed; 16])
}

#[test]
fn exact_selection_custody_excludes_creation_close_and_process_closing() {
    let registry = RuntimeBackedWindowProcessRegistry::new(ProcessAdmissionGate::new());
    let resident = registry.reserve_main_window(window(1)).unwrap();
    assert!(!registry.test_process_closing_is_blocked());
    assert!(
        registry
            .test_admit_selection(&[window(1), window(1)], window(1))
            .is_err()
    );
    assert!(
        registry
            .test_admit_selection(&[window(1)], window(2))
            .is_err()
    );
    let lease = registry
        .test_admit_selection(&[window(1)], window(1))
        .unwrap();
    assert!(lease.validate_publication().is_ok());
    assert!(registry.test_close_is_blocked(&[window(1)]));
    assert!(registry.test_process_closing_is_blocked());
    assert!(registry.test_acquisition_is_blocked(window(2)));
    assert!(matches!(
        registry.reserve_main_window(window(2)),
        Err(RuntimeBackedWindowMainWindowReservationError::SelectionInProgress)
    ));
    assert!(
        registry
            .test_admit_selection(&[window(1)], window(1))
            .is_err()
    );
    assert_eq!(lease.admit_commit(|| 42).unwrap(), 42);
    lease
        .admit_commit(|| {
            assert!(
                registry
                    .test_admit_selection(&[window(1)], window(1))
                    .is_err()
            );
            assert!(lease.validate_publication().is_err());
        })
        .unwrap();
    drop(lease);
    assert!(!registry.test_process_closing_is_blocked());
    assert!(!registry.test_close_is_blocked(&[window(1)]));
    let second = registry.reserve_main_window(window(2)).unwrap();
    assert!(
        registry
            .test_admit_selection(&[window(1)], window(1))
            .is_err()
    );
    let lease = registry
        .test_admit_selection(&[window(1), window(2)], window(1))
        .unwrap();
    drop(second);
    assert!(lease.validate_publication().is_err());
    let mut ran = false;
    assert!(lease.admit_commit(|| ran = true).is_err());
    assert!(!ran);
    drop(lease);
    drop(resident);
}

#[test]
fn commit_guard_preserves_membership_until_home_command_returns() {
    let registry = RuntimeBackedWindowProcessRegistry::new(ProcessAdmissionGate::new());
    let resident = registry.reserve_main_window(window(1)).unwrap();
    let lease = registry
        .test_admit_selection(&[window(1)], window(1))
        .unwrap();
    let (start_tx, start_rx) = mpsc::channel();
    let (done_tx, done_rx) = mpsc::channel();
    let worker = std::thread::spawn(move || {
        start_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        done_tx.send("started").unwrap();
        drop(resident);
        done_tx.send("dropped").unwrap();
    });
    lease
        .admit_commit(|| {
            start_tx.send(()).unwrap();
            assert_eq!(
                done_rx.recv_timeout(Duration::from_secs(5)).unwrap(),
                "started"
            );
            assert!(matches!(done_rx.try_recv(), Err(mpsc::TryRecvError::Empty)));
        })
        .unwrap();
    assert_eq!(
        done_rx.recv_timeout(Duration::from_secs(5)).unwrap(),
        "dropped"
    );
    worker.join().unwrap();
    assert!(lease.validate_publication().is_err());
}

#[test]
fn dropping_stale_lease_does_not_release_replacement_owner() {
    let registry = RuntimeBackedWindowProcessRegistry::new(ProcessAdmissionGate::new());
    let _resident = registry.reserve_main_window(window(1)).unwrap();
    let lease = registry
        .test_admit_selection(&[window(1)], window(1))
        .unwrap();
    lease.test_replace_owner();
    assert!(lease.validate_publication().is_err());
    drop(lease);
    assert!(
        registry
            .test_admit_selection(&[window(1)], window(1))
            .is_err()
    );
}
