use super::{
    EventRouter, LiveEventTargetCloseReason, LongLivedAuthoritySettlement, RouterState,
    TargetRegistration,
    target::{close_target, fence_thread_lane, retire_router_state},
};
use crate::cas_projection::ProjectionCoordinatorError;
use beryl_model::{CasProcessGeneration, CasThreadId, RuntimeId};

pub(in crate::cas_projection::connection) enum TargetProjectionDropSettlement {
    Ordinary {
        projection: crate::cas_projection::LoadedCasProjection,
        connection_retired: bool,
    },
    PersistentFailure,
    Unavailable(crate::cas_projection::LoadedCasProjection),
}

impl EventRouter {
    /// Settles one exact long-lived router owner on the ordinary side of the master cut.
    ///
    /// Router authority is acquired before the gate mutex. The transition must remain a bounded
    /// in-memory mutation and must not wait, perform I/O, or reacquire router authority.
    pub(super) fn settle_long_lived_authority<T>(
        &self,
        settle: impl FnOnce(&mut RouterState) -> T,
    ) -> LongLivedAuthoritySettlement<T> {
        let mut state = match self.state.lock() {
            Ok(state) => state,
            Err(_) => return LongLivedAuthoritySettlement::Unavailable,
        };
        let state = std::cell::RefCell::new(&mut *state);
        let settle = std::cell::RefCell::new(Some(settle));
        let settle_ordinary = || {
            let settle = settle
                .borrow_mut()
                .take()
                .expect("one gate branch settles long-lived router authority");
            LongLivedAuthoritySettlement::Settled(settle(&mut **state.borrow_mut()))
        };
        self.commands
            .settle_authority(
                settle_ordinary,
                |_| LongLivedAuthoritySettlement::PreservedForPersistentFailure,
                settle_ordinary,
            )
            .unwrap_or(LongLivedAuthoritySettlement::Unavailable)
    }

    #[cfg(test)]
    pub(in crate::cas_projection) fn authorize_command_for_test(
        &self,
    ) -> Result<
        crate::cas_projection::LiveCommandPermit,
        crate::cas_projection::LiveCommandAdmissionError,
    > {
        self.commands.authorize()
    }

    #[cfg(test)]
    pub(in crate::cas_projection) fn new(
        runtime_id: RuntimeId,
        process_generation: CasProcessGeneration,
        connection_generation: u64,
    ) -> Result<Self, ProjectionCoordinatorError> {
        let gate = crate::cas_projection::persistent_failure::MasterCommandGate::new(
            crate::cas_projection::ProjectionServiceGeneration::allocate()
                .map_err(|_| ProjectionCoordinatorError::ProjectionServiceGenerationExhausted)?,
            None,
        );
        Self::new_with_scheduler(
            runtime_id,
            process_generation,
            connection_generation,
            crate::cas_projection::accepted_input_scheduler::AcceptedInputSchedulerSignal::new(),
            gate.authorizer(),
            None,
        )
    }

    pub(in crate::cas_projection) fn new_with_scheduler(
        runtime_id: RuntimeId,
        process_generation: CasProcessGeneration,
        connection_generation: u64,
        scheduler_signal: crate::cas_projection::accepted_input_scheduler::AcceptedInputSchedulerSignal,
        commands: crate::cas_projection::LiveCommandAuthorizer,
        terminal_disposer: Option<
            crate::cas_projection::persistent_failure::PersistentFailureTerminalDisposer,
        >,
    ) -> Result<Self, ProjectionCoordinatorError> {
        let process = super::process::ConnectionProcessFact::register(
            runtime_id,
            process_generation,
            connection_generation,
        )?;
        Self::new_with_process(
            runtime_id,
            process_generation,
            connection_generation,
            scheduler_signal,
            commands,
            terminal_disposer,
            process.observe(),
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub(in crate::cas_projection::connection) fn new_with_process(
        runtime_id: RuntimeId,
        process_generation: CasProcessGeneration,
        connection_generation: u64,
        scheduler_signal: crate::cas_projection::accepted_input_scheduler::AcceptedInputSchedulerSignal,
        commands: crate::cas_projection::LiveCommandAuthorizer,
        terminal_disposer: Option<
            crate::cas_projection::persistent_failure::PersistentFailureTerminalDisposer,
        >,
        process: super::process::ProcessEventObservation,
    ) -> Result<Self, ProjectionCoordinatorError> {
        Ok(Self {
            runtime_id,
            process_generation,
            connection_generation,
            process,
            commands,
            terminal_disposer,
            state: std::sync::Mutex::new(RouterState {
                revision: 0,
                next_registration: 0,
                next_steering_attempt: 0,
                active_steering_attempt: None,
                active_steering_attempt_waiter: false,
                next_stop_election: 0,
                active_stop_election: None,
                volatile_stop_admission: None,
                persistent_failure: None,
                retired: None,
                targets: std::collections::HashMap::new(),
                retired_thread_lanes: std::collections::HashSet::new(),
                routed_operation_count: 0,
                unmatched_operation_count: 0,
                rejected_operation_count: 0,
                queue_pressure_count: 0,
                quiet_poll_count: 0,
            }),
            publication_changed: std::sync::Condvar::new(),
            scheduler_signal,
            #[cfg(test)]
            stop_election_wait_observer: std::sync::Mutex::new(None),
            #[cfg(test)]
            terminal_publication_wait_observer: std::sync::Mutex::new(None),
        })
    }

    pub(in crate::cas_projection) fn unregister(
        &self,
        registration: &TargetRegistration,
        reason: LiveEventTargetCloseReason,
    ) -> bool {
        match self.commands.authorize() {
            Ok(command) => self.unregister_admitted(&command, registration, reason),
            Err(_) if self.commands.is_persistent_failure_cut() => false,
            Err(_) => self.unregister_after_ordinary_close(registration, reason),
        }
    }

    pub(in crate::cas_projection::connection) fn settle_abandoned_target_projection(
        &self,
        registration: &TargetRegistration,
        projection: crate::cas_projection::LoadedCasProjection,
    ) -> TargetProjectionDropSettlement {
        enum GateSettlement {
            Ordinary {
                projection: crate::cas_projection::LoadedCasProjection,
                connection_retired: bool,
            },
            PersistentFailure,
            RetainAfterCut(crate::cas_projection::LoadedCasProjection),
        }

        let mut state = match self.state.lock() {
            Ok(state) => state,
            Err(_) => return TargetProjectionDropSettlement::Unavailable(projection),
        };
        let state_cell = std::cell::RefCell::new(&mut *state);
        let projection_cell = std::cell::RefCell::new(Some(projection));
        let settle_ordinary = || {
            let connection_retired = unregister_locked(
                *state_cell.borrow_mut(),
                registration,
                LiveEventTargetCloseReason::ReceiverAbandoned,
            );
            GateSettlement::Ordinary {
                projection: projection_cell
                    .borrow_mut()
                    .take()
                    .expect("target drop settles its exact loaded projection once"),
                connection_retired,
            }
        };
        let settle_closed = || {
            let connection_retired = unregister_locked(
                *state_cell.borrow_mut(),
                registration,
                LiveEventTargetCloseReason::ReceiverAbandoned,
            );
            GateSettlement::Ordinary {
                projection: projection_cell
                    .borrow_mut()
                    .take()
                    .expect("closed target drop settles its exact loaded projection once"),
                connection_retired,
            }
        };
        let settlement = self.commands.settle_authority(
            settle_ordinary,
            |_| {
                let mut state = state_cell.borrow_mut();
                let thread_id = &registration.key.cas_thread_id;
                let matching = state.targets.get(thread_id).is_some_and(|target| {
                    target.registration == registration.registration
                        && target.owner == registration.owner
                        && target.loaded_generation == registration.loaded_generation
                });
                let already_frozen = state.persistent_failure.is_some();
                if matching && !already_frozen {
                    let target = state
                        .targets
                        .get_mut(thread_id)
                        .expect("validated failure target remains under the router lock");
                    if target.persistent_failure_projection.is_none() {
                        target.persistent_failure_projection = projection_cell.borrow_mut().take();
                        return GateSettlement::PersistentFailure;
                    }
                }
                GateSettlement::RetainAfterCut(
                    projection_cell
                        .borrow_mut()
                        .take()
                        .expect("failure target projection remains available for retention"),
                )
            },
            settle_closed,
        );
        drop(state_cell);
        drop(state);
        self.publication_changed.notify_all();

        let settlement = match settlement {
            Ok(settlement) => settlement,
            Err(_) => {
                return TargetProjectionDropSettlement::Unavailable(
                    projection_cell
                        .borrow_mut()
                        .take()
                        .expect("unsettled target drop retains its projection"),
                );
            }
        };
        match settlement {
            GateSettlement::Ordinary {
                projection,
                connection_retired,
            } => TargetProjectionDropSettlement::Ordinary {
                projection,
                connection_retired,
            },
            GateSettlement::PersistentFailure => TargetProjectionDropSettlement::PersistentFailure,
            GateSettlement::RetainAfterCut(projection) => {
                if let Some(disposer) = self.terminal_disposer.clone() {
                    disposer.dispose_target(projection);
                    TargetProjectionDropSettlement::PersistentFailure
                } else {
                    TargetProjectionDropSettlement::Unavailable(projection)
                }
            }
        }
    }

    pub(super) fn unregister_admitted(
        &self,
        command: &crate::cas_projection::LiveCommandPermit,
        registration: &TargetRegistration,
        reason: LiveEventTargetCloseReason,
    ) -> bool {
        let Ok(mut state) = self.state.lock() else {
            return true;
        };
        let retired = command
            .commit_if_current(|| unregister_locked(&mut state, registration, reason))
            .unwrap_or(false);
        drop(state);
        self.publication_changed.notify_all();
        retired
    }

    fn unregister_after_ordinary_close(
        &self,
        registration: &TargetRegistration,
        reason: LiveEventTargetCloseReason,
    ) -> bool {
        let Ok(mut state) = self.state.lock() else {
            return true;
        };
        let retired = unregister_locked(&mut state, registration, reason);
        drop(state);
        self.publication_changed.notify_all();
        retired
    }

    pub(in crate::cas_projection) fn retire(&self, reason: LiveEventTargetCloseReason) {
        let command = match self.commands.authorize() {
            Ok(command) => Some(command),
            Err(_) if self.commands.is_persistent_failure_cut() => return,
            Err(_) => None,
        };
        let mut state = match self.state.lock() {
            Ok(state) => state,
            Err(poisoned) => poisoned.into_inner(),
        };
        let mut retire = || retire_locked(&mut state, reason);
        let effective_reason = match command.as_ref() {
            Some(command) => command.commit_if_current(retire).ok().flatten(),
            None => retire(),
        };
        drop(state);
        if effective_reason.is_some() {
            self.publication_changed.notify_all();
        }
    }

    pub(in crate::cas_projection) fn record_quiet_poll(
        &self,
        command: &crate::cas_projection::LiveCommandPermit,
    ) {
        if let Ok(mut state) = self.state.lock() {
            let _ = command.commit_if_current(|| {
                if state.retired.is_none() && state.persistent_failure.is_none() {
                    state.quiet_poll_count = state.quiet_poll_count.saturating_add(1);
                    advance_revision(&mut state);
                }
            });
        }
    }

    /// Records one connection-scoped remote thread closure before loaded authority is revoked.
    ///
    /// This lane mutation deliberately does not use the service command gate: a close already read
    /// by the stable connection remains authoritative on either side of persistent-failure
    /// election. The caller must release this router lock before acquiring connection authority.
    pub(in crate::cas_projection) fn record_thread_closed(
        &self,
        thread_id: &CasThreadId,
    ) -> Result<bool, ProjectionCoordinatorError> {
        let mut state =
            self.state
                .lock()
                .map_err(|_| ProjectionCoordinatorError::RegistryPoisoned {
                    registry: crate::cas_projection::ProjectionRegistryKind::LiveEventRouter,
                })?;
        let connection_retired = if state.retired.is_some() {
            true
        } else if state.targets.contains_key(thread_id) {
            close_target(
                &mut state,
                thread_id,
                LiveEventTargetCloseReason::ThreadClosed,
            )
        } else {
            let connection_retired = fence_thread_lane(&mut state, thread_id);
            if !connection_retired {
                advance_revision(&mut state);
            }
            connection_retired
        };
        drop(state);
        self.publication_changed.notify_all();
        Ok(connection_retired)
    }
}

fn unregister_locked(
    state: &mut RouterState,
    registration: &TargetRegistration,
    reason: LiveEventTargetCloseReason,
) -> bool {
    if state.persistent_failure.is_some() {
        return false;
    }
    if state
        .targets
        .get(&registration.key.cas_thread_id)
        .is_some_and(|target| target.registration == registration.registration)
    {
        close_target(state, &registration.key.cas_thread_id, reason)
    } else {
        false
    }
}

fn retire_locked(
    state: &mut RouterState,
    reason: LiveEventTargetCloseReason,
) -> Option<LiveEventTargetCloseReason> {
    if state.persistent_failure.is_some() {
        return None;
    }
    let effective_reason = state.retired.unwrap_or(reason);
    retire_router_state(state, effective_reason);
    Some(effective_reason)
}

pub(super) fn advance_revision(state: &mut RouterState) {
    state.revision = state.revision.saturating_add(1);
}
