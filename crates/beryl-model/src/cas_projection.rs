//! Pure CAS projection binding and graph-action reflection types.

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CasProjectionBindingStatus {
    Valid,
    Stale,
    Unbound,
    Active,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CasLineageProof {
    Exact,
    Missing,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CasGraphAction {
    UiOnly,
    NonExecutingView,
    CreateThreadView,
    AppendUserTurn,
    BranchAtExactPrefix,
    BranchAtUnprovenPoint,
    DeleteTail,
    DeleteMiddle,
    EditReplacementTail,
    EditReplacementNonTail,
    ReparentTurns,
    AncestorMutationDuringActiveTurn,
    StopActiveTurn,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CasNativeOperationKind {
    TurnStart,
    ActiveTurnSteer,
    Fork,
    Rollback,
    EditTailReplacement,
    StopActiveTurn,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CasReflectionOutcome {
    NoCasEffect,
    CasNativeOperation(CasNativeOperationKind),
    InvalidateCasProjection,
    MaterializeFreshCasProjectionOnNextRun,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CasBindingMutation {
    Preserve,
    MarkUnbound,
    MarkStale,
    LockActive,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CasGraphActionClassificationInput {
    pub action: CasGraphAction,
    pub binding_status: CasProjectionBindingStatus,
    pub lineage_proof: CasLineageProof,
}

impl CasGraphActionClassificationInput {
    pub fn new(
        action: CasGraphAction,
        binding_status: CasProjectionBindingStatus,
        lineage_proof: CasLineageProof,
    ) -> Self {
        Self {
            action,
            binding_status,
            lineage_proof,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CasGraphActionClassification {
    pub outcome: CasReflectionOutcome,
    pub binding_mutation: CasBindingMutation,
}

impl CasGraphActionClassification {
    pub fn new(outcome: CasReflectionOutcome, binding_mutation: CasBindingMutation) -> Self {
        Self {
            outcome,
            binding_mutation,
        }
    }
}

pub fn classify_cas_graph_action(
    input: CasGraphActionClassificationInput,
) -> CasGraphActionClassification {
    use CasBindingMutation::{LockActive, MarkStale, MarkUnbound, Preserve};
    use CasGraphAction::{
        AncestorMutationDuringActiveTurn, AppendUserTurn, BranchAtExactPrefix,
        BranchAtUnprovenPoint, CreateThreadView, DeleteMiddle, DeleteTail, EditReplacementNonTail,
        EditReplacementTail, NonExecutingView, ReparentTurns, StopActiveTurn, UiOnly,
    };
    use CasLineageProof::Exact;
    use CasNativeOperationKind::{
        ActiveTurnSteer, EditTailReplacement, Fork, Rollback, StopActiveTurn as NativeStop,
        TurnStart,
    };
    use CasProjectionBindingStatus::{Active, Valid};
    use CasReflectionOutcome::{
        CasNativeOperation, InvalidateCasProjection, MaterializeFreshCasProjectionOnNextRun,
        NoCasEffect,
    };

    match input.action {
        UiOnly | NonExecutingView => CasGraphActionClassification::new(NoCasEffect, Preserve),
        CreateThreadView => CasGraphActionClassification::new(NoCasEffect, MarkUnbound),
        AppendUserTurn => match (input.binding_status, input.lineage_proof) {
            (Valid, Exact) => {
                CasGraphActionClassification::new(CasNativeOperation(TurnStart), LockActive)
            }
            (Active, Exact) => {
                CasGraphActionClassification::new(CasNativeOperation(ActiveTurnSteer), Preserve)
            }
            (Valid, CasLineageProof::Missing) => {
                CasGraphActionClassification::new(MaterializeFreshCasProjectionOnNextRun, MarkStale)
            }
            _ => {
                CasGraphActionClassification::new(MaterializeFreshCasProjectionOnNextRun, Preserve)
            }
        },
        BranchAtExactPrefix => native_with_exact_proof(input, Fork, MarkUnbound),
        DeleteTail => native_with_exact_proof(input, Rollback, MarkStale),
        EditReplacementTail => native_with_exact_proof(input, EditTailReplacement, MarkStale),
        BranchAtUnprovenPoint => {
            CasGraphActionClassification::new(MaterializeFreshCasProjectionOnNextRun, MarkUnbound)
        }
        DeleteMiddle
        | EditReplacementNonTail
        | ReparentTurns
        | AncestorMutationDuringActiveTurn => {
            CasGraphActionClassification::new(InvalidateCasProjection, MarkStale)
        }
        StopActiveTurn => match (input.binding_status, input.lineage_proof) {
            (Active, Exact) => {
                CasGraphActionClassification::new(CasNativeOperation(NativeStop), Preserve)
            }
            _ => CasGraphActionClassification::new(NoCasEffect, Preserve),
        },
    }
}

fn native_with_exact_proof(
    input: CasGraphActionClassificationInput,
    operation: CasNativeOperationKind,
    success_mutation: CasBindingMutation,
) -> CasGraphActionClassification {
    if input.binding_status == CasProjectionBindingStatus::Valid
        && input.lineage_proof == CasLineageProof::Exact
    {
        CasGraphActionClassification::new(
            CasReflectionOutcome::CasNativeOperation(operation),
            success_mutation,
        )
    } else {
        CasGraphActionClassification::new(
            CasReflectionOutcome::MaterializeFreshCasProjectionOnNextRun,
            CasBindingMutation::MarkUnbound,
        )
    }
}
