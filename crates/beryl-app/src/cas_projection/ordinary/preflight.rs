use beryl_home_store::{CursorReadLimits, HomeStore};
use beryl_model::{
    BindingRevision, InputGateRevision, SealedAssetReferenceSetProof, SyndicItemId, SyndicThreadId,
    SyndicTurnId,
};
use beryl_state::{AssetOwner, AssetOwnerHeadRecord, AssetState};
use syndic_storage::{
    BindingState, CanonicalItemKind, CanonicalItemPresentation, ContentReference, InputGateState,
    SelectedPathProof, SyndicPointReadLimit, SyndicStorage, SyndicTimestamp, TurnItemOrdinal,
    TurnKind, TurnLifecycle, TurnStateRevision,
};

use super::OrdinaryTurnExecutionError;
use crate::cas_projection::LoadedCasProjection;

const TURN_ITEMS_READ_BYTES: usize = 4 * 1024;

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

    #[cfg(test)]
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
        let thread = storage
            .thread(store, thread_id, limit)?
            .ok_or(OrdinaryTurnExecutionError::PendingTurnUnavailable { thread_id })?;
        let selected_path = SelectedPathProof::new(
            thread.committed_tail(),
            thread.revision(),
            thread.selected_path_digest(),
        );
        let turn_id = selected_path
            .tail()
            .ok_or(OrdinaryTurnExecutionError::PendingTurnUnavailable { thread_id })?;
        let binding = storage
            .current_binding(store, thread_id, limit)?
            .ok_or(OrdinaryTurnExecutionError::ProjectionMismatch { thread_id })?;
        let gate = storage
            .input_gate(store, thread_id, limit)?
            .ok_or(OrdinaryTurnExecutionError::PendingTurnUnavailable { thread_id })?;
        let turn =
            storage
                .turn(store, turn_id, limit)?
                .ok_or(OrdinaryTurnExecutionError::Invariant(
                    "pending turn is missing",
                ))?;
        let state = storage.turn_state(store, turn_id, limit)?.ok_or(
            OrdinaryTurnExecutionError::Invariant("pending turn state is missing"),
        )?;
        let summary = storage.history_summary(store, thread_id, limit)?.ok_or(
            OrdinaryTurnExecutionError::Invariant("pending thread history summary is missing"),
        )?;
        let items = storage.turn_items(
            store,
            turn_id,
            None,
            CursorReadLimits::new(2, TURN_ITEMS_READ_BYTES)
                .expect("ordinary pending-item read bounds are nonzero"),
        )?;
        if items.has_more() || items.records().len() != 1 {
            return Err(OrdinaryTurnExecutionError::Invariant(
                "pending ordinary turn does not have exactly one canonical input item",
            ));
        }
        let item_index = &items.records()[0];
        let item = storage
            .canonical_item(store, item_index.item_id(), limit)?
            .ok_or(OrdinaryTurnExecutionError::Invariant(
                "pending ordinary input item is missing",
            ))?;
        let input = item
            .presentation_content()
            .ok_or(OrdinaryTurnExecutionError::Invariant(
                "pending ordinary input item has no sealed content",
            ))?;
        let manifest = storage
            .content_manifest(store, input.id(), limit)?
            .ok_or(OrdinaryTurnExecutionError::InputContentUnavailable)?;
        let asset_reference_set = item.presentation().asset_reference_set();
        let asset_owner = AssetOwner::SubmittedTurnItem(item.id());
        let asset_owner_head = assets.owner_head(store, asset_owner)?;
        let asset_proof = asset_reference_set
            .map(|proof| -> Result<_, beryl_state::AssetReadError> {
                assets.sealed_reference_set_manifest(store, proof)?;
                Ok(proof)
            })
            .transpose()?;
        before_confirmation();
        let confirmed_thread = storage.thread(store, thread_id, limit)?;
        let confirmed_binding = storage.current_binding(store, thread_id, limit)?;
        let confirmed_gate = storage.input_gate(store, thread_id, limit)?;
        let confirmed_turn = storage.turn(store, turn_id, limit)?;
        let confirmed_state = storage.turn_state(store, turn_id, limit)?;
        let confirmed_summary = storage.history_summary(store, thread_id, limit)?;
        let confirmed_items = storage.turn_items(
            store,
            turn_id,
            None,
            CursorReadLimits::new(2, TURN_ITEMS_READ_BYTES)
                .expect("ordinary pending-item read bounds are nonzero"),
        )?;
        let confirmed_item = storage.canonical_item(store, item_index.item_id(), limit)?;
        let confirmed_manifest = storage.content_manifest(store, input.id(), limit)?;
        let confirmed_asset_owner_head = assets.owner_head(store, asset_owner)?;
        let confirmed_asset_proof = asset_reference_set
            .map(|proof| -> Result<_, beryl_state::AssetReadError> {
                assets.sealed_reference_set_manifest(store, proof)?;
                Ok(proof)
            })
            .transpose()?;
        if confirmed_thread.as_ref() != Some(&thread)
            || confirmed_binding.as_ref() != Some(&binding)
            || confirmed_gate.as_ref() != Some(&gate)
            || confirmed_turn.as_ref() != Some(&turn)
            || confirmed_state.as_ref() != Some(&state)
            || confirmed_summary.as_ref() != Some(&summary)
            || confirmed_items != items
            || confirmed_item.as_ref() != Some(&item)
            || confirmed_manifest.as_ref() != Some(&manifest)
            || confirmed_asset_owner_head != asset_owner_head
            || confirmed_asset_proof != asset_proof
        {
            return Err(OrdinaryTurnExecutionError::ConcurrentChange { thread_id });
        }

        let BindingState::Valid(usable) = binding.binding().state() else {
            return Err(OrdinaryTurnExecutionError::ProjectionMismatch { thread_id });
        };
        if witness.expected_binding_revision() != binding.binding().revision()
            || witness.expected_execution_binding() != usable.execution()
            || witness.expected_cas_thread_id() != usable.cas_thread_id()
            || witness.expected_lineage_proof() != usable.lineage()
            || !selected_path.is_compatible_descendant_of(binding.binding().selected_path())
            || thread.context_owner_id().is_some()
        {
            return Err(OrdinaryTurnExecutionError::ProjectionMismatch { thread_id });
        }
        if gate.state() != &InputGateState::PendingTurn(turn_id)
            || turn.origin_thread_id() != thread_id
            || turn.kind() != TurnKind::OrdinaryUser
            || state.lifecycle() != TurnLifecycle::Pending
            || state.source_event_count() != 0
            || state.item_count() != 1
            || state.finalized_item_count() != 0
            || item_index.turn_id() != turn_id
            || item_index.ordinal() != TurnItemOrdinal::FIRST
            || item.turn_id() != turn_id
            || item.ordinal() != TurnItemOrdinal::FIRST
            || item.revision() != item_index.item_revision()
            || item.kind() != CanonicalItemKind::UserInput
            || !matches!(
                item.presentation(),
                CanonicalItemPresentation::UserInput { .. }
            )
            || item.source_event().is_some()
        {
            return Err(OrdinaryTurnExecutionError::PendingTurnUnavailable { thread_id });
        }
        if manifest.sealed_reference() != Some(input) {
            return Err(OrdinaryTurnExecutionError::InputContentUnavailable);
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
            thread_id,
            turn_id,
            item_id: item.id(),
            selected_path,
            binding_revision: binding.binding().revision(),
            gate_revision: gate.revision(),
            state_revision: state.revision(),
            input,
            asset_reference_set,
            asset_owner_head,
            minimum_observed_at: state.updated_at().max(summary.last_activity_at()),
        })
    }
}
