#[path = "../src/shell/backend_availability.rs"]
mod backend_availability;

use std::{io, time::Duration};

use backend_availability::BackendUnavailableKind;
use beryl_backend::{CompatibilityError, ManagedBackendError};

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
    let old_codex =
        ManagedBackendError::Compatibility(CompatibilityError::AppServerVersionMismatch {
            required_version: "0.137.0",
            actual_version: "0.128.0".to_string(),
            user_agent: "beryl/0.128.0 (Windows 10.0.26200; aarch64)".to_string(),
        });
    let missing_history_contract = ManagedBackendError::Compatibility(
        CompatibilityError::ThreadTurnsListItemsViewUnsupported {
            method: "thread/turns/list",
            code: -32600,
            message: "unknown field `itemsView`".to_string(),
        },
    );

    assert_eq!(
        BackendUnavailableKind::from_backend_error(&incompatible),
        BackendUnavailableKind::Incompatible
    );
    assert_eq!(
        BackendUnavailableKind::from_backend_error(&old_codex),
        BackendUnavailableKind::Incompatible
    );
    assert_eq!(
        BackendUnavailableKind::from_backend_error(&missing_history_contract),
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
}
