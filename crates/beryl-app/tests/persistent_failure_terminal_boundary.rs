use std::{fs, path::Path};

#[test]
fn removed_running_recovery_sources_and_authority_surfaces_stay_absent() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/cas_projection");
    for relative in [
        "service/adoption.rs",
        "connection/adoption.rs",
        "connection/driver/adoption.rs",
        "persistent_failure/quarantine.rs",
        "persistent_failure/retention.rs",
        "persistent_failure/coordinator/recovery.rs",
        "accepted_input_scheduler/recovered_projection.rs",
        "service_supervisor/recovery.rs",
        "service_supervisor/provider.rs",
        "connection/authority/candidate_set.rs",
        "reacquisition.rs",
        "service_startup.rs",
        "persistent_failure/notification/flight.rs",
        "persistent_failure/notification/lifecycle.rs",
    ] {
        assert!(
            !root.join(relative).exists(),
            "removed source returned: {relative}"
        );
    }

    let forbidden = [
        "PersistentFailureCutHandoff",
        "PersistentFailureRecoveryInventory",
        "PersistentFailurePendingProjectionQuarantine",
        "PendingProjectionConnectionOwner",
        "CandidateSetConvergedProjectionConnectionOwner",
        "InstalledRecoveredServiceEpoch",
        "PublishedServiceEpoch",
        "new_dormant_with_startup_gate",
        "install_recovered",
        "ProviderBrokerAdoptionStopped",
        "StableConnectionProcessFact",
        "ConnectionServiceEpoch",
        "ConnectionEpochIdentity",
        "ForwardingEpochEndpoint",
        "ForwardingHubEpochGuard",
        "PersistentFailureProjectionRetainer",
        "FailureRetainedPromotionReservation",
        "LoadedRegistryRecovery",
        "ProjectionPreactivationRecoveryHold",
        "PersistentFailureTargetGuardObservation",
        "commit_pair_if_current",
        "ProjectionPreactivationSurrender",
        "ReacquisitionReservation",
        "VerificationPending",
        "ServiceStartupPublicationGuard",
        "publish_verified_current_completion",
        "finish_completed_recovery_supervisor_flight",
        "arm_for_publication",
        "ScheduledOrdinaryExecutionProviderFactory",
        "ScheduledOrdinaryProviderEpochContext",
        "RunningSessionRecoverySupervisor",
        "RunningProjectionServiceLease",
        "RunningSessionRecoveryDiagnostics",
        "cfg(any())",
    ];
    let mut pending = vec![root];
    let mut offenders = Vec::new();
    while let Some(directory) = pending.pop() {
        for entry in fs::read_dir(directory).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                pending.push(path);
            } else if path.extension().and_then(|value| value.to_str()) == Some("rs") {
                let source = fs::read_to_string(&path).unwrap();
                for removed in forbidden {
                    if source.contains(removed) {
                        offenders.push(format!("{} contains {removed}", path.display()));
                    }
                }
            }
        }
    }
    assert!(
        offenders.is_empty(),
        "removed authority remains: {offenders:?}"
    );
}

#[test]
fn terminal_connection_attachment_has_no_replacement_or_early_close_surface() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/cas_projection/connection");
    let forwarding = fs::read_to_string(root.join("forwarding_hub.rs")).unwrap();
    assert!(!forwarding.contains("fn replace("));
    assert!(!forwarding.contains("service_generation"));

    let lifecycle = fs::read_to_string(root.join("lifecycle.rs")).unwrap();
    assert!(lifecycle.contains("fn execute_terminal_shutdown"));
    assert!(lifecycle.contains("stop_and_join_ingester_terminal"));
    assert!(lifecycle.contains("runtime.driver.join()"));

    let attachment = fs::read_to_string(root.join("attachment.rs")).unwrap();
    assert!(attachment.contains("poison.into_inner().take()"));
    assert!(attachment.contains("validate_exact("));
}

#[test]
fn terminal_service_supervisor_has_no_public_recovery_or_publication_surface() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/cas_projection");
    let supervisor = fs::read_to_string(root.join("service_supervisor.rs")).unwrap();
    assert!(!supervisor.contains("Arc<HomeStore>"));
    assert!(!supervisor.contains("Arc< HomeStore"));
    assert!(!supervisor.contains("pub struct TerminalServiceSupervisor"));
    assert!(!supervisor.contains("pub enum TerminalServiceStartError"));
    assert!(!supervisor.contains("pub enum TerminalServiceShutdownError"));

    let initial_start = fs::read_to_string(root.join("initial_start.rs")).unwrap();
    assert!(!initial_start.contains("Mutex"));
    assert!(!initial_start.contains("Publication"));
    assert!(!initial_start.contains("Replacement"));
}

#[test]
fn obsolete_phase62_running_recovery_tests_stay_absent() {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    for relative in [
        "tests/phase62_accepted_next_scheduler.rs",
        "tests/phase62_accepted_next_scheduler/shutdown.rs",
        "tests/phase62_accepted_next_scheduler/support.rs",
        "tests/phase62_accepted_next_scheduler/support/execution.rs",
    ] {
        let source = fs::read_to_string(manifest.join(relative)).unwrap();
        for obsolete in [
            "verification_successes",
            "recovery_cycles",
            "same-generation verification",
            "phase63_restart_handoff",
            "home_generation_failure_before_reservation_makes_supervisor_terminally_unavailable",
            "ReadyProviderFactory",
            "ScheduledOrdinaryExecutionProviderFactory",
            "ScheduledOrdinaryProviderEpochContext",
        ] {
            assert!(
                !source.contains(obsolete),
                "{relative} retains obsolete running recovery assertion {obsolete}"
            );
        }
    }
    assert!(
        !manifest
            .join("tests/phase62_accepted_next_scheduler/promotion_collision.rs")
            .exists()
    );
    assert!(
        !manifest
            .join("tests/phase62_accepted_next_scheduler/promotion_faults.rs")
            .exists()
    );
}

#[test]
fn obsolete_compile_only_test_facades_stay_absent() {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    for relative in [
        "tests/unit/provider_broker_checked_user.rs",
        "tests/unit/provider_broker_ingester_operations.rs",
    ] {
        let source = fs::read_to_string(manifest.join(relative)).unwrap();
        assert!(
            !source.contains("cfg(any())"),
            "{relative} hides an obsolete test behind a compile-only facade"
        );
    }
}
