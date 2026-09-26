use super::*;
use std::{sync::Barrier, thread};

fn id(value: u8) -> WindowId {
    WindowId::from_bytes([value; 16])
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
