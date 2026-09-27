use super::*;

pub(crate) struct RunningExitCommands(StartupCommands);

pub(crate) struct RunningExitRequest(Rc<()>);

impl RunningExitCommands {
    pub(super) fn new(commands: StartupCommands) -> Self {
        assert!(matches!(commands.0.borrow().stage, Stage::Running));
        Self(commands)
    }

    pub(crate) fn exit_requested(&self) -> bool {
        let state = self.0.0.borrow();
        state.exit || state.active_exit.is_some()
    }

    pub(crate) async fn next_exit(&mut self) -> RunningExitRequest {
        std::future::poll_fn(|cx| {
            let mut state = self.0.0.borrow_mut();
            if state.exit && state.active_exit.is_none() {
                state.exit = false;
                let request = Rc::new(());
                state.active_exit = Some(request.clone());
                state.wake = None;
                return Poll::Ready(RunningExitRequest(request));
            }
            state.wake = Some(cx.waker().clone());
            Poll::Pending
        })
        .await
    }

    pub(crate) fn finish_exit(&mut self, request: &RunningExitRequest) -> bool {
        let mut state = self.0.0.borrow_mut();
        if !state
            .active_exit
            .as_ref()
            .is_some_and(|active| Rc::ptr_eq(active, &request.0))
        {
            return false;
        }
        state.active_exit = None;
        let wake = state.wake.take();
        drop(state);
        if let Some(wake) = wake {
            wake.wake();
        }
        true
    }
}

#[cfg(test)]
mod tests {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/unit/startup_exit_delivery.rs"
    ));
}
