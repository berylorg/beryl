use super::*;
use crate::process_admission::{ProcessAdmissionError, ProcessAdmissionGate};
use beryl_model::{AdmittedHostPath, PathFlavor, RootId, RuntimeMode, RuntimeNativePath};

#[test]
fn failed_runtime_cleanup_retains_acquisition_and_prevents_reopening() {
    let process = ProcessAdmissionGate::new();
    let gate = MasterCommandGate::new(
        process.clone(),
        ProjectionServiceGeneration::allocate().unwrap(),
        None,
    );
    let mut harness = RuntimeInterestTestHarness {
        owner: RuntimeInterestOwner::new(
            RuntimeInterestConfig::new(
                NonZeroUsize::new(1).unwrap(),
                NonZeroUsize::new(1).unwrap(),
                Duration::from_secs(5),
            )
            .unwrap(),
            gate.authorizer(),
            crate::cas_projection::accepted_input_scheduler::AcceptedInputSchedulerSignal::new(),
            crate::runtime_activity_enrollment::RuntimeActivityEnrollmentOperations::new(
                beryl_model::BerylHomeId::from_bytes([1; 16]),
                NonZeroUsize::new(1).unwrap(),
            ),
        ),
        gate,
    };
    let native = |path: &str| {
        RuntimeNativePath::from_admitted(RuntimeMode::Host, PathFlavor::Windows, path).unwrap()
    };
    let runtime = RuntimeId::from_bytes([71; 16]);
    let spec = ManagedBackendLaunchSpec::new(
        runtime,
        AdmittedHostPath::from_admitted(PathFlavor::Windows, r"C:\runtime\codex.exe").unwrap(),
        RuntimeMode::Host,
        native(r"C:\runtime\codex.exe"),
        native(r"C:\work\beryl"),
        AdmittedHostPath::from_admitted(PathFlavor::Windows, r"C:\tokens").unwrap(),
        native(r"C:\tokens"),
    )
    .unwrap();
    let binding = ExecutionBinding::new(
        runtime,
        RootId::from_bytes([72; 16]),
        native(r"C:\work\beryl"),
    );
    let probe = RuntimeInterestTestProbe::new(CasProcessGeneration::new(91_003).unwrap());
    probe.fail_health(RuntimeFailure::Admission);
    probe.allow_retirement(false);
    probe.fail_retirement(RuntimeFailure::BackendDisposal);
    let interest = harness
        .acquire(
            spec,
            binding,
            RuntimeInterestKind::RequiredWork,
            probe.clone(),
        )
        .unwrap();
    assert!(probe.wait_for_retirement(Duration::from_secs(5)));
    let fence = process.fence().unwrap();
    assert_eq!(
        fence.validate_settled_for(&process),
        Err(ProcessAdmissionError::Unsettled)
    );
    probe.allow_retirement(true);
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    loop {
        let state = harness.owner.shared.lock();
        let entry = state.runtimes.get(&runtime).unwrap();
        if entry.failed_acquisition.is_some() {
            assert!(!entry.cleanup_complete);
            assert_eq!(
                entry.status,
                RuntimeInterestStatus::Unavailable(RuntimeFailure::BackendDisposal)
            );
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "cleanup did not finish"
        );
        drop(state);
        std::thread::sleep(Duration::from_millis(1));
    }
    drop(interest);
    assert_eq!(fence.reopen_if(true), Err(ProcessAdmissionError::Unsettled));
    assert!(!harness.shutdown());
}
