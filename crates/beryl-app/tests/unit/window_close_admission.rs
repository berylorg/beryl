use super::*;
use std::{sync::Barrier, thread};

fn id(value: u8) -> WindowId {
    WindowId::from_bytes([value; 16])
}

#[test]
fn shutdown_validation_requires_exact_gate_registry_member_and_available_custody() {
    let process = crate::process_admission::ProcessAdmissionGate::new();
    let permit = process.execution_permit();
    let registry = RuntimeBackedWindowProcessRegistry::new(process.clone());
    let other_registry = RuntimeBackedWindowProcessRegistry::new(process.clone());
    let _resident = registry.reserve_main_window(id(1)).unwrap();
    let lease = registry
        .admit_close(registry.snapshot_for_close(&[id(1)]).unwrap())
        .unwrap();
    assert!(matches!(
        other_registry.prepare_shutdown_admission(&lease, id(1), true),
        Err(WindowCloseAdmissionError::WindowSetChanged)
    ));
    let window = registry
        .prepare_shutdown_admission(&lease, id(1), true)
        .unwrap();
    let foreign = crate::process_admission::ProcessAdmissionGate::new();
    assert_eq!(
        window.validate(&foreign.prepare_closing().unwrap()),
        Err(WindowCloseAdmissionError::WindowSetChanged)
    );
    {
        let closing = process.prepare_closing().unwrap();
        assert_eq!(window.validate(&closing), Ok(()));
        assert_eq!(
            registry
                .prepare_shutdown_admission(&lease, id(2), false)
                .unwrap()
                .validate(&closing),
            Err(WindowCloseAdmissionError::WindowSetChanged)
        );
        let held = registry.flights.lock().unwrap();
        assert_eq!(
            window.validate(&closing),
            Err(WindowCloseAdmissionError::Busy)
        );
        drop(held);
        assert_eq!(window.validate(&closing), Ok(()));
    }
    permit.commit(|| ()).unwrap();
    // A lost exact close owner cannot be replaced by membership equality.
    registry.flights.lock().unwrap().close_owner = Some(Arc::new(()));
    assert_eq!(
        window.validate(&process.prepare_closing().unwrap()),
        Err(WindowCloseAdmissionError::WindowSetChanged)
    );
    permit.commit(|| ()).unwrap();
}

#[test]
fn closing_guard_protects_validated_membership_until_fence_publication() {
    let process = crate::process_admission::ProcessAdmissionGate::new();
    let permit = process.execution_permit();
    let registry = RuntimeBackedWindowProcessRegistry::new(process.clone());
    let resident = registry.reserve_main_window(id(1)).unwrap();
    let lease = registry
        .admit_close(registry.snapshot_for_close(&[id(1)]).unwrap())
        .unwrap();
    let window = registry
        .prepare_shutdown_admission(&lease, id(1), true)
        .unwrap();
    let closing = process.prepare_closing().unwrap();
    window.validate(&closing).unwrap();
    let (started_tx, started_rx) = std::sync::mpsc::channel();
    let (done_tx, done_rx) = std::sync::mpsc::channel();
    let worker = thread::spawn(move || {
        started_tx.send(()).unwrap();
        drop(resident);
        done_tx.send(()).unwrap();
    });
    started_rx
        .recv_timeout(std::time::Duration::from_secs(5))
        .unwrap();
    assert_eq!(
        done_rx.try_recv(),
        Err(std::sync::mpsc::TryRecvError::Empty)
    );
    window.validate(&closing).unwrap();
    let fence = closing.publish();
    done_rx
        .recv_timeout(std::time::Duration::from_secs(5))
        .unwrap();
    worker.join().unwrap();
    assert_eq!(permit.commit(|| ()), Err(ProcessAdmissionError::Fenced));
    fence.reopen_if(true).unwrap();
    assert_eq!(
        window.validate(&process.prepare_closing().unwrap()),
        Err(WindowCloseAdmissionError::WindowSetChanged)
    );
}

#[test]
fn poisoned_window_registry_refuses_unpublished_shutdown_validation() {
    let process = crate::process_admission::ProcessAdmissionGate::new();
    let permit = process.execution_permit();
    let registry = RuntimeBackedWindowProcessRegistry::new(process.clone());
    let _resident = registry.reserve_main_window(id(1)).unwrap();
    let lease = registry
        .admit_close(registry.snapshot_for_close(&[id(1)]).unwrap())
        .unwrap();
    let poisoned = registry.flights.clone();
    assert!(
        thread::spawn(move || {
            let _held = poisoned.lock().unwrap();
            panic!("poison window registry");
        })
        .join()
        .is_err()
    );
    let window = registry
        .prepare_shutdown_admission(&lease, id(1), true)
        .unwrap();
    assert_eq!(
        window.validate(&process.prepare_closing().unwrap()),
        Err(WindowCloseAdmissionError::Unavailable)
    );
    permit.commit(|| ()).unwrap();
}

#[test]
fn inspection_preserves_execution_and_construction_and_cannot_authorize_later_close() {
    let process = crate::process_admission::ProcessAdmissionGate::new();
    let permit = process.execution_permit();
    let registry = RuntimeBackedWindowProcessRegistry::new(process);
    let _resident = registry.reserve_main_window(id(1)).unwrap();
    let snapshot = registry.snapshot_for_close(&[id(1)]).unwrap();
    assert_eq!(registry.inspect_close_snapshot(&snapshot, id(1)), Ok(true));
    assert_eq!(permit.commit(|| 42), Ok(42));
    let second = registry.reserve_main_window(id(2)).unwrap();
    assert_eq!(
        registry.inspect_close_snapshot(&snapshot, id(1)),
        Err(WindowCloseAdmissionError::WindowSetChanged)
    );
    let both = registry.snapshot_for_close(&[id(1), id(2)]).unwrap();
    assert_eq!(registry.inspect_close_snapshot(&both, id(1)), Ok(false));
    assert_eq!(registry.inspect_close_snapshot(&both, id(2)), Ok(false));
    drop(second);
    assert_eq!(
        registry.inspect_close_snapshot(&snapshot, id(1)),
        Err(WindowCloseAdmissionError::WindowSetChanged)
    );
    assert!(matches!(
        registry.admit_close(snapshot),
        Err(WindowCloseAdmissionError::WindowSetChanged)
    ));
    assert_eq!(permit.commit(|| 43), Ok(43));
}

#[test]
fn inspection_refuses_foreign_absent_busy_and_fenced_authority_without_changing_it() {
    let process = crate::process_admission::ProcessAdmissionGate::new();
    let permit = process.execution_permit();
    let registry = RuntimeBackedWindowProcessRegistry::new(process.clone());
    let foreign = RuntimeBackedWindowProcessRegistry::new(Default::default());
    let _resident = registry.reserve_main_window(id(1)).unwrap();
    let _foreign = foreign.reserve_main_window(id(1)).unwrap();
    let snapshot = registry.snapshot_for_close(&[id(1)]).unwrap();
    assert_eq!(
        foreign.inspect_close_snapshot(&snapshot, id(1)),
        Err(WindowCloseAdmissionError::WindowSetChanged)
    );
    assert_eq!(
        registry.inspect_close_snapshot(&snapshot, id(2)),
        Err(WindowCloseAdmissionError::WindowSetChanged)
    );
    let lease = registry
        .admit_close(registry.snapshot_for_close(&[id(1)]).unwrap())
        .unwrap();
    assert_eq!(
        registry.inspect_close_snapshot(&snapshot, id(1)),
        Err(WindowCloseAdmissionError::Busy)
    );
    assert_eq!(lease.is_final(id(1)), Ok(true));
    assert_eq!(permit.commit(|| ()), Ok(()));
    drop(lease);
    assert_eq!(registry.inspect_close_snapshot(&snapshot, id(1)), Ok(true));
    let _fence = process.fence().unwrap();
    assert_eq!(
        registry.inspect_close_snapshot(&snapshot, id(1)),
        Err(WindowCloseAdmissionError::Process(
            ProcessAdmissionError::Fenced
        ))
    );
    assert_eq!(permit.commit(|| ()), Err(ProcessAdmissionError::Fenced));
}

#[test]
fn pending_construction_and_incomplete_resident_sets_cannot_designate_final() {
    let registry = RuntimeBackedWindowProcessRegistry::new(Default::default());
    let _resident = registry.reserve_main_window(id(1)).unwrap();
    let pending = registry.reserve_main_window(id(2)).unwrap();
    assert!(matches!(
        registry.snapshot_for_close(&[id(1)]),
        Err(WindowCloseAdmissionError::WindowSetChanged)
    ));
    drop(pending);
    let snapshot = registry.snapshot_for_close(&[id(1)]).unwrap();
    let lease = registry.admit_close(snapshot).unwrap();
    assert_eq!(lease.is_final(id(1)), Ok(true));
    assert_eq!(
        lease.is_final(id(2)),
        Err(WindowCloseAdmissionError::WindowSetChanged)
    );
}

#[test]
fn cancellation_preserves_execution_authority_and_releases_only_close_exclusion() {
    let process = crate::process_admission::ProcessAdmissionGate::new();
    let permit = process.execution_permit();
    let registry = RuntimeBackedWindowProcessRegistry::new(process);
    let _first = registry.reserve_main_window(id(1)).unwrap();
    let _second = registry.reserve_main_window(id(2)).unwrap();
    let snapshot = registry.snapshot_for_close(&[id(1), id(2)]).unwrap();
    let competing = registry.snapshot_for_close(&[id(2), id(1)]).unwrap();
    let lease = registry.admit_close(snapshot).unwrap();
    assert_eq!(lease.is_final(id(1)), Ok(false));
    assert!(matches!(
        registry.admit_close(competing),
        Err(WindowCloseAdmissionError::Busy)
    ));
    assert!(matches!(
        registry.snapshot_for_close(&[id(1), id(2)]),
        Err(WindowCloseAdmissionError::Busy)
    ));
    assert!(matches!(
        registry.reserve_main_window(id(3)),
        Err(RuntimeBackedWindowMainWindowReservationError::CloseInProgress)
    ));
    assert_eq!(permit.commit(|| 42), Ok(42));
    drop(lease);
    assert_eq!(permit.commit(|| 43), Ok(43));
    assert_eq!(registry.main_window_occupancy(), 2);
    let _third = registry.reserve_main_window(id(3)).unwrap();
}

#[test]
fn stale_membership_cannot_be_reused_even_when_identities_return_to_original_set() {
    let registry = RuntimeBackedWindowProcessRegistry::new(Default::default());
    let original = registry.reserve_main_window(id(1)).unwrap();
    let snapshot = registry.snapshot_for_close(&[id(1)]).unwrap();
    drop(original);
    let _replacement = registry.reserve_main_window(id(1)).unwrap();
    assert!(matches!(
        registry.admit_close(snapshot),
        Err(WindowCloseAdmissionError::WindowSetChanged)
    ));
    let snapshot = registry.snapshot_for_close(&[id(1)]).unwrap();
    let temporary = registry.reserve_main_window(id(2)).unwrap();
    drop(temporary);
    assert!(matches!(
        registry.admit_close(snapshot),
        Err(WindowCloseAdmissionError::WindowSetChanged)
    ));
}

#[test]
fn foreign_and_malformed_snapshots_grant_no_close_authority() {
    let first = RuntimeBackedWindowProcessRegistry::new(Default::default());
    let second = RuntimeBackedWindowProcessRegistry::new(Default::default());
    let _a = first.reserve_main_window(id(1)).unwrap();
    let _b = second.reserve_main_window(id(1)).unwrap();
    let snapshot = first.snapshot_for_close(&[id(1)]).unwrap();
    assert!(matches!(
        second.admit_close(snapshot),
        Err(WindowCloseAdmissionError::WindowSetChanged)
    ));
    for members in [
        vec![],
        vec![id(1), id(1)],
        vec![id(1); MAX_RESTORABLE_WINDOWS + 1],
    ] {
        assert!(matches!(
            first.snapshot_for_close(&members),
            Err(WindowCloseAdmissionError::WindowSetChanged)
        ));
    }
}

#[test]
fn unexpected_release_invalidates_lease_without_allowing_construction_to_escape_it() {
    let registry = RuntimeBackedWindowProcessRegistry::new(Default::default());
    let _first = registry.reserve_main_window(id(1)).unwrap();
    let second = registry.reserve_main_window(id(2)).unwrap();
    let snapshot = registry.snapshot_for_close(&[id(1), id(2)]).unwrap();
    let lease = registry.admit_close(snapshot).unwrap();
    drop(second);
    assert_eq!(
        lease.is_final(id(1)),
        Err(WindowCloseAdmissionError::WindowSetChanged)
    );
    assert!(matches!(
        registry.reserve_main_window(id(3)),
        Err(RuntimeBackedWindowMainWindowReservationError::CloseInProgress)
    ));
    drop(lease);
    let snapshot = registry.snapshot_for_close(&[id(1)]).unwrap();
    assert_eq!(
        registry.admit_close(snapshot).unwrap().is_final(id(1)),
        Ok(true)
    );
}

#[test]
fn construction_and_close_have_one_serialized_winner() {
    for _ in 0..32 {
        let registry = RuntimeBackedWindowProcessRegistry::new(Default::default());
        let _resident = registry.reserve_main_window(id(1)).unwrap();
        let snapshot = registry.snapshot_for_close(&[id(1)]).unwrap();
        let barrier = Arc::new(Barrier::new(2));
        let construction_registry = registry.clone();
        let construction_barrier = barrier.clone();
        let construction = thread::spawn(move || {
            construction_barrier.wait();
            construction_registry.reserve_main_window(id(2))
        });
        barrier.wait();
        let close = registry.admit_close(snapshot);
        let construction = construction.join().unwrap();
        match (close, construction) {
            (Ok(lease), Err(RuntimeBackedWindowMainWindowReservationError::CloseInProgress)) => {
                assert_eq!(lease.is_final(id(1)), Ok(true));
            }
            (Err(WindowCloseAdmissionError::WindowSetChanged), Ok(_reservation)) => {}
            _ => panic!("close and construction did not serialize"),
        }
    }
}

#[test]
fn existing_shutdown_fence_rejects_close_without_reopening_process() {
    let process = crate::process_admission::ProcessAdmissionGate::new();
    let registry = RuntimeBackedWindowProcessRegistry::new(process.clone());
    let _resident = registry.reserve_main_window(id(1)).unwrap();
    let snapshot = registry.snapshot_for_close(&[id(1)]).unwrap();
    let _fence = process.fence().unwrap();
    assert!(matches!(
        registry.admit_close(snapshot),
        Err(WindowCloseAdmissionError::Process(
            ProcessAdmissionError::Fenced
        ))
    ));
    assert!(matches!(
        registry.snapshot_for_close(&[id(1)]),
        Err(WindowCloseAdmissionError::Process(
            ProcessAdmissionError::Fenced
        ))
    ));
    assert_eq!(
        process.execution_permit().commit(|| ()),
        Err(ProcessAdmissionError::Fenced)
    );
}
