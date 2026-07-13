use std::any::TypeId;

use beryl_model::{
    Availability, CasThreadId, CasTurnId, CommandId, DraftRevision, DynamicToolCallId,
    DynamicToolName, HomeRevision, IdempotencyKey, JobId, MonitorHint, MonitorId, PlacementError,
    Provenance, RevisionError, UnavailableReason, VirtualDesktopId, WindowBounds,
    WindowDisplayState, WindowPlacement,
};

#[test]
fn revisions_are_nonzero_monotonic_and_scope_typed() {
    assert_eq!(HomeRevision::new(0), Err(RevisionError::Zero));

    let revision = HomeRevision::new(7).unwrap();
    assert_eq!(revision.get(), 7);
    assert_eq!(revision.checked_next().unwrap().get(), 8);
    assert_eq!(
        HomeRevision::new(u64::MAX).unwrap().checked_next(),
        Err(RevisionError::Exhausted)
    );
    assert_ne!(TypeId::of::<HomeRevision>(), TypeId::of::<DraftRevision>());
    assert!(serde_json::from_str::<HomeRevision>("0").is_err());
}

#[test]
fn availability_has_only_bounded_reason_categories() {
    let state = Availability::Unavailable(UnavailableReason::OpenElsewhere);
    let json = serde_json::to_string(&state).unwrap();

    assert_eq!(serde_json::from_str::<Availability>(&json).unwrap(), state);
}

#[test]
fn placement_rejects_empty_geometry_and_round_trips() {
    assert!(MonitorId::new("m".repeat(MonitorId::MAX_BYTES)).is_ok());
    assert!(MonitorId::new("m".repeat(MonitorId::MAX_BYTES + 1)).is_err());
    assert_eq!(
        WindowBounds::new(10, 20, 0, 600),
        Err(PlacementError::ZeroWidth)
    );
    let bounds = WindowBounds::new(-200, 50, 1200, 800).unwrap();
    let monitor = MonitorHint::new(
        MonitorId::new(r"\\?\DISPLAY#PRIMARY").unwrap(),
        WindowBounds::new(0, 0, 1920, 1080).unwrap(),
    );
    let placement = WindowPlacement::new(
        bounds,
        WindowDisplayState::Maximized,
        Some(monitor),
        Some(VirtualDesktopId::from_bytes([9; 16])),
    );
    let json = serde_json::to_string(&placement).unwrap();
    let restored: WindowPlacement = serde_json::from_str(&json).unwrap();

    assert_eq!(restored, placement);
    assert_eq!(restored.bounds().width(), 1200);
}

#[test]
fn provenance_retains_exact_bounded_external_identities() {
    let provenance = Provenance::DynamicToolCall {
        thread_id: CasThreadId::new("cas-thread-1").unwrap(),
        turn_id: CasTurnId::new("cas-turn-2").unwrap(),
        tool_name: DynamicToolName::new("beryl.resolve_discussion").unwrap(),
        call_id: DynamicToolCallId::new("call-3").unwrap(),
    };
    let json = serde_json::to_string(&provenance).unwrap();

    assert_eq!(
        serde_json::from_str::<Provenance>(&json).unwrap(),
        provenance
    );
    assert!(DynamicToolCallId::new("x".repeat(257)).is_err());
}

#[test]
fn generated_and_recovery_provenance_use_distinct_identities() {
    let handoff = Provenance::BerylGeneratedHandoff {
        job_id: JobId::from_bytes([3; 16]),
        idempotency_key: IdempotencyKey::from_bytes([4; 16]),
    };
    let recovery = Provenance::DurableRecovery {
        command_id: CommandId::from_bytes([5; 16]),
        idempotency_key: IdempotencyKey::from_bytes([6; 16]),
    };

    assert_ne!(handoff, recovery);
}
