use super::*;

impl ScheduledExecutionSessions {
    pub fn retire_if_idle(
        &self,
        registration: ScheduledSessionRegistration,
    ) -> Result<bool, crate::cas_projection::ProjectionCoordinatorError> {
        let (connection, resources) = {
            let mut state = self.lock();
            if state.closed
                || !state
                    .context
                    .as_ref()
                    .is_some_and(|context| context.commands.is_open())
            {
                return Ok(false);
            }
            let Some(slot) = state.slots.get_mut(&registration.thread_id) else {
                return Ok(false);
            };
            if slot.registration != registration
                || slot.retiring
                || slot.checked_out
                || slot.resources.is_none()
                || !slot.connection.elect_idle_session_retirement()?
            {
                return Ok(false);
            }
            slot.retiring = true;
            let retired = (Arc::clone(&slot.connection), slot.resources.take());
            state.work_changed();
            retired
        };
        connection.signal_idle_session_retirement();
        drop(resources);
        self.reap();
        Ok(true)
    }
}
