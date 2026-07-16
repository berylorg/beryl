use beryl_home_store::{CursorReadLimits, HomeStore};
use beryl_model::{
    BindingRevision, CasConversationToolProfile, CasNativeTurnCount, InputGateRevision,
    SyndicItemId, SyndicThreadId, SyndicTurnId,
};
use syndic_storage::{
    BindingState, CanonicalItemKind, CasLineageProof, CasRepresentedPrefixProof, ContentReference,
    InputGateState, SelectedPathProof, SyndicPointReadLimit, SyndicStorage, SyndicTimestamp,
    TurnItemOrdinal, TurnKind, TurnLifecycle, TurnStateRevision,
};

use super::OrdinaryTurnExecutionError;
use crate::cas_projection::LoadedCasProjection;

const TURN_ITEMS_READ_BYTES: usize = 4 * 1024;
const INPUT_PAGE_BYTES: usize = 64 * 1024;

pub(super) struct PendingOrdinaryExecution {
    pub(super) thread_id: SyndicThreadId,
    pub(super) turn_id: SyndicTurnId,
    pub(super) item_id: SyndicItemId,
    pub(super) selected_path: SelectedPathProof,
    pub(super) binding_revision: BindingRevision,
    pub(super) gate_revision: InputGateRevision,
    pub(super) state_revision: TurnStateRevision,
    pub(super) input: ContentReference,
    pub(super) represented_prefix: CasRepresentedPrefixProof,
    pub(super) native_turn_count: CasNativeTurnCount,
    pub(super) tool_profile: CasConversationToolProfile,
    pub(super) lineage: CasLineageProof,
    pub(super) minimum_observed_at: SyndicTimestamp,
}

impl PendingOrdinaryExecution {
    pub(super) fn read(
        store: &HomeStore,
        storage: SyndicStorage,
        projection: &LoadedCasProjection,
        limit: SyndicPointReadLimit,
    ) -> Result<Self, OrdinaryTurnExecutionError> {
        let thread_id = projection.syndic_thread_id();
        let thread = storage
            .thread(store, thread_id, limit)?
            .ok_or(OrdinaryTurnExecutionError::PendingTurnUnavailable { thread_id })?;
        let selected_path = SelectedPathProof::new(
            thread.record().committed_tail(),
            thread.record().revision(),
            thread.record().selected_path_digest(),
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
        let confirmed_thread = storage.thread(store, thread_id, limit)?;
        let confirmed_binding = storage.current_binding(store, thread_id, limit)?;
        let confirmed_gate = storage.input_gate(store, thread_id, limit)?;
        let confirmed_state = storage.turn_state(store, turn_id, limit)?;
        let confirmed_summary = storage.history_summary(store, thread_id, limit)?;
        if confirmed_thread.as_ref() != Some(&thread)
            || confirmed_binding.as_ref() != Some(&binding)
            || confirmed_gate.as_ref() != Some(&gate)
            || confirmed_state.as_ref() != Some(&state)
            || confirmed_summary.as_ref() != Some(&summary)
        {
            return Err(OrdinaryTurnExecutionError::ConcurrentChange { thread_id });
        }

        let BindingState::Valid(usable) = binding.binding().state() else {
            return Err(OrdinaryTurnExecutionError::ProjectionMismatch { thread_id });
        };
        if projection.binding_revision() != binding.binding().revision()
            || projection.execution_binding() != usable.execution()
            || projection.cas_thread_id() != usable.cas_thread_id()
            || projection.lineage_proof() != usable.lineage()
            || binding.binding().selected_path() != selected_path
            || thread.record().context_owner_id().is_some()
        {
            return Err(OrdinaryTurnExecutionError::ProjectionMismatch { thread_id });
        }
        if gate.record().state() != &InputGateState::PendingTurn(turn_id)
            || turn.record().origin_thread_id() != thread_id
            || turn.record().kind() != TurnKind::OrdinaryUser
            || state.record().lifecycle() != TurnLifecycle::Pending
            || state.record().source_event_count() != 0
            || state.record().item_count() != 1
            || state.record().finalized_item_count() != 0
            || item_index.turn_id() != turn_id
            || item_index.ordinal() != TurnItemOrdinal::FIRST
            || item.record().turn_id() != turn_id
            || item.record().ordinal() != TurnItemOrdinal::FIRST
            || item.record().revision() != item_index.item_revision()
            || item.record().kind() != CanonicalItemKind::UserInput
            || item.record().source_event().is_some()
        {
            return Err(OrdinaryTurnExecutionError::PendingTurnUnavailable { thread_id });
        }
        let input =
            item.record()
                .payload()
                .content()
                .ok_or(OrdinaryTurnExecutionError::Invariant(
                    "pending ordinary input item has no sealed content",
                ))?;
        if item.record().payload().marker_count() != 0 || input.summary().image_marker_count() != 0
        {
            return Err(OrdinaryTurnExecutionError::ImageInputNotImplemented);
        }
        Ok(Self {
            thread_id,
            turn_id,
            item_id: item.record().id(),
            selected_path,
            binding_revision: binding.binding().revision(),
            gate_revision: gate.record().revision(),
            state_revision: state.record().revision(),
            input,
            represented_prefix: usable.represented_prefix(),
            native_turn_count: usable.native_turn_count(),
            tool_profile: usable.tool_profile(),
            lineage: usable.lineage(),
            minimum_observed_at: state
                .record()
                .updated_at()
                .max(summary.record().last_activity_at()),
        })
    }

    pub(super) fn assemble_input(
        &self,
        store: &HomeStore,
        storage: SyndicStorage,
    ) -> Result<String, OrdinaryTurnExecutionError> {
        let capacity =
            usize::try_from(self.input.summary().logical_utf8_bytes()).map_err(|_| {
                OrdinaryTurnExecutionError::Invariant(
                    "ordinary input length exceeds the process address space",
                )
            })?;
        let mut input = String::with_capacity(capacity);
        let mut offset = 0_u64;
        loop {
            let page = storage
                .sealed_content_text_range(store, self.input, offset, INPUT_PAGE_BYTES)?
                .ok_or(OrdinaryTurnExecutionError::Invariant(
                    "ordinary input content is missing",
                ))?;
            input.push_str(page.text());
            let Some(next) = page.next_offset() else {
                break;
            };
            if next <= offset {
                return Err(OrdinaryTurnExecutionError::Invariant(
                    "ordinary input content reader did not advance",
                ));
            }
            offset = next;
        }
        if input.is_empty() {
            return Err(OrdinaryTurnExecutionError::EmptyInput);
        }
        Ok(input)
    }
}
