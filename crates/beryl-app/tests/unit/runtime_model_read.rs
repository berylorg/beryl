use super::*;
use crate::cas_projection::{
    RuntimeInterestConfig,
    persistent_failure::{MasterCommandGate, ProjectionServiceGeneration},
};
use beryl_model::{BerylHomeId, PathFlavor, RootId, RuntimeId, RuntimeMode, RuntimeNativePath};
use std::num::NonZeroUsize;

#[test]
fn absent_runtime_read_never_creates_launch_or_runtime_interest() {
    let one = NonZeroUsize::new(1).unwrap();
    let gate = MasterCommandGate::new(
        crate::process_admission::ProcessAdmissionGate::new(),
        ProjectionServiceGeneration::allocate().unwrap(),
        None,
    );
    let owner = RuntimeInterestOwner::new(
        RuntimeInterestConfig::new(one, one, Duration::from_secs(1)).unwrap(),
        gate.authorizer(),
        crate::cas_projection::accepted_input_scheduler::AcceptedInputSchedulerSignal::new(),
        crate::runtime_activity_enrollment::RuntimeActivityEnrollmentOperations::new(
            BerylHomeId::from_bytes([31; 16]),
            one,
        ),
    );
    let binding = ExecutionBinding::new(
        RuntimeId::from_bytes([32; 16]),
        RootId::from_bytes([33; 16]),
        RuntimeNativePath::from_admitted(
            RuntimeMode::Host,
            PathFlavor::Windows,
            r"C:\runtime-read-fixture",
        )
        .unwrap(),
    );
    for _ in 0..4 {
        assert!(matches!(
            owner.model_read(binding.clone()),
            Err(RuntimeModelReadError::Unavailable)
        ));
    }
    let state = owner.shared.state.lock().unwrap();
    assert!(state.runtimes.is_empty());
    assert_eq!(state.interest_count, 0);
    assert_eq!(state.next_identity, 1);
}
