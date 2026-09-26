use super::*;
use crate::window_acquisition::{
    RuntimeBackedWindowMainWindowReservationError, RuntimeBackedWindowProcessRegistry,
};
use beryl_model::WindowId;

#[test]
fn declined_or_failed_no_work_validation_preserves_existing_execution_authority() {
    let gate = ProcessAdmissionGate::new();
    let permit = gate.execution_permit();
    assert!(
        gate.fence_if_quiescent(|| Ok::<_, ProcessAdmissionError>(false))
            .unwrap()
            .is_none()
    );
    permit.commit(|| ()).unwrap();
    assert_eq!(
        gate.fence_if_quiescent(|| Err(ProcessAdmissionError::Stale))
            .unwrap_err(),
        ProcessAdmissionError::Stale
    );
    permit.commit(|| ()).unwrap();
    let reservation = permit.reserve().unwrap();
    assert_eq!(
        gate.fence_if_quiescent::<ProcessAdmissionError>(|| panic!("unsettled validation"))
            .unwrap_err(),
        ProcessAdmissionError::Unsettled
    );
    drop(reservation);
    permit.commit(|| ()).unwrap();
}

#[test]
fn accepted_no_work_cut_invalidates_old_permits_and_cannot_join_an_existing_fence() {
    let gate = ProcessAdmissionGate::new();
    let permit = gate.execution_permit();
    let fence = gate
        .fence_if_quiescent(|| Ok::<_, ProcessAdmissionError>(true))
        .unwrap()
        .unwrap();
    assert_eq!(permit.commit(|| ()), Err(ProcessAdmissionError::Fenced));
    assert_eq!(
        gate.fence_if_quiescent::<ProcessAdmissionError>(|| panic!("fenced validation"))
            .unwrap_err(),
        ProcessAdmissionError::Fenced
    );
    fence.reopen_if(true).unwrap();
    assert_eq!(permit.commit(|| ()), Err(ProcessAdmissionError::Stale));
    gate.execution_permit().commit(|| ()).unwrap();
}

#[test]
fn no_work_cut_observes_publication_of_concurrent_atomic_work() {
    let gate = ProcessAdmissionGate::new();
    let permit = gate.execution_permit();
    let work = std::sync::atomic::AtomicBool::new(false);
    let (entered_tx, entered_rx) = std::sync::mpsc::channel();
    let (finish_tx, finish_rx) = std::sync::mpsc::channel();
    std::thread::scope(|scope| {
        let permit = &permit;
        let work = &work;
        let admission = scope.spawn(move || {
            permit.commit(|| {
                entered_tx.send(()).unwrap();
                finish_rx.recv().unwrap();
                work.store(true, std::sync::atomic::Ordering::SeqCst);
            })
        });
        entered_rx.recv().unwrap();
        let closing = scope.spawn(|| {
            gate.fence_if_quiescent(|| {
                Ok::<_, ProcessAdmissionError>(!work.load(std::sync::atomic::Ordering::SeqCst))
            })
        });
        finish_tx.send(()).unwrap();
        admission.join().unwrap().unwrap();
        assert!(closing.join().unwrap().unwrap().is_none());
    });
    permit.commit(|| ()).unwrap();
}

#[test]
fn reservation_and_no_work_cut_have_exactly_one_winner() {
    for _ in 0..32 {
        let gate = ProcessAdmissionGate::new();
        let start = std::sync::Barrier::new(2);
        std::thread::scope(|scope| {
            let admission = scope.spawn(|| {
                let permit = gate.execution_permit();
                start.wait();
                permit.reserve()
            });
            start.wait();
            let closing = gate.fence_if_quiescent(|| Ok::<_, ProcessAdmissionError>(true));
            let admission = admission.join().unwrap();
            match (closing, admission) {
                (Ok(Some(fence)), Err(ProcessAdmissionError::Fenced)) => {
                    fence.reopen_if(true).unwrap();
                }
                (Err(ProcessAdmissionError::Unsettled), Ok(reservation)) => {
                    drop(reservation);
                    gate.execution_permit().commit(|| ()).unwrap();
                }
                other => panic!("inconsistent admission winners: {other:?}"),
            }
        });
    }
}

#[test]
fn no_work_cut_revalidates_exact_window_lease_before_installing_fence() {
    use crate::window_acquisition::WindowCloseAdmissionError;

    let gate = ProcessAdmissionGate::new();
    let registry = RuntimeBackedWindowProcessRegistry::new(gate.clone());
    let id = WindowId::from_bytes([11; 16]);
    let resident = registry.reserve_main_window(id).unwrap();
    let lease = registry
        .admit_close(registry.snapshot_for_close(&[id]).unwrap())
        .unwrap();
    let fence = gate
        .fence_if_quiescent(|| lease.is_final(id))
        .unwrap()
        .unwrap();
    fence.reopen_if(true).unwrap();
    let permit = gate.execution_permit();
    drop(resident);
    assert_eq!(
        gate.fence_if_quiescent(|| lease.is_final(id)).unwrap_err(),
        WindowCloseAdmissionError::WindowSetChanged
    );
    permit.commit(|| ()).unwrap();
    drop(lease);
}

#[test]
fn closing_invalidates_queued_execution_even_after_reopening() {
    let gate = ProcessAdmissionGate::new();
    let before = gate.execution_permit();
    let fence = gate.fence().unwrap();
    let behind = gate.execution_permit();
    assert_eq!(before.commit(|| ()), Err(ProcessAdmissionError::Fenced));
    fence.reopen_if(true).unwrap();
    assert_eq!(before.commit(|| ()), Err(ProcessAdmissionError::Stale));
    assert_eq!(behind.commit(|| ()), Err(ProcessAdmissionError::Stale));
    gate.execution_permit().commit(|| ()).unwrap();
    assert_eq!(fence.reopen_if(true), Err(ProcessAdmissionError::Stale));
}

#[test]
fn reopening_waits_for_admission_handoff_and_coherent_reconciliation() {
    let gate = ProcessAdmissionGate::new();
    let admitted = gate.execution_permit().reserve().unwrap();
    let fence = gate.fence().unwrap();
    assert_eq!(fence.reopen_if(true), Err(ProcessAdmissionError::Unsettled));
    drop(admitted);
    assert_eq!(
        fence.reopen_if(false),
        Err(ProcessAdmissionError::Unsettled)
    );
    fence.reopen_if(true).unwrap();
}

#[test]
fn joined_attempt_cannot_reopen_a_later_attempt() {
    let gate = ProcessAdmissionGate::new();
    let first = gate.fence().unwrap();
    let joined = gate.fence().unwrap();
    first.reopen_if(true).unwrap();
    let second = gate.fence().unwrap();
    assert_eq!(joined.reopen_if(true), Err(ProcessAdmissionError::Stale));
    second.reopen_if(true).unwrap();
}

#[test]
fn window_registries_share_admission_and_can_release_behind_the_fence() {
    let gate = ProcessAdmissionGate::new();
    let first = RuntimeBackedWindowProcessRegistry::new(gate.clone());
    let second = RuntimeBackedWindowProcessRegistry::new(gate.clone());
    let id = WindowId::from_bytes([1; 16]);
    let reserved = first.reserve_main_window(id).unwrap();
    let fence = gate.fence().unwrap();
    assert_eq!(
        second.reserve_main_window(id).unwrap_err(),
        RuntimeBackedWindowMainWindowReservationError::ProcessAdmission(
            ProcessAdmissionError::Fenced
        )
    );
    drop(reserved);
    assert_eq!(first.main_window_occupancy(), 0);
    fence.reopen_if(true).unwrap();
    let _reserved = second.reserve_main_window(id).unwrap();
}

#[test]
fn fence_waits_until_an_atomic_admission_has_published() {
    let gate = ProcessAdmissionGate::new();
    let (entered_tx, entered_rx) = std::sync::mpsc::channel();
    let (finish_tx, finish_rx) = std::sync::mpsc::channel();
    let published = Arc::new(std::sync::atomic::AtomicBool::new(false));
    std::thread::scope(|scope| {
        let admission_gate = gate.clone();
        let published = &published;
        let admission = scope.spawn(move || {
            admission_gate.admit(|| {
                entered_tx.send(()).unwrap();
                finish_rx.recv().unwrap();
                published.store(true, std::sync::atomic::Ordering::SeqCst);
            })
        });
        entered_rx.recv().unwrap();
        let closing = scope.spawn(|| {
            let fence = gate.fence().unwrap();
            assert!(published.load(std::sync::atomic::Ordering::SeqCst));
            fence
        });
        finish_tx.send(()).unwrap();
        admission.join().unwrap().unwrap();
        closing.join().unwrap().reopen_if(true).unwrap();
    });
}
