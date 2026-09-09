use super::*;

impl ScheduledExecutionSessions {
    pub fn register(
        &self,
        thread_id: SyndicThreadId,
        binding: ExecutionBinding,
        session: AdmittedProjectionSession,
        policy: ScheduledOrdinaryRequestPolicy,
        assets: AssetState,
        tools: Box<dyn OrdinaryDynamicToolAuthority>,
    ) -> Result<ScheduledSessionRegistration, ScheduledSessionRegistrationError> {
        self.reap();
        let (registration, ready) = {
            let mut state = self.lock();
            let context = state
                .context
                .as_ref()
                .ok_or(ScheduledSessionRegistrationError::Unattached)?;
            if state.closed || !context.commands.is_open() {
                return Err(ScheduledSessionRegistrationError::Closed);
            }
            if !session.permits_execution_binding(&binding) || !context.owns(&session) {
                return Err(ScheduledSessionRegistrationError::SessionAuthorityUnavailable);
            }
            if policy.thread_options().is_ephemeral() {
                return Err(ScheduledSessionRegistrationError::EphemeralThreadPolicy);
            }
            if state.slots.contains_key(&thread_id) {
                return Err(ScheduledSessionRegistrationError::ThreadOccupied);
            }
            if state.slots.len() >= context.capacity {
                return Err(ScheduledSessionRegistrationError::CapacityFull);
            }
            let registration = ScheduledSessionRegistration {
                service_generation: context.service_generation,
                thread_id,
                serial: state
                    .next_serial
                    .checked_add(1)
                    .ok_or(ScheduledSessionRegistrationError::RegistrationExhausted)?,
            };
            let ready = context.ready.clone();
            state.next_serial = registration.serial;
            state.slots.insert(
                thread_id,
                SessionSlot {
                    registration,
                    binding,
                    policy,
                    assets,
                    connection: Arc::clone(session.connection()),
                    resources: Some(SessionResources { session, tools }),
                    checked_out: false,
                    retiring: false,
                },
            );
            state.high_water = state.high_water.max(state.slots.len());
            state.work_changed();
            (registration, ready)
        };
        ready.notify();
        Ok(registration)
    }

    pub fn retire(&self, registration: ScheduledSessionRegistration) -> bool {
        let resources = {
            let mut state = self.lock();
            let Some(slot) = state.slots.get_mut(&registration.thread_id) else {
                return false;
            };
            if slot.registration != registration {
                return false;
            }
            let changed = !slot.retiring || slot.resources.is_some();
            slot.retiring = true;
            let resources = slot.resources.take();
            if changed {
                state.work_changed();
            }
            resources
        };
        drop(resources);
        self.reap();
        true
    }

    pub fn close(&self) {
        self.request_close();
        self.close_preparation();
        self.reap();
    }

    pub(super) fn request_close(&self) {
        let (context, resources) = {
            let mut state = self.lock();
            let changed = !state.closed
                || state
                    .slots
                    .values()
                    .any(|slot| !slot.retiring || slot.resources.is_some());
            state.closed = true;
            let context = state.preparation.take();
            let resources: Vec<_> = state
                .slots
                .values_mut()
                .filter_map(|slot| {
                    slot.retiring = true;
                    slot.resources.take()
                })
                .collect();
            if changed {
                state.work_changed();
            }
            (context, resources)
        };
        drop(context);
        drop(resources);
        self.reap();
    }

    pub fn diagnostics(&self) -> ScheduledSessionDiagnostics {
        self.reap();
        let state = self.lock();
        ScheduledSessionDiagnostics {
            capacity: state.context.as_ref().map_or(0, |context| context.capacity),
            retained: state.slots.len(),
            available: state
                .slots
                .values()
                .filter(|slot| slot.resources.is_some() && !slot.retiring)
                .count(),
            checked_out: state.slots.values().filter(|slot| slot.checked_out).count(),
            retiring: state.slots.values().filter(|slot| slot.retiring).count(),
            high_water: state.high_water,
            closed: state.closed
                || state
                    .context
                    .as_ref()
                    .is_some_and(|context| !context.commands.is_open()),
        }
    }

    pub(super) fn settle_return(
        &self,
        registration: ScheduledSessionRegistration,
        resources: SessionResources,
    ) {
        let mut resources = Some(resources);
        let ready = {
            let mut state = self.lock();
            let current = !state.closed
                && state
                    .context
                    .as_ref()
                    .is_some_and(|context| context.commands.is_open());
            if let Some(slot) = state.slots.get_mut(&registration.thread_id)
                && slot.registration == registration
                && slot.checked_out
            {
                slot.checked_out = false;
                if !current || slot.connection.is_retired() || slot.connection.is_detached() {
                    slot.retiring = true;
                }
                let ready = if !slot.retiring {
                    slot.resources = resources.take();
                    state.context.as_ref().map(|context| context.ready.clone())
                } else {
                    None
                };
                state.work_changed();
                ready
            } else {
                None
            }
        };
        drop(resources);
        if let Some(ready) = ready {
            ready.notify();
        }
    }
}
