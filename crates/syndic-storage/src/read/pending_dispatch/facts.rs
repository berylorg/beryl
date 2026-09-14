use beryl_home_store::{CursorReadLimits, HomeGeneration, HomeStore};
use beryl_model::{BerylHomeId, DomainRevision, SyndicThreadId};

use crate::{
    BindingState, CanonicalItemKind, CanonicalItemPresentation, CanonicalItemRecord,
    ContentManifestRecord, HistorySummaryRecord, InputGateRecord, InputGateState,
    SelectedPathProof, SyndicCurrentBinding, SyndicPage, SyndicPointReadLimit, SyndicReadError,
    SyndicStorage, ThreadRecord, TurnDispatchProvenance, TurnItemIndexRecord, TurnItemOrdinal,
    TurnKind, TurnLifecycle, TurnRecord, TurnStateRecord,
};

use super::PendingDispatchEvidence;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct PendingDispatchFacts {
    thread: ThreadRecord,
    binding: SyndicCurrentBinding,
    gate: InputGateRecord,
    turn: TurnRecord,
    state: TurnStateRecord,
    summary: HistorySummaryRecord,
    items: SyndicPage<TurnItemIndexRecord>,
    item: CanonicalItemRecord,
    manifest: ContentManifestRecord,
}

pub(super) fn read(
    storage: &SyndicStorage,
    store: &HomeStore,
    thread_id: SyndicThreadId,
    limit: SyndicPointReadLimit,
) -> Result<Option<PendingDispatchFacts>, SyndicReadError> {
    let Some(gate) = storage.input_gate(store, thread_id, limit)? else {
        return Ok(None);
    };
    let InputGateState::PendingTurn(turn_id) = gate.state() else {
        return Ok(None);
    };
    if gate.selected_route().is_some() {
        return Ok(None);
    }
    let turn_id = *turn_id;
    let thread = required(
        storage.thread(store, thread_id, limit)?,
        "pending dispatch thread is missing",
    )?;
    let binding = required(
        storage.current_binding(store, thread_id, limit)?,
        "pending dispatch binding is missing",
    )?;
    let turn = required(
        storage.turn(store, turn_id, limit)?,
        "pending dispatch turn is missing",
    )?;
    let state = required(
        storage.turn_state(store, turn_id, limit)?,
        "pending dispatch turn state is missing",
    )?;
    let summary = required(
        storage.history_summary(store, thread_id, limit)?,
        "pending dispatch history summary is missing",
    )?;
    if matches!(binding.binding().state(), BindingState::Active(_))
        || state.lifecycle() != TurnLifecycle::Pending
        || !matches!(
            turn.kind(),
            TurnKind::OrdinaryUser | TurnKind::BerylLifecycleContinuation
        )
    {
        return Err(SyndicReadError::Invariant(
            "pending dispatch binding, lifecycle or turn kind disagrees",
        ));
    }
    match state.dispatch_provenance() {
        TurnDispatchProvenance::Unattempted => {}
        TurnDispatchProvenance::Cancelled(anchor) => {
            if !storage
                .authenticated_cancelled_dispatch(store, thread_id, turn_id, anchor, limit)?
            {
                return Err(SyndicReadError::Invariant(
                    "pending dispatch cancellation provenance disagrees",
                ));
            }
        }
        TurnDispatchProvenance::Activated(_) => return Ok(None),
        TurnDispatchProvenance::ProviderOperation => {
            return Err(SyndicReadError::Invariant(
                "pending dispatch has provider-operation provenance",
            ));
        }
    }
    let items = storage.turn_items(
        store,
        turn_id,
        None,
        CursorReadLimits::new(2, 4096).expect("fixed pending input bound"),
    )?;
    if items.has_more() || items.records().len() != 1 {
        return Err(SyndicReadError::Invariant(
            "pending dispatch turn lacks one exact canonical input",
        ));
    }
    let item = required(
        storage.canonical_item(store, items.records()[0].item_id(), limit)?,
        "pending dispatch canonical input is missing",
    )?;
    let input = required(
        item.presentation_content(),
        "pending dispatch canonical input has no sealed content",
    )?;
    let manifest = required(
        storage.content_manifest(store, input.id(), limit)?,
        "pending dispatch input content is missing",
    )?;
    Ok(Some(PendingDispatchFacts {
        thread,
        binding,
        gate,
        turn,
        state,
        summary,
        items,
        item,
        manifest,
    }))
}

impl PendingDispatchFacts {
    pub(super) fn prove(
        self,
        home_id: BerylHomeId,
        home_generation: HomeGeneration,
        source_revision: DomainRevision,
    ) -> Result<PendingDispatchEvidence, SyndicReadError> {
        let Self {
            thread,
            binding,
            gate,
            turn,
            state,
            summary,
            items,
            item,
            manifest,
        } = self;
        let path = SelectedPathProof::new(
            thread.committed_tail(),
            thread.revision(),
            thread.selected_path_digest(),
        );
        let index = &items.records()[0];
        if path.tail() != Some(turn.id())
            || path.digest() != turn.chain_digest()
            || !path.is_compatible_descendant_of(binding.binding().selected_path())
            || binding.binding().thread_id() != thread.id()
            || gate.thread_id() != thread.id()
            || gate.state() != &InputGateState::PendingTurn(turn.id())
            || gate.live_steering_count() != 0
            || turn.origin_thread_id() != thread.id()
            || state.turn_id() != turn.id()
            || state.source_event_count() != 0
            || state.item_count() != 1
            || state.finalized_item_count() != 0
            || state.open_item_count() != 1
            || state.history_blocking_item_count() != 0
            || state.provider_observation_issue().is_some()
            || state.updated_at() < turn.submitted_at()
            || summary.thread_id() != thread.id()
            || summary.committed_tail() != Some(turn.id())
            || summary.selected_path_digest() != path.digest()
            || summary.complete()
            || index.turn_id() != turn.id()
            || index.ordinal() != TurnItemOrdinal::FIRST
            || item.id() != index.item_id()
            || item.turn_id() != turn.id()
            || item.ordinal() != TurnItemOrdinal::FIRST
            || item.revision() != index.item_revision()
            || item.kind() != CanonicalItemKind::UserInput
            || !matches!(
                item.presentation(),
                CanonicalItemPresentation::UserInput { .. }
            )
            || item.source_event().is_some()
        {
            return Err(SyndicReadError::Invariant(
                "pending dispatch canonical authority disagrees",
            ));
        }
        let input = required(
            item.presentation_content(),
            "pending dispatch input content is absent",
        )?;
        if manifest.sealed_reference() != Some(input) || manifest.owner().is_some() {
            return Err(SyndicReadError::Invariant(
                "pending dispatch sealed input content disagrees",
            ));
        }
        Ok(PendingDispatchEvidence {
            home_id,
            home_generation,
            source_revision,
            thread_id: thread.id(),
            turn_id: turn.id(),
            turn_kind: turn.kind(),
            selected_path: path,
            context_owner_id: thread.context_owner_id(),
            binding_revision: binding.binding().revision(),
            gate_revision: gate.revision(),
            state_revision: state.revision(),
            dispatch_provenance: state.dispatch_provenance(),
            item_id: item.id(),
            item_revision: item.revision(),
            input,
            asset_reference_set: item.presentation().asset_reference_set(),
            minimum_timestamp: state.updated_at().max(summary.last_activity_at()),
        })
    }
}

fn required<T>(value: Option<T>, message: &'static str) -> Result<T, SyndicReadError> {
    value.ok_or(SyndicReadError::Invariant(message))
}
