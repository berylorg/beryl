use beryl_home_store::HomeStore;
use beryl_model::{
    BindingRevision, InputGateRevision, SealedAssetReferenceSetProof, SyndicItemId, SyndicThreadId,
    SyndicTurnId,
};
use beryl_state::{AssetOwner, AssetOwnerHeadRecord, AssetState};
use syndic_storage::{
    BindingState, ContentReference, SelectedPathProof, SyndicPointReadLimit, SyndicStorage,
    SyndicTimestamp, TurnStateRevision,
};

use super::OrdinaryTurnExecutionError;
use crate::cas_projection::LoadedCasProjection;

#[cfg(all(test, feature = "test-faults"))]
#[path = "../../../tests/unit/pending_ordinary_preflight.rs"]
mod tests;

/// Exact immutable projection facts required to stabilize one pending ordinary turn.
///
/// Keeping this witness separate from executable projection ownership lets startup recovery
/// authenticate durable authority without first manufacturing a live wrapper.
pub(in crate::cas_projection) trait PendingOrdinaryExecutionWitness {
    fn expected_syndic_thread_id(&self) -> SyndicThreadId;
    fn expected_binding_revision(&self) -> BindingRevision;
    fn expected_execution_binding(&self) -> &beryl_model::ExecutionBinding;
    fn expected_cas_thread_id(&self) -> &beryl_model::CasThreadId;
    fn expected_lineage_proof(&self) -> syndic_storage::CasLineageProof;
}

impl PendingOrdinaryExecutionWitness for LoadedCasProjection {
    fn expected_syndic_thread_id(&self) -> SyndicThreadId {
        self.syndic_thread_id()
    }

    fn expected_binding_revision(&self) -> BindingRevision {
        self.binding_revision()
    }

    fn expected_execution_binding(&self) -> &beryl_model::ExecutionBinding {
        self.execution_binding()
    }

    fn expected_cas_thread_id(&self) -> &beryl_model::CasThreadId {
        self.cas_thread_id()
    }

    fn expected_lineage_proof(&self) -> syndic_storage::CasLineageProof {
        self.lineage_proof()
    }
}

pub(in crate::cas_projection) struct PendingOrdinaryExecution {
    pub(super) terminal_completion:
        Option<crate::cas_projection::service::TerminalCompletionPublisher>,
    pub(super) thread_id: SyndicThreadId,
    pub(super) turn_id: SyndicTurnId,
    pub(super) item_id: SyndicItemId,
    pub(super) selected_path: SelectedPathProof,
    pub(super) binding_revision: BindingRevision,
    pub(super) gate_revision: InputGateRevision,
    pub(super) state_revision: TurnStateRevision,
    pub(super) input: ContentReference,
    pub(super) asset_reference_set: Option<SealedAssetReferenceSetProof>,
    pub(super) asset_owner_head: Option<AssetOwnerHeadRecord>,
    pub(super) minimum_observed_at: SyndicTimestamp,
}

impl PendingOrdinaryExecution {
    pub(in crate::cas_projection) fn read(
        store: &HomeStore,
        storage: &SyndicStorage,
        assets: &AssetState,
        witness: &impl PendingOrdinaryExecutionWitness,
        limit: SyndicPointReadLimit,
    ) -> Result<Self, OrdinaryTurnExecutionError> {
        Self::read_inner(store, storage, assets, witness, limit, || {})
    }

    #[cfg(any(test, feature = "test-faults"))]
    pub(in crate::cas_projection) fn read_with_confirmation_hook(
        store: &HomeStore,
        storage: &SyndicStorage,
        assets: &AssetState,
        witness: &impl PendingOrdinaryExecutionWitness,
        limit: SyndicPointReadLimit,
        before_confirmation: impl FnOnce(),
    ) -> Result<Self, OrdinaryTurnExecutionError> {
        Self::read_inner(store, storage, assets, witness, limit, before_confirmation)
    }

    fn read_inner(
        store: &HomeStore,
        storage: &SyndicStorage,
        assets: &AssetState,
        witness: &impl PendingOrdinaryExecutionWitness,
        limit: SyndicPointReadLimit,
        before_confirmation: impl FnOnce(),
    ) -> Result<Self, OrdinaryTurnExecutionError> {
        let thread_id = witness.expected_syndic_thread_id();
        let read_pending = || {
            storage
                .pending_dispatch_evidence(store, thread_id, limit)
                .map_err(|error| match error {
                    syndic_storage::SyndicReadError::ConcurrentChange { .. } => {
                        OrdinaryTurnExecutionError::ConcurrentChange { thread_id }
                    }
                    error => OrdinaryTurnExecutionError::Read(error),
                })
        };
        let pending = read_pending()?
            .ok_or(OrdinaryTurnExecutionError::PendingTurnUnavailable { thread_id })?;
        let binding = storage
            .current_binding(store, thread_id, limit)?
            .ok_or(OrdinaryTurnExecutionError::ProjectionMismatch { thread_id })?;
        let input = pending.input();
        let asset_reference_set = pending.asset_reference_set();
        let asset_owner = AssetOwner::SubmittedTurnItem(pending.item_id());
        let asset_owner_head = assets.owner_head(store, asset_owner)?;
        let asset_proof = asset_reference_set
            .map(|proof| -> Result<_, beryl_state::AssetReadError> {
                assets.sealed_reference_set_manifest(store, proof)?;
                Ok(proof)
            })
            .transpose()?;
        before_confirmation();
        let confirmed_pending = read_pending();
        if storage.revision(store)? != pending.source_revision() {
            return Err(OrdinaryTurnExecutionError::ConcurrentChange { thread_id });
        }
        let confirmed_pending = confirmed_pending?;
        let confirmed_binding = storage.current_binding(store, thread_id, limit)?;
        let confirmed_asset_owner_head = assets.owner_head(store, asset_owner)?;
        let confirmed_asset_proof = asset_reference_set
            .map(|proof| -> Result<_, beryl_state::AssetReadError> {
                assets.sealed_reference_set_manifest(store, proof)?;
                Ok(proof)
            })
            .transpose()?;
        if confirmed_pending != Some(pending)
            || confirmed_binding.as_ref() != Some(&binding)
            || confirmed_asset_owner_head != asset_owner_head
            || confirmed_asset_proof != asset_proof
        {
            return Err(OrdinaryTurnExecutionError::ConcurrentChange { thread_id });
        }
        let BindingState::Valid(usable) = binding.binding().state() else {
            return Err(OrdinaryTurnExecutionError::ProjectionMismatch { thread_id });
        };
        if witness.expected_binding_revision() != binding.binding().revision()
            || pending.binding_revision() != binding.binding().revision()
            || witness.expected_execution_binding() != usable.execution()
            || witness.expected_cas_thread_id() != usable.cas_thread_id()
            || witness.expected_lineage_proof() != usable.lineage()
            || !pending
                .selected_path()
                .is_compatible_descendant_of(binding.binding().selected_path())
            || pending.context_owner_id().is_some()
        {
            return Err(OrdinaryTurnExecutionError::ProjectionMismatch { thread_id });
        }
        let marker_summary = input
            .sealed_marker_summary()
            .map_err(|_| OrdinaryTurnExecutionError::InputAssetReferenceSetMismatch)?;
        match (
            input.summary().image_marker_count(),
            asset_reference_set,
            asset_owner_head.as_ref(),
            asset_proof.as_ref(),
        ) {
            (0, None, None, None) => {}
            (0, _, _, _) | (_, None, _, _) | (_, _, None, _) | (_, _, _, None) => {
                return Err(OrdinaryTurnExecutionError::InputAssetReferenceSetMismatch);
            }
            (_, Some(proof), Some(head), Some(authenticated_proof))
                if proof.sequential() == marker_summary.sequential()
                    && head.owner() == asset_owner
                    && head.set() == proof
                    && *authenticated_proof == proof => {}
            _ => return Err(OrdinaryTurnExecutionError::InputAssetReferenceSetMismatch),
        }
        Ok(Self {
            terminal_completion: None,
            thread_id,
            turn_id: pending.turn_id(),
            item_id: pending.item_id(),
            selected_path: pending.selected_path(),
            binding_revision: pending.binding_revision(),
            gate_revision: pending.gate_revision(),
            state_revision: pending.state_revision(),
            input,
            asset_reference_set,
            asset_owner_head,
            minimum_observed_at: pending.minimum_timestamp(),
        })
    }
}
