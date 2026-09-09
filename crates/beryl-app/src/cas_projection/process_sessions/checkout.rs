use super::*;
use crate::cas_projection::{
    OrdinaryDynamicToolHandlers, ScheduledOrdinaryAdmission, ScheduledOrdinaryAdmissionError,
    ScheduledOrdinaryAdmissionResult, ScheduledOrdinaryExecutionProvider,
    ScheduledOrdinaryExecutionUnavailable, ScheduledProjectionSessionAuthority,
};

struct SessionCheckout {
    session: Option<AdmittedProjectionSession>,
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
}

impl ScheduledOrdinaryExecutionProvider for ProcessScheduledExecutionProvider {
    fn attach(&mut self, context: ScheduledExecutionProviderContext) {
        let mut state = self.sessions.lock();
        if state.context.is_some() {
            state.closed = true;
        } else {
            state.context = Some(context);
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
                return Ok(
                    admission.decline(ScheduledOrdinaryExecutionUnavailable::RuntimeNotReady)
                );
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
            (
                slot.registration,
                resources,
                slot.policy.clone(),
                slot.assets.clone(),
            )
        };
        let returned = Arc::new(CheckoutReturn {
            owner: self.sessions.clone(),
            registration,
            session: Mutex::new(None),
            tools: Mutex::new(None),
        });
        let session = SessionCheckout {
            session: Some(resources.session),
            returned: Arc::clone(&returned),
        };
        let tools = ToolCheckout {
            tools: Some(resources.tools),
            returned,
        };
        admission
            .issue(Box::new(session), policy, assets, Box::new(tools))
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
        self.sessions.close();
    }
}

impl ScheduledProjectionSessionAuthority for SessionCheckout {
    fn session(&mut self) -> &mut AdmittedProjectionSession {
        self.session.as_mut().expect("checkout retains its session")
    }
}

impl Drop for SessionCheckout {
    fn drop(&mut self) {
        *self
            .returned
            .session
            .lock()
            .unwrap_or_else(|poison| poison.into_inner()) = self.session.take();
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
        self.owner
            .settle_return(self.registration, SessionResources { session, tools });
    }
}
