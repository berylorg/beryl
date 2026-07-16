mod descriptor;
mod text;

use beryl_backend::{ItemDelta, ItemDeltaPayload, ThreadItem};
use beryl_home_store::HomeStore;
use beryl_model::CasItemId;
use syndic_storage::{
    CasItemSource, ProviderItemDisposition, ProviderItemKind, ProviderItemLifecycle,
    SourceEventPayload, SourceEventText, SourceItemDescriptor, SyndicCaptureItem,
    SyndicPointReadLimit, SyndicStorage, TurnIncompleteReason,
};

use self::descriptor::{ItemDescriptor, ItemText, provider_kind};
use self::text::{COALESCED_DELTA_MAX_BYTES, bounded_utf8_parts};
use super::LiveCapture;
use crate::cas_projection::ordinary::OrdinaryTurnExecutionError;

pub(super) struct PendingDelta {
    item_id: beryl_model::SyndicItemId,
    cas_item_id: CasItemId,
    expected_kind: ProviderItemKind,
    text: String,
}

impl LiveCapture {
    pub(super) fn start_item(
        &mut self,
        store: &HomeStore,
        storage: SyndicStorage,
        item: &ThreadItem,
        limit: SyndicPointReadLimit,
    ) -> Result<(), OrdinaryTurnExecutionError> {
        if item.kind().lifecycle_contract()
            == beryl_backend::ThreadItemLifecycleContract::CompletionOnly
        {
            self.note_incomplete(TurnIncompleteReason::CompletionMismatch);
            return Ok(());
        }
        let descriptor = ItemDescriptor::new(self, item)?;
        if descriptor.source.kind() == ProviderItemKind::UserMessage {
            if !self.validate_user_message(store, storage, item, limit)?
                || !self.validate_user_lifecycle(
                    store,
                    storage,
                    &descriptor.source,
                    ProviderItemLifecycle::AwaitingCorrelation,
                    false,
                    limit,
                )?
            {
                self.note_incomplete(TurnIncompleteReason::CompletionMismatch);
                return Ok(());
            }
        } else if self
            .capture_item(store, storage, descriptor.source.cas_item_id(), limit)?
            .is_some()
        {
            self.note_incomplete(TurnIncompleteReason::CompletionMismatch);
            return Ok(());
        }
        self.emit(
            store,
            storage,
            limit,
            SourceEventPayload::ItemStarted {
                item: descriptor.source.clone(),
                assistant_phase: descriptor.assistant_phase,
            },
        )?;
        if descriptor.source.disposition() == ProviderItemDisposition::CanonicalText {
            for fragment in descriptor.text {
                self.queue_text(
                    store,
                    storage,
                    descriptor.source.cas_item_id().clone(),
                    descriptor.source.kind(),
                    fragment,
                    limit,
                )?;
            }
        }
        Ok(())
    }

    pub(super) fn complete_item(
        &mut self,
        store: &HomeStore,
        storage: SyndicStorage,
        item: &ThreadItem,
        limit: SyndicPointReadLimit,
    ) -> Result<(), OrdinaryTurnExecutionError> {
        let descriptor = ItemDescriptor::new(self, item)?;
        if descriptor.source.kind() == ProviderItemKind::UserMessage {
            if !self.validate_user_message(store, storage, item, limit)?
                || !self.validate_user_lifecycle(
                    store,
                    storage,
                    &descriptor.source,
                    ProviderItemLifecycle::Started,
                    true,
                    limit,
                )?
            {
                self.note_incomplete(TurnIncompleteReason::CompletionMismatch);
                return Ok(());
            }
            return self.emit_completed(store, storage, descriptor, limit);
        }
        let existing = self.capture_item(store, storage, descriptor.source.cas_item_id(), limit)?;
        if existing.is_none() && descriptor.source.kind().permits_completion_only() {
            return self.emit_completed(store, storage, descriptor, limit);
        }
        let Some(existing) = existing else {
            self.note_incomplete(TurnIncompleteReason::CompletionMismatch);
            return Ok(());
        };
        if !matching_started_item(&existing, &descriptor.source) {
            self.note_incomplete(TurnIncompleteReason::CompletionMismatch);
            return Ok(());
        }
        if descriptor.source.disposition() == ProviderItemDisposition::CanonicalText
            && !self.append_authoritative_text(
                store,
                storage,
                &existing,
                descriptor.text.clone(),
                limit,
            )?
        {
            return Ok(());
        }
        self.emit_completed(store, storage, descriptor, limit)
    }

    fn emit_completed(
        &mut self,
        store: &HomeStore,
        storage: SyndicStorage,
        descriptor: ItemDescriptor<'_>,
        limit: SyndicPointReadLimit,
    ) -> Result<(), OrdinaryTurnExecutionError> {
        self.emit(
            store,
            storage,
            limit,
            SourceEventPayload::ItemCompleted {
                item: descriptor.source,
                assistant_phase: descriptor.assistant_phase,
            },
        )
    }

    pub(super) fn handle_delta(
        &mut self,
        store: &HomeStore,
        storage: SyndicStorage,
        delta: ItemDelta,
        limit: SyndicPointReadLimit,
    ) -> Result<(), OrdinaryTurnExecutionError> {
        let expected_kind = provider_kind(delta.expected_item_kind());
        if expected_kind != provider_kind(delta.payload().expected_item_kind()) {
            self.note_incomplete(TurnIncompleteReason::CompletionMismatch);
            return Ok(());
        }
        let item = match self.require_started_item(
            store,
            storage,
            delta.item_id(),
            expected_kind,
            limit,
        )? {
            Some(item) => item,
            None => return Ok(()),
        };
        match delta.into_payload() {
            ItemDeltaPayload::AgentMessage { delta }
            | ItemDeltaPayload::Plan { delta }
            | ItemDeltaPayload::CommandExecutionOutput { delta }
            | ItemDeltaPayload::FileChangeOutput { delta } => self.queue_text(
                store,
                storage,
                item.item()
                    .cas_source()
                    .expect("sourced item")
                    .item_id()
                    .clone(),
                expected_kind,
                &delta,
                limit,
            ),
            ItemDeltaPayload::ReasoningSummaryText { delta, .. } => self.queue_text(
                store,
                storage,
                item.item()
                    .cas_source()
                    .expect("sourced item")
                    .item_id()
                    .clone(),
                expected_kind,
                &delta,
                limit,
            ),
            ItemDeltaPayload::FileChangePatchUpdated { changes } => {
                self.append_authoritative_text(
                    store,
                    storage,
                    &item,
                    ItemText::file_changes(&changes),
                    limit,
                )?;
                Ok(())
            }
            ItemDeltaPayload::McpToolCallProgress { .. }
            | ItemDeltaPayload::ReasoningSummaryPartAdded { .. }
            | ItemDeltaPayload::ReasoningTextObserved { .. } => Ok(()),
        }
    }

    pub(super) fn queue_text(
        &mut self,
        store: &HomeStore,
        storage: SyndicStorage,
        cas_item_id: CasItemId,
        expected_kind: ProviderItemKind,
        text: &str,
        limit: SyndicPointReadLimit,
    ) -> Result<(), OrdinaryTurnExecutionError> {
        if text.is_empty() {
            return Ok(());
        }
        let Some(item) =
            self.require_started_item(store, storage, &cas_item_id, expected_kind, limit)?
        else {
            return Ok(());
        };
        if item.item().disposition() != ProviderItemDisposition::CanonicalText {
            self.note_incomplete(TurnIncompleteReason::CompletionMismatch);
            return Ok(());
        }
        for part in bounded_utf8_parts(text) {
            let append = self.pending_delta.as_ref().is_some_and(|pending| {
                pending.cas_item_id == cas_item_id
                    && pending.expected_kind == expected_kind
                    && pending.text.len().saturating_add(part.len()) <= COALESCED_DELTA_MAX_BYTES
            });
            if append {
                self.pending_delta
                    .as_mut()
                    .expect("matching pending delta exists")
                    .text
                    .push_str(part);
            } else {
                self.flush_delta(store, storage, limit)?;
                self.pending_delta = Some(PendingDelta {
                    item_id: item.item().id(),
                    cas_item_id: cas_item_id.clone(),
                    expected_kind,
                    text: part.to_owned(),
                });
            }
        }
        Ok(())
    }

    pub(super) fn flush_delta(
        &mut self,
        store: &HomeStore,
        storage: SyndicStorage,
        limit: SyndicPointReadLimit,
    ) -> Result<(), OrdinaryTurnExecutionError> {
        let Some(pending) = self.pending_delta.take() else {
            return Ok(());
        };
        let Some(item) = self.require_started_item(
            store,
            storage,
            &pending.cas_item_id,
            pending.expected_kind,
            limit,
        )?
        else {
            return Ok(());
        };
        if item.item().id() != pending.item_id
            || item.item().disposition() != ProviderItemDisposition::CanonicalText
        {
            self.note_incomplete(TurnIncompleteReason::CompletionMismatch);
            return Ok(());
        }
        self.emit(
            store,
            storage,
            limit,
            SourceEventPayload::ItemDelta {
                item_id: pending.item_id,
                cas_item_id: pending.cas_item_id,
                expected_kind: pending.expected_kind,
                text: SourceEventText::new(&pending.text)?,
            },
        )
    }

    pub(super) fn capture_item(
        &self,
        store: &HomeStore,
        storage: SyndicStorage,
        cas_item_id: &CasItemId,
        limit: SyndicPointReadLimit,
    ) -> Result<Option<SyndicCaptureItem>, OrdinaryTurnExecutionError> {
        let source = CasItemSource::new(self.source.clone(), cas_item_id.clone());
        let item = storage.capture_item(store, &source, limit)?;
        if let Some(item) = &item
            && item.item().turn_id() != self.context.turn_id()
        {
            return Err(OrdinaryTurnExecutionError::Invariant(
                "CAS item selected a different Syndic live-capture turn",
            ));
        }
        Ok(item)
    }

    fn require_started_item(
        &mut self,
        store: &HomeStore,
        storage: SyndicStorage,
        cas_item_id: &CasItemId,
        expected_kind: ProviderItemKind,
        limit: SyndicPointReadLimit,
    ) -> Result<Option<SyndicCaptureItem>, OrdinaryTurnExecutionError> {
        let Some(item) = self.capture_item(store, storage, cas_item_id, limit)? else {
            self.note_incomplete(TurnIncompleteReason::CompletionMismatch);
            return Ok(None);
        };
        if item.item().provider_kind() != expected_kind
            || item.item().provider_lifecycle() != ProviderItemLifecycle::Started
        {
            self.note_incomplete(TurnIncompleteReason::CompletionMismatch);
            return Ok(None);
        }
        Ok(Some(item))
    }

    fn validate_user_message(
        &self,
        store: &HomeStore,
        storage: SyndicStorage,
        item: &ThreadItem,
        _limit: SyndicPointReadLimit,
    ) -> Result<bool, OrdinaryTurnExecutionError> {
        let ThreadItem::UserMessage(message) = item else {
            return Ok(false);
        };
        let [
            beryl_backend::UserInput::Text {
                text,
                text_elements,
            },
        ] = message.content.as_slice()
        else {
            return Ok(false);
        };
        if !text_elements.is_empty()
            || u64::try_from(text.len()).ok()
                != Some(self.submitted_content.summary().logical_utf8_bytes())
        {
            return Ok(false);
        }
        let mut offset = 0_u64;
        loop {
            let Some(page) = storage.sealed_content_text_range(
                store,
                self.submitted_content,
                offset,
                COALESCED_DELTA_MAX_BYTES,
            )?
            else {
                return Err(OrdinaryTurnExecutionError::Invariant(
                    "submitted ordinary input content disappeared during correlation",
                ));
            };
            let start = usize::try_from(offset).map_err(|_| {
                OrdinaryTurnExecutionError::Invariant(
                    "submitted input comparison offset exceeds the process address space",
                )
            })?;
            let end = start.checked_add(page.text().len()).ok_or(
                OrdinaryTurnExecutionError::Invariant(
                    "submitted input comparison frontier overflowed",
                ),
            )?;
            if text.get(start..end) != Some(page.text()) {
                return Ok(false);
            }
            let Some(next) = page.next_offset() else {
                return Ok(end == text.len());
            };
            if next <= offset {
                return Err(OrdinaryTurnExecutionError::Invariant(
                    "submitted input correlation read did not advance",
                ));
            }
            offset = next;
        }
    }

    fn validate_user_lifecycle(
        &self,
        store: &HomeStore,
        storage: SyndicStorage,
        descriptor: &SourceItemDescriptor,
        expected_lifecycle: ProviderItemLifecycle,
        expect_correlation: bool,
        limit: SyndicPointReadLimit,
    ) -> Result<bool, OrdinaryTurnExecutionError> {
        let Some(item) = storage.canonical_item(store, self.submitted_item_id, limit)? else {
            return Err(OrdinaryTurnExecutionError::Invariant(
                "submitted user item disappeared during provider correlation",
            ));
        };
        let item = item.record();
        let expected_source = expect_correlation
            .then(|| CasItemSource::new(self.source.clone(), descriptor.cas_item_id().clone()));
        Ok(item.id() == self.submitted_item_id
            && item.turn_id() == self.context.turn_id()
            && item.provider_kind() == ProviderItemKind::UserMessage
            && item.provider_lifecycle() == expected_lifecycle
            && item.disposition() == descriptor.disposition()
            && item.cas_source() == expected_source.as_ref())
    }
}

fn matching_started_item(item: &SyndicCaptureItem, descriptor: &SourceItemDescriptor) -> bool {
    item.item().id() == descriptor.item_id()
        && item.item().provider_kind() == descriptor.kind()
        && item.item().provider_lifecycle() == ProviderItemLifecycle::Started
        && item.item().disposition() == descriptor.disposition()
}
