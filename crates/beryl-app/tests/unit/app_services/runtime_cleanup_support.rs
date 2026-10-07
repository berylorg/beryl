use super::*;
use crate::cas_projection::{
    RuntimeFailure, RuntimeInterest, RuntimeInterestStatus, RuntimeInterestTestProbe,
};
use beryl_backend::ManagedBackendLaunchSpec;
use beryl_model::{
    AdmittedHostPath, CasProcessGeneration, ExecutionBinding, PathFlavor, RootId, RuntimeId,
    RuntimeMode, RuntimeNativePath,
};

pub(super) fn failed_runtime(
    owner: &ProcessServiceOwner,
) -> (RuntimeInterestTestProbe, RuntimeInterest) {
    let runtime = RuntimeId::from_bytes([217; 16]);
    let native = |path: &str| {
        RuntimeNativePath::from_admitted(RuntimeMode::Host, PathFlavor::Windows, path).unwrap()
    };
    let directory = native(r"C:\work\runtime-cleanup");
    let spec = ManagedBackendLaunchSpec::new(
        runtime,
        AdmittedHostPath::from_admitted(PathFlavor::Windows, r"C:\runtime-cleanup\codex.exe")
            .unwrap(),
        RuntimeMode::Host,
        native(r"C:\runtime-cleanup\codex.exe"),
        directory.clone(),
        AdmittedHostPath::from_admitted(PathFlavor::Windows, r"C:\tokens").unwrap(),
        native(r"C:\tokens"),
    )
    .unwrap();
    let binding = ExecutionBinding::new(runtime, RootId::from_bytes([218; 16]), directory);
    let probe = RuntimeInterestTestProbe::new(CasProcessGeneration::new(111_017).unwrap());
    probe.fail_retirement(RuntimeFailure::AppRetirement);
    let interest = owner
        .graph()
        .unwrap()
        .cas()
        .acquire_runtime_probe_for_test(spec, binding, probe.clone())
        .unwrap();
    let readiness =
        match interest.wait_for_change(RuntimeInterestStatus::Starting, Duration::from_secs(5)) {
            RuntimeInterestStatus::Ready(readiness) => readiness,
            status => panic!("owned runtime did not publish readiness: {status:?}"),
        };
    probe.fail_health(RuntimeFailure::ProcessExited);
    assert_eq!(
        interest.wait_for_change(
            RuntimeInterestStatus::Ready(readiness),
            Duration::from_secs(5)
        ),
        RuntimeInterestStatus::Unavailable(RuntimeFailure::ProcessExited)
    );
    assert_eq!(probe.counts().0, 1);
    (probe, interest)
}
