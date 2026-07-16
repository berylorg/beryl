use beryl_backend::{CompletedTurn, CompletedTurnStatus};
use beryl_home_store::{CursorReadLimits, HomeStore};
use syndic_storage::{
    CanonicalItemPayload, ContentLifecycle, GeneratedMediaResourceDisposition,
    ProviderItemDisposition, ProviderItemKind, ProviderItemLifecycle, ResourceBacking,
    SyndicPointReadLimit, SyndicStorage, TurnEndStatus, TurnIncompleteReason, TurnItemIndexRecord,
    TurnTerminalOutcome,
};

use super::LiveCapture;
use crate::cas_projection::ordinary::OrdinaryTurnExecutionError;

const TERMINAL_AUDIT_PAGE_ITEMS: usize = 64;
const TERMINAL_AUDIT_PAGE_BYTES: usize = 64 * 1024;

impl LiveCapture {
    pub(super) fn terminal_status(
        &self,
        store: &HomeStore,
        storage: SyndicStorage,
        turn: &CompletedTurn,
        limit: SyndicPointReadLimit,
    ) -> Result<TurnEndStatus, OrdinaryTurnExecutionError> {
        let outcome = match turn.status {
            CompletedTurnStatus::Completed => TurnTerminalOutcome::Complete,
            CompletedTurnStatus::Interrupted => TurnTerminalOutcome::Interrupted,
            CompletedTurnStatus::Failed => TurnTerminalOutcome::Failed,
        };
        let audit_reason = self.audit_terminal_items(store, storage, limit)?;
        Ok(TurnEndStatus::new(
            outcome,
            self.incomplete_reason().or(audit_reason),
        )?)
    }

    fn audit_terminal_items(
        &self,
        store: &HomeStore,
        storage: SyndicStorage,
        limit: SyndicPointReadLimit,
    ) -> Result<Option<TurnIncompleteReason>, OrdinaryTurnExecutionError> {
        let turn_id = self.context.turn_id();
        let expected_state = storage
            .turn_state(store, turn_id, limit)?
            .ok_or(OrdinaryTurnExecutionError::Invariant(
                "live-capture turn state disappeared before terminal audit",
            ))?
            .record()
            .clone();
        if expected_state.turn_id() != turn_id || expected_state.revision() != self.state_revision {
            return Err(OrdinaryTurnExecutionError::ConcurrentChange {
                thread_id: self.context.thread_id(),
            });
        }
        let page_limits =
            CursorReadLimits::new(TERMINAL_AUDIT_PAGE_ITEMS, TERMINAL_AUDIT_PAGE_BYTES)
                .expect("terminal audit page bounds are nonzero");
        let mut after = None;
        let mut observed = 0_u64;
        let mut reason = None;

        loop {
            let before = storage.turn_state(store, turn_id, limit)?.ok_or(
                OrdinaryTurnExecutionError::Invariant(
                    "live-capture turn state disappeared during terminal audit",
                ),
            )?;
            if before.record() != &expected_state {
                return Err(OrdinaryTurnExecutionError::ConcurrentChange {
                    thread_id: self.context.thread_id(),
                });
            }
            let page = storage.turn_items(store, turn_id, after, page_limits)?;
            if page.records().is_empty() && page.has_more() {
                return Err(OrdinaryTurnExecutionError::Invariant(
                    "terminal item audit returned an empty continuing page",
                ));
            }
            for index in page.records() {
                reason = reason.or(self.audit_terminal_item(store, storage, index, limit)?);
                after = Some(index.ordinal());
                observed = observed
                    .checked_add(1)
                    .ok_or(OrdinaryTurnExecutionError::Invariant(
                        "terminal item audit count overflowed",
                    ))?;
            }
            let after_state = storage.turn_state(store, turn_id, limit)?.ok_or(
                OrdinaryTurnExecutionError::Invariant(
                    "live-capture turn state disappeared during terminal audit",
                ),
            )?;
            if after_state.record() != &expected_state {
                return Err(OrdinaryTurnExecutionError::ConcurrentChange {
                    thread_id: self.context.thread_id(),
                });
            }
            if !page.has_more() {
                break;
            }
        }
        if observed != expected_state.item_count() {
            return Err(OrdinaryTurnExecutionError::Invariant(
                "terminal item audit disagrees with the durable turn item count",
            ));
        }
        Ok(reason)
    }

    fn audit_terminal_item(
        &self,
        store: &HomeStore,
        storage: SyndicStorage,
        index: &TurnItemIndexRecord,
        limit: SyndicPointReadLimit,
    ) -> Result<Option<TurnIncompleteReason>, OrdinaryTurnExecutionError> {
        let item = storage
            .canonical_item(store, index.item_id(), limit)?
            .ok_or(OrdinaryTurnExecutionError::Invariant(
                "terminal item audit selected a missing canonical item",
            ))?;
        let item = item.record();
        if index.turn_id() != self.context.turn_id()
            || item.turn_id() != self.context.turn_id()
            || item.id() != index.item_id()
            || item.ordinal() != index.ordinal()
            || item.revision() != index.item_revision()
        {
            return Err(OrdinaryTurnExecutionError::Invariant(
                "terminal item index disagrees with its canonical item",
            ));
        }
        if item.provider_lifecycle() != ProviderItemLifecycle::Completed {
            return Ok(Some(TurnIncompleteReason::CompletionMismatch));
        }
        if let ProviderItemDisposition::Unsupported(reason) = item.disposition() {
            return Ok(Some(TurnIncompleteReason::UnsupportedHistory(reason)));
        }
        self.audit_completed_payload(store, storage, item, limit)
    }

    fn audit_completed_payload(
        &self,
        store: &HomeStore,
        storage: SyndicStorage,
        item: &syndic_storage::CanonicalItemRecord,
        limit: SyndicPointReadLimit,
    ) -> Result<Option<TurnIncompleteReason>, OrdinaryTurnExecutionError> {
        match item.payload() {
            CanonicalItemPayload::UserInput { content, .. } => {
                if item.provider_kind() != ProviderItemKind::UserMessage {
                    return Ok(Some(TurnIncompleteReason::ItemAuditFailed));
                }
                let manifest = storage
                    .content_manifest(store, content.id(), limit)?
                    .ok_or(OrdinaryTurnExecutionError::Invariant(
                        "terminal user item selected a missing content manifest",
                    ))?;
                let manifest = manifest.record();
                if manifest.current_reference() != Some(*content)
                    || manifest.lifecycle() != ContentLifecycle::Sealed
                    || manifest.owner().is_some()
                {
                    return Ok(Some(TurnIncompleteReason::ItemAuditFailed));
                }
                Ok(None)
            }
            CanonicalItemPayload::Text(content) => {
                let manifest = storage
                    .content_manifest(store, content.id(), limit)?
                    .ok_or(OrdinaryTurnExecutionError::Invariant(
                        "terminal text item selected a missing content manifest",
                    ))?;
                let manifest = manifest.record();
                if manifest.current_reference() != Some(*content)
                    || manifest.lifecycle() != ContentLifecycle::Finalized
                    || manifest.owner() != Some(item.id())
                {
                    return Ok(Some(TurnIncompleteReason::ItemAuditFailed));
                }
                Ok(None)
            }
            CanonicalItemPayload::Activity => Ok(None),
            CanonicalItemPayload::GeneratedMedia(resource_id) => {
                let resource = storage.resource(store, *resource_id, limit)?.ok_or(
                    OrdinaryTurnExecutionError::Invariant(
                        "terminal generated item selected a missing resource",
                    ),
                )?;
                let pending = resource.record().item_id() == item.id()
                    && matches!(
                        resource.record().backing(),
                        ResourceBacking::GeneratedMedia(
                            GeneratedMediaResourceDisposition::PendingAsset
                        )
                    );
                if !pending {
                    return Ok(Some(TurnIncompleteReason::ItemAuditFailed));
                }
                Ok(None)
            }
            CanonicalItemPayload::Unsupported(reason) => {
                Ok(Some(TurnIncompleteReason::UnsupportedHistory(*reason)))
            }
        }
    }
}
