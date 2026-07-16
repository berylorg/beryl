mod abandon;
mod active;
mod cancel;
mod transition;
mod validation;

pub(in crate::mutation) use validation::{advance_reservation, membership};

use beryl_home_store::{CurrentDomainCommand, MutationContribution};
use beryl_model::{
    BindingRevision, CasConversationToolProfile, CasLoadedSessionGeneration, CasNativeTurnCount,
    CasThreadId, CasTurnId, DomainRevision, ExecutionBinding, InputGateRevision,
    SyndicExecutionSnapshotId, SyndicThreadId, SyndicTurnId,
};

use crate::{
    BindingState, CasLineageProof, CasRepresentedPrefixProof, SelectedPathProof, StaleCasBinding,
    SyndicRecordError, SyndicStorage, SyndicTimestamp,
};

use abandon::AbandonActiveBindingMutation;
use active::{ActivateBindingMutation, PublishActiveCasTurnMutation};
use cancel::CancelBindingActivationMutation;
use transition::PublishBindingMutation;

/// Exact immutable inputs for publishing one usable CAS projection.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PublishValidBinding {
    thread_id: SyndicThreadId,
    expected_binding_revision: BindingRevision,
    selected_path: SelectedPathProof,
    execution: ExecutionBinding,
    cas_thread_id: CasThreadId,
    represented_prefix: CasRepresentedPrefixProof,
    native_turn_count: CasNativeTurnCount,
    tool_profile: CasConversationToolProfile,
    lineage: CasLineageProof,
}

impl PublishValidBinding {
    #[allow(clippy::too_many_arguments)]
    #[must_use]
    pub const fn new(
        thread_id: SyndicThreadId,
        expected_binding_revision: BindingRevision,
        selected_path: SelectedPathProof,
        execution: ExecutionBinding,
        cas_thread_id: CasThreadId,
        represented_prefix: CasRepresentedPrefixProof,
        native_turn_count: CasNativeTurnCount,
        tool_profile: CasConversationToolProfile,
        lineage: CasLineageProof,
    ) -> Self {
        Self {
            thread_id,
            expected_binding_revision,
            selected_path,
            execution,
            cas_thread_id,
            represented_prefix,
            native_turn_count,
            tool_profile,
            lineage,
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
    pub const fn selected_path(&self) -> SelectedPathProof {
        self.selected_path
    }

    #[must_use]
    pub const fn execution(&self) -> &ExecutionBinding {
        &self.execution
    }

    #[must_use]
    pub const fn cas_thread_id(&self) -> &CasThreadId {
        &self.cas_thread_id
    }

    #[must_use]
    pub const fn represented_prefix(&self) -> CasRepresentedPrefixProof {
        self.represented_prefix
    }

    /// Returns the coordinator-proven exact native CAS turn count for the prefix.
    #[must_use]
    pub const fn native_turn_count(&self) -> CasNativeTurnCount {
        self.native_turn_count
    }

    /// Returns the exact canonical conversation-tool profile established on the CAS thread.
    #[must_use]
    pub const fn tool_profile(&self) -> CasConversationToolProfile {
        self.tool_profile
    }

    #[must_use]
    pub const fn lineage(&self) -> CasLineageProof {
        self.lineage
    }
}

/// Exact immutable inputs for retaining one stale or abandoned CAS projection.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PublishStaleBinding {
    thread_id: SyndicThreadId,
    expected_binding_revision: BindingRevision,
    selected_path: SelectedPathProof,
    stale: StaleCasBinding,
}

impl PublishStaleBinding {
    #[must_use]
    pub const fn new(
        thread_id: SyndicThreadId,
        expected_binding_revision: BindingRevision,
        selected_path: SelectedPathProof,
        stale: StaleCasBinding,
    ) -> Self {
        Self {
            thread_id,
            expected_binding_revision,
            selected_path,
            stale,
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
    pub const fn selected_path(&self) -> SelectedPathProof {
        self.selected_path
    }

    #[must_use]
    pub const fn stale(&self) -> &StaleCasBinding {
        &self.stale
    }
}

/// Exact immutable inputs for abandoning one active CAS projection after its authority is lost.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AbandonActiveBinding {
    thread_id: SyndicThreadId,
    expected_binding_revision: BindingRevision,
    expected_gate_revision: InputGateRevision,
    selected_path: SelectedPathProof,
    stale: StaleCasBinding,
}

impl AbandonActiveBinding {
    #[must_use]
    pub const fn new(
        thread_id: SyndicThreadId,
        expected_binding_revision: BindingRevision,
        expected_gate_revision: InputGateRevision,
        selected_path: SelectedPathProof,
        stale: StaleCasBinding,
    ) -> Self {
        Self {
            thread_id,
            expected_binding_revision,
            expected_gate_revision,
            selected_path,
            stale,
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
    pub const fn stale(&self) -> &StaleCasBinding {
        &self.stale
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
