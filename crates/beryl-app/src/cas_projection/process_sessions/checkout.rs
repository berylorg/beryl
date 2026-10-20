use super::*;
use crate::cas_projection::{
    OrdinaryDynamicToolHandlers, ScheduledOrdinaryAdmission, ScheduledOrdinaryAdmissionError,
    ScheduledOrdinaryAdmissionResult, ScheduledOrdinaryExecutionProvider,
    ScheduledOrdinaryExecutionUnavailable, ScheduledProjectionSessionAuthority,
};

struct SessionCheckout {
    session: Option<AdmittedProjectionSession>,
    projection: Option<super::super::LoadedCasProjection>,
    home_id: BerylHomeId,
    home_generation: HomeGeneration,
    binding: ExecutionBinding,
    returned: Arc<CheckoutReturn>,
}

struct ToolCheckout {
    tools: Option<Box<dyn OrdinaryDynamicToolAuthority>>,
    returned: Arc<CheckoutReturn>,
}

struct CheckoutReturn {
    owner: ScheduledExecutionSessions,
    registration: ScheduledSessionRegistration,
    session: Mutex<Option<AdmittedProjectionSession>>,
    tools: Mutex<Option<Box<dyn OrdinaryDynamicToolAuthority>>>,
    projection: Mutex<Option<super::super::LoadedCasProjection>>,
}

impl ScheduledOrdinaryExecutionProvider for ProcessScheduledExecutionProvider {
    fn recheck_idle_sessions(&mut self) {
        self.sessions.recheck_idle_sessions();
    }

    fn attach(&mut self, context: ScheduledExecutionProviderContext) {
        let mut state = self.sessions.lock();
        if state.context.is_some()
            || self.handoff.as_ref().is_some_and(|handoff| {
                !handoff.matches_home_generation(context.home_id, context.home_generation)
            })
        {
            state.closed = true;
            state.work_changed();
            return;
        }
        let ready = context.ready.clone();
        state.context = Some(context);
        state.work_changed();
        drop(state);
        if let Some(handoff) = &self.handoff {
            handoff.set_dispatch_capacity_waker(std::task::Waker::from(Arc::new(ready)));
        }
    }

    fn try_issue(
        &mut self,
        admission: ScheduledOrdinaryAdmission,
    ) -> Result<ScheduledOrdinaryAdmissionResult, ScheduledOrdinaryAdmissionError> {
        self.sessions.reap();
        let (registration, resources, policy, assets) = {
            let mut state = self.sessions.lock();
            let Some(context) = state.context.as_ref() else {
                return Ok(
                    admission.decline(ScheduledOrdinaryExecutionUnavailable::RuntimeNotReady)
                );
            };
            if state.closed
                || !context.commands.is_open()
                || admission.home_id() != context.home_id
                || admission.home_generation() != context.home_generation
                || admission.service_generation() != context.service_generation
            {
                return Ok(admission.decline(ScheduledOrdinaryExecutionUnavailable::ShuttingDown));
            }
            let Some(slot) = state.slots.get_mut(&admission.thread_id()) else {
                drop(state);
                return Ok(self.sessions.prepare(admission, self.handoff.clone()));
            };
            if slot.retiring
                || slot.connection.is_retired()
                || slot.connection.is_detached()
                || &slot.binding != admission.execution_binding()
            {
                return Ok(
                    admission.decline(ScheduledOrdinaryExecutionUnavailable::RuntimeNotReady)
                );
            }
            let Some(resources) = slot.resources.take() else {
                return Ok(admission.decline(ScheduledOrdinaryExecutionUnavailable::SessionBusy));
            };
            slot.checked_out = true;
            let checkout = (
                slot.registration,
                resources,
                slot.policy.clone(),
                slot.assets.clone(),
            );
            state.work_changed();
            checkout
        };
        let policy = match self
            .sessions
            .pending_model_choice(admission.thread_id(), admission.execution_binding())
        {
            Some(pending) => policy.with_pending_model_choice(pending),
            None => policy,
        };
        let returned = Arc::new(CheckoutReturn {
            owner: self.sessions.clone(),
            registration,
            session: Mutex::new(None),
            tools: Mutex::new(None),
            projection: Mutex::new(None),
        });
        let session = SessionCheckout {
            session: Some(resources.session),
            projection: resources.projection,
            home_id: admission.home_id(),
            home_generation: admission.home_generation(),
            binding: admission.execution_binding().clone(),
            returned: Arc::clone(&returned),
        };
        let tools = ToolCheckout {
            tools: Some(resources.tools),
            returned,
        };
        admission
            .issue(
                Box::new(session),
                policy,
                assets,
                self.handoff.clone(),
                Box::new(tools),
            )
            .map(ScheduledOrdinaryAdmissionResult::Issued)
    }

    fn shutdown(&mut self) {
        self.sessions.close();
        let connections: Vec<_> = self
            .sessions
            .lock()
            .slots
            .values()
            .filter(|slot| !slot.checked_out)
            .map(|slot| Arc::clone(&slot.connection))
            .collect();
        for connection in connections {
            let _ = connection.shutdown();
        }
        self.sessions.reap();
    }
}

impl Drop for ProcessScheduledExecutionProvider {
    fn drop(&mut self) {
        self.sessions.request_close();
    }
}

impl ScheduledProjectionSessionAuthority for SessionCheckout {
    fn session(&mut self) -> &mut AdmittedProjectionSession {
        self.session.as_mut().expect("checkout retains its session")
    }

    fn take_projection(&mut self) -> Option<super::super::LoadedCasProjection> {
        self.projection.take()
    }

    fn return_projection(
        &mut self,
        projection: super::super::LoadedCasProjection,
    ) -> Result<(), super::super::LoadedCasProjection> {
        if self.projection.is_some()
            || projection.home_id() != self.home_id
            || projection.home_generation() != self.home_generation
            || projection.syndic_thread_id() != self.returned.registration.thread_id
            || projection.execution_binding() != &self.binding
            || !projection.belongs_to_session(self.session())
            || !projection.is_live().unwrap_or(false)
        {
            return Err(projection);
        }
        self.projection = Some(projection);
        Ok(())
    }
}

impl Drop for SessionCheckout {
    fn drop(&mut self) {
        *self
            .returned
            .session
            .lock()
            .unwrap_or_else(|poison| poison.into_inner()) = self.session.take();
        *self
            .returned
            .projection
            .lock()
            .unwrap_or_else(|poison| poison.into_inner()) = self.projection.take();
    }
}

impl OrdinaryDynamicToolAuthority for ToolCheckout {
    fn handlers(&mut self) -> OrdinaryDynamicToolHandlers<'_> {
        self.tools
            .as_mut()
            .expect("checkout retains its tool authority")
            .handlers()
    }
}

impl Drop for ToolCheckout {
    fn drop(&mut self) {
        *self
            .returned
            .tools
            .lock()
            .unwrap_or_else(|poison| poison.into_inner()) = self.tools.take();
    }
}

impl Drop for CheckoutReturn {
    fn drop(&mut self) {
        let session = self
            .session
            .get_mut()
            .unwrap_or_else(|poison| poison.into_inner())
            .take()
            .expect("paired checkout returns its session exactly once");
        let tools = self
            .tools
            .get_mut()
            .unwrap_or_else(|poison| poison.into_inner())
            .take()
            .expect("paired checkout returns its tools exactly once");
        self.owner.settle_return(
            self.registration,
            SessionResources {
                session,
                tools,
                projection: self
                    .projection
                    .get_mut()
                    .unwrap_or_else(|poison| poison.into_inner())
                    .take(),
            },
        );
    }
}
