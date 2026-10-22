use std::num::NonZeroUsize;

use beryl_backend::ManagedBackendLaunchSpec;
use beryl_home_store::HomeStore;
use beryl_model::{
    AdmittedHostPath, ExecutionBinding, PathFlavor, RuntimeMode, RuntimeNativePath, SyndicThreadId,
    SyndicTurnId,
};
use syndic_storage::SyndicStorage;

use crate::cas_projection::{
    AdmittedProjectionSession, RuntimeInterestConfig, RuntimeInterestKind, RuntimeInterestStatus,
    runtime_interest::{RuntimeInterestTestHarness, RuntimeInterestTestProbe},
};

pub(super) fn enroll_session(
    home: &HomeStore,
    storage: &SyndicStorage,
    binding: ExecutionBinding,
    thread: SyndicThreadId,
    turn: SyndicTurnId,
    session: AdmittedProjectionSession,
) -> (RuntimeInterestTestHarness, AdmittedProjectionSession) {
    let generation = session.process_generation();
    let native = |path: &str| {
        RuntimeNativePath::from_admitted(RuntimeMode::Host, PathFlavor::Windows, path).unwrap()
    };
    let spec = ManagedBackendLaunchSpec::new(
        binding.runtime_id(),
        AdmittedHostPath::from_admitted(PathFlavor::Windows, r"C:\runtime\codex.exe").unwrap(),
        RuntimeMode::Host,
        beryl_model::RuntimeLaunchForm::CodexCli,
        native(r"C:\runtime\codex.exe"),
        binding.root_path().clone(),
        AdmittedHostPath::from_admitted(PathFlavor::Windows, r"C:\tokens").unwrap(),
        native(r"C:\tokens"),
    )
    .unwrap();
    let harness = RuntimeInterestTestHarness::for_home(
        RuntimeInterestConfig::new(NonZeroUsize::MIN, NonZeroUsize::MIN, super::TIMEOUT).unwrap(),
        home.home_id(),
    );
    let interest = harness
        .acquire(
            spec,
            binding,
            RuntimeInterestKind::RequiredWork,
            RuntimeInterestTestProbe::new(generation),
        )
        .unwrap();
    let RuntimeInterestStatus::Ready(ready) =
        interest.wait_for_change(RuntimeInterestStatus::Starting, super::TIMEOUT)
    else {
        panic!("fixture runtime did not become ready");
    };
    assert_eq!(ready.process_generation(), generation);
    interest
        .enroll_activity_for_test(home, storage, thread, turn)
        .unwrap();
    let session = interest.publish_session(ready, session).unwrap();
    (harness, session)
}
