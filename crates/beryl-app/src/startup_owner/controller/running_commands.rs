use super::*;

#[derive(Clone, Copy)]
pub(crate) enum RunningExitGate {
    Unavailable,
    SettingsReconciliation,
    HomeUnavailable,
}

#[derive(Default)]
pub(super) struct RunningExitGates {
    unavailable: bool,
    settings_reconciliation: bool,
    home_unavailable: bool,
    home: Option<(
        beryl_home_store::HomeServiceReference,
        Option<beryl_home_store::HomeGeneration>,
    )>,
}

impl RunningExitGates {
    fn set(&mut self, gate: RunningExitGate, blocked: bool) {
        *match gate {
            RunningExitGate::Unavailable => &mut self.unavailable,
            RunningExitGate::SettingsReconciliation => &mut self.settings_reconciliation,
            RunningExitGate::HomeUnavailable => &mut self.home_unavailable,
        } = blocked;
    }

    pub(super) fn disabled_reason(&self) -> Option<&'static str> {
        let home_unavailable = self.home.as_ref().is_some_and(|(home, generation)| {
            let health = home.health();
            health.state() != beryl_home_store::HomeHealthState::Healthy
                || generation.is_none()
                || health.generation() != *generation
        });
        if self.home_unavailable || home_unavailable {
            Some(
                "The Beryl home store is unavailable. See the Beryl-home failure notice for automatic recovery.",
            )
        } else if self.settings_reconciliation {
            Some("Application Exit is waiting for Settings reconciliation.")
        } else if self.unavailable {
            Some("Application Exit is not available.")
        } else {
            None
        }
    }
}

pub(crate) struct RunningExitCommands(StartupCommands);

pub(crate) struct RunningExitRequest {
    identity: Rc<()>,
    invoking: Option<WindowId>,
    ordinary_close: bool,
}

impl RunningExitRequest {
    pub(crate) fn retain_for_recovery(&self) -> Self {
        Self {
            identity: self.identity.clone(),
            invoking: self.invoking,
            ordinary_close: self.ordinary_close,
        }
    }
    #[cfg(test)]
    pub(crate) fn test_foreign(&self) -> Self {
        Self {
            identity: Rc::new(()),
            invoking: self.invoking,
            ordinary_close: self.ordinary_close,
        }
    }

    pub(crate) fn invoking_window(&self) -> Option<WindowId> {
        self.invoking
    }

    pub(crate) fn is_ordinary_close(&self) -> bool {
        self.ordinary_close
    }

    pub(crate) fn shutdown_intent(&self) -> crate::running_owner::ShutdownIntent {
        if self.ordinary_close {
            crate::running_owner::ShutdownIntent::FinalWindowClose
        } else {
            crate::running_owner::ShutdownIntent::ApplicationExit
        }
    }

    pub(crate) fn identity(&self) -> Rc<()> {
        self.identity.clone()
    }
}

#[derive(Clone)]
pub(crate) struct RunningWindowExit {
    commands: StartupCommands,
    invoking: WindowId,
    generation: Option<beryl_home_store::HomeGeneration>,
}

impl RunningWindowExit {
    pub(crate) fn same_recovered_binding(&self, expected: &Self) -> bool {
        Rc::ptr_eq(&self.commands.0, &expected.commands.0)
            && self.invoking == expected.invoking
            && self.generation == expected.generation
    }

    pub(crate) fn disabled_reason(&self) -> Option<&'static str> {
        let state = self.commands.0.borrow();
        if state
            .exit_gates
            .home
            .as_ref()
            .and_then(|(_, generation)| *generation)
            != self.generation
        {
            return Some("This window command belongs to a retired home generation.");
        }
        state.exit_gates.disabled_reason().or_else(|| {
            (state.active_exit.is_some() || state.exit).then_some(if state.ordinary_close {
                "A main window is waiting for its draft and durable close state."
            } else {
                "Application Exit is waiting for active work and durable state."
            })
        })
    }

    pub(crate) fn request_exit(&self) {
        if self.disabled_reason().is_some() {
            return;
        }
        self.commands.request_exit_from(Some(self.invoking));
    }

    pub(crate) fn request_close(&self) {
        if self.disabled_reason().is_some() {
            return;
        }
        let mut state = self.commands.0.borrow_mut();
        if !matches!(state.stage, Stage::Running)
            || state.exit
            || state.active_exit.is_some()
            || state.exit_gates.disabled_reason().is_some()
        {
            return;
        }
        state.exit = true;
        state.exit_window = Some(self.invoking);
        state.ordinary_close = true;
        let wake = state.wake.take();
        drop(state);
        if let Some(wake) = wake {
            wake.wake();
        }
    }
}

impl RunningExitCommands {
    pub(crate) fn validate_recovered_home_binding(
        &self,
        request: Option<&RunningExitRequest>,
        home: &beryl_home_store::HomeServiceReference,
    ) -> Result<(), String> {
        let state = self.0.0.borrow();
        if !state.exit_gates.home_unavailable
            || match request {
                Some(request) => state
                    .active_exit
                    .as_ref()
                    .is_none_or(|active| !Rc::ptr_eq(active, &request.identity)),
                None => state.active_exit.is_some(),
            }
        {
            return Err("Recovered process binding lost its original lifecycle fence".into());
        }
        let (bound, generation) = state
            .exit_gates
            .home
            .as_ref()
            .ok_or("Recovered process Home binding is missing")?;
        let health = home.health();
        let bound_health = bound.health();
        if bound.home_id() != home.home_id()
            || bound.database_path() != home.database_path()
            || generation.is_none()
            || *generation != health.generation()
            || *generation != bound_health.generation()
            || health.state() != beryl_home_store::HomeHealthState::Healthy
            || bound_health.state() != beryl_home_store::HomeHealthState::Healthy
        {
            return Err("Recovered process Home binding changed".into());
        }
        Ok(())
    }

    pub(crate) fn disabled_reason(&self) -> Option<&'static str> {
        self.0.0.borrow().exit_gates.disabled_reason()
    }

    pub(crate) fn bind_home(&self, home: beryl_home_store::HomeServiceReference) {
        let mut state = self.0.0.borrow_mut();
        assert!(
            state.exit_gates.home.is_none(),
            "Exit home is already bound"
        );
        let generation = home.health().generation();
        state.exit_gates.home = Some((home, generation));
    }

    pub(crate) fn set_gate(&self, gate: RunningExitGate, blocked: bool) {
        let mut state = self.0.0.borrow_mut();
        state.exit_gates.set(gate, blocked);
        let admitted = state.admit_process_exit();
        let wake = if admitted { state.wake.take() } else { None };
        drop(state);
        if let Some(wake) = wake {
            wake.wake();
        }
    }

    pub(crate) fn bind_recovered_home(
        &self,
        request: &RunningExitRequest,
        home: beryl_home_store::HomeServiceReference,
    ) -> Result<(), String> {
        let mut state = self.0.0.borrow_mut();
        if !state
            .active_exit
            .as_ref()
            .is_some_and(|active| Rc::ptr_eq(active, &request.identity))
        {
            return Err("Recovered Exit home requires the active request".into());
        }
        let (previous, generation) = state
            .exit_gates
            .home
            .as_ref()
            .ok_or("Exit home is not bound")?;
        let health = home.health();
        if previous.home_id() != home.home_id()
            || generation.is_none()
            || health.state() != beryl_home_store::HomeHealthState::Healthy
            || health.generation().is_none()
            || health.generation() == *generation
        {
            return Err("Recovered Exit home requires a healthy same-home replacement".into());
        }
        state.exit_gates.home_unavailable = true;
        state.exit_gates.home = Some((home, health.generation()));
        Ok(())
    }

    pub(crate) fn bind_recovered_running_home(
        &self,
        home: beryl_home_store::HomeServiceReference,
    ) -> Result<(), String> {
        let mut state = self.0.0.borrow_mut();
        if state.active_exit.is_some() || !state.exit_gates.home_unavailable {
            return Err("Recovered Running home requires fenced lifecycle admission".into());
        }
        let (previous, generation) = state
            .exit_gates
            .home
            .as_ref()
            .ok_or("Running home is not bound")?;
        let health = home.health();
        if previous.home_id() != home.home_id()
            || generation.is_none()
            || health.state() != beryl_home_store::HomeHealthState::Healthy
            || health.generation().is_none()
            || health.generation() == *generation
        {
            return Err("Recovered Running home requires a healthy same-home replacement".into());
        }
        state.exit_gates.home = Some((home, health.generation()));
        Ok(())
    }

    pub(crate) fn lifecycle_admitted(&self) -> bool {
        self.0.0.borrow().active_exit.is_some()
    }

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
            generation: self
                .0
                .0
                .borrow()
                .exit_gates
                .home
                .as_ref()
                .and_then(|(_, generation)| *generation),
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

    pub(crate) fn is_active(&self, request: &RunningExitRequest) -> bool {
        self.is_active_identity(&request.identity)
    }

    pub(crate) fn is_active_identity(&self, identity: &Rc<()>) -> bool {
        self.0
            .0
            .borrow()
            .active_exit
            .as_ref()
            .is_some_and(|active| Rc::ptr_eq(active, identity))
    }

    pub(crate) async fn next_exit(&mut self) -> RunningExitRequest {
        std::future::poll_fn(|cx| self.poll_exit(cx)).await
    }

    pub(crate) fn poll_exit(
        &mut self,
        cx: &mut std::task::Context<'_>,
    ) -> Poll<RunningExitRequest> {
        let mut state = self.0.0.borrow_mut();
        state.admit_process_exit();
        if state.exit && state.active_exit.is_none() {
            state.exit = false;
            let request = Rc::new(());
            state.active_exit = Some(request.clone());
            state.wake = None;
            return Poll::Ready(RunningExitRequest {
                identity: request,
                invoking: state.exit_window.take(),
                ordinary_close: state.ordinary_close,
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
        state.admit_process_exit();
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
