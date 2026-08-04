use beryl_home_store::{CurrentDomainCommand, MutationContribution};
use beryl_model::{DomainRevision, InputGateRevision};

use crate::{
    AcceptedRouteHeadProof, StopAttemptNonce, StopCause, StopCauseSet, StopOperationId,
    StopOperationRevision, StopOperationTarget, SyndicStorage,
};

mod abandonment;
mod admission;
mod authority;
mod transition;

pub use abandonment::AbandonStopOperation;

/// Stable intent for atomically electing one exact active operation for stopping.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AdmitStopOperation {
    operation_id: StopOperationId,
    target: StopOperationTarget,
    expected_gate_revision: InputGateRevision,
    expected_route: Option<AcceptedRouteHeadProof>,
    causes: StopCauseSet,
}

impl AdmitStopOperation {
    /// Captures the exact gate, route, target, identity, and initial causes to admit.
    #[must_use]
    pub const fn new(
        operation_id: StopOperationId,
        target: StopOperationTarget,
        expected_gate_revision: InputGateRevision,
        expected_route: AcceptedRouteHeadProof,
        causes: StopCauseSet,
    ) -> Self {
        Self {
            operation_id,
            target,
            expected_gate_revision,
            expected_route: Some(expected_route),
            causes,
        }
    }

    #[must_use]
    pub(crate) const fn provider_operation(
        operation_id: StopOperationId,
        target: StopOperationTarget,
        expected_gate_revision: InputGateRevision,
        causes: StopCauseSet,
    ) -> Self {
        Self {
            operation_id,
            target,
            expected_gate_revision,
            expected_route: None,
            causes,
        }
    }

    #[must_use]
    pub const fn operation_id(&self) -> StopOperationId {
        self.operation_id
    }

    #[must_use]
    pub const fn target(&self) -> &StopOperationTarget {
        &self.target
    }

    #[must_use]
    pub const fn expected_gate_revision(&self) -> InputGateRevision {
        self.expected_gate_revision
    }

    #[must_use]
    pub const fn expected_route(&self) -> AcceptedRouteHeadProof {
        match self.expected_route {
            Some(route) => route,
            None => panic!("provider-operation stop has no selected route"),
        }
    }

    #[must_use]
    pub const fn expected_route_option(&self) -> Option<AcceptedRouteHeadProof> {
        self.expected_route
    }

    #[must_use]
    pub const fn causes(&self) -> StopCauseSet {
        self.causes
    }
}

/// Stable intent for monotonically adding one cause to one live stop operation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct JoinStopCause {
    operation_id: StopOperationId,
    target: StopOperationTarget,
    expected_gate_revision: InputGateRevision,
    expected_stop_revision: StopOperationRevision,
    cause: StopCause,
}

impl JoinStopCause {
    #[must_use]
    pub const fn new(
        operation_id: StopOperationId,
        target: StopOperationTarget,
        expected_gate_revision: InputGateRevision,
        expected_stop_revision: StopOperationRevision,
        cause: StopCause,
    ) -> Self {
        Self {
            operation_id,
            target,
            expected_gate_revision,
            expected_stop_revision,
            cause,
        }
    }

    #[must_use]
    pub const fn operation_id(&self) -> StopOperationId {
        self.operation_id
    }

    #[must_use]
    pub const fn target(&self) -> &StopOperationTarget {
        &self.target
    }

    #[must_use]
    pub const fn expected_gate_revision(&self) -> InputGateRevision {
        self.expected_gate_revision
    }

    #[must_use]
    pub const fn expected_stop_revision(&self) -> StopOperationRevision {
        self.expected_stop_revision
    }

    #[must_use]
    pub const fn cause(&self) -> StopCause {
        self.cause
    }
}

/// Stable intent for claiming the sole backend dispatch attempt of one stop operation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ClaimStopDispatch {
    operation_id: StopOperationId,
    target: StopOperationTarget,
    expected_gate_revision: InputGateRevision,
    expected_stop_revision: StopOperationRevision,
    attempt: StopAttemptNonce,
}

impl ClaimStopDispatch {
    #[must_use]
    pub const fn new(
        operation_id: StopOperationId,
        target: StopOperationTarget,
        expected_gate_revision: InputGateRevision,
        expected_stop_revision: StopOperationRevision,
        attempt: StopAttemptNonce,
    ) -> Self {
        Self {
            operation_id,
            target,
            expected_gate_revision,
            expected_stop_revision,
            attempt,
        }
    }

    #[must_use]
    pub const fn operation_id(&self) -> StopOperationId {
        self.operation_id
    }

    #[must_use]
    pub const fn target(&self) -> &StopOperationTarget {
        &self.target
    }

    #[must_use]
    pub const fn expected_gate_revision(&self) -> InputGateRevision {
        self.expected_gate_revision
    }

    #[must_use]
    pub const fn expected_stop_revision(&self) -> StopOperationRevision {
        self.expected_stop_revision
    }

    #[must_use]
    pub const fn attempt(&self) -> StopAttemptNonce {
        self.attempt
    }
}

/// Stable closed proof that one live stop request was prevented from dispatching locally.
///
/// Construction is the caller's assertion that every request byte was prevented. Storage still
/// requires the exact live operation and execution authority to remain current.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SafelyReopenStopOperation {
    operation_id: StopOperationId,
    target: StopOperationTarget,
    expected_gate_revision: InputGateRevision,
    expected_stop_revision: StopOperationRevision,
}

impl SafelyReopenStopOperation {
    #[must_use]
    pub const fn new(
        operation_id: StopOperationId,
        target: StopOperationTarget,
        expected_gate_revision: InputGateRevision,
        expected_stop_revision: StopOperationRevision,
    ) -> Self {
        Self {
            operation_id,
            target,
            expected_gate_revision,
            expected_stop_revision,
        }
    }

    #[must_use]
    pub const fn operation_id(&self) -> StopOperationId {
        self.operation_id
    }

    #[must_use]
    pub const fn target(&self) -> &StopOperationTarget {
        &self.target
    }

    #[must_use]
    pub const fn expected_gate_revision(&self) -> InputGateRevision {
        self.expected_gate_revision
    }

    #[must_use]
    pub const fn expected_stop_revision(&self) -> StopOperationRevision {
        self.expected_stop_revision
    }
}

pub(super) struct AdmitStopOperationMutation {
    request: AdmitStopOperation,
}

pub(super) struct JoinStopCauseMutation {
    request: JoinStopCause,
}

pub(super) struct ClaimStopDispatchMutation {
    request: ClaimStopDispatch,
}

pub(super) struct SafelyReopenStopOperationMutation {
    request: SafelyReopenStopOperation,
}

use abandonment::AbandonStopOperationMutation;

impl SyndicStorage {
    /// Atomically selects one exact active operation and makes its ready steering work next-turn.
    #[must_use]
    pub fn admit_stop_operation(
        &self,
        expected_domain_revision: DomainRevision,
        request: AdmitStopOperation,
    ) -> MutationContribution {
        self.handle.contribution(
            expected_domain_revision,
            AdmitStopOperationMutation { request },
        )
    }

    /// Admits one exact stop through the current physical domain revision.
    #[must_use]
    pub fn current_admit_stop_operation(
        &self,
        request: AdmitStopOperation,
    ) -> CurrentDomainCommand {
        self.handle
            .current_command(AdmitStopOperationMutation { request })
    }

    /// Monotonically adds one cause to an exact live stop operation.
    #[must_use]
    pub fn join_stop_cause(
        &self,
        expected_domain_revision: DomainRevision,
        request: JoinStopCause,
    ) -> MutationContribution {
        self.handle
            .contribution(expected_domain_revision, JoinStopCauseMutation { request })
    }

    /// Joins one cause through the current physical domain revision.
    #[must_use]
    pub fn current_join_stop_cause(&self, request: JoinStopCause) -> CurrentDomainCommand {
        self.handle
            .current_command(JoinStopCauseMutation { request })
    }

    /// Claims the only backend dispatch attempt admitted for one stop operation.
    #[must_use]
    pub fn claim_stop_dispatch(
        &self,
        expected_domain_revision: DomainRevision,
        request: ClaimStopDispatch,
    ) -> MutationContribution {
        self.handle.contribution(
            expected_domain_revision,
            ClaimStopDispatchMutation { request },
        )
    }

    /// Claims the only dispatch attempt through the current physical domain revision.
    #[must_use]
    pub fn current_claim_stop_dispatch(&self, request: ClaimStopDispatch) -> CurrentDomainCommand {
        self.handle
            .current_command(ClaimStopDispatchMutation { request })
    }

    /// Consumes a live stop after the caller proves every request byte was prevented.
    #[must_use]
    pub fn safely_reopen_stop_operation(
        &self,
        expected_domain_revision: DomainRevision,
        request: SafelyReopenStopOperation,
    ) -> MutationContribution {
        self.handle.contribution(
            expected_domain_revision,
            SafelyReopenStopOperationMutation { request },
        )
    }

    /// Safely reopens one stop through the current physical domain revision.
    #[must_use]
    pub fn current_safely_reopen_stop_operation(
        &self,
        request: SafelyReopenStopOperation,
    ) -> CurrentDomainCommand {
        self.handle
            .current_command(SafelyReopenStopOperationMutation { request })
    }

    /// Consumes a live stop through classified target-authority loss.
    #[must_use]
    pub fn abandon_stop_operation(
        &self,
        expected_domain_revision: DomainRevision,
        request: AbandonStopOperation,
    ) -> MutationContribution {
        self.handle.contribution(
            expected_domain_revision,
            AbandonStopOperationMutation { request },
        )
    }

    /// Abandons one stop through the current physical domain revision.
    #[must_use]
    pub fn current_abandon_stop_operation(
        &self,
        request: AbandonStopOperation,
    ) -> CurrentDomainCommand {
        self.handle
            .current_command(AbandonStopOperationMutation { request })
    }
}
