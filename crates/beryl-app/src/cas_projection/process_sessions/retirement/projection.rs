use super::*;

impl ScheduledExecutionSessions {
    pub(super) fn release_owned_idle_projection(
        &self,
        registration: ScheduledSessionRegistration,
        binding: &ExecutionBinding,
        owner: &crate::cas_projection::runtime_interest::RuntimeInterestOwner,
        observation: &beryl_home_store::HomeMutationObservation,
    ) -> bool {
        let Ok(Some(resources)) = self.take_owned_projection(registration, binding, |projection| {
            projection.elect_idle_release(owner, observation)
        }) else {
            return false;
        };
        let _ = self.release_owned_projection(registration, resources);
        true
    }

    pub(super) fn release_owned_shutdown_projection(
        &self,
        registration: ScheduledSessionRegistration,
        binding: &ExecutionBinding,
        fence: &crate::process_admission::ProcessAdmissionFence,
    ) -> Result<bool, crate::cas_projection::ProjectionCoordinatorError> {
        let resources = self
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
                let Some(slot) = state.slots.get(&registration.thread_id) else {
                    return Ok(None);
                };
                if slot.registration != registration
                    || slot.retiring
                    || slot.checked_out
                    || &slot.binding != binding
                {
                    return Ok(None);
                }
                let Some(resources) = slot.resources.as_ref() else {
                    return Ok(None);
                };
                let Some(projection) = resources.projection.as_ref() else {
                    return Ok(None);
                };
                if !projection.belongs_to_session(&resources.session) {
                    return Ok(None);
                }
                let Some(identity) = projection.shutdown_release_identity() else {
                    return Ok(None);
                };
                identity.elect(command, || {
                    let slot = state
                        .slots
                        .get_mut(&registration.thread_id)
                        .expect("the session gate retains the elected registration");
                    slot.checked_out = true;
                    let resources = slot
                        .resources
                        .take()
                        .expect("the available session retains its resources");
                    state.work_changed();
                    resources
                })
            })?
            .flatten();
        let Some(resources) = resources else {
            return Ok(false);
        };
        self.release_owned_projection(registration, resources)?;
        Ok(true)
    }

    fn take_owned_projection(
        &self,
        registration: ScheduledSessionRegistration,
        binding: &ExecutionBinding,
        elect: impl FnOnce(
            &crate::cas_projection::LoadedCasProjection,
        ) -> Result<bool, crate::cas_projection::ProjectionCoordinatorError>,
    ) -> Result<Option<SessionResources>, crate::cas_projection::ProjectionCoordinatorError> {
        let mut state = self.lock();
        if state.closed
            || !state.context.as_ref().is_some_and(|context| {
                context.commands.is_open()
                    && context.service_generation == registration.service_generation
            })
        {
            return Ok(None);
        }
        Self::take_owned_projection_from_state(&mut state, registration, binding, elect)
    }

    fn take_owned_projection_from_state(
        state: &mut SessionState,
        registration: ScheduledSessionRegistration,
        binding: &ExecutionBinding,
        elect: impl FnOnce(
            &crate::cas_projection::LoadedCasProjection,
        ) -> Result<bool, crate::cas_projection::ProjectionCoordinatorError>,
    ) -> Result<Option<SessionResources>, crate::cas_projection::ProjectionCoordinatorError> {
        let Some(slot) = state.slots.get_mut(&registration.thread_id) else {
            return Ok(None);
        };
        if slot.registration != registration
            || slot.retiring
            || slot.checked_out
            || &slot.binding != binding
        {
            return Ok(None);
        }
        let Some(resources) = slot.resources.as_ref() else {
            return Ok(None);
        };
        let Some(projection) = resources.projection.as_ref() else {
            return Ok(None);
        };
        if !projection.belongs_to_session(&resources.session) || !elect(projection)? {
            return Ok(None);
        }
        slot.checked_out = true;
        let resources = slot
            .resources
            .take()
            .expect("available session retained its resources");
        state.work_changed();
        Ok(Some(resources))
    }

    fn release_owned_projection(
        &self,
        registration: ScheduledSessionRegistration,
        resources: SessionResources,
    ) -> Result<(), crate::cas_projection::ProjectionCoordinatorError> {
        let mut resources = resources;
        let projection = resources
            .projection
            .take()
            .expect("elected idle release retained its projection");
        let result = projection.release();
        self.settle_return(registration, resources);
        result.map(|_| ()).map_err(|error| match error {
            crate::cas_projection::LoadedProjectionReleaseError::Registry(source) => source,
            _ => crate::cas_projection::ProjectionCoordinatorError::ProjectionWorkerStopped,
        })
    }
}
