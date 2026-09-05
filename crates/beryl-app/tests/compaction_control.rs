#![allow(dead_code, unused_imports)]
#[path = "../src/shell/compaction_control.rs"]
mod compaction_control;
#[path = "../src/compaction_diagnostics.rs"]
mod compaction_diagnostics;
#[path = "../src/shell/compaction_observer.rs"]
mod compaction_observer;
use beryl_model::workspace::WorkspaceId;
use compaction_control::CompactionControl;
use compaction_observer::*;

fn target() -> ObserverTarget {
    ObserverTarget {
        workspace_id: "workspace".into(),
        execution_target: WorkspaceId::host_windows("C:\\work"),
        generation: 8,
        thread_id: "thread".into(),
    }
}
fn operation() -> OperationIdentity {
    OperationIdentity {
        thread_id: "thread".into(),
        operation_id: "operation".into(),
        observation_session_id: "backend-process".into(),
    }
}
fn update(kind: UpdateKind) -> ObserverUpdate {
    ObserverUpdate {
        target: target(),
        operation: Some(operation()),
        kind,
    }
}
fn prepared() -> CompactionControl {
    let mut state = CompactionControl::new(target());
    assert!(state.accept(update(UpdateKind::Prepared)).is_some());
    state
}

#[test]
fn warning_and_uncertainty_keep_the_same_operation_admitted_until_late_success() {
    let mut state = prepared();
    for kind in [
        UpdateKind::Warning,
        UpdateKind::Unconfirmed(UnconfirmedReason::Unavailable),
        UpdateKind::TurnKnown {
            turn_id: "compact-turn".into(),
        },
    ] {
        assert_eq!(state.accept(update(kind.clone())), Some(kind));
    }
    assert_eq!(
        state.accept(update(UpdateKind::Finished(Outcome::Succeeded))),
        Some(UpdateKind::Finished(Outcome::Succeeded))
    );
    assert!(
        state
            .accept(update(UpdateKind::Finished(Outcome::Succeeded)))
            .is_none()
    );
}

#[test]
fn different_generation_workspace_runtime_and_operation_cannot_complete_current_work() {
    let mut state = prepared();
    for variant in 0..5 {
        let mut stale = update(UpdateKind::Finished(Outcome::Succeeded));
        match variant {
            0 => stale.target.generation += 1,
            1 => stale.target.workspace_id = "other".into(),
            2 => stale.target.execution_target = WorkspaceId::host_windows("C:\\other"),
            3 => stale.operation.as_mut().unwrap().observation_session_id = "new-backend".into(),
            _ => stale.operation.as_mut().unwrap().operation_id = "other-operation".into(),
        }
        assert!(state.accept(stale).is_none());
    }
    assert!(
        state
            .accept(update(UpdateKind::Finished(Outcome::Succeeded)))
            .is_some()
    );
}

#[test]
fn known_stop_identity_cannot_be_retargeted() {
    let mut state = prepared();
    assert!(
        state
            .accept(update(UpdateKind::TurnKnown {
                turn_id: "exact".into()
            }))
            .is_some()
    );
    assert!(
        state
            .accept(update(UpdateKind::TurnKnown {
                turn_id: "unrelated".into()
            }))
            .is_none()
    );
    assert!(
        state
            .accept(update(UpdateKind::TurnKnown {
                turn_id: "exact".into()
            }))
            .is_some()
    );
}

#[test]
fn subscription_rejection_before_prepared_update_ends_local_operation() {
    let mut state = CompactionControl::new(target());
    let rejection = UpdateKind::Finished(Outcome::Rejected {
        message: "subscription failed".into(),
    });
    assert_eq!(state.accept(update(rejection.clone())), Some(rejection));
    assert!(state.accept(update(UpdateKind::Prepared)).is_none());
}

#[test]
fn capability_rejection_without_operation_is_terminal_but_unprepared_success_is_not() {
    let mut state = CompactionControl::new(target());
    assert!(
        state
            .accept(ObserverUpdate {
                operation: None,
                ..update(UpdateKind::Finished(Outcome::Succeeded))
            })
            .is_none()
    );
    let rejection = UpdateKind::Finished(Outcome::Rejected {
        message: "unsupported".into(),
    });
    assert_eq!(
        state.accept(ObserverUpdate {
            operation: None,
            ..update(rejection.clone())
        }),
        Some(rejection)
    );
}

#[test]
fn interruption_and_failure_remain_distinct_terminal_effects() {
    for outcome in [
        Outcome::Interrupted,
        Outcome::Failed {
            message: "backend failure".into(),
        },
    ] {
        let mut state = prepared();
        assert_eq!(
            state.accept(update(UpdateKind::Finished(outcome.clone()))),
            Some(UpdateKind::Finished(outcome))
        );
        assert!(
            state
                .accept(update(UpdateKind::Finished(Outcome::Succeeded)))
                .is_none()
        );
    }
}
