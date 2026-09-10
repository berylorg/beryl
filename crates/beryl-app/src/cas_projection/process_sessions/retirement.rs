use super::*;

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
        let retirement = {
            let mut state = self.lock();
            if state.closed
                || !state
                    .context
                    .as_ref()
                    .is_some_and(|context| context.commands.is_open())
            {
                return Ok(None);
            }
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
            retired
        };
        Ok(Some(retirement))
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
        let Ok(observation) = preparation.work_sources.mutation_observation() else {
            return;
        };
        let Ok(records) = preparation.work_sources.required_session_work(
            self,
            &crate::cas_projection::ProjectionCancellationToken::new(),
        ) else {
            return;
        };
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
