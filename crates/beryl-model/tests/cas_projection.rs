use beryl_model::cas_projection::{
    CasBindingMutation, CasGraphAction, CasGraphActionClassificationInput, CasLineageProof,
    CasNativeOperationKind, CasProjectionBindingStatus, CasReflectionOutcome,
    classify_cas_graph_action,
};

#[test]
fn ui_only_actions_do_not_touch_cas_projection() {
    let classification = classify_cas_graph_action(CasGraphActionClassificationInput::new(
        CasGraphAction::UiOnly,
        CasProjectionBindingStatus::Valid,
        CasLineageProof::Exact,
    ));

    assert_eq!(classification.outcome, CasReflectionOutcome::NoCasEffect);
    assert_eq!(
        classification.binding_mutation,
        CasBindingMutation::Preserve
    );
}

#[test]
fn new_syndic_view_is_unbound_without_immediate_cas_effect() {
    let classification = classify_cas_graph_action(CasGraphActionClassificationInput::new(
        CasGraphAction::CreateThreadView,
        CasProjectionBindingStatus::Unbound,
        CasLineageProof::Missing,
    ));

    assert_eq!(classification.outcome, CasReflectionOutcome::NoCasEffect);
    assert_eq!(
        classification.binding_mutation,
        CasBindingMutation::MarkUnbound
    );
}

#[test]
fn valid_exact_append_uses_native_turn_start_and_locks_active_binding() {
    let classification = classify_cas_graph_action(CasGraphActionClassificationInput::new(
        CasGraphAction::AppendUserTurn,
        CasProjectionBindingStatus::Valid,
        CasLineageProof::Exact,
    ));

    assert_eq!(
        classification.outcome,
        CasReflectionOutcome::CasNativeOperation(CasNativeOperationKind::TurnStart)
    );
    assert_eq!(
        classification.binding_mutation,
        CasBindingMutation::LockActive
    );
}

#[test]
fn stale_append_materializes_on_next_run_without_native_cas_call() {
    let classification = classify_cas_graph_action(CasGraphActionClassificationInput::new(
        CasGraphAction::AppendUserTurn,
        CasProjectionBindingStatus::Stale,
        CasLineageProof::Missing,
    ));

    assert_eq!(
        classification.outcome,
        CasReflectionOutcome::MaterializeFreshCasProjectionOnNextRun
    );
    assert_eq!(
        classification.binding_mutation,
        CasBindingMutation::Preserve
    );
}

#[test]
fn exact_tail_delete_uses_native_rollback() {
    let classification = classify_cas_graph_action(CasGraphActionClassificationInput::new(
        CasGraphAction::DeleteTail,
        CasProjectionBindingStatus::Valid,
        CasLineageProof::Exact,
    ));

    assert_eq!(
        classification.outcome,
        CasReflectionOutcome::CasNativeOperation(CasNativeOperationKind::Rollback)
    );
    assert_eq!(
        classification.binding_mutation,
        CasBindingMutation::MarkStale
    );
}

#[test]
fn missing_tail_proof_materializes_instead_of_guessing_rollback() {
    let classification = classify_cas_graph_action(CasGraphActionClassificationInput::new(
        CasGraphAction::DeleteTail,
        CasProjectionBindingStatus::Valid,
        CasLineageProof::Missing,
    ));

    assert_eq!(
        classification.outcome,
        CasReflectionOutcome::MaterializeFreshCasProjectionOnNextRun
    );
    assert_eq!(
        classification.binding_mutation,
        CasBindingMutation::MarkUnbound
    );
}

#[test]
fn middle_delete_invalidates_projection() {
    let classification = classify_cas_graph_action(CasGraphActionClassificationInput::new(
        CasGraphAction::DeleteMiddle,
        CasProjectionBindingStatus::Valid,
        CasLineageProof::Exact,
    ));

    assert_eq!(
        classification.outcome,
        CasReflectionOutcome::InvalidateCasProjection
    );
    assert_eq!(
        classification.binding_mutation,
        CasBindingMutation::MarkStale
    );
}

#[test]
fn active_stop_requires_active_exact_binding() {
    let classification = classify_cas_graph_action(CasGraphActionClassificationInput::new(
        CasGraphAction::StopActiveTurn,
        CasProjectionBindingStatus::Active,
        CasLineageProof::Exact,
    ));

    assert_eq!(
        classification.outcome,
        CasReflectionOutcome::CasNativeOperation(CasNativeOperationKind::StopActiveTurn)
    );
    assert_eq!(
        classification.binding_mutation,
        CasBindingMutation::Preserve
    );

    let idle_classification = classify_cas_graph_action(CasGraphActionClassificationInput::new(
        CasGraphAction::StopActiveTurn,
        CasProjectionBindingStatus::Valid,
        CasLineageProof::Exact,
    ));

    assert_eq!(
        idle_classification.outcome,
        CasReflectionOutcome::NoCasEffect
    );
    assert_eq!(
        idle_classification.binding_mutation,
        CasBindingMutation::Preserve
    );
}
