use super::*;
use crate::window_acquisition::{
    RuntimeBackedWindowMainWindowReservationError, RuntimeBackedWindowProcessRegistry,
};
use beryl_model::WindowId;

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
