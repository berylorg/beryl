use beryl_model::DraftRevision;
use beryl_state::RecordRevision;
use syndic_storage::{ComposerPayload, SyndicTimestamp};

use super::{
    DraftAutosaveAction, DraftAutosaveInterval, DraftAutosavePublication,
    DraftAutosavePublicationAction, DraftCompletionAction, DraftEditGeneration, DraftFlushAction,
    DraftPersistenceBinding, DraftPersistenceError, DraftPersistenceSeed, DraftPersistenceTime,
    DraftReconciliationAction, DraftRequestGeneration, DraftSaveExecution, DraftSaveOutcome,
    DraftSaveRequest, DraftSaveToken, DraftSuspensionCause, DraftTimerGeneration, DurableDraftBase,
    ImmutableDraftShape,
};

#[derive(Clone, Debug)]
struct InFlightSave {
    request: DraftSaveRequest,
}

#[derive(Clone, Debug)]
struct SuspendedSave {
    request: DraftSaveRequest,
    cause: DraftSuspensionCause,
}

/// Deterministic non-GPUI coordinator for one exact current draft.
pub struct DraftPersistenceService {
    binding: DraftPersistenceBinding,
    immutable_shape: ImmutableDraftShape,
    durable: DurableDraftBase,
    editor_payload: ComposerPayload,
    editor_updated_at: SyndicTimestamp,
    edit_generation: DraftEditGeneration,
    timer_generation: DraftTimerGeneration,
    last_request_generation: Option<DraftRequestGeneration>,
    interval: DraftAutosaveInterval,
    setting_revision: Option<RecordRevision>,
    timer_anchor: DraftPersistenceTime,
    in_flight: Option<InFlightSave>,
    suspended: Option<SuspendedSave>,
    flush_pending: bool,
}

impl DraftPersistenceService {
    /// Preloads the caller-visible editor from one exact durable current draft.
    #[must_use]
    pub fn from_seed(seed: DraftPersistenceSeed, publication: DraftAutosavePublication) -> Self {
        let current = seed.current();
        let draft = current.draft();
        let binding = DraftPersistenceBinding::initial(
            seed.home_id(),
            seed.home_generation(),
            draft.thread_id(),
            draft.id(),
        );
        Self {
            binding,
            immutable_shape: ImmutableDraftShape::from_record(draft),
            durable: DurableDraftBase {
                revision: draft.revision(),
                payload: seed.payload().clone(),
                updated_at: draft.updated_at(),
            },
            editor_payload: seed.payload().clone(),
            editor_updated_at: draft.updated_at(),
            edit_generation: DraftEditGeneration::FIRST,
            timer_generation: DraftTimerGeneration::FIRST,
            last_request_generation: None,
            interval: publication.interval(),
            setting_revision: publication.revision(),
            timer_anchor: seed.published_at(),
            in_flight: None,
            suspended: None,
            flush_pending: false,
        }
    }

    #[must_use]
    pub const fn binding(&self) -> DraftPersistenceBinding {
        self.binding
    }

    #[must_use]
    pub const fn durable_revision(&self) -> DraftRevision {
        self.durable.revision
    }

    #[must_use]
    pub const fn editor_payload(&self) -> &ComposerPayload {
        &self.editor_payload
    }

    #[must_use]
    pub const fn edit_generation(&self) -> DraftEditGeneration {
        self.edit_generation
    }

    #[must_use]
    pub const fn timer_generation(&self) -> DraftTimerGeneration {
        self.timer_generation
    }

    #[must_use]
    pub const fn interval(&self) -> DraftAutosaveInterval {
        self.interval
    }

    #[must_use]
    pub fn is_dirty(&self) -> bool {
        self.editor_payload != self.durable.payload
    }

    #[must_use]
    pub fn in_flight(&self) -> Option<DraftSaveToken> {
        self.in_flight.as_ref().map(|save| save.request.token())
    }

    #[must_use]
    pub const fn suspension(&self) -> Option<DraftSuspensionCause> {
        match &self.suspended {
            Some(save) => Some(save.cause),
            None => None,
        }
    }

    /// Replaces caller-facing editor content without touching durable authority.
    pub fn edit(
        &mut self,
        payload: ComposerPayload,
        updated_at: SyndicTimestamp,
    ) -> Result<bool, DraftPersistenceError> {
        if payload == self.editor_payload {
            return Ok(false);
        }
        if updated_at < self.editor_updated_at || updated_at < self.durable.updated_at {
            return Err(DraftPersistenceError::RegressedEditTimestamp);
        }
        let generation = self.edit_generation.next()?;
        self.editor_payload = payload;
        self.editor_updated_at = updated_at;
        self.edit_generation = generation;
        Ok(true)
    }

    /// Applies only a strictly newer committed setting and rearms from its publication time.
    pub fn apply_autosave_publication(
        &mut self,
        publication: DraftAutosavePublication,
        published_at: DraftPersistenceTime,
    ) -> Result<DraftAutosavePublicationAction, DraftPersistenceError> {
        let Some(revision) = publication.revision() else {
            return Ok(DraftAutosavePublicationAction::Stale);
        };
        if self
            .setting_revision
            .is_some_and(|current| revision <= current)
        {
            return Ok(DraftAutosavePublicationAction::Stale);
        }
        let generation = self.timer_generation.next()?;
        self.interval = publication.interval();
        self.setting_revision = Some(revision);
        self.timer_anchor = published_at;
        self.timer_generation = generation;
        Ok(DraftAutosavePublicationAction::Applied)
    }

    pub fn poll_autosave(
        &mut self,
        now: DraftPersistenceTime,
    ) -> Result<DraftAutosaveAction, DraftPersistenceError> {
        if let Some(suspended) = &self.suspended {
            return Ok(DraftAutosaveAction::Suspended(suspended.cause));
        }
        if let Some(in_flight) = &self.in_flight {
            return Ok(DraftAutosaveAction::InFlight(in_flight.request.token()));
        }
        if !self.is_dirty() {
            return Ok(DraftAutosaveAction::Clean);
        }
        if now.elapsed_since(self.timer_anchor) < self.interval.duration() {
            return Ok(DraftAutosaveAction::NotDue);
        }
        self.start_request().map(DraftAutosaveAction::Started)
    }

    /// Requests a barrier for the latest editor generation without cancelling admitted work.
    pub fn flush(&mut self) -> Result<DraftFlushAction, DraftPersistenceError> {
        if let Some(suspended) = &self.suspended {
            self.flush_pending = true;
            return Ok(DraftFlushAction::Suspended(suspended.cause));
        }
        if let Some(in_flight) = &self.in_flight {
            self.flush_pending = true;
            return Ok(DraftFlushAction::Waiting(in_flight.request.token()));
        }
        if !self.is_dirty() {
            return Ok(DraftFlushAction::Complete);
        }
        let request = self.start_request()?;
        self.flush_pending = true;
        Ok(DraftFlushAction::Started(request))
    }

    /// Consumes one executor-issued completion and rejects any noncurrent exact request token.
    pub fn complete(
        &mut self,
        execution: DraftSaveExecution,
        completed_at: DraftPersistenceTime,
    ) -> Result<DraftCompletionAction, DraftPersistenceError> {
        let (token, outcome) = execution.into_completion();
        let Some(in_flight) = self.in_flight.take() else {
            return Ok(DraftCompletionAction::Stale);
        };
        if in_flight.request.token_ref() != &token {
            self.in_flight = Some(in_flight);
            return Ok(DraftCompletionAction::Stale);
        }
        match outcome {
            DraftSaveOutcome::KnownUnchanged(reason) => {
                let flush_failed = std::mem::take(&mut self.flush_pending);
                Ok(DraftCompletionAction::KnownUnchanged {
                    reason,
                    flush_failed,
                })
            }
            DraftSaveOutcome::RequiresReconciliation(cause) => {
                self.suspended = Some(SuspendedSave {
                    request: in_flight.request,
                    cause,
                });
                Ok(DraftCompletionAction::Suspended(cause))
            }
            DraftSaveOutcome::Committed { revision } => {
                self.publish_commit(in_flight.request, revision, completed_at)
            }
        }
    }

    fn publish_commit(
        &mut self,
        request: DraftSaveRequest,
        revision: DraftRevision,
        completed_at: DraftPersistenceTime,
    ) -> Result<DraftCompletionAction, DraftPersistenceError> {
        let expected = request.expected_revision().checked_next().ok();
        if expected != Some(revision) {
            let cause = DraftSuspensionCause::InvalidCommitRevision;
            self.suspended = Some(SuspendedSave { request, cause });
            return Ok(DraftCompletionAction::Suspended(cause));
        }
        let request_timer_generation = request.token_ref().timer_generation();
        let next_timer_generation = (request_timer_generation == self.timer_generation)
            .then(|| self.timer_generation.next())
            .transpose()?;
        self.durable = DurableDraftBase {
            revision,
            payload: request.payload().clone(),
            updated_at: request.updated_at(),
        };
        if let Some(timer_generation) = next_timer_generation {
            self.timer_generation = timer_generation;
            self.timer_anchor = completed_at;
        }
        if self.flush_pending && self.is_dirty() {
            return self.start_request().map(DraftCompletionAction::Chained);
        }
        let flush_complete = std::mem::take(&mut self.flush_pending);
        Ok(DraftCompletionAction::Published { flush_complete })
    }

    pub fn reconcile(
        &mut self,
        seed: DraftPersistenceSeed,
    ) -> Result<DraftReconciliationAction, DraftPersistenceError> {
        let suspended = self
            .suspended
            .as_ref()
            .ok_or(DraftPersistenceError::NotSuspended)?;
        self.validate_reconciliation_seed(&seed, suspended)?;
        let retry_conflict = suspended.cause == DraftSuspensionCause::RevisionConflict;
        let request_timer_generation = suspended.request.token_ref().timer_generation();
        let binding_generation = self.binding.generation().next()?;
        let next_timer_generation = (request_timer_generation == self.timer_generation)
            .then(|| self.timer_generation.next())
            .transpose()?;
        let draft = seed.current().draft();
        self.binding = self
            .binding
            .recovered(seed.home_generation(), binding_generation);
        self.durable = DurableDraftBase {
            revision: draft.revision(),
            payload: seed.payload().clone(),
            updated_at: draft.updated_at(),
        };
        if let Some(timer_generation) = next_timer_generation {
            self.timer_generation = timer_generation;
            self.timer_anchor = seed.published_at();
        }
        self.suspended = None;
        if self.is_dirty() && (self.flush_pending || retry_conflict) {
            return self.start_request().map(DraftReconciliationAction::Chained);
        }
        if std::mem::take(&mut self.flush_pending) {
            return Ok(DraftReconciliationAction::FlushComplete);
        }
        Ok(DraftReconciliationAction::Ready)
    }

    fn validate_reconciliation_seed(
        &self,
        seed: &DraftPersistenceSeed,
        suspended: &SuspendedSave,
    ) -> Result<(), DraftPersistenceError> {
        if seed.home_id() != self.binding.home_id() {
            return Err(DraftPersistenceError::ForeignHome);
        }
        if seed.home_generation() < self.binding.home_generation() {
            return Err(DraftPersistenceError::StaleHomeGeneration);
        }
        let current = seed.current();
        let draft = current.draft();
        if current.thread().id() != self.binding.thread_id()
            || draft.thread_id() != self.binding.thread_id()
            || draft.id() != self.binding.draft_id()
        {
            return Err(DraftPersistenceError::ForeignDraft);
        }
        if !self.immutable_shape.matches(draft) {
            return Err(DraftPersistenceError::ChangedImmutableDraft);
        }
        if !matches!(
            suspended.cause,
            DraftSuspensionCause::AmbiguousStorageFailure | DraftSuspensionCause::RevisionConflict
        ) {
            return Ok(());
        }
        let old = draft.revision() == self.durable.revision
            && seed.payload() == &self.durable.payload
            && draft.updated_at() == self.durable.updated_at;
        let new = suspended
            .request
            .expected_revision()
            .checked_next()
            .is_ok_and(|revision| revision == draft.revision())
            && suspended.request.payload() == seed.payload()
            && suspended.request.updated_at() == draft.updated_at();
        if old || new {
            Ok(())
        } else {
            Err(
                if suspended.cause == DraftSuspensionCause::RevisionConflict {
                    DraftPersistenceError::UnexplainedConflictState
                } else {
                    DraftPersistenceError::UnexplainedAmbiguousState
                },
            )
        }
    }

    fn start_request(&mut self) -> Result<DraftSaveRequest, DraftPersistenceError> {
        debug_assert!(self.in_flight.is_none());
        debug_assert!(self.suspended.is_none());
        let generation = match self.last_request_generation {
            None => DraftRequestGeneration::FIRST,
            Some(previous) => previous.next()?,
        };
        let request = DraftSaveRequest::new(
            self.binding,
            self.durable.revision,
            self.editor_payload.clone(),
            self.editor_updated_at,
            self.edit_generation,
            self.timer_generation,
            generation,
        );
        self.last_request_generation = Some(generation);
        self.in_flight = Some(InFlightSave {
            request: request.clone(),
        });
        Ok(request)
    }
}

#[cfg(test)]
mod tests;
