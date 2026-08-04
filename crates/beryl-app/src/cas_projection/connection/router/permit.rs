use std::sync::Arc;

use beryl_model::{CasLoadedSessionGeneration, CasThreadId, CasTurnId, SyndicThreadId};

use super::{
    EventRouter, LiveEventTargetCloseReason, ProvenTerminalOutcome,
    SourcePublicationPermitError::*,
    TargetInvalidation, TargetPublication, TargetTurn,
    state::advance_revision,
    target::{close_target, invalidation, retire_router_state, set_proven_terminal},
};

/// Non-cloneable authority to publish one ordered source operation for an exact target.
pub(in crate::cas_projection) struct SourcePublicationPermit {
    router: Arc<EventRouter>,
    thread_id: CasThreadId,
    turn_id: CasTurnId,
    registration: u64,
    owner: SyndicThreadId,
    loaded_generation: CasLoadedSessionGeneration,
    home_generation: u64,
    activation: Option<crate::cas_projection::PendingTurnActivation>,
    compaction: Option<crate::cas_projection::context_compaction::ContextCompactionTargetAuthority>,
    command: Option<crate::cas_projection::LiveCommandPermit>,
    finished: bool,
}

/// Non-cloneable post-commit fence retained while process-local source effects settle.
pub(in crate::cas_projection) struct SourcePublicationPostCommit {
    router: Arc<EventRouter>,
    thread_id: CasThreadId,
    registration: u64,
    target_became_ready: bool,
    command: crate::cas_projection::LiveCommandPermit,
    released: bool,
}

impl SourcePublicationPermit {
    pub(in crate::cas_projection) const fn syndic_thread_id(&self) -> SyndicThreadId {
        self.owner
    }

    pub(in crate::cas_projection) const fn loaded_generation(&self) -> CasLoadedSessionGeneration {
        self.loaded_generation
    }

    pub(in crate::cas_projection) const fn cas_thread_id(&self) -> &CasThreadId {
        &self.thread_id
    }

    pub(in crate::cas_projection) const fn cas_turn_id(&self) -> &CasTurnId {
        &self.turn_id
    }

    pub(in crate::cas_projection) fn pending_syndic_turn_id(
        &self,
    ) -> Option<beryl_model::SyndicTurnId> {
        self.activation
            .as_ref()
            .map(crate::cas_projection::PendingTurnActivation::turn_id)
    }

    pub(in crate::cas_projection) const fn compaction(
        &self,
    ) -> Option<crate::cas_projection::context_compaction::ContextCompactionTargetAuthority> {
        self.compaction
    }

    pub(in crate::cas_projection) fn home_generation(
        &self,
        home: &beryl_home_store::HomeStore,
    ) -> Option<beryl_home_store::HomeGeneration> {
        let health = home.health();
        (health.state() == beryl_home_store::HomeHealthState::Healthy)
            .then(|| health.generation())
            .flatten()
            .filter(|generation| generation.get() == self.home_generation)
    }

    pub(in crate::cas_projection) fn admitted_route(
        &self,
    ) -> syndic_storage::ProviderObservationRoute {
        syndic_storage::ProviderObservationRoute::new(self.thread_id.clone(), self.turn_id.clone())
    }

    pub(in crate::cas_projection) fn active_turn_request(
        &self,
    ) -> Option<syndic_storage::PublishActiveCasTurn> {
        self.activation
            .as_ref()
            .map(|activation| activation.active_turn(self.thread_id.clone(), self.turn_id.clone()))
    }

    pub(in crate::cas_projection) fn activation_event(
        &self,
        published_gate_revision: beryl_model::InputGateRevision,
    ) -> Result<Option<syndic_storage::LiveSourceEvent>, syndic_storage::SyndicRecordError> {
        self.activation
            .as_ref()
            .map(|activation| {
                activation.activation_event(
                    self.thread_id.clone(),
                    self.turn_id.clone(),
                    published_gate_revision,
                )
            })
            .transpose()
    }

    pub(in crate::cas_projection) fn finish(mut self) -> Result<(), SourcePublicationFinishError> {
        let result = self
            .router
            .finish_source_publication(&self, SourcePublicationCompletion::Published);
        self.finished = true;
        match result? {
            SourcePublicationResolution::Released(_) => Ok(()),
            SourcePublicationResolution::Held { .. } => Err(SourcePublicationFinishError::Router),
        }
    }

    pub(in crate::cas_projection) fn finish_held(
        mut self,
    ) -> Result<SourcePublicationPostCommit, SourcePublicationFinishError> {
        let result = self
            .router
            .finish_source_publication(&self, SourcePublicationCompletion::PublishedHeld);
        self.finished = true;
        match result? {
            SourcePublicationResolution::Held {
                target_became_ready,
            } => Ok(SourcePublicationPostCommit {
                router: Arc::clone(&self.router),
                thread_id: self.thread_id.clone(),
                registration: self.registration,
                target_became_ready,
                command: self
                    .command
                    .take()
                    .expect("held source publication retains its live command"),
                released: false,
            }),
            SourcePublicationResolution::Released(_) => Err(SourcePublicationFinishError::Router),
        }
    }

    pub(in crate::cas_projection) fn finish_terminal(
        mut self,
        outcome: ProvenTerminalOutcome,
    ) -> Result<(), SourcePublicationFinishError> {
        let result = self
            .router
            .finish_source_publication(&self, SourcePublicationCompletion::Terminal(outcome));
        self.finished = true;
        match result? {
            SourcePublicationResolution::Released(_) => Ok(()),
            SourcePublicationResolution::Held { .. } => Err(SourcePublicationFinishError::Router),
        }
    }

    pub(in crate::cas_projection) fn fail(
        mut self,
    ) -> Result<TargetInvalidation, SourcePublicationFinishError> {
        let result = self
            .router
            .finish_source_publication(&self, SourcePublicationCompletion::Failed);
        self.finished = true;
        match result {
            Err(error) => Err(error),
            Ok(SourcePublicationResolution::Released(Some(invalidation))) => Ok(invalidation),
            Ok(
                SourcePublicationResolution::Released(None)
                | SourcePublicationResolution::Held { .. },
            ) => Err(SourcePublicationFinishError::Router),
        }
    }

    /// Consumes provider publication authority after an already-typed service-authority loss.
    ///
    /// This releases the process-local publication owner and nested live-command permit without
    /// claiming that publication succeeded and without converting authority loss into target
    /// failure. Ordinary abandoned permits retain their fail-closed `Drop` behavior.
    pub(in crate::cas_projection) fn settle_authority_lost(mut self) {
        let _ = self
            .router
            .finish_source_publication(&self, SourcePublicationCompletion::AuthorityLost);
        self.finished = true;
        self.command
            .take()
            .expect("source publication authority retains its live command")
            .release_after_authority_loss();
    }
}

impl SourcePublicationPostCommit {
    pub(in crate::cas_projection) fn release(mut self) {
        self.router.release_source_publication(&self);
        self.released = true;
    }
}

impl Drop for SourcePublicationPostCommit {
    fn drop(&mut self) {
        if !self.released {
            self.router.release_source_publication(self);
        }
    }
}

impl Drop for SourcePublicationPermit {
    fn drop(&mut self) {
        if !self.finished {
            let _ = self
                .router
                .finish_source_publication(self, SourcePublicationCompletion::Failed);
        }
    }
}

#[derive(Debug)]
pub(in crate::cas_projection) enum SourcePublicationPermitError {
    Unmatched,
    Target(TargetInvalidation),
    Router,
}

#[derive(Debug)]
pub(in crate::cas_projection) enum SourcePublicationFinishError {
    Target(TargetInvalidation),
    Router,
}

#[derive(Clone, Copy, Debug)]
enum SourcePublicationCompletion {
    Published,
    PublishedHeld,
    Terminal(ProvenTerminalOutcome),
    AuthorityLost,
    Failed,
}

enum SourcePublicationResolution {
    Released(Option<TargetInvalidation>),
    Held { target_became_ready: bool },
}

struct SourcePublicationFinishEffect {
    result: Result<SourcePublicationResolution, SourcePublicationFinishError>,
    notify: bool,
    wake_scheduler: bool,
}

struct SourcePublicationAdmission {
    registration: u64,
    owner: SyndicThreadId,
    loaded_generation: CasLoadedSessionGeneration,
    home_generation: u64,
    activation: Option<crate::cas_projection::PendingTurnActivation>,
    compaction: Option<crate::cas_projection::context_compaction::ContextCompactionTargetAuthority>,
}

fn fail_busy_terminal_publication(
    state: &mut super::RouterState,
    thread_id: &CasThreadId,
) -> Result<SourcePublicationPermit, SourcePublicationPermitError> {
    let target = state.targets.get(thread_id).ok_or(Unmatched)?;
    let invalidation = invalidation(
        target,
        LiveEventTargetCloseReason::SourcePublicationRouteUnavailable,
    );
    close_target(
        state,
        thread_id,
        LiveEventTargetCloseReason::SourcePublicationRouteUnavailable,
    );
    Err(Target(invalidation))
}

fn admit_source_publication_locked(
    state: &mut super::RouterState,
    thread_id: &CasThreadId,
    turn_id: &CasTurnId,
) -> Result<SourcePublicationAdmission, SourcePublicationPermitError> {
    if state.retired.is_some() || state.persistent_failure.is_some() {
        return Err(Router);
    }
    let Some(target) = state.targets.get_mut(thread_id) else {
        return Err(Unmatched);
    };
    if target.loss_requested {
        return Err(Unmatched);
    }
    let close_reason = match target.turn_state {
        TargetTurn::AwaitingStart
            if !target.start_dispatched || target.pending_activation.is_none() =>
        {
            Some(LiveEventTargetCloseReason::EventBeforeTurnStart)
        }
        TargetTurn::AwaitingStart => None,
        TargetTurn::AwaitingCompactionTurn => {
            Some(LiveEventTargetCloseReason::EventBeforeTurnStart)
        }
        TargetTurn::Terminal => Some(LiveEventTargetCloseReason::EventAfterTurnCompletion),
        TargetTurn::Exact if target.turn_id.as_ref() != Some(turn_id) => {
            Some(LiveEventTargetCloseReason::ConflictingTurnIdentity)
        }
        TargetTurn::Exact
            if target.publication_closing.is_some() || target.publication_in_flight.is_some() =>
        {
            Some(LiveEventTargetCloseReason::SourcePublicationRouteUnavailable)
        }
        TargetTurn::Exact => None,
    };
    if let Some(reason) = close_reason {
        let invalidation = invalidation(target, reason);
        close_target(state, thread_id, reason);
        return Err(Target(invalidation));
    }
    if target.turn_state == TargetTurn::AwaitingStart {
        target.turn_state = TargetTurn::Exact;
        target.turn_id = Some(turn_id.clone());
        *target
            .bound_turn
            .lock()
            .unwrap_or_else(|poison| poison.into_inner()) = Some(turn_id.clone());
    }
    let activation = (!target.activation_durable)
        .then(|| target.pending_activation.clone())
        .flatten();
    if !target.activation_durable && activation.is_none() {
        let reason = LiveEventTargetCloseReason::SourcePublicationRouteUnavailable;
        let invalidation = invalidation(target, reason);
        close_target(state, thread_id, reason);
        return Err(Target(invalidation));
    }
    target.publication_in_flight = Some(TargetPublication::Source);
    let admission = SourcePublicationAdmission {
        registration: target.registration,
        owner: target.owner,
        loaded_generation: target.loaded_generation,
        home_generation: target.home_generation,
        activation,
        compaction: target.compaction,
    };
    advance_revision(state);
    Ok(admission)
}

impl EventRouter {
    pub(in crate::cas_projection) fn acquire_source_publication(
        self: &Arc<Self>,
        thread_id: &CasThreadId,
        turn_id: &CasTurnId,
    ) -> Result<SourcePublicationPermit, SourcePublicationPermitError> {
        self.acquire_source_publication_inner(thread_id, turn_id, false)
    }

    pub(in crate::cas_projection) fn acquire_terminal_source_publication(
        self: &Arc<Self>,
        thread_id: &CasThreadId,
        turn_id: &CasTurnId,
    ) -> Result<SourcePublicationPermit, SourcePublicationPermitError> {
        self.acquire_source_publication_inner(thread_id, turn_id, true)
    }

    fn acquire_source_publication_inner(
        self: &Arc<Self>,
        thread_id: &CasThreadId,
        turn_id: &CasTurnId,
        terminal: bool,
    ) -> Result<SourcePublicationPermit, SourcePublicationPermitError> {
        let admission_check = self.commands.authorize().map_err(|_| Router)?;
        let mut state = self.state.lock().map_err(|_| Router)?;
        let request_timeout = admission_check
            .commit_if_current(|| {
                if state.retired.is_some() || state.persistent_failure.is_some() {
                    return Err(Router);
                }
                Ok(state
                    .targets
                    .get(thread_id)
                    .ok_or(Unmatched)?
                    .request_timeout)
            })
            .unwrap_or(Err(Router))?;
        drop(admission_check);
        let deadline = std::time::Instant::now()
            .checked_add(request_timeout)
            .ok_or(Router)?;
        while terminal
            && state.active_stop_election.as_ref().is_some_and(|active| {
                active.thread_id == *thread_id
                    && state
                        .targets
                        .get(thread_id)
                        .is_some_and(|target| target.registration == active.registration)
            })
        {
            #[cfg(test)]
            self.publish_terminal_publication_wait_for_test();
            let Some(remaining) = deadline.checked_duration_since(std::time::Instant::now()) else {
                let command = self.commands.authorize().map_err(|_| Router)?;
                return command
                    .commit_if_current(|| fail_busy_terminal_publication(&mut state, thread_id))
                    .unwrap_or(Err(Router));
            };
            let (next, wait) = self
                .publication_changed
                .wait_timeout(state, remaining)
                .map_err(|_| Router)?;
            state = next;
            if wait.timed_out() {
                let command = self.commands.authorize().map_err(|_| Router)?;
                return command
                    .commit_if_current(|| fail_busy_terminal_publication(&mut state, thread_id))
                    .unwrap_or(Err(Router));
            }
            if !self.commands.is_open()
                || state.retired.is_some()
                || state.persistent_failure.is_some()
            {
                return Err(Router);
            }
        }
        let command = self.commands.authorize().map_err(|_| Router)?;
        let admission = command
            .commit_if_current(|| admit_source_publication_locked(&mut state, thread_id, turn_id))
            .unwrap_or(Err(Router))?;
        Ok(SourcePublicationPermit {
            router: Arc::clone(self),
            thread_id: thread_id.clone(),
            turn_id: turn_id.clone(),
            registration: admission.registration,
            owner: admission.owner,
            loaded_generation: admission.loaded_generation,
            home_generation: admission.home_generation,
            activation: admission.activation,
            compaction: admission.compaction,
            command: Some(command),
            finished: false,
        })
    }

    #[cfg(test)]
    pub(in crate::cas_projection) fn observe_next_terminal_publication_wait_for_test(
        &self,
        observer: std::sync::mpsc::SyncSender<()>,
    ) {
        *self
            .terminal_publication_wait_observer
            .lock()
            .expect("terminal-publication test observer mutex remains healthy") = Some(observer);
    }

    #[cfg(test)]
    fn publish_terminal_publication_wait_for_test(&self) {
        let observer = self
            .terminal_publication_wait_observer
            .lock()
            .expect("terminal-publication test observer mutex remains healthy")
            .take();
        if let Some(observer) = observer {
            let _ = observer.send(());
        }
    }

    fn finish_source_publication(
        &self,
        permit: &SourcePublicationPermit,
        completion: SourcePublicationCompletion,
    ) -> Result<SourcePublicationResolution, SourcePublicationFinishError> {
        if permit.command.is_none() {
            return Err(SourcePublicationFinishError::Router);
        }
        let effect = match self.settle_long_lived_authority(|state| {
            finish_source_publication_locked(state, permit, completion)
        }) {
            super::LongLivedAuthoritySettlement::Settled(effect) => effect,
            super::LongLivedAuthoritySettlement::PreservedForPersistentFailure
            | super::LongLivedAuthoritySettlement::Unavailable => {
                return Err(SourcePublicationFinishError::Router);
            }
        };
        if effect.notify {
            self.publication_changed.notify_all();
        }
        if effect.wake_scheduler {
            self.scheduler_signal.wake(
                crate::cas_projection::accepted_input_scheduler::AcceptedInputWakeReason::TargetReady,
            );
        }
        effect.result
    }

    fn release_source_publication(&self, release: &SourcePublicationPostCommit) {
        let became_ready = match self
            .settle_long_lived_authority(|state| release_source_publication_locked(state, release))
        {
            super::LongLivedAuthoritySettlement::Settled(became_ready) => became_ready,
            super::LongLivedAuthoritySettlement::PreservedForPersistentFailure
            | super::LongLivedAuthoritySettlement::Unavailable => return,
        };
        self.publication_changed.notify_all();
        if became_ready.unwrap_or(false) {
            self.scheduler_signal.wake(
                crate::cas_projection::accepted_input_scheduler::AcceptedInputWakeReason::TargetReady,
            );
        }
    }
}

fn finish_source_publication_locked(
    state: &mut super::RouterState,
    permit: &SourcePublicationPermit,
    completion: SourcePublicationCompletion,
) -> SourcePublicationFinishEffect {
    if state.persistent_failure.is_some() {
        return SourcePublicationFinishEffect {
            result: Err(SourcePublicationFinishError::Router),
            notify: false,
            wake_scheduler: false,
        };
    }
    let valid = state.targets.get(&permit.thread_id).is_some_and(|target| {
        target.registration == permit.registration
            && target.owner == permit.owner
            && target.loaded_generation == permit.loaded_generation
            && target.home_generation == permit.home_generation
            && target.turn_id.as_ref() == Some(&permit.turn_id)
            && target.publication_in_flight == Some(TargetPublication::Source)
    });
    if !valid {
        if matches!(completion, SourcePublicationCompletion::AuthorityLost) {
            return SourcePublicationFinishEffect {
                result: Err(SourcePublicationFinishError::Router),
                notify: false,
                wake_scheduler: false,
            };
        }
        if let Some(target) = state.targets.get_mut(&permit.thread_id) {
            target.publication_in_flight = None;
        }
        retire_router_state(state, LiveEventTargetCloseReason::StreamFailure);
        return SourcePublicationFinishEffect {
            result: Err(SourcePublicationFinishError::Router),
            notify: true,
            wake_scheduler: false,
        };
    }
    let target = state
        .targets
        .get_mut(&permit.thread_id)
        .expect("validated source target remains registered");
    if matches!(completion, SourcePublicationCompletion::AuthorityLost) {
        target.publication_in_flight = None;
        advance_revision(state);
        return SourcePublicationFinishEffect {
            result: Ok(SourcePublicationResolution::Released(None)),
            notify: true,
            wake_scheduler: false,
        };
    }
    let was_ready = target.turn_state == TargetTurn::Exact
        && target.start_dispatched
        && target.activation_durable
        && target.compaction.is_none()
        && target.publication_closing.is_none()
        && !target.loss_requested;
    let published = !matches!(completion, SourcePublicationCompletion::Failed);
    let hold_after_commit = matches!(completion, SourcePublicationCompletion::PublishedHeld);
    let closing = match completion {
        SourcePublicationCompletion::Published
        | SourcePublicationCompletion::PublishedHeld
        | SourcePublicationCompletion::Terminal(_)
        | SourcePublicationCompletion::AuthorityLost => target.publication_closing,
        SourcePublicationCompletion::Failed => Some(
            target
                .publication_closing
                .unwrap_or(LiveEventTargetCloseReason::SourcePublicationFailed),
        ),
    };
    let target_failure = closing.map(|reason| invalidation(target, reason));
    if let (None, SourcePublicationCompletion::Terminal(outcome)) = (closing, completion) {
        if !set_proven_terminal(&target.terminal, outcome) {
            target.publication_in_flight = None;
            retire_router_state(state, LiveEventTargetCloseReason::StreamFailure);
            return SourcePublicationFinishEffect {
                result: Err(SourcePublicationFinishError::Router),
                notify: true,
                wake_scheduler: false,
            };
        }
        target.turn_state = TargetTurn::Terminal;
        target.sender.take();
    }
    if published && permit.activation.is_some() {
        target.activation_durable = true;
    }
    let became_ready = !was_ready
        && target.turn_state == TargetTurn::Exact
        && target.start_dispatched
        && target.activation_durable
        && target.compaction.is_none()
        && target.publication_closing.is_none()
        && !target.loss_requested
        && !matches!(completion, SourcePublicationCompletion::Terminal(_));
    let held = hold_after_commit && closing.is_none();
    if !held {
        target.publication_in_flight = None;
    }
    if published {
        state.routed_operation_count = state.routed_operation_count.saturating_add(1);
    }
    if let Some(reason) = closing {
        close_target(state, &permit.thread_id, reason);
    } else {
        advance_revision(state);
    }
    let result = match target_failure {
        Some(invalidation) if published => Err(SourcePublicationFinishError::Target(invalidation)),
        Some(invalidation) => Ok(SourcePublicationResolution::Released(Some(invalidation))),
        None if held => Ok(SourcePublicationResolution::Held {
            target_became_ready: became_ready,
        }),
        None => Ok(SourcePublicationResolution::Released(None)),
    };
    SourcePublicationFinishEffect {
        result,
        notify: !held,
        wake_scheduler: became_ready && !held,
    }
}

fn release_source_publication_locked(
    state: &mut super::RouterState,
    release: &SourcePublicationPostCommit,
) -> Option<bool> {
    if state.persistent_failure.is_some() {
        return None;
    }
    let valid = state.targets.get(&release.thread_id).is_some_and(|target| {
        target.registration == release.registration
            && target.publication_in_flight == Some(TargetPublication::Source)
    });
    if !valid {
        return None;
    }
    let retired = state.retired;
    let target = state
        .targets
        .get_mut(&release.thread_id)
        .expect("validated post-commit source target remains registered");
    let closing = target.publication_closing.or(retired);
    let became_ready = release.target_became_ready
        && closing.is_none()
        && target.turn_state == TargetTurn::Exact
        && target.start_dispatched
        && target.activation_durable
        && target.compaction.is_none()
        && !target.loss_requested;
    target.publication_in_flight = None;
    if let Some(reason) = closing {
        close_target(state, &release.thread_id, reason);
    } else {
        advance_revision(state);
    }
    Some(became_ready)
}
