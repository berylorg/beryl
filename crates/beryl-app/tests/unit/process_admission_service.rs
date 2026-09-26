use super::*;
use crate::process_admission::{
    ProcessAdmissionError, ProcessAdmissionGate, ProcessExecutionAdmissionError,
};

fn service_gate(process: &ProcessAdmissionGate) -> MasterCommandGate {
    MasterCommandGate::new(
        process.clone(),
        ProjectionServiceGeneration::allocate().unwrap(),
        None,
    )
}

#[test]
fn runtime_work_check_refuses_command_contention_under_process_admission() {
    use crate::cas_projection::runtime_work::RuntimeWorkError;
    let process = ProcessAdmissionGate::new();
    let service = service_gate(&process);
    let authorizer = service.authorizer();
    let permit = authorizer.authorize().unwrap();
    let before = authorizer.active_command_count_for_test();
    let (send, receive) = std::sync::mpsc::channel();
    std::thread::scope(|scope| {
        let held = authorizer.inner.state.lock().unwrap();
        let reader = scope.spawn(|| {
            let result = process.admit(|| authorizer.try_check_work_open());
            send.send(result).unwrap();
        });
        let result = receive.recv_timeout(std::time::Duration::from_secs(2));
        drop(held);
        reader.join().unwrap();
        assert_eq!(result.unwrap().unwrap(), Err(RuntimeWorkError::Busy));
    });
    assert_eq!(authorizer.active_command_count_for_test(), before);
    authorizer.try_check_work_open().unwrap();
    permit.commit_execution_if_current(|| ()).unwrap();
    service.close_for_shutdown();
    assert_eq!(
        authorizer.try_check_work_open(),
        Err(RuntimeWorkError::Closed)
    );
}

#[test]
fn runtime_work_check_does_not_mutate_a_poisoned_command_gate() {
    use crate::cas_projection::runtime_work::RuntimeWorkError;
    let process = ProcessAdmissionGate::new();
    let service = service_gate(&process);
    let authorizer = service.authorizer();
    service.poison_for_test();
    assert_eq!(
        authorizer.try_check_work_open(),
        Err(RuntimeWorkError::Unavailable)
    );
    let state = authorizer.inner.state.lock().err().unwrap().into_inner();
    assert!(!state.local_failure);
    assert!(matches!(state.election, GateElection::Open));
    assert_eq!(state.active, 0);
}

#[test]
fn queued_execution_candidate_keeps_its_epoch_without_retaining_health_command_custody() {
    let process = ProcessAdmissionGate::new();
    let service = service_gate(&process);
    let authorizer = service.authorizer();
    let candidate = authorizer.execution_candidate().unwrap();
    assert_eq!(authorizer.active_command_count_for_test(), 0);
    let fence = process.fence().unwrap();
    assert!(matches!(
        candidate.reserve(),
        Err(ProcessExecutionAdmissionError::Process(
            ProcessAdmissionError::Fenced
        ))
    ));
    authorizer
        .authorize()
        .unwrap()
        .reopen_process_admission_for_test(&fence)
        .unwrap();
    assert!(matches!(
        candidate.reserve(),
        Err(ProcessExecutionAdmissionError::Process(
            ProcessAdmissionError::Stale
        ))
    ));
    assert_eq!(authorizer.active_command_count_for_test(), 0);
    service.close_for_shutdown();
    assert!(matches!(
        candidate.reserve(),
        Err(ProcessExecutionAdmissionError::Service(
            LiveCommandAdmissionError::Closed
        ))
    ));
}

#[test]
fn process_fence_survives_service_generation_changes_and_preserves_health_commands() {
    let process = ProcessAdmissionGate::new();
    let first = service_gate(&process);
    let first_permit = first.authorizer().authorize().unwrap();
    let reservation = first_permit.reserve_execution().unwrap();
    let fence = process.fence().unwrap();
    first_permit.commit_if_current(|| ()).unwrap();
    assert_eq!(
        first_permit.commit_execution_if_current(|| ()),
        Err(ProcessExecutionAdmissionError::Process(
            ProcessAdmissionError::Fenced
        ))
    );

    let second = service_gate(&process);
    let second_permit = second.authorizer().authorize().unwrap();
    second_permit.commit_if_current(|| ()).unwrap();
    assert_eq!(
        second_permit.commit_execution_if_current(|| ()),
        Err(ProcessExecutionAdmissionError::Process(
            ProcessAdmissionError::Fenced
        ))
    );
    first.close_for_local_failure();
    assert_eq!(
        first_permit.reopen_process_admission_for_test(&fence),
        Err(ProcessExecutionAdmissionError::Service(
            LiveCommandAdmissionError::Closed
        ))
    );
    assert_eq!(
        second_permit.reopen_process_admission_for_test(&fence),
        Err(ProcessExecutionAdmissionError::Process(
            ProcessAdmissionError::Unsettled
        ))
    );
    drop(reservation);
    second_permit
        .reopen_process_admission_for_test(&fence)
        .unwrap();
    assert_eq!(
        second_permit.commit_execution_if_current(|| ()),
        Err(ProcessExecutionAdmissionError::Process(
            ProcessAdmissionError::Stale
        ))
    );
    second
        .authorizer()
        .authorize()
        .unwrap()
        .commit_execution_if_current(|| ())
        .unwrap();
    assert!(!first_permit.is_current());
}

#[test]
fn a_foreign_process_service_cannot_reopen_the_fence() {
    let process = ProcessAdmissionGate::new();
    let fence = process.fence().unwrap();
    let foreign = service_gate(&ProcessAdmissionGate::new());
    let permit = foreign.authorizer().authorize().unwrap();
    assert_eq!(
        permit.reopen_process_admission_for_test(&fence),
        Err(ProcessExecutionAdmissionError::Process(
            ProcessAdmissionError::Stale
        ))
    );
    assert_eq!(
        process.execution_permit().commit(|| ()),
        Err(ProcessAdmissionError::Fenced)
    );
}

#[test]
fn admission_and_coherent_reopening_use_the_same_health_then_process_lock_order() {
    let process = ProcessAdmissionGate::new();
    let service = service_gate(&process);
    let fence = process.fence().unwrap();
    let ready = Arc::new(std::sync::Barrier::new(2));
    std::thread::scope(|scope| {
        let authorizer = service.authorizer();
        let ready = &ready;
        let commands = scope.spawn(move || {
            ready.wait();
            for _ in 0..64 {
                let permit = authorizer.authorize().unwrap();
                permit.commit_if_current(|| ()).unwrap();
                let _ = permit.commit_execution_if_current(|| ());
            }
        });
        ready.wait();
        service
            .authorizer()
            .authorize()
            .unwrap()
            .reopen_process_admission_for_test(&fence)
            .unwrap();
        commands.join().unwrap();
    });
}
