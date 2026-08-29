use beryl_backend::{
    NormalTurnTerminal, NormalTurnTerminalStatus, OrderedTurnStreamCompletion,
    OrderedTurnStreamOperation, OrderedTurnStreamRejection, OrderedTurnStreamSubmitCause,
};
use beryl_home_store::CursorReadLimits;
use syndic_storage::{
    CasItemSource, ProviderFrameHistorySupportV1, ProviderFrameObservationSummaryV1,
    ProviderItemLifecycle, ProviderNarrativeCompletionDisposition, SourceEventPayload,
    SyndicPointReadLimit, SyndicStorage, TurnEndStatus, TurnIncompleteReason, TurnItemIndexRecord,
    TurnItemOrdinal, TurnStateRecord, TurnTerminalOutcome, UnsupportedHistoryReason,
};

use super::{Ingester, TargetRouteOutcome, WholeConnectionRoutingFailure};
use crate::cas_projection::{
    connection::router::{
        LiveEventTargetCloseReason, ProvenTerminalOutcome, SourcePublicationFinishError,
        SourcePublicationPermit, SourcePublicationPermitError, TargetInvalidation,
    },
    live_source::{LiveSourceFrontier, LiveSourceTarget, publish_reconciled},
};

const TERMINAL_ITEM_PAGE_RECORDS: usize = 64;
const TERMINAL_ITEM_PAGE_BYTES: usize = 64 * 1024;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct TerminalAuditOutcome {
    incomplete_reason: Option<TurnIncompleteReason>,
}

struct TerminalAuditProof {
    state: TurnStateRecord,
    outcome: TerminalAuditOutcome,
}

#[derive(Default)]
struct TerminalAudit {
    provider_issue: bool,
    narrative_mismatch: bool,
    first_unsupported: Option<UnsupportedHistoryReason>,
    unresolved_item: bool,
    item_count: u64,
    open_item_count: u64,
    history_blocking_item_count: u64,
}

impl TerminalAudit {
    fn new(provider_issue: bool) -> Self {
        Self {
            provider_issue,
            ..Self::default()
        }
    }

    fn observe(&mut self, item: &syndic_storage::CanonicalItemRecord) -> Result<(), ()> {
        self.item_count = self.item_count.checked_add(1).ok_or(())?;
        if item.provider_lifecycle() != ProviderItemLifecycle::Completed {
            self.unresolved_item = true;
        }
        if item.provider_lifecycle() == ProviderItemLifecycle::Started {
            self.open_item_count = self.open_item_count.checked_add(1).ok_or(())?;
        }
        if let ProviderFrameHistorySupportV1::Unsupported(reason) = item.history_support()
            && self.first_unsupported.is_none()
        {
            self.first_unsupported = Some(reason);
        }
        if item
            .narrative_completion()
            .is_some_and(ProviderNarrativeCompletionDisposition::is_mismatch)
        {
            self.narrative_mismatch = true;
        }
        if item.is_history_blocking() {
            self.history_blocking_item_count =
                self.history_blocking_item_count.checked_add(1).ok_or(())?;
        }
        Ok(())
    }

    fn finish(self, state: &TurnStateRecord) -> Result<TerminalAuditOutcome, ()> {
        if self.item_count != state.item_count()
            || self.open_item_count != state.open_item_count()
            || self.history_blocking_item_count != state.history_blocking_item_count()
        {
            return Err(());
        }
        Ok(self.classify())
    }

    fn classify(&self) -> TerminalAuditOutcome {
        let incomplete_reason = if self.provider_issue || self.narrative_mismatch {
            Some(TurnIncompleteReason::CompletionMismatch)
        } else if let Some(reason) = self.first_unsupported {
            Some(TurnIncompleteReason::UnsupportedHistory(reason))
        } else if self.unresolved_item {
            Some(TurnIncompleteReason::ItemAuditFailed)
        } else {
            None
        };
        TerminalAuditOutcome { incomplete_reason }
    }
}

impl Ingester {
    pub(super) fn normal_turn_terminal(
        &mut self,
        terminal: NormalTurnTerminal,
    ) -> (super::BrokerReply, bool) {
        if self.active.is_some() {
            return self.reject(
                OrderedTurnStreamOperation::NormalTurnTerminal(terminal),
                OrderedTurnStreamRejection::InvalidControl,
            );
        }
        let permit = match self
            .router
            .acquire_terminal_source_publication(terminal.thread_id(), terminal.turn_id())
        {
            Ok(permit) => permit,
            Err(SourcePublicationPermitError::Unmatched) => {
                return self.reject(
                    OrderedTurnStreamOperation::NormalTurnTerminal(terminal),
                    OrderedTurnStreamRejection::InvalidControl,
                );
            }
            Err(SourcePublicationPermitError::Target(invalidation)) => {
                if invalidation.reason == LiveEventTargetCloseReason::EventBeforeTurnStart {
                    return self.normal_terminal_target_failure(invalidation);
                }
                return self.reject_normal_terminal_route(terminal, invalidation);
            }
            Err(SourcePublicationPermitError::Router) => {
                return self.reject(
                    OrderedTurnStreamOperation::NormalTurnTerminal(terminal),
                    OrderedTurnStreamRejection::InvalidControl,
                );
            }
        };
        if permit.compaction().is_some() {
            return self.context_compaction_terminal(permit, terminal);
        }
        let limit = point_limit();
        let (home_generation, storage) = match self.publish_source_activation(&permit, limit) {
            Ok(authority) => authority,
            Err(error) if error.authority().is_some() => {
                permit.settle_authority_lost();
                return self.authority_lost_terminal();
            }
            Err(_) => return self.failed_normal_terminal_permit(permit, terminal),
        };
        let target = match LiveSourceTarget::resolve(
            &self.home,
            &storage,
            permit.syndic_thread_id(),
            permit.cas_thread_id(),
            permit.cas_turn_id(),
            limit,
        ) {
            Ok(target) => target,
            Err(_) => return self.failed_normal_terminal_permit(permit, terminal),
        };
        let audit = match self.audit_terminal_items(&storage, &target, limit) {
            Ok(audit) => audit,
            Err(()) => return self.failed_normal_terminal_permit(permit, terminal),
        };
        let frontier = match LiveSourceFrontier::read(&self.home, &storage, &target, limit) {
            Ok(frontier) if frontier.state() == &audit.state => frontier,
            Ok(_) | Err(_) => return self.failed_normal_terminal_permit(permit, terminal),
        };
        let status = terminal_status(terminal.status(), audit.outcome.incomplete_reason);
        let event = match frontier.event(
            &target,
            Some(target.source().clone()),
            SourceEventPayload::TurnEnded(status),
        ) {
            Ok(event) => event,
            Err(_) => return self.failed_normal_terminal_permit(permit, terminal),
        };
        let observed_at = event.observed_at();
        if publish_reconciled(
            &self.home,
            self.home_id,
            home_generation,
            &storage,
            &event,
            limit,
        )
        .is_err()
        {
            return self.failed_normal_terminal_permit(permit, terminal);
        }
        self.stop_coordinator
            .terminal_consumed(target.thread_id(), target.turn_id());
        self.finish_normal_terminal_permit(permit, ProvenTerminalOutcome::new(status, observed_at))
    }

    fn context_compaction_terminal(
        &mut self,
        permit: SourcePublicationPermit,
        terminal: NormalTurnTerminal,
    ) -> (super::BrokerReply, bool) {
        let authority = permit
            .compaction()
            .expect("dedicated compaction terminal retains operation authority");
        let status = terminal_status(terminal.status(), None);
        let observed_at = timestamp_now();
        if self
            .context_compaction
            .publish_provider_event(
                authority,
                syndic_storage::CompactionProviderEvent::Terminal(status),
                observed_at,
            )
            .is_err()
        {
            return self.failed_normal_terminal_permit(permit, terminal);
        }
        self.stop_coordinator.terminal_consumed(
            authority.operation_id().thread_id(),
            authority.provider_turn_id(),
        );
        self.finish_normal_terminal_permit(permit, ProvenTerminalOutcome::new(status, observed_at))
    }

    fn audit_terminal_items(
        &self,
        storage: &SyndicStorage,
        target: &LiveSourceTarget,
        limit: SyndicPointReadLimit,
    ) -> Result<TerminalAuditProof, ()> {
        let state = storage
            .turn_state(&self.home, target.turn_id(), limit)
            .map_err(|_| ())?
            .ok_or(())?;
        if state.turn_id() != target.turn_id()
            || state.lifecycle() != syndic_storage::TurnLifecycle::Active
            || state.end_status().is_some()
        {
            return Err(());
        }
        let mut audit = TerminalAudit::new(state.provider_observation_issue().is_some());
        self.scan_terminal_items(storage, target, limit, &mut audit)?;
        let confirmed = storage
            .turn_state(&self.home, target.turn_id(), limit)
            .map_err(|_| ())?
            .ok_or(())?;
        if confirmed != state {
            return Err(());
        }
        let outcome = audit.finish(&state)?;
        Ok(TerminalAuditProof { state, outcome })
    }

    fn scan_terminal_items(
        &self,
        storage: &SyndicStorage,
        target: &LiveSourceTarget,
        limit: SyndicPointReadLimit,
        audit: &mut TerminalAudit,
    ) -> Result<(), ()> {
        let page_limits =
            CursorReadLimits::new(TERMINAL_ITEM_PAGE_RECORDS, TERMINAL_ITEM_PAGE_BYTES)
                .map_err(|_| ())?;
        let mut after = None;
        let mut expected = Some(TurnItemOrdinal::FIRST);
        loop {
            let page = storage
                .turn_items(&self.home, target.turn_id(), after, page_limits)
                .map_err(|_| ())?;
            if page.records().is_empty() && page.has_more() {
                return Err(());
            }
            for index in page.records() {
                if index.turn_id() != target.turn_id() || Some(index.ordinal()) != expected {
                    return Err(());
                }
                self.audit_terminal_item(storage, target, index, limit, audit)?;
                after = Some(index.ordinal());
                expected = index.ordinal().checked_next().ok();
            }
            if !page.has_more() {
                return Ok(());
            }
            if after.is_none() || expected.is_none() {
                return Err(());
            }
        }
    }

    fn audit_terminal_item(
        &self,
        storage: &SyndicStorage,
        target: &LiveSourceTarget,
        index: &TurnItemIndexRecord,
        limit: SyndicPointReadLimit,
        audit: &mut TerminalAudit,
    ) -> Result<(), ()> {
        let item = storage
            .canonical_item(&self.home, index.item_id(), limit)
            .map_err(|_| ())?
            .ok_or(())?;
        if item.id() != index.item_id()
            || item.turn_id() != target.turn_id()
            || item.ordinal() != index.ordinal()
            || item.revision() != index.item_revision()
        {
            return Err(());
        }
        let captured = match item.cas_source() {
            Some(source) => {
                if source.turn() != target.source() {
                    return Err(());
                }
                let captured = storage
                    .capture_item(&self.home, source, limit)
                    .map_err(|_| ())?
                    .ok_or(())?;
                if captured.item() != &item
                    || captured.cas_index().item_id() != index.item_id()
                    || captured.cas_index().item_revision() != index.item_revision()
                {
                    return Err(());
                }
                Some(captured)
            }
            None => None,
        };
        audit.observe(&item)?;
        if item.provider_lifecycle() != ProviderItemLifecycle::Completed {
            return Ok(());
        }
        let captured = captured.as_ref().ok_or(())?;
        validate_completed_item(captured, item.cas_source().ok_or(())?)
    }

    fn finish_normal_terminal_permit(
        &mut self,
        permit: SourcePublicationPermit,
        outcome: ProvenTerminalOutcome,
    ) -> (super::BrokerReply, bool) {
        match permit.finish_terminal(outcome) {
            Ok(()) => (
                super::BrokerReply::Applied(OrderedTurnStreamCompletion::Applied),
                false,
            ),
            Err(SourcePublicationFinishError::Target(invalidation)) => {
                let terminal = self.invalidate_target(invalidation) == TargetRouteOutcome::Terminal;
                (
                    super::BrokerReply::Applied(OrderedTurnStreamCompletion::Applied),
                    terminal,
                )
            }
            Err(SourcePublicationFinishError::Router) => {
                self.retire();
                (
                    super::BrokerReply::Applied(OrderedTurnStreamCompletion::Applied),
                    true,
                )
            }
        }
    }

    fn failed_normal_terminal_permit(
        &mut self,
        permit: SourcePublicationPermit,
        terminal: NormalTurnTerminal,
    ) -> (super::BrokerReply, bool) {
        if self.exact_persistent_failure() {
            drop(permit);
            return (
                super::BrokerReply::Applied(OrderedTurnStreamCompletion::Applied),
                true,
            );
        }
        match permit.fail() {
            Ok(invalidation) | Err(SourcePublicationFinishError::Target(invalidation)) => {
                self.normal_terminal_target_failure(invalidation)
            }
            Err(SourcePublicationFinishError::Router) => self.reject(
                OrderedTurnStreamOperation::NormalTurnTerminal(terminal),
                OrderedTurnStreamRejection::InvalidControl,
            ),
        }
    }

    fn normal_terminal_target_failure(
        &mut self,
        invalidation: TargetInvalidation,
    ) -> (super::BrokerReply, bool) {
        let outcome = self.invalidate_target(invalidation);
        (
            super::BrokerReply::Applied(OrderedTurnStreamCompletion::Applied),
            outcome == TargetRouteOutcome::Terminal,
        )
    }

    fn reject_normal_terminal_route(
        &mut self,
        terminal: NormalTurnTerminal,
        invalidation: TargetInvalidation,
    ) -> (super::BrokerReply, bool) {
        let reason = invalidation.reason;
        let _ = self.invalidate_target(invalidation);
        self.retire_for(WholeConnectionRoutingFailure::Router, reason);
        (
            super::BrokerReply::Rejected(
                OrderedTurnStreamOperation::NormalTurnTerminal(terminal),
                OrderedTurnStreamSubmitCause::Rejected(OrderedTurnStreamRejection::InvalidControl),
            ),
            true,
        )
    }
}

fn validate_completed_item(
    captured: &syndic_storage::SyndicCaptureItem,
    source: &CasItemSource,
) -> Result<(), ()> {
    let item = captured.item();
    let provider = item.provider().ok_or(())?;
    let content = captured.content().ok_or(())?;
    if provider.stream_state().item_id() != source.item_id()
        || provider.stream_state().kind() != item.provider_kind()
        || !provider.stream_state().is_complete()
        || !matches!(
            provider.observation(),
            ProviderFrameObservationSummaryV1::Completed(_)
        )
        || content.current_reference() != Some(provider.content())
    {
        return Err(());
    }
    Ok(())
}

fn terminal_status(
    status: NormalTurnTerminalStatus,
    incomplete_reason: Option<TurnIncompleteReason>,
) -> TurnEndStatus {
    let outcome = match status {
        NormalTurnTerminalStatus::Completed => TurnTerminalOutcome::Complete,
        NormalTurnTerminalStatus::Failed => TurnTerminalOutcome::Failed,
        NormalTurnTerminalStatus::Interrupted => TurnTerminalOutcome::Interrupted,
    };
    TurnEndStatus::new(outcome, incomplete_reason)
        .expect("normal provider terminal status and history disposition are compatible")
}

fn point_limit() -> SyndicPointReadLimit {
    SyndicPointReadLimit::new(super::super::PROVIDER_POINT_READ_BYTES)
        .expect("provider broker point-read bound is nonzero")
}

fn timestamp_now() -> syndic_storage::SyndicTimestamp {
    let millis = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .ok()
        .and_then(|duration| u64::try_from(duration.as_millis()).ok())
        .unwrap_or(0);
    syndic_storage::SyndicTimestamp::from_unix_millis(millis)
}

#[cfg(test)]
mod tests {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/unit/provider_broker_terminal.rs"
    ));
}
