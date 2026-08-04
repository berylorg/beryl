mod abandon;
mod active;
mod cancel;
mod publish;
mod transition;
mod validation;

pub use publish::{PublishStaleBinding, PublishValidBinding};
pub(in crate::mutation) use validation::{advance_reservation, membership};

use beryl_home_store::{CurrentDomainCommand, MutationContribution};
use beryl_model::{
    AcceptedInputRevision, BindingRevision, CasConversationToolProfile, CasLoadedSessionGeneration,
    CasNativeTurnCount, CasThreadId, CasTurnId, DomainRevision, ExecutionBinding,
    InputGateRevision, SyndicAcceptedInputId, SyndicExecutionSnapshotId, SyndicThreadId,
    SyndicTurnId,
};

use crate::{
    AcceptedRouteAbandonmentKind, AcceptedRouteAbandonmentProof, AcceptedRouteGeneration,
    AcceptedRouteHeadProof, AcceptedRouteLostTarget, BindingState, CasLineageProof,
    CasRepresentedPrefixProof, SelectedPathProof, StaleCasBinding, SyndicRecordError,
    SyndicStorage, SyndicTimestamp,
};

use abandon::AbandonActiveBindingMutation;
use active::{ActivateBindingMutation, PublishActiveCasTurnMutation};
use cancel::CancelBindingActivationMutation;
use transition::PublishBindingMutation;

#[cfg(feature = "test-faults")]
pub(crate) fn active_cas_turn_fault_scope() -> beryl_home_store::test_faults::FaultScope {
    beryl_home_store::test_faults::FaultScope::of::<PublishActiveCasTurnMutation>()
}

/// Exact delivering input proven rejected without a closed active-target verdict.
///
/// The surrounding active-binding abandonment preserves this one input as retryable
/// projection-lost next-turn work. Other delivering inputs remain possibly dispatched.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ExactRejectedInputDelivery {
    input_id: SyndicAcceptedInputId,
    expected_input_revision: AcceptedInputRevision,
}

impl ExactRejectedInputDelivery {
    #[must_use]
    pub const fn new(
        input_id: SyndicAcceptedInputId,
        expected_input_revision: AcceptedInputRevision,
    ) -> Self {
        Self {
            input_id,
            expected_input_revision,
        }
    }

    #[must_use]
    pub const fn input_id(self) -> SyndicAcceptedInputId {
        self.input_id
    }

    #[must_use]
    pub const fn expected_input_revision(self) -> AcceptedInputRevision {
        self.expected_input_revision
    }
}

/// Exact immutable inputs for abandoning one active CAS projection after its authority is lost.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AbandonActiveBinding {
    thread_id: SyndicThreadId,
    expected_binding_revision: BindingRevision,
    route_generation: AcceptedRouteGeneration,
    target: AcceptedRouteLostTarget,
    selected_path: SelectedPathProof,
    stale: StaleCasBinding,
    exact_rejected_delivery: Option<ExactRejectedInputDelivery>,
}

impl AbandonActiveBinding {
    #[must_use]
    pub const fn new(
        thread_id: SyndicThreadId,
        expected_binding_revision: BindingRevision,
        route_generation: AcceptedRouteGeneration,
        target: AcceptedRouteLostTarget,
        selected_path: SelectedPathProof,
        stale: StaleCasBinding,
    ) -> Self {
        Self {
            thread_id,
            expected_binding_revision,
            route_generation,
            target,
            selected_path,
            stale,
            exact_rejected_delivery: None,
        }
    }

    /// Constructs an atomic abandonment that preserves one exactly rejected delivery.
    #[must_use]
    pub fn after_exact_rejection(
        thread_id: SyndicThreadId,
        expected_binding_revision: BindingRevision,
        route_generation: AcceptedRouteGeneration,
        target: AcceptedRouteLostTarget,
        selected_path: SelectedPathProof,
        stale: StaleCasBinding,
        exact_rejected_delivery: ExactRejectedInputDelivery,
    ) -> Self {
        Self {
            thread_id,
            expected_binding_revision,
            route_generation,
            target,
            selected_path,
            stale,
            exact_rejected_delivery: Some(exact_rejected_delivery),
        }
    }

    #[must_use]
    pub const fn thread_id(&self) -> SyndicThreadId {
        self.thread_id
    }

    #[must_use]
    pub const fn expected_binding_revision(&self) -> BindingRevision {
        self.expected_binding_revision
    }

    #[must_use]
    pub const fn route_generation(&self) -> AcceptedRouteGeneration {
        self.route_generation
    }

    #[must_use]
    pub const fn target(&self) -> &AcceptedRouteLostTarget {
        &self.target
    }

    #[must_use]
    pub const fn selected_path(&self) -> SelectedPathProof {
        self.selected_path
    }

    #[must_use]
    pub const fn stale(&self) -> &StaleCasBinding {
        &self.stale
    }

    #[must_use]
    pub const fn exact_rejected_delivery(&self) -> Option<ExactRejectedInputDelivery> {
        self.exact_rejected_delivery
    }

    pub(crate) const fn route_abandonment_proof(
        &self,
        source_gate_revision: InputGateRevision,
        source_route: AcceptedRouteHeadProof,
    ) -> AcceptedRouteAbandonmentProof {
        AcceptedRouteAbandonmentProof::new(
            self.expected_binding_revision,
            source_gate_revision,
            source_route,
            self.route_abandonment_kind(),
        )
    }

    pub(crate) const fn route_abandonment_kind(&self) -> AcceptedRouteAbandonmentKind {
        match self.exact_rejected_delivery {
            Some(rejected) => AcceptedRouteAbandonmentKind::ExactRejectedInput {
                input_id: rejected.input_id(),
                expected_input_revision: rejected.expected_input_revision(),
            },
            None => AcceptedRouteAbandonmentKind::Generic,
        }
    }
}

/// Exact immutable inputs for publishing an explicit absence of usable CAS projection.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PublishUnboundBinding {
    thread_id: SyndicThreadId,
    expected_binding_revision: BindingRevision,
    selected_path: SelectedPathProof,
    state: BindingState,
}

impl PublishUnboundBinding {
    pub fn new(
        thread_id: SyndicThreadId,
        expected_binding_revision: BindingRevision,
        selected_path: SelectedPathProof,
        reason: impl AsRef<str>,
    ) -> Result<Self, SyndicRecordError> {
        Ok(Self {
            thread_id,
            expected_binding_revision,
            selected_path,
            state: BindingState::unbound(reason)?,
        })
    }

    #[must_use]
    pub const fn thread_id(&self) -> SyndicThreadId {
        self.thread_id
    }

    #[must_use]
    pub const fn expected_binding_revision(&self) -> BindingRevision {
        self.expected_binding_revision
    }

    #[must_use]
    pub const fn selected_path(&self) -> SelectedPathProof {
        self.selected_path
    }

    #[must_use]
    pub const fn state(&self) -> &BindingState {
        &self.state
    }
}

/// Exact immutable inputs for activating the pending selected turn on a valid binding.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ActivateBinding {
    thread_id: SyndicThreadId,
    expected_binding_revision: BindingRevision,
    expected_gate_revision: InputGateRevision,
    selected_path: SelectedPathProof,
    snapshot_id: SyndicExecutionSnapshotId,
    turn_id: SyndicTurnId,
    loaded_generation: CasLoadedSessionGeneration,
    started_at: SyndicTimestamp,
}

/// Exact immutable inputs for cancelling an activation proven not to have dispatched.
///
/// This transition preserves the same usable CAS projection and pending turn. It is valid only
/// before a CAS turn identity or any source event exists and while no accepted input is queued
/// against the provisional active target.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CancelBindingActivation {
    thread_id: SyndicThreadId,
    expected_binding_revision: BindingRevision,
    expected_gate_revision: InputGateRevision,
    selected_path: SelectedPathProof,
    snapshot_id: SyndicExecutionSnapshotId,
    turn_id: SyndicTurnId,
}

impl CancelBindingActivation {
    #[must_use]
    pub const fn new(
        thread_id: SyndicThreadId,
        expected_binding_revision: BindingRevision,
        expected_gate_revision: InputGateRevision,
        selected_path: SelectedPathProof,
        snapshot_id: SyndicExecutionSnapshotId,
        turn_id: SyndicTurnId,
    ) -> Self {
        Self {
            thread_id,
            expected_binding_revision,
            expected_gate_revision,
            selected_path,
            snapshot_id,
            turn_id,
        }
    }

    #[must_use]
    pub const fn thread_id(&self) -> SyndicThreadId {
        self.thread_id
    }

    #[must_use]
    pub const fn expected_binding_revision(&self) -> BindingRevision {
        self.expected_binding_revision
    }

    #[must_use]
    pub const fn expected_gate_revision(&self) -> InputGateRevision {
        self.expected_gate_revision
    }

    #[must_use]
    pub const fn selected_path(&self) -> SelectedPathProof {
        self.selected_path
    }

    #[must_use]
    pub const fn snapshot_id(&self) -> SyndicExecutionSnapshotId {
        self.snapshot_id
    }

    #[must_use]
    pub const fn turn_id(&self) -> SyndicTurnId {
        self.turn_id
    }
}

impl ActivateBinding {
    #[allow(clippy::too_many_arguments)]
    #[must_use]
    pub const fn new(
        thread_id: SyndicThreadId,
        expected_binding_revision: BindingRevision,
        expected_gate_revision: InputGateRevision,
        selected_path: SelectedPathProof,
        snapshot_id: SyndicExecutionSnapshotId,
        turn_id: SyndicTurnId,
        loaded_generation: CasLoadedSessionGeneration,
        started_at: SyndicTimestamp,
    ) -> Self {
        Self {
            thread_id,
            expected_binding_revision,
            expected_gate_revision,
            selected_path,
            snapshot_id,
            turn_id,
            loaded_generation,
            started_at,
        }
    }

    #[must_use]
    pub const fn thread_id(&self) -> SyndicThreadId {
        self.thread_id
    }

    #[must_use]
    pub const fn expected_binding_revision(&self) -> BindingRevision {
        self.expected_binding_revision
    }

    #[must_use]
    pub const fn expected_gate_revision(&self) -> InputGateRevision {
        self.expected_gate_revision
    }

    #[must_use]
    pub const fn selected_path(&self) -> SelectedPathProof {
        self.selected_path
    }

    #[must_use]
    pub const fn snapshot_id(&self) -> SyndicExecutionSnapshotId {
        self.snapshot_id
    }

    #[must_use]
    pub const fn turn_id(&self) -> SyndicTurnId {
        self.turn_id
    }

    #[must_use]
    pub const fn loaded_generation(&self) -> CasLoadedSessionGeneration {
        self.loaded_generation
    }

    #[must_use]
    pub const fn started_at(&self) -> SyndicTimestamp {
        self.started_at
    }
}

/// Exact immutable inputs for the one-way CAS-turn publication of an active snapshot.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PublishActiveCasTurn {
    thread_id: SyndicThreadId,
    binding_revision: BindingRevision,
    expected_gate_revision: InputGateRevision,
    snapshot_id: SyndicExecutionSnapshotId,
    cas_thread_id: CasThreadId,
    cas_turn_id: CasTurnId,
    published_at: SyndicTimestamp,
}

impl PublishActiveCasTurn {
    #[allow(clippy::too_many_arguments)]
    #[must_use]
    pub const fn new(
        thread_id: SyndicThreadId,
        binding_revision: BindingRevision,
        expected_gate_revision: InputGateRevision,
        snapshot_id: SyndicExecutionSnapshotId,
        cas_thread_id: CasThreadId,
        cas_turn_id: CasTurnId,
        published_at: SyndicTimestamp,
    ) -> Self {
        Self {
            thread_id,
            binding_revision,
            expected_gate_revision,
            snapshot_id,
            cas_thread_id,
            cas_turn_id,
            published_at,
        }
    }

    #[must_use]
    pub const fn thread_id(&self) -> SyndicThreadId {
        self.thread_id
    }

    #[must_use]
    pub const fn binding_revision(&self) -> BindingRevision {
        self.binding_revision
    }

    #[must_use]
    pub const fn expected_gate_revision(&self) -> InputGateRevision {
        self.expected_gate_revision
    }

    #[must_use]
    pub const fn snapshot_id(&self) -> SyndicExecutionSnapshotId {
        self.snapshot_id
    }

    #[must_use]
    pub const fn cas_thread_id(&self) -> &CasThreadId {
        &self.cas_thread_id
    }

    #[must_use]
    pub const fn cas_turn_id(&self) -> &CasTurnId {
        &self.cas_turn_id
    }

    #[must_use]
    pub const fn published_at(&self) -> SyndicTimestamp {
        self.published_at
    }
}

/// Exact result of reconciling an ambiguously surfaced revisioned binding publication.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BindingPublicationStatus {
    Prior,
    Exact,
    Collision,
}

/// Exact result of reconciling one one-way active-CAS-turn publication.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ActiveCasTurnPublicationStatus {
    Absent,
    Exact,
    Collision,
}

impl SyndicStorage {
    #[must_use]
    pub fn current_publish_valid_binding(
        &self,
        request: PublishValidBinding,
    ) -> CurrentDomainCommand {
        self.handle
            .current_command(PublishBindingMutation::valid(request))
    }

    #[must_use]
    pub fn publish_valid_binding(
        &self,
        expected_domain_revision: DomainRevision,
        request: PublishValidBinding,
    ) -> MutationContribution {
        self.handle.contribution(
            expected_domain_revision,
            PublishBindingMutation::valid(request),
        )
    }

    #[must_use]
    pub fn publish_stale_binding(
        &self,
        expected_domain_revision: DomainRevision,
        request: PublishStaleBinding,
    ) -> MutationContribution {
        self.handle.contribution(
            expected_domain_revision,
            PublishBindingMutation::stale(request),
        )
    }

    #[must_use]
    pub fn current_publish_stale_binding(
        &self,
        request: PublishStaleBinding,
    ) -> CurrentDomainCommand {
        self.handle
            .current_command(PublishBindingMutation::stale(request))
    }

    /// Retires one active projection, rerouting undispatched work and terminalizing possibly dispatched work.
    #[must_use]
    pub fn abandon_active_binding(
        &self,
        expected_domain_revision: DomainRevision,
        request: AbandonActiveBinding,
    ) -> MutationContribution {
        self.handle.contribution(
            expected_domain_revision,
            AbandonActiveBindingMutation { request },
        )
    }

    #[must_use]
    pub fn current_abandon_active_binding(
        &self,
        request: AbandonActiveBinding,
    ) -> CurrentDomainCommand {
        self.handle
            .current_command(AbandonActiveBindingMutation { request })
    }

    #[must_use]
    pub fn publish_unbound_binding(
        &self,
        expected_domain_revision: DomainRevision,
        request: PublishUnboundBinding,
    ) -> MutationContribution {
        self.handle.contribution(
            expected_domain_revision,
            PublishBindingMutation::unbound(request),
        )
    }

    #[must_use]
    pub fn activate_binding(
        &self,
        expected_domain_revision: DomainRevision,
        request: ActivateBinding,
    ) -> MutationContribution {
        self.handle.contribution(
            expected_domain_revision,
            ActivateBindingMutation { request },
        )
    }

    #[must_use]
    pub fn current_activate_binding(&self, request: ActivateBinding) -> CurrentDomainCommand {
        self.handle
            .current_command(ActivateBindingMutation { request })
    }

    /// Restores an activated binding after exact proof that no remote turn was dispatched.
    #[must_use]
    pub fn cancel_binding_activation(
        &self,
        expected_domain_revision: DomainRevision,
        request: CancelBindingActivation,
    ) -> MutationContribution {
        self.handle.contribution(
            expected_domain_revision,
            CancelBindingActivationMutation { request },
        )
    }

    #[must_use]
    pub fn current_cancel_binding_activation(
        &self,
        request: CancelBindingActivation,
    ) -> CurrentDomainCommand {
        self.handle
            .current_command(CancelBindingActivationMutation { request })
    }

    #[must_use]
    pub fn publish_active_cas_turn(
        &self,
        expected_domain_revision: DomainRevision,
        request: PublishActiveCasTurn,
    ) -> MutationContribution {
        self.handle.contribution(
            expected_domain_revision,
            PublishActiveCasTurnMutation { request },
        )
    }

    #[must_use]
    pub fn current_publish_active_cas_turn(
        &self,
        request: PublishActiveCasTurn,
    ) -> CurrentDomainCommand {
        self.handle
            .current_command(PublishActiveCasTurnMutation { request })
    }
}
