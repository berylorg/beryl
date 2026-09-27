use super::*;

pub(crate) struct RunningExitCommands(StartupCommands);

pub(crate) struct RunningExitRequest {
    identity: Rc<()>,
    invoking: Option<WindowId>,
}

impl RunningExitRequest {
    pub(crate) fn invoking_window(&self) -> Option<WindowId> {
        self.invoking
    }

    pub(crate) fn identity(&self) -> Rc<()> {
        self.identity.clone()
    }
}

#[derive(Clone)]
pub(crate) struct RunningWindowExit {
    commands: StartupCommands,
    invoking: WindowId,
}

impl RunningWindowExit {
    pub(crate) fn request_exit(&self) {
        self.commands.request_exit_from(Some(self.invoking));
    }
}

impl RunningExitCommands {
    pub(super) fn new(commands: StartupCommands) -> Self {
        assert!(matches!(commands.0.borrow().stage, Stage::Running));
        Self(commands)
    }

    pub(crate) fn exit_requested(&self) -> bool {
        let state = self.0.0.borrow();
        state.exit || state.active_exit.is_some()
    }

    pub(crate) fn window_command(&self, invoking: WindowId) -> RunningWindowExit {
        RunningWindowExit {
            commands: self.0.clone(),
            invoking,
        }
    }

    pub(crate) fn bind_invoking_window(
        &self,
        request: &mut RunningExitRequest,
        startup_window: Option<WindowId>,
    ) -> Option<WindowId> {
        if !self.is_active(request) {
            return None;
        }
        if request.invoking.is_none() {
            request.invoking = startup_window;
        }
        request.invoking
    }

    fn is_active(&self, request: &RunningExitRequest) -> bool {
        self.0
            .0
            .borrow()
            .active_exit
            .as_ref()
            .is_some_and(|active| Rc::ptr_eq(active, &request.identity))
    }

    pub(crate) async fn next_exit(&mut self) -> RunningExitRequest {
        std::future::poll_fn(|cx| self.poll_exit(cx)).await
    }

    pub(crate) fn poll_exit(
        &mut self,
        cx: &mut std::task::Context<'_>,
    ) -> Poll<RunningExitRequest> {
        let mut state = self.0.0.borrow_mut();
        if state.exit && state.active_exit.is_none() {
            state.exit = false;
            let request = Rc::new(());
            state.active_exit = Some(request.clone());
            state.wake = None;
            return Poll::Ready(RunningExitRequest {
                identity: request,
                invoking: state.exit_window.take(),
            });
        }
        state.wake = Some(cx.waker().clone());
        Poll::Pending
    }

    pub(crate) fn finish_exit(&mut self, request: &RunningExitRequest) -> bool {
        let Ok(wake) = self.finish_exit_deferred_wake(request) else {
            return false;
        };
        if let Some(wake) = wake {
            wake.wake();
        }
        true
    }

    pub(crate) fn finish_exit_deferred_wake(
        &mut self,
        request: &RunningExitRequest,
    ) -> Result<Option<Waker>, ()> {
        let mut state = self.0.0.borrow_mut();
        if !state
            .active_exit
            .as_ref()
            .is_some_and(|active| Rc::ptr_eq(active, &request.identity))
        {
            return Err(());
        }
        state.active_exit = None;
        Ok(state.wake.take())
    }
}

#[cfg(test)]
mod tests {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/unit/startup_exit_delivery.rs"
    ));
}
