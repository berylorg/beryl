use std::sync::Arc;

use beryl_model::{CasThreadId, CasTurnId};

use super::{
    EventRouter, LiveEventTargetCloseReason, TargetAuthorizationFailure, TargetInvalidation,
    TargetPublication, TargetRegistrationProof, TargetTurn,
    state::advance_revision,
    target::{close_target, invalidation},
};

/// Ordered compact control selected for one exact pre-turn compaction target.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(in crate::cas_projection) enum CompactionControlPublication {
    ThreadStatus,
    TurnStarted(CasTurnId),
}

/// Non-cloneable router fence held while one compaction control becomes durable.
pub(in crate::cas_projection) struct CompactionControlPermit {
    router: Arc<EventRouter>,
    thread_id: CasThreadId,
    registration: u64,
    publication: CompactionControlPublication,
    authority: crate::cas_projection::context_compaction::ContextCompactionTargetAuthority,
    command: crate::cas_projection::LiveCommandPermit,
    finished: bool,
}

impl CompactionControlPermit {
    pub(in crate::cas_projection) const fn authority(
        &self,
    ) -> crate::cas_projection::context_compaction::ContextCompactionTargetAuthority {
        self.authority
    }

    pub(in crate::cas_projection) fn finish(mut self) -> Result<(), CompactionControlPermitError> {
        let result = self.router.finish_compaction_control(&self, true);
        self.finished = true;
        result.map(|_| ())
    }

    pub(in crate::cas_projection) fn fail(
        mut self,
    ) -> Result<TargetInvalidation, CompactionControlPermitError> {
        let result = self.router.finish_compaction_control(&self, false);
        self.finished = true;
        result?.ok_or(CompactionControlPermitError::Router)
    }
}

impl Drop for CompactionControlPermit {
    fn drop(&mut self) {
        if !self.finished {
            let _ = self.router.finish_compaction_control(self, false);
        }
    }
}

#[derive(Debug)]
pub(in crate::cas_projection) enum CompactionControlPermitError {
    Unmatched,
    Target(TargetInvalidation),
    Router,
}

impl EventRouter {
    pub(in crate::cas_projection) fn authorize_context_compaction_command(
        &self,
        registration: &TargetRegistrationProof,
    ) -> Result<(), TargetAuthorizationFailure> {
        let command = self
            .commands
            .authorize()
            .map_err(|_| TargetAuthorizationFailure::Router)?;
        let mut state = self
            .state
            .lock()
            .map_err(|_| TargetAuthorizationFailure::Router)?;
        command
            .commit_if_current(|| {
                if state.retired.is_some() || state.persistent_failure.is_some() {
                    return Err(TargetAuthorizationFailure::Router);
                }
                let Some(target) = state.targets.get_mut(&registration.key.cas_thread_id) else {
                    return Err(TargetAuthorizationFailure::Target(
                        registration
                            .terminal_reason()
                            .unwrap_or(LiveEventTargetCloseReason::WorkerStopped),
                    ));
                };
                if target.registration != registration.registration
                    || target.owner != registration.owner
                    || target.loaded_generation != registration.loaded_generation
                    || target.compaction != registration.compaction
                    || target.compaction.is_none()
                    || target.turn_state != TargetTurn::AwaitingCompactionTurn
                    || target.start_dispatched
                    || target.publication_in_flight.is_some()
                    || target.publication_closing.is_some()
                    || target.loss_requested
                {
                    return Err(TargetAuthorizationFailure::Target(
                        LiveEventTargetCloseReason::DuplicateTurnStart,
                    ));
                }
                target.start_dispatched = true;
                advance_revision(&mut state);
                Ok(())
            })
            .unwrap_or(Err(TargetAuthorizationFailure::Router))
    }

    pub(in crate::cas_projection) fn acquire_compaction_thread_status(
        self: &Arc<Self>,
        thread_id: &CasThreadId,
    ) -> Result<CompactionControlPermit, CompactionControlPermitError> {
        self.acquire_compaction_control(thread_id, CompactionControlPublication::ThreadStatus)
    }

    pub(in crate::cas_projection) fn acquire_compaction_turn_started(
        self: &Arc<Self>,
        thread_id: &CasThreadId,
        turn_id: &CasTurnId,
    ) -> Result<CompactionControlPermit, CompactionControlPermitError> {
        self.acquire_compaction_control(
            thread_id,
            CompactionControlPublication::TurnStarted(turn_id.clone()),
        )
    }

    fn acquire_compaction_control(
        self: &Arc<Self>,
        thread_id: &CasThreadId,
        publication: CompactionControlPublication,
    ) -> Result<CompactionControlPermit, CompactionControlPermitError> {
        let command = self
            .commands
            .authorize()
            .map_err(|_| CompactionControlPermitError::Router)?;
        let mut state = self
            .state
            .lock()
            .map_err(|_| CompactionControlPermitError::Router)?;
        let admission = command
            .commit_if_current(|| {
                if state.retired.is_some() || state.persistent_failure.is_some() {
                    return Err(CompactionControlPermitError::Router);
                }
                let Some(target) = state.targets.get_mut(thread_id) else {
                    return Err(CompactionControlPermitError::Unmatched);
                };
                if target.compaction.is_none() {
                    return Err(CompactionControlPermitError::Unmatched);
                }
                let valid_state = match &publication {
                    CompactionControlPublication::ThreadStatus => matches!(
                        target.turn_state,
                        TargetTurn::AwaitingCompactionTurn | TargetTurn::Exact
                    ),
                    CompactionControlPublication::TurnStarted(turn_id) => {
                        target.turn_state == TargetTurn::AwaitingCompactionTurn
                            || (target.turn_state == TargetTurn::Exact
                                && target.turn_id.as_ref() == Some(turn_id))
                    }
                };
                if !target.start_dispatched
                    || !valid_state
                    || target.publication_in_flight.is_some()
                    || target.publication_closing.is_some()
                    || target.loss_requested
                {
                    let reason = LiveEventTargetCloseReason::SourcePublicationRouteUnavailable;
                    let invalidation = invalidation(target, reason);
                    close_target(&mut state, thread_id, reason);
                    return Err(CompactionControlPermitError::Target(invalidation));
                }
                target.publication_in_flight = Some(TargetPublication::CompactionControl);
                let admission = (
                    target.registration,
                    target
                        .compaction
                        .expect("validated compaction target retains durable authority"),
                );
                advance_revision(&mut state);
                Ok(admission)
            })
            .unwrap_or(Err(CompactionControlPermitError::Router))?;
        Ok(CompactionControlPermit {
            router: Arc::clone(self),
            thread_id: thread_id.clone(),
            registration: admission.0,
            publication,
            authority: admission.1,
            command,
            finished: false,
        })
    }

    fn finish_compaction_control(
        &self,
        permit: &CompactionControlPermit,
        published: bool,
    ) -> Result<Option<TargetInvalidation>, CompactionControlPermitError> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| CompactionControlPermitError::Router)?;
        let result = permit
            .command
            .commit_if_current(|| finish_compaction_locked(&mut state, permit, published))
            .unwrap_or(Err(CompactionControlPermitError::Router));
        drop(state);
        if result.is_ok() {
            self.publication_changed.notify_all();
        }
        result
    }
}

fn finish_compaction_locked(
    state: &mut super::RouterState,
    permit: &CompactionControlPermit,
    published: bool,
) -> Result<Option<TargetInvalidation>, CompactionControlPermitError> {
    if state.persistent_failure.is_some() {
        return Err(CompactionControlPermitError::Router);
    }
    let valid = state.targets.get(&permit.thread_id).is_some_and(|target| {
        target.registration == permit.registration
            && target.compaction == Some(permit.authority)
            && target.publication_in_flight == Some(TargetPublication::CompactionControl)
    });
    if !valid {
        return Err(CompactionControlPermitError::Router);
    }
    let retired = state.retired;
    let target = state
        .targets
        .get_mut(&permit.thread_id)
        .expect("validated compaction target remains registered");
    target.publication_in_flight = None;
    let closing = if published {
        target.publication_closing.or(retired)
    } else {
        Some(
            target
                .publication_closing
                .or(retired)
                .unwrap_or(LiveEventTargetCloseReason::SourcePublicationFailed),
        )
    };
    if published && let CompactionControlPublication::TurnStarted(turn_id) = &permit.publication {
        target.turn_state = TargetTurn::Exact;
        target.turn_id = Some(turn_id.clone());
        *target
            .bound_turn
            .lock()
            .unwrap_or_else(|poison| poison.into_inner()) = Some(turn_id.clone());
    }
    let invalidated = closing.map(|reason| invalidation(target, reason));
    if published {
        state.routed_operation_count = state.routed_operation_count.saturating_add(1);
    }
    if let Some(reason) = closing {
        close_target(state, &permit.thread_id, reason);
    } else {
        advance_revision(state);
    }
    Ok(invalidated)
}
