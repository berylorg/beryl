#![cfg(feature = "lifecycle-test-support")]

use beryl_backend::{
    ManagedBackendError, lifecycle_test_support::WslControlProgressForLifecycleTest,
};
use beryl_wsl_supervisor::{DiagnosticTail, ExitStatus, FailureKind, Frame, Role, WorkloadResult};

fn closed() -> Frame {
    Frame::OwnedNamespaceClosed {
        result: WorkloadResult {
            exit: ExitStatus::Exited(0),
            observation: None,
            stdout: DiagnosticTail::default(),
            stderr: DiagnosticTail::default(),
        },
    }
}

#[test]
fn launch_requires_supervisor_context_before_workload_evidence() {
    let mut owner = WslControlProgressForLifecycleTest::default();
    assert!(owner.accept(Frame::WorkloadStarted).is_err());
    assert!(!owner.workload_started());
    assert!(
        owner
            .accept(Frame::Ready {
                role: Role::Supervisor
            })
            .is_err()
    );
}

#[test]
fn namespace_closure_is_monotonic_progress_before_broker_closure() {
    let mut owner = WslControlProgressForLifecycleTest::default();
    owner
        .accept(Frame::Ready {
            role: Role::Supervisor,
        })
        .unwrap();
    owner.accept(Frame::WorkloadStarted).unwrap();
    owner.accept(closed()).unwrap();
    assert!(owner.namespace_closed());
    assert!(!owner.companions_closed());
    owner.accept(Frame::ShutdownPending).unwrap();
    assert!(owner.namespace_closed());
    owner.accept(Frame::LinuxCompanionsClosed).unwrap();
    assert!(owner.companions_closed());
}

#[test]
fn failed_exec_keeps_the_original_cleanup_sequence_usable() {
    let mut owner = WslControlProgressForLifecycleTest::default();
    owner
        .accept(Frame::Ready {
            role: Role::Supervisor,
        })
        .unwrap();
    let error = owner
        .accept(Frame::Failure {
            kind: FailureKind::Exec,
            errno: Some(2),
        })
        .unwrap_err();
    assert!(matches!(
        error,
        ManagedBackendError::WslSupervisionFailure {
            kind: FailureKind::Exec,
            errno: Some(2)
        }
    ));
    assert!(!owner.workload_started());
    owner.accept(closed()).unwrap();
    owner.accept(Frame::LinuxCompanionsClosed).unwrap();
    assert!(owner.companions_closed());
}

#[test]
fn premature_companion_closure_cannot_be_repaired_by_later_records() {
    let mut owner = WslControlProgressForLifecycleTest::default();
    owner
        .accept(Frame::Ready {
            role: Role::Supervisor,
        })
        .unwrap();
    assert!(owner.accept(Frame::LinuxCompanionsClosed).is_err());
    assert!(owner.accept(closed()).is_err());
    assert!(!owner.namespace_closed());
    assert!(!owner.companions_closed());
}

#[test]
fn duplicate_namespace_record_is_not_new_cleanup_authority() {
    let mut owner = WslControlProgressForLifecycleTest::default();
    owner
        .accept(Frame::Ready {
            role: Role::Supervisor,
        })
        .unwrap();
    owner.accept(closed()).unwrap();
    assert!(owner.accept(closed()).is_err());
    assert!(owner.namespace_closed());
    assert!(owner.accept(Frame::LinuxCompanionsClosed).is_err());
    assert!(!owner.companions_closed());
}

#[test]
fn broker_readiness_cannot_authorize_a_supervisor_workload() {
    let mut owner = WslControlProgressForLifecycleTest::default();
    assert!(
        owner
            .accept(Frame::Ready {
                role: Role::ContextBroker
            })
            .is_err()
    );
    assert!(owner.accept(Frame::WorkloadStarted).is_err());
}

#[test]
fn pre_context_failure_cannot_promote_into_a_workload_launch() {
    let mut owner = WslControlProgressForLifecycleTest::default();
    assert!(
        owner
            .accept(Frame::Failure {
                kind: FailureKind::Context,
                errno: None
            })
            .is_err()
    );
    assert!(
        owner
            .accept(Frame::Ready {
                role: Role::Supervisor
            })
            .is_err()
    );
    assert!(owner.accept(Frame::WorkloadStarted).is_err());
    assert!(!owner.workload_started());
}

#[test]
fn disposal_retry_consumes_original_queued_closure_after_writer_disconnect() {
    let mut owner = WslControlProgressForLifecycleTest::default();
    owner
        .accept(Frame::Ready {
            role: Role::Supervisor,
        })
        .unwrap();
    owner.accept(Frame::WorkloadStarted).unwrap();
    assert!(matches!(
        owner.retire(Ok(()), || Err(ManagedBackendError::WslSupervisionTimeout)),
        Err(ManagedBackendError::WslSupervisionTimeout)
    ));
    assert!(!owner.companions_closed());
    let (original_reader_tx, original_reader_rx) = std::sync::mpsc::sync_channel(8);
    original_reader_tx.send(closed()).unwrap();
    original_reader_tx
        .send(Frame::LinuxCompanionsClosed)
        .unwrap();
    drop(original_reader_tx);
    owner
        .retire(Err(ManagedBackendError::WslSupervisionUnavailable), || {
            original_reader_rx
                .recv()
                .map_err(|_| ManagedBackendError::WslSupervisionUnavailable)
        })
        .unwrap();
    assert!(owner.namespace_closed());
    assert!(owner.companions_closed());
    owner
        .retire(Err(ManagedBackendError::WslSupervisionUnavailable), || {
            panic!("joined progress must not read or replace the original channel")
        })
        .unwrap();
}

#[test]
fn writer_disconnect_does_not_supply_missing_or_invalid_closure() {
    let mut owner = WslControlProgressForLifecycleTest::default();
    owner
        .accept(Frame::Ready {
            role: Role::Supervisor,
        })
        .unwrap();
    assert!(matches!(
        owner.retire(Err(ManagedBackendError::WslSupervisionUnavailable), || Ok(
            Frame::LinuxCompanionsClosed
        )),
        Err(ManagedBackendError::WslSupervisionUnavailable)
    ));
    assert!(!owner.companions_closed());
    assert!(
        owner
            .retire(Ok(()), || panic!(
                "invalid original progress must remain sealed"
            ))
            .is_err()
    );
}
