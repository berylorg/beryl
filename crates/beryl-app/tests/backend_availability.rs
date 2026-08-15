#[path = "../src/shell/backend_availability.rs"]
mod backend_availability;

use std::{io, time::Duration};

use backend_availability::BackendUnavailableKind;
use beryl_backend::{CompatibilityError, ManagedBackendError, ManagedBackendLaunchOptionsError};

#[test]
fn backend_error_classification_distinguishes_missing_executable_from_spawn_failure() {
    let missing = ManagedBackendError::Spawn {
        program: "codex".to_string(),
        source: io::Error::new(io::ErrorKind::NotFound, "not found"),
    };
    let denied = ManagedBackendError::Spawn {
        program: "codex".to_string(),
        source: io::Error::new(io::ErrorKind::PermissionDenied, "denied"),
    };

    assert_eq!(
        BackendUnavailableKind::from_backend_error(&missing),
        BackendUnavailableKind::MissingExecutable
    );
    assert_eq!(
        BackendUnavailableKind::from_backend_error(&denied),
        BackendUnavailableKind::SpawnFailed
    );
}

#[test]
fn backend_error_classification_keeps_incompatibility_target_scoped() {
    let incompatible =
        ManagedBackendError::Compatibility(CompatibilityError::PlatformFamilyMismatch {
            runtime_mode: "Windows host".to_string(),
            expected_platform_family: "windows",
            actual_platform_family: "unix".to_string(),
        });

    assert_eq!(
        BackendUnavailableKind::from_backend_error(&incompatible),
        BackendUnavailableKind::Incompatible
    );
    assert_eq!(
        BackendUnavailableKind::Incompatible.diagnostic_label(),
        "incompatible"
    );
}

#[test]
fn backend_error_classification_groups_probe_and_transport_failures() {
    let timeout = ManagedBackendError::RequestTimeout {
        method: "initialize".to_string(),
        timeout: Duration::from_secs(1),
    };

    assert_eq!(
        BackendUnavailableKind::from_backend_error(&timeout),
        BackendUnavailableKind::ProbeFailed
    );
    assert_eq!(
        BackendUnavailableKind::ProbeFailed.diagnostic_label(),
        "probe_failed"
    );

    let malformed_error_notification =
        ManagedBackendError::MalformedTurnErrorNotificationEnvelope {
            detail: "params are required",
        };
    assert_eq!(
        BackendUnavailableKind::from_backend_error(&malformed_error_notification),
        BackendUnavailableKind::ProbeFailed
    );
}

#[test]
fn backend_error_classification_covers_new_launch_and_combined_shutdown_failures() {
    let invalid_options = ManagedBackendError::InvalidLaunchOptions {
        source: ManagedBackendLaunchOptionsError::EmptyExactHostWindowsProgram,
    };
    let combined_shutdown = ManagedBackendError::ShutdownProcessAndAuth {
        process: Box::new(ManagedBackendError::ProcessExited {
            method: "shutdown".to_string(),
        }),
        auth: Box::new(ManagedBackendError::RequestTimeout {
            method: "auth cleanup".to_string(),
            timeout: Duration::from_secs(1),
        }),
    };

    assert_eq!(
        BackendUnavailableKind::from_backend_error(&invalid_options),
        BackendUnavailableKind::SpawnFailed
    );
    assert_eq!(
        BackendUnavailableKind::from_backend_error(&combined_shutdown),
        BackendUnavailableKind::ProbeFailed
    );
}

#[test]
fn backend_error_classification_groups_session_and_stdio_failures_with_probe_failures() {
    let errors = [
        ManagedBackendError::SessionPoisoned {
            method: "config/read".to_string(),
        },
        ManagedBackendError::StdioWriterStopped {
            method: "config/read".to_string(),
        },
        ManagedBackendError::StdioWriterPanicked,
        ManagedBackendError::StdioCleanupFailures {
            failures: vec![ManagedBackendError::ProcessExited {
                method: "shutdown".to_string(),
            }],
        },
    ];

    for error in &errors {
        assert_eq!(
            BackendUnavailableKind::from_backend_error(error),
            BackendUnavailableKind::ProbeFailed
        );
    }
}
