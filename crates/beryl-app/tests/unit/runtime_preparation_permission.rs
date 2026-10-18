use super::*;
use crate::cas_projection::persistent_failure::{MasterCommandGate, ProjectionServiceGeneration};
use beryl_model::{
    AdmittedHostPath, BerylHomeId, CasProcessGeneration, PathFlavor, RootId, RuntimeMode,
    RuntimeNativePath,
};
use std::num::NonZeroUsize;

const TIMEOUT: Duration = Duration::from_secs(5);

fn owner() -> (RuntimeInterestOwner, MasterCommandGate) {
    let gate = MasterCommandGate::new(
        Default::default(),
        ProjectionServiceGeneration::allocate().unwrap(),
        None,
    );
    let capacity = NonZeroUsize::new(1).unwrap();
    let owner = RuntimeInterestOwner::new(
        RuntimeInterestConfig::new(capacity, capacity, TIMEOUT).unwrap(),
        gate.authorizer(),
        crate::cas_projection::accepted_input_scheduler::AcceptedInputSchedulerSignal::new(),
        crate::runtime_activity_enrollment::RuntimeActivityEnrollmentOperations::new(
            BerylHomeId::from_bytes([31; 16]),
            capacity,
        ),
    );
    (owner, gate)
}

fn demand() -> (ManagedBackendLaunchSpec, ExecutionBinding) {
    let native = |path: &str| {
        RuntimeNativePath::from_admitted(RuntimeMode::Host, PathFlavor::Windows, path).unwrap()
    };
    let runtime = RuntimeId::from_bytes([32; 16]);
    let root = native(r"C:\permission-test-root");
    let spec = ManagedBackendLaunchSpec::new(
        runtime,
        AdmittedHostPath::from_admitted(PathFlavor::Windows, r"C:\permission-test\codex.exe")
            .unwrap(),
        RuntimeMode::Host,
        beryl_model::RuntimeLaunchForm::CodexCli,
        native(r"C:\permission-test\codex.exe"),
        root.clone(),
        AdmittedHostPath::from_admitted(PathFlavor::Windows, r"C:\permission-test-tokens").unwrap(),
        native(r"C:\permission-test-tokens"),
    )
    .unwrap();
    (
        spec,
        ExecutionBinding::new(runtime, RootId::from_bytes([33; 16]), root),
    )
}

fn acquire(
    owner: &RuntimeInterestOwner,
    probe: RuntimeInterestTestProbe,
) -> (ExecutionBinding, RuntimeInterest) {
    let (spec, binding) = demand();
    let interest = owner
        .acquire(spec, binding.clone(), RuntimeInterestKind::View, || {
            Ok(Box::new(move || probe.launch()))
        })
        .unwrap();
    assert!(matches!(
        interest.wait_for_change(RuntimeInterestStatus::Starting, TIMEOUT),
        RuntimeInterestStatus::Ready(_)
    ));
    (binding, interest)
}

fn wait_status(
    owner: &RuntimeInterestOwner,
    binding: &ExecutionBinding,
    status: RuntimeInterestStatus,
) {
    let (state, _) = owner
        .shared
        .changed
        .wait_timeout_while(owner.shared.lock(), TIMEOUT, |state| {
            state
                .runtimes
                .get(&binding.runtime_id())
                .is_none_or(|entry| entry.status != status)
        })
        .unwrap();
    assert_eq!(state.runtimes[&binding.runtime_id()].status, status);
}

#[test]
fn clean_retired_runtime_reopens_internal_preparation_without_gui_readiness_or_reaping() {
    let (owner, _gate) = owner();
    let probe = RuntimeInterestTestProbe::new(CasProcessGeneration::new(34).unwrap());
    probe.allow_retirement(false);
    let (binding, interest) = acquire(&owner, probe.clone());
    assert!(owner.initial_submission_preparation_open(&binding));
    drop(interest);
    assert!(probe.wait_for_retirement(TIMEOUT));
    assert!(!owner.initial_submission_preparation_open(&binding));
    probe.complete_retirement();
    wait_status(&owner, &binding, RuntimeInterestStatus::Retired);
    {
        let state = owner.shared.lock();
        let entry = &state.runtimes[&binding.runtime_id()];
        assert!(entry.cleanup_complete && entry.interests.is_empty());
        assert!(entry.failed_runtime.is_none() && entry.failed_acquisition.is_none());
        assert!(entry.worker.is_some());
    }
    assert!(owner.initial_submission_preparation_open(&binding));
    assert!(!owner.gui_submission_ready(&binding));
    assert_eq!(owner.shared.lock().runtimes.len(), 1);
    assert!(owner.shutdown());
}

#[test]
fn failed_retirement_keeps_internal_preparation_and_gui_submission_unavailable() {
    let (owner, _gate) = owner();
    let probe = RuntimeInterestTestProbe::new(CasProcessGeneration::new(35).unwrap());
    probe.fail_retirement(RuntimeFailure::BackendDisposal);
    let (binding, interest) = acquire(&owner, probe.clone());
    drop(interest);
    wait_status(
        &owner,
        &binding,
        RuntimeInterestStatus::Unavailable(RuntimeFailure::BackendDisposal),
    );
    {
        let state = owner.shared.lock();
        let entry = &state.runtimes[&binding.runtime_id()];
        assert!(!entry.cleanup_complete);
        assert!(entry.failed_runtime.is_some());
    }
    assert!(!owner.initial_submission_preparation_open(&binding));
    assert!(!owner.gui_submission_ready(&binding));
    probe.complete_retirement();
    assert!(owner.shutdown());
}

#[test]
fn closed_owner_or_command_generation_never_reopens_clean_retired_preparation() {
    for close_owner in [false, true] {
        let (owner, gate) = owner();
        let probe = RuntimeInterestTestProbe::new(CasProcessGeneration::new(36).unwrap());
        let (binding, interest) = acquire(&owner, probe);
        drop(interest);
        wait_status(&owner, &binding, RuntimeInterestStatus::Retired);
        assert!(owner.initial_submission_preparation_open(&binding));
        if close_owner {
            assert!(owner.shutdown());
        } else {
            gate.close_for_shutdown();
        }
        assert!(!owner.initial_submission_preparation_open(&binding));
        assert!(!owner.gui_submission_ready(&binding));
        assert!(owner.shutdown());
    }
}
