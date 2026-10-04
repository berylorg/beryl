use super::*;
use crate::window_acquisition::{
    RuntimeBackedWindowMainWindowReservationError, RuntimeBackedWindowProcessRegistry,
};
use beryl_model::WindowId;

#[test]
fn closing_publishes_while_subordinate_validation_is_still_held() {
    let gate = ProcessAdmissionGate::new();
    let permit = gate.execution_permit();
    let source = Mutex::new(7);
    let closing = gate.prepare_closing().unwrap();
    let validated = source.try_lock().unwrap();
    assert_eq!(*validated, 7);
    let fence = closing.publish();
    assert!(matches!(
        source.try_lock(),
        Err(std::sync::TryLockError::WouldBlock)
    ));
    assert_eq!(permit.commit(|| ()), Err(ProcessAdmissionError::Fenced));
    drop(validated);
    fence.reopen_if(true).unwrap();
    assert_eq!(permit.commit(|| ()), Err(ProcessAdmissionError::Stale));
}

#[test]
fn unpublished_closing_excludes_admission_until_disposal() {
    let gate = ProcessAdmissionGate::new();
    let permit = gate.execution_permit();
    let (started_tx, started_rx) = std::sync::mpsc::channel();
    let (done_tx, done_rx) = std::sync::mpsc::channel();
    std::thread::scope(|scope| {
        let closing = gate.prepare_closing().unwrap();
        let admission = scope.spawn(|| {
            started_tx.send(()).unwrap();
            let result = permit.commit(|| ());
            done_tx.send(result).unwrap();
        });
        started_rx.recv().unwrap();
        assert!(
            done_rx
                .recv_timeout(std::time::Duration::from_millis(50))
                .is_err()
        );
        drop(closing);
        assert_eq!(
            done_rx
                .recv_timeout(std::time::Duration::from_secs(2))
                .unwrap(),
            Ok(())
        );
        admission.join().unwrap();
    });
}

#[test]
fn exhausted_or_poisoned_closing_authority_never_publishes() {
    let gate = ProcessAdmissionGate::new();
    gate.inner.lock().unwrap().epoch = u64::MAX;
    let permit = gate.execution_permit();
    assert_eq!(
        gate.prepare_closing().unwrap_err(),
        ProcessAdmissionError::Unavailable
    );
    permit.commit(|| ()).unwrap();
    let gate = ProcessAdmissionGate::new();
    assert!(
        std::panic::catch_unwind(|| {
            let _closing = gate.prepare_closing().unwrap();
            panic!("inject unpublished closing guard poison");
        })
        .is_err()
    );
    assert_eq!(
        gate.prepare_closing().unwrap_err(),
        ProcessAdmissionError::Unavailable
    );
    let state = gate.inner.lock().unwrap_err().into_inner();
    assert_eq!(state.epoch, 1);
    assert!(!state.fenced);
}

#[test]
fn unpublished_closing_preserves_existing_execution_authority() {
    let gate = ProcessAdmissionGate::new();
    let permit = gate.execution_permit();
    drop(gate.prepare_closing().unwrap());
    permit.commit(|| ()).unwrap();
    let reservation = permit.reserve().unwrap();
    assert_eq!(
        gate.prepare_closing().unwrap_err(),
        ProcessAdmissionError::Unsettled
    );
    drop(reservation);
    permit.commit(|| ()).unwrap();
}

#[test]
fn accepted_no_work_cut_invalidates_old_permits_and_cannot_join_an_existing_fence() {
    let gate = ProcessAdmissionGate::new();
    let permit = gate.execution_permit();
    let fence = gate.prepare_closing().unwrap().publish();
    assert_eq!(permit.commit(|| ()), Err(ProcessAdmissionError::Fenced));
    assert_eq!(
        gate.prepare_closing().unwrap_err(),
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
            let closing = gate.prepare_closing().unwrap();
            (!work.load(std::sync::atomic::Ordering::SeqCst)).then(|| closing.publish())
        });
        finish_tx.send(()).unwrap();
        admission.join().unwrap().unwrap();
        assert!(closing.join().unwrap().is_none());
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
            let closing = gate.prepare_closing().map(ProcessAdmissionClosing::publish);
            let admission = admission.join().unwrap();
            match (closing, admission) {
                (Ok(fence), Err(ProcessAdmissionError::Fenced)) => {
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
    let closing = gate.prepare_closing().unwrap();
    assert!(lease.is_final(id).unwrap());
    let fence = closing.publish();
    fence.reopen_if(true).unwrap();
    let permit = gate.execution_permit();
    drop(resident);
    let closing = gate.prepare_closing().unwrap();
    assert_eq!(
        lease.is_final(id),
        Err(WindowCloseAdmissionError::WindowSetChanged)
    );
    drop(closing);
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

#[test]
fn shell_capacity_and_selection_gates_return_while_claim_commit_is_held() {
    let registry = RuntimeBackedWindowProcessRegistry::new(ProcessAdmissionGate::new());
    let id = WindowId::from_bytes([31; 16]);
    let resident = registry.reserve_main_window(id).unwrap();
    let lease = registry.admit_selection(&[id], id).unwrap();
    let (entered_tx, entered_rx) = std::sync::mpsc::channel();
    let (release_tx, release_rx) = std::sync::mpsc::channel();
    let (observed_tx, observed_rx) = std::sync::mpsc::channel();
    std::thread::scope(|scope| {
        let admitted = &lease;
        let commit = scope.spawn(move || {
            admitted.admit_commit(|| {
                entered_tx.send(()).unwrap();
                release_rx
                    .recv_timeout(std::time::Duration::from_secs(3))
                    .unwrap();
            })
        });
        entered_rx
            .recv_timeout(std::time::Duration::from_secs(3))
            .unwrap();
        let registry = &registry;
        let read = scope.spawn(move || {
            observed_tx
                .send((
                    registry.try_main_window_occupancy(),
                    registry.selection_pending(),
                ))
                .unwrap();
        });
        let observed = observed_rx.recv_timeout(std::time::Duration::from_secs(1));
        release_tx.send(()).unwrap();
        commit.join().unwrap().unwrap();
        read.join().unwrap();
        assert_eq!(observed.unwrap(), (None, true));
    });
    assert_eq!(registry.try_main_window_occupancy(), Some(1));
    assert!(registry.selection_pending());
    drop(lease);
    assert!(!registry.selection_pending());
    drop(resident);
}
