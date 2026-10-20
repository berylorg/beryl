use super::*;

mod projection;

struct IdleSessionRetirement {
    connection: Arc<ProjectionConnection>,
    resources: Option<SessionResources>,
}

impl IdleSessionRetirement {
    fn finish(self, sessions: &ScheduledExecutionSessions) {
        self.connection.signal_idle_session_retirement();
        drop(self.resources);
        sessions.reap();
    }
}

impl ScheduledExecutionSessions {
    pub(in crate::cas_projection) fn retire_idle_for_shutdown(
        &self,
        fence: &crate::process_admission::ProcessAdmissionFence,
    ) -> Result<(), crate::cas_projection::ProjectionCoordinatorError> {
        let mut after = None;
        loop {
            let registration = {
                let state = self.lock();
                let start = after.map_or(std::ops::Bound::Unbounded, std::ops::Bound::Excluded);
                state
                    .slots
                    .range((start, std::ops::Bound::Unbounded))
                    .next()
                    .map(|(_, slot)| {
                        (
                            slot.registration,
                            slot.binding.clone(),
                            Arc::clone(&slot.connection),
                        )
                    })
            };
            let Some((registration, binding, connection)) = registration else {
                return Ok(());
            };
            after = Some(registration.thread_id);
            let _change = connection.begin_shutdown_cleanup_change();
            self.release_owned_shutdown_projection(registration, &binding, fence)
                .inspect_err(|error| connection.settle_shutdown_authority_error(error))?;
            let retirement = self
                .with_shutdown_fence(fence, |command| {
                    let Some(mut state) = self.try_lock_for_shutdown() else {
                        return Ok(None);
                    };
                    if state.closed
                        || !state.context.as_ref().is_some_and(|context| {
                            context.service_generation == command.service_generation()
                                && context.service_generation == registration.service_generation
                        })
                    {
                        return Ok(None);
                    }
                    Self::elect_idle_retirement_from_state(&mut state, registration, |slot| {
                        slot.connection.elect_shutdown_session_retirement(command)
                    })
                })
                .inspect_err(|error| connection.settle_shutdown_authority_error(error))?
                .flatten();
            if let Some(retirement) = retirement {
                retirement.finish(self);
            }
        }
    }

    fn try_lock_for_shutdown(&self) -> Option<MutexGuard<'_, SessionState>> {
        match self.state.try_lock() {
            Ok(state) => Some(state),
            Err(std::sync::TryLockError::WouldBlock) => None,
            Err(std::sync::TryLockError::Poisoned(poison)) => {
                let mut state = poison.into_inner();
                if !state.closed {
                    state.work_changed();
                }
                state.closed = true;
                Some(state)
            }
        }
    }

    fn with_shutdown_fence<T>(
        &self,
        fence: &crate::process_admission::ProcessAdmissionFence,
        publish: impl FnOnce(
            &crate::cas_projection::LiveCommandPermit,
        ) -> Result<T, crate::cas_projection::ProjectionCoordinatorError>,
    ) -> Result<Option<T>, crate::cas_projection::ProjectionCoordinatorError> {
        let commands = {
            let state = self.lock();
            let Some(context) = state.context.as_ref().filter(|_| !state.closed) else {
                return Ok(None);
            };
            context.commands.clone()
        };
        let Ok(command) = commands.authorize() else {
            return Ok(None);
        };
        match command.commit_if_current(|| {
            commands.with_process_settlement_fence(fence, || publish(&command))
        }) {
            Ok(Ok(result)) => result.map(Some),
            _ => Ok(None),
        }
    }

    pub fn retire_if_idle(
        &self,
        registration: ScheduledSessionRegistration,
    ) -> Result<bool, crate::cas_projection::ProjectionCoordinatorError> {
        let Some(retirement) = self.elect_idle_retirement(registration, |slot| {
            slot.connection.elect_idle_session_retirement()
        })?
        else {
            return Ok(false);
        };
        retirement.finish(self);
        Ok(true)
    }

    fn elect_idle_retirement(
        &self,
        registration: ScheduledSessionRegistration,
        elect: impl FnOnce(
            &SessionSlot,
        ) -> Result<bool, crate::cas_projection::ProjectionCoordinatorError>,
    ) -> Result<Option<IdleSessionRetirement>, crate::cas_projection::ProjectionCoordinatorError>
    {
        let mut state = self.lock();
        if state.closed
            || !state
                .context
                .as_ref()
                .is_some_and(|context| context.commands.is_open())
        {
            return Ok(None);
        }
        Self::elect_idle_retirement_from_state(&mut state, registration, elect)
    }

    fn elect_idle_retirement_from_state(
        state: &mut SessionState,
        registration: ScheduledSessionRegistration,
        elect: impl FnOnce(
            &SessionSlot,
        ) -> Result<bool, crate::cas_projection::ProjectionCoordinatorError>,
    ) -> Result<Option<IdleSessionRetirement>, crate::cas_projection::ProjectionCoordinatorError>
    {
        let Some(slot) = state.slots.get_mut(&registration.thread_id) else {
            return Ok(None);
        };
        if slot.registration != registration
            || slot.retiring
            || slot.checked_out
            || slot.resources.is_none()
            || !elect(slot)?
        {
            return Ok(None);
        }
        slot.retiring = true;
        let retired = IdleSessionRetirement {
            connection: Arc::clone(&slot.connection),
            resources: slot.resources.take(),
        };
        state.work_changed();
        Ok(Some(retired))
    }

    pub(super) fn recheck_idle_sessions(&self) {
        self.reap();
        let (preparation, service_generation) = {
            let state = self.lock();
            let (Some(preparation), Some(context)) = (&state.preparation, &state.context) else {
                return;
            };
            if state.closed || !context.commands.is_open() {
                return;
            }
            (Arc::clone(preparation), context.service_generation)
        };
        let Ok(mut observation) = preparation.work_sources.mutation_observation() else {
            return;
        };
        let Ok(mut records) = preparation.work_sources.required_session_work(
            self,
            &crate::cas_projection::ProjectionCancellationToken::new(),
        ) else {
            return;
        };
        let mut released = false;
        for record in &records {
            if record.facts != crate::cas_projection::ProcessWorkFacts::default()
                || record.session.state() != ScheduledSessionWorkState::Available
            {
                continue;
            }
            let registration = ScheduledSessionRegistration {
                service_generation,
                thread_id: record.thread_id,
                serial: record.session.registration_serial(),
            };
            #[cfg(feature = "test-faults")]
            self.pause_idle_election_for_test(record.thread_id);
            released |= self.release_owned_idle_projection(
                registration,
                record.session.execution_binding(),
                &preparation.owner,
                &observation,
            );
        }
        if released {
            let Ok(fresh_observation) = preparation.work_sources.mutation_observation() else {
                return;
            };
            let Ok(fresh_records) = preparation.work_sources.required_session_work(
                self,
                &crate::cas_projection::ProjectionCancellationToken::new(),
            ) else {
                return;
            };
            observation = fresh_observation;
            records = fresh_records;
        }
        for record in records {
            if record.facts != crate::cas_projection::ProcessWorkFacts::default()
                || record.session.state() != ScheduledSessionWorkState::Available
            {
                continue;
            }
            let registration = ScheduledSessionRegistration {
                service_generation,
                thread_id: record.thread_id,
                serial: record.session.registration_serial(),
            };
            #[cfg(feature = "test-faults")]
            self.pause_idle_election_for_test(record.thread_id);
            let retirement = self.elect_idle_retirement(registration, |slot| {
                if &slot.binding != record.session.execution_binding() {
                    return Ok(false);
                }
                slot.connection.elect_unviewed_session_retirement(
                    &preparation.owner,
                    &slot.binding,
                    &observation,
                )
            });
            if let Ok(Some(retirement)) = retirement {
                retirement.finish(self);
            }
        }
    }
}
