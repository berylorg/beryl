use beryl_backend::{
    ManagedBackendError, ManagedBackendLaunchSpec, ManagedBackendServer, WslFilesystemObservation,
    WslFilesystemOperation, WslSupervisorArtifact,
};
use beryl_model::{
    AdmittedHostPath, PathFlavor, RuntimeId, RuntimeMode, RuntimeNativePath, WslDistributionName,
};
use std::{
    sync::{Arc, atomic::AtomicBool},
    time::Duration,
};

fn artifact(version: u16, path: &str) -> Result<Arc<WslSupervisorArtifact>, ManagedBackendError> {
    WslSupervisorArtifact::from_verified_release(
        r"C:\Beryl\beryl-wsl-supervisor".into(),
        path.into(),
        [8; 32],
        version,
        Arc::new(tempfile::tempfile().unwrap()),
    )
    .map(Arc::new)
}

#[test]
fn incompatible_or_unmapped_artifact_is_unavailable() {
    for (version, path) in [
        (2, "/mnt/c/Beryl/beryl-wsl-supervisor"),
        (1, "/tmp/beryl-wsl-supervisor"),
        (1, "/mnt/c/../beryl-wsl-supervisor"),
    ] {
        assert!(matches!(
            artifact(version, path),
            Err(ManagedBackendError::WslArtifactUnavailable)
        ));
    }
}

#[test]
fn invalid_observation_requests_never_acquire_a_process_owner() {
    let artifact = artifact(1, "/mnt/c/Beryl/beryl-wsl-supervisor").unwrap();
    let distribution = WslDistributionName::new("Ubuntu").unwrap();
    for (operation, timeout) in [
        (
            WslFilesystemOperation::Directory("relative/path".into()),
            Duration::from_secs(1),
        ),
        (
            WslFilesystemOperation::Executable(format!("/{}", "a".repeat(4096))),
            Duration::from_secs(1),
        ),
        (
            WslFilesystemOperation::Directory("/bad\0path".into()),
            Duration::from_secs(1),
        ),
        (WslFilesystemOperation::UserHome, Duration::ZERO),
        (WslFilesystemOperation::UserHome, Duration::from_secs(31)),
    ] {
        let failure = WslFilesystemObservation::observe(
            artifact.clone(),
            &distribution,
            operation,
            timeout,
            &AtomicBool::new(false),
        )
        .unwrap_err();
        let (error, cleanup) = failure.into_parts();
        assert!(matches!(error, ManagedBackendError::WslObservationInvalid));
        assert!(cleanup.is_none());
    }
}

#[test]
fn cancellation_before_observation_does_not_start_a_companion() {
    let failure = WslFilesystemObservation::observe(
        artifact(1, "/mnt/c/Beryl/beryl-wsl-supervisor").unwrap(),
        &WslDistributionName::new("Ubuntu").unwrap(),
        WslFilesystemOperation::UserHome,
        Duration::from_secs(1),
        &AtomicBool::new(true),
    )
    .unwrap_err();
    let (error, cleanup) = failure.into_parts();
    assert!(matches!(
        error,
        ManagedBackendError::WslObservationCancelled
    ));
    assert!(cleanup.is_none());
}

#[test]
fn unbundled_wsl_launch_refuses_before_authentication_or_process_creation() {
    let directory = tempfile::tempdir().unwrap();
    let token_directory = directory.path().join("tokens");
    let mode = RuntimeMode::wsl("Ubuntu").unwrap();
    let native =
        |path| RuntimeNativePath::from_admitted(mode.clone(), PathFlavor::Posix, path).unwrap();
    let spec = ManagedBackendLaunchSpec::new(
        RuntimeId::from_bytes([1; 16]),
        AdmittedHostPath::from_admitted(
            PathFlavor::Windows,
            r"\\wsl.localhost\Ubuntu\usr\bin\codex",
        )
        .unwrap(),
        mode.clone(),
        native("/usr/bin/codex"),
        native("/work"),
        AdmittedHostPath::from_admitted(
            if cfg!(windows) {
                PathFlavor::Windows
            } else {
                PathFlavor::Posix
            },
            token_directory.to_str().unwrap(),
        )
        .unwrap(),
        native("/tmp/tokens"),
    )
    .unwrap();
    let mut failure = ManagedBackendServer::launch(spec).unwrap_err();
    assert!(matches!(
        failure.error(),
        ManagedBackendError::WslArtifactUnavailable
    ));
    assert!(!failure.cleanup_pending());
    failure.shutdown().unwrap();
    assert!(!token_directory.exists());
}
