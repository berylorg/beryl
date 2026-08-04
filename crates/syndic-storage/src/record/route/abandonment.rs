use beryl_model::{
    AcceptedInputRevision, BindingRevision, CasThreadId, InputGateRevision, SyndicAcceptedInputId,
    SyndicExecutionSnapshotId,
};

use crate::{PendingSteeringTargetProof, SteeringTargetProof};

use super::AcceptedRouteHeadProof;

/// Exact pre-loss active target retained by one projection-loss transition.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AcceptedRouteLostTarget {
    AwaitingSteering(PendingSteeringTargetProof),
    Steering(SteeringTargetProof),
    AwaitingTerminal(SteeringTargetProof),
}

impl AcceptedRouteLostTarget {
    #[must_use]
    pub const fn pending(&self) -> &PendingSteeringTargetProof {
        match self {
            Self::AwaitingSteering(target) => target,
            Self::Steering(target) | Self::AwaitingTerminal(target) => target.pending(),
        }
    }
}

/// Exact disposition selected by one active-binding abandonment.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AcceptedRouteAbandonmentKind {
    Generic,
    ExactRejectedInput {
        input_id: SyndicAcceptedInputId,
        expected_input_revision: AcceptedInputRevision,
    },
}

/// Durable request witness for one active-binding abandonment.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AcceptedRouteAbandonmentProof {
    expected_binding_revision: BindingRevision,
    expected_gate_revision: InputGateRevision,
    expected_route: AcceptedRouteHeadProof,
    kind: AcceptedRouteAbandonmentKind,
}

impl AcceptedRouteAbandonmentProof {
    #[must_use]
    pub const fn new(
        expected_binding_revision: BindingRevision,
        expected_gate_revision: InputGateRevision,
        expected_route: AcceptedRouteHeadProof,
        kind: AcceptedRouteAbandonmentKind,
    ) -> Self {
        Self {
            expected_binding_revision,
            expected_gate_revision,
            expected_route,
            kind,
        }
    }

    #[must_use]
    pub const fn expected_binding_revision(self) -> BindingRevision {
        self.expected_binding_revision
    }

    #[must_use]
    pub const fn expected_gate_revision(self) -> InputGateRevision {
        self.expected_gate_revision
    }

    #[must_use]
    pub const fn expected_route(self) -> AcceptedRouteHeadProof {
        self.expected_route
    }

    #[must_use]
    pub const fn kind(self) -> AcceptedRouteAbandonmentKind {
        self.kind
    }
}

/// Bounded exact provenance for one O(1) route-generation projection loss.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AcceptedRouteProjectionLostProof {
    prior_target: AcceptedRouteLostTarget,
    abandonment: AcceptedRouteAbandonmentProof,
    retirement_binding_revision: BindingRevision,
    snapshot_id: SyndicExecutionSnapshotId,
    cas_thread_id: CasThreadId,
}

impl AcceptedRouteProjectionLostProof {
    #[must_use]
    pub const fn new(
        prior_target: AcceptedRouteLostTarget,
        abandonment: AcceptedRouteAbandonmentProof,
        retirement_binding_revision: BindingRevision,
        snapshot_id: SyndicExecutionSnapshotId,
        cas_thread_id: CasThreadId,
    ) -> Self {
        Self {
            prior_target,
            abandonment,
            retirement_binding_revision,
            snapshot_id,
            cas_thread_id,
        }
    }

    #[must_use]
    pub const fn prior_target(&self) -> &AcceptedRouteLostTarget {
        &self.prior_target
    }

    #[must_use]
    pub const fn abandonment(&self) -> AcceptedRouteAbandonmentProof {
        self.abandonment
    }

    #[must_use]
    pub const fn retirement_binding_revision(&self) -> BindingRevision {
        self.retirement_binding_revision
    }

    #[must_use]
    pub const fn snapshot_id(&self) -> SyndicExecutionSnapshotId {
        self.snapshot_id
    }

    #[must_use]
    pub const fn cas_thread_id(&self) -> &CasThreadId {
        &self.cas_thread_id
    }
}
