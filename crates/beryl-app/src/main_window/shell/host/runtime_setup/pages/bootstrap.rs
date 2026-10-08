use super::*;

impl MainWindowShellRoot {
    pub(super) fn publish_setup_bootstrap(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.runtime_setup.bootstrap_root_page.is_none()
            || self.runtime_setup.bootstrap_runtime_page.is_none()
        {
            return;
        }
        let Some(picker) = self.runtime_setup.picker.clone() else {
            return;
        };
        let Some(services) = self.runtime_setup.services.clone() else {
            return;
        };
        let Some(observation) = self.runtime_setup.bootstrap_observation.take() else {
            return;
        };
        let flight = self
            .runtime_setup
            .flight
            .clone()
            .filter(|_| self.runtime_setup.refresh_command);
        let retained = flight.as_ref().and_then(|flight| flight.take_outcome());
        if let Some(RuntimeAdmissionOutcome::Committed { admission, .. }) = &retained {
            if let Err(error) = admission.validate_publication() {
                if let (Some(flight), Some(outcome)) = (&flight, retained) {
                    flight.retain_outcome(outcome);
                }
                self.setup_failure(&error.to_string(), true);
                self.setup_command_state("Unavailable", true, cx);
                return;
            }
        }
        let result = services.elect(&observation, || {
            let roots = self.runtime_setup.bootstrap_root_page.take().unwrap();
            let runtimes = self.runtime_setup.bootstrap_runtime_page.take().unwrap();
            self.runtime_setup.bootstrap_ready = true;
            picker.update(cx, |picker, pcx| {
                picker.settle_page(PickerPageOutcome::Success(roots), window, pcx);
                picker.settle_runtime_page(
                    PickerRuntimePageOutcome::Success(runtimes),
                    window,
                    pcx,
                );
            });
            if self.runtime_setup.refresh_command {
                self.runtime_setup.refresh_command = false;
                self.finish_setup_command(window, cx);
            }
        });
        if let Err(error) = result {
            if let (Some(flight), Some(outcome)) = (&flight, retained) {
                flight.retain_outcome(outcome);
            }
            self.fail_setup_bootstrap(window, cx);
            self.setup_failure(&error, false);
            return;
        }
        if flight.is_some() {
            self.runtime_setup.flight = None;
        }
        drop(retained);
        self.sync_thread_confirmation(cx);
        cx.notify();
    }

    pub(super) fn fail_setup_bootstrap(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(picker) = self.runtime_setup.picker.clone() else {
            return;
        };
        let roots = self.runtime_setup.bootstrap_root_page.take();
        let runtimes = self.runtime_setup.bootstrap_runtime_page.take();
        self.runtime_setup.bootstrap_observation = None;
        self.runtime_setup.revision = None;
        self.runtime_setup.roots.clear();
        self.runtime_setup.runtimes.clear();
        picker.update(cx, |picker, pcx| {
            if let Some(page) = roots {
                picker.settle_page(
                    PickerPageOutcome::Failed {
                        request: page.request,
                        message: "Runtime/root collection changed during read.".into(),
                    },
                    window,
                    pcx,
                );
            }
            if let Some(page) = runtimes {
                picker.settle_runtime_page(
                    PickerRuntimePageOutcome::Failed {
                        request: page.request,
                        message: "Runtime/root collection changed during read.".into(),
                    },
                    window,
                    pcx,
                );
            }
        });
    }
}
