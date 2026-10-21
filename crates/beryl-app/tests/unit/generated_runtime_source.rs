use super::*;
use crate::cas_projection::persistent_failure::{MasterCommandGate, ProjectionServiceGeneration};
use beryl_model::{AdmittedHostPath, BerylHomeId, PathFlavor, RootId, RuntimeLaunchForm, RuntimeMode};

fn fixture(seed: u8) -> (RuntimeInterestOwner, MasterCommandGate, ManagedBackendLaunchSpec, ExecutionBinding) {
    let gate = MasterCommandGate::new(Default::default(), ProjectionServiceGeneration::allocate().unwrap(), None);
    let owner = RuntimeInterestOwner::new(
        RuntimeInterestConfig::new(NonZeroUsize::new(1).unwrap(), NonZeroUsize::new(2).unwrap(), Duration::from_secs(5)).unwrap(),
        gate.authorizer(),
        crate::cas_projection::accepted_input_scheduler::AcceptedInputSchedulerSignal::new(),
        crate::runtime_activity_enrollment::RuntimeActivityEnrollmentOperations::new(BerylHomeId::from_bytes([seed; 16]), NonZeroUsize::new(1).unwrap()),
    );
    let runtime_id = RuntimeId::from_bytes([seed; 16]);
    let native = |path: &str| RuntimeNativePath::from_admitted(RuntimeMode::Host, PathFlavor::Windows, path).unwrap();
    let spec = ManagedBackendLaunchSpec::new(runtime_id,
        AdmittedHostPath::from_admitted(PathFlavor::Windows, r"C:\runtime\codex.exe").unwrap(),
        RuntimeMode::Host, RuntimeLaunchForm::CodexCli, native(r"C:\runtime\codex.exe"), native(r"C:\work"),
        AdmittedHostPath::from_admitted(PathFlavor::Windows, r"C:\tokens").unwrap(), native(r"C:\tokens"),
    ).unwrap();
    let binding = ExecutionBinding::new(runtime_id, RootId::from_bytes([seed; 16]), native(r"C:\work"));
    (owner, gate, spec, binding)
}

fn wait_status(owner: &RuntimeInterestOwner, runtime_id: RuntimeId, expected: impl Fn(&RuntimeEntry) -> bool) {
    let (state, timeout) = owner.shared.changed.wait_timeout_while(owner.shared.lock(), Duration::from_secs(5), |state| {
        !state.runtimes.get(&runtime_id).is_some_and(&expected)
    }).unwrap();
    assert!(!timeout.timed_out(), "runtime status did not settle");
    assert!(expected(state.runtimes.get(&runtime_id).unwrap()));
}

#[test]
fn pending_owner_is_busy_and_exact_retired_process_is_lost() {
    let (owner, _gate, spec, binding) = fixture(231);
    let runtime_id = binding.runtime_id();
    let process = CasProcessGeneration::new(330_001).unwrap();
    let probe = super::super::test_support::RuntimeInterestTestProbe::new(process);
    probe.allow_launch(false);
    let interest = owner.acquire_with_test_probe(spec, binding, probe.clone()).unwrap();
    assert!(probe.wait_for_launch(Duration::from_secs(5)));
    assert!(matches!(owner.generated_source_owner(runtime_id, process), Err(GeneratedSourceEligibility::Busy)));
    probe.allow_launch(true);
    wait_status(&owner, runtime_id, |entry| matches!(entry.status, RuntimeInterestStatus::Ready(_)));
    assert!(owner.generated_source_owner(runtime_id, process).is_ok());
    assert!(matches!(owner.generated_source_owner(runtime_id, CasProcessGeneration::new(330_002).unwrap()), Err(GeneratedSourceEligibility::LostSourceAuthority)));
    drop(interest);
    wait_status(&owner, runtime_id, |entry| entry.cleanup_complete);
    assert!(matches!(owner.generated_source_owner(runtime_id, process), Err(GeneratedSourceEligibility::LostSourceAuthority)));
}

#[test]
fn original_retirement_can_start_while_source_custody_prevents_reported_drain() {
    let (owner, _gate, spec, binding) = fixture(232);
    let runtime_id = binding.runtime_id();
    let process = CasProcessGeneration::new(330_003).unwrap();
    let probe = super::super::test_support::RuntimeInterestTestProbe::new(process);
    let interest = owner.acquire_with_test_probe(spec, binding, probe.clone()).unwrap();
    wait_status(&owner, runtime_id, |entry| matches!(entry.status, RuntimeInterestStatus::Ready(_)));
    let slot = Arc::new(GeneratedRuntimeSlot { state: Mutex::new(SlotState::Active), response: Mutex::new(None) });
    let attempt = {
        let mut state = owner.shared.lock();
        let entry = state.runtimes.get_mut(&runtime_id).unwrap();
        entry.generated_source = Some(Arc::clone(&slot));
        entry.attempt
    };
    let lease = RuntimeSourceLease { shared: Arc::clone(&owner.shared), runtime_id, attempt, slot };
    assert!(!source_drained(&owner.shared, runtime_id, attempt));
    drop(interest);
    assert!(probe.wait_for_retirement(Duration::from_secs(5)));
    {
        let state = owner.shared.lock();
        let entry = state.runtimes.get(&runtime_id).unwrap();
        assert!(!entry.cleanup_complete);
        assert!(entry.generated_source.is_some());
    }
    lease.finish();
    wait_status(&owner, runtime_id, |entry| entry.cleanup_complete);
    assert!(source_drained(&owner.shared, runtime_id, attempt));
}

#[test]
fn delayed_old_attempt_release_cannot_clear_replacement_source_slot() {
    let (owner, _gate, spec, binding) = fixture(233);
    let runtime_id = binding.runtime_id();
    let process = CasProcessGeneration::new(330_004).unwrap();
    let probe = super::super::test_support::RuntimeInterestTestProbe::new(process);
    let interest = owner.acquire_with_test_probe(spec, binding, probe).unwrap();
    wait_status(&owner, runtime_id, |entry| matches!(entry.status, RuntimeInterestStatus::Ready(_)));
    let current_slot = Arc::new(GeneratedRuntimeSlot { state: Mutex::new(SlotState::Active), response: Mutex::new(None) });
    let old_slot = Arc::new(GeneratedRuntimeSlot { state: Mutex::new(SlotState::Active), response: Mutex::new(None) });
    let attempt = {
        let mut state = owner.shared.lock();
        let entry = state.runtimes.get_mut(&runtime_id).unwrap();
        entry.generated_source = Some(Arc::clone(&current_slot));
        entry.attempt
    };
    let stale = RuntimeSourceLease { shared: Arc::clone(&owner.shared), runtime_id, attempt: attempt + 1, slot: old_slot };
    stale.finish();
    assert!(!source_drained(&owner.shared, runtime_id, attempt));
    RuntimeSourceLease { shared: Arc::clone(&owner.shared), runtime_id, attempt, slot: current_slot }.finish();
    drop(interest);
    wait_status(&owner, runtime_id, |entry| entry.cleanup_complete);
}

#[test]
fn one_original_runtime_slot_refuses_a_peer_until_actual_custody_release() {
    let (owner, _gate, spec, binding) = fixture(234);
    let runtime_id = binding.runtime_id();
    let process = CasProcessGeneration::new(330_005).unwrap();
    let probe = super::super::test_support::RuntimeInterestTestProbe::new(process);
    let interest = owner.acquire_with_test_probe(spec, binding, probe).unwrap();
    wait_status(&owner, runtime_id, |entry| matches!(entry.status, RuntimeInterestStatus::Ready(_)));
    let source_owner = owner.generated_source_owner(runtime_id, process).unwrap();
    let slot = || Arc::new(GeneratedRuntimeSlot { state: Mutex::new(SlotState::Active), response: Mutex::new(None) });
    let first = slot();
    source_owner.admit_slot(Arc::clone(&first)).unwrap();
    assert!(matches!(source_owner.admit_slot(slot()), Err(GeneratedSourceEligibility::Busy)));
    assert!(!source_drained(&owner.shared, runtime_id, source_owner.attempt));
    RuntimeSourceLease { shared: Arc::clone(&owner.shared), runtime_id, attempt: source_owner.attempt, slot: first }.finish();
    let second = slot();
    source_owner.admit_slot(Arc::clone(&second)).unwrap();
    RuntimeSourceLease { shared: Arc::clone(&owner.shared), runtime_id, attempt: source_owner.attempt, slot: second }.finish();
    drop(interest);
    wait_status(&owner, runtime_id, |entry| entry.cleanup_complete);
}

#[test]
fn namespace_close_does_not_discard_an_active_source_lease() {
    let (owner, _gate, spec, binding) = fixture(235);
    let runtime_id = binding.runtime_id();
    let process = CasProcessGeneration::new(330_006).unwrap();
    let probe = super::super::test_support::RuntimeInterestTestProbe::new(process);
    let interest = owner.acquire_with_test_probe(spec, binding, probe.clone()).unwrap();
    wait_status(&owner, runtime_id, |entry| matches!(entry.status, RuntimeInterestStatus::Ready(_)));
    let source_owner = owner.generated_source_owner(runtime_id, process).unwrap();
    let slot = Arc::new(GeneratedRuntimeSlot { state: Mutex::new(SlotState::Active), response: Mutex::new(None) });
    source_owner.admit_slot(Arc::clone(&slot)).unwrap();
    drop(interest);
    assert!(probe.wait_for_retirement(Duration::from_secs(5)));
    assert!(!source_drained_after_retirement(&owner.shared, runtime_id, source_owner.attempt));
    assert!(!owner.shared.lock().runtimes.get(&runtime_id).unwrap().cleanup_complete);
    RuntimeSourceLease { shared: Arc::clone(&owner.shared), runtime_id, attempt: source_owner.attempt, slot }.finish();
    wait_status(&owner, runtime_id, |entry| entry.cleanup_complete);
}
