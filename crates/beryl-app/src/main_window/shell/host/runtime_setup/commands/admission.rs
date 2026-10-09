use super::*;
use crate::runtime_admission::AdmissionReconciliationOutcome;
use beryl_model::RuntimeLaunchForm;
use std::path::PathBuf;

impl MainWindowShellRoot {
    pub(in crate::main_window::shell::host) fn start_setup_admission(
        &mut self,
        form: Option<RuntimeLaunchForm>,
        runtime: Option<RuntimeId>,
        path: PathBuf,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let result = (|| {
            #[cfg(all(test, feature = "test-faults"))]
            if let Some(flight) = self.runtime_setup.fixture_admission.take() {
                return Ok(flight);
            }
            let services = self
                .runtime_setup
                .services
                .clone()
                .filter(|services| services.current())
                .ok_or("Runtime setup services are unavailable.")?;
            let members = self.setup_members(cx)?;
            let source = self
                .controller()
                .ok_or("Runtime setup shell is unavailable.")?
                .window_id();
            if let Some(runtime) = runtime {
                services.start_add_root_for_window(source, members, runtime, path)
            } else {
                services.start_add_runtime_for_window(
                    source,
                    members,
                    path,
                    form.ok_or("The executable form was not retained.")?,
                )
            }
        })();
        match result {
            Ok(flight) => {
                self.runtime_setup.flight = Some(flight);
                self.setup_command_state(
                    if runtime.is_some() {
                        "Adding root…"
                    } else {
                        "Adding runtime…"
                    },
                    false,
                    cx,
                );
                self.start_setup_poll(window, cx);
            }
            Err(error) => {
                self.setup_failure(&error, false);
                self.finish_setup_command(window, cx);
            }
        }
    }

    pub(in crate::main_window::shell::host) fn start_setup_poll(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.runtime_setup.poll.is_some() {
            return;
        }
        self.runtime_setup.poll = Some(cx.spawn_in(window, async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(10))
                    .await;
                let active = this
                    .update_in(cx, |root, window, cx| {
                        root.drive_setup_flight(window, cx);
                        let active = root.runtime_setup.flight.is_some()
                            && !root.runtime_setup.retired
                            && root.runtime_setup.unavailable.is_none()
                            && root
                                .runtime_setup
                                .services
                                .as_ref()
                                .is_some_and(|services| services.current());
                        if !active {
                            root.runtime_setup.poll = None;
                        }
                        active
                    })
                    .unwrap_or(false);
                if !active {
                    break;
                }
            }
        }));
    }

    pub(in crate::main_window::shell::host) fn drive_setup_flight(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(flight) = self.runtime_setup.flight.clone() else {
            return;
        };
        if !self.setup_enabled()
            || !self
                .runtime_setup
                .services
                .as_ref()
                .is_some_and(|services| services.current())
        {
            flight.cancellation().cancel();
            return;
        }
        if flight.is_pending() {
            return;
        }
        if self.runtime_setup.refresh_command {
            return;
        }
        if let Some(error) = flight.failure() {
            self.setup_failure(&error, true);
            self.setup_command_state("Unavailable", true, cx);
            return;
        }
        if self.runtime_setup.first_preparing {
            if let Err(error) = self.drive_first_conversation(&flight, window, cx) {
                self.setup_failure(&error, true);
                self.setup_command_state("Unavailable", true, cx);
            }
            return;
        }
        if let Some(outcome) = flight.take_reconciliation_outcome() {
            match outcome {
                AdmissionReconciliationOutcome::NotCommitted => {
                    self.setup_failure("The original runtime setup request did not commit.", false);
                    self.runtime_setup.flight = None;
                    self.finish_setup_command(window, cx);
                }
                AdmissionReconciliationOutcome::Committed { admission, receipt } => {
                    self.settle_setup_outcome(
                        &flight,
                        RuntimeAdmissionOutcome::Committed {
                            admission,
                            receipt,
                            later_failure: None,
                            local_finalization: None,
                        },
                        window,
                        cx,
                    );
                }
                outcome @ AdmissionReconciliationOutcome::Pending { .. } => {
                    flight.retain_reconciliation_outcome(outcome);
                    self.setup_command_state("Reconciling…", false, cx);
                    let now = std::time::Instant::now();
                    if self
                        .runtime_setup
                        .reconciliation_retry_at
                        .is_none_or(|at| now >= at)
                    {
                        self.runtime_setup.reconciliation_retry_at =
                            Some(now + Duration::from_millis(250));
                        if let Err(error) = flight.start_reconciliation() {
                            self.setup_failure(&error, true);
                            self.setup_command_state("Unavailable", true, cx);
                        }
                    }
                }
                outcome @ AdmissionReconciliationOutcome::Unavailable { .. } => {
                    flight.retain_reconciliation_outcome(outcome);
                    self.setup_failure("The original runtime setup result cannot be proven. Same-home recovery is required.", true);
                    self.setup_command_state("Unavailable", true, cx);
                }
            }
            return;
        }
        if let Some(outcome) = flight.take_outcome() {
            self.settle_setup_outcome(&flight, outcome, window, cx);
        }
    }

    pub(in crate::main_window::shell::host) fn settle_setup_outcome(
        &mut self,
        flight: &Arc<RuntimeSetupFlight>,
        outcome: RuntimeAdmissionOutcome,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match outcome {
            outcome @ RuntimeAdmissionOutcome::Existing { .. } => {
                if self.runtime_setup.switcher_command_picker.is_some() {
                    self.refresh_switcher_setup_outcome(outcome, window, cx);
                    return;
                }
                self.runtime_setup.flight = None;
                self.runtime_setup.refresh_command = true;
                if let Some(picker) = &self.runtime_setup.picker {
                    picker.update(cx, |picker, cx| {
                        picker.set_pending_page_retry_allowed(true, cx)
                    });
                }
                self.setup_command_state("Refreshing…", false, cx);
                self.refresh_setup_collections(window, cx);
            }
            RuntimeAdmissionOutcome::NotCommitted { error } => {
                self.setup_failure(&error.to_string(), false);
                self.runtime_setup.flight = None;
                self.finish_setup_command(window, cx);
            }
            outcome @ RuntimeAdmissionOutcome::Committed { .. } => {
                let first = matches!(&outcome, RuntimeAdmissionOutcome::Committed { admission, .. }
                    if admission.facts().onboarding().is_some());
                if first {
                    flight.retain_outcome(outcome);
                    self.runtime_setup.first_preparing = true;
                    self.setup_command_state("Preparing conversation…", false, cx);
                } else {
                    if let RuntimeAdmissionOutcome::Committed { admission, .. } = &outcome {
                        if let Err(error) = admission.validate_publication() {
                            flight.retain_outcome(outcome);
                            self.setup_failure(&error.to_string(), true);
                            self.setup_command_state("Unavailable", true, cx);
                            return;
                        }
                    }
                    if self.runtime_setup.switcher_command_picker.is_some() {
                        self.refresh_switcher_setup_outcome(outcome, window, cx);
                        return;
                    }
                    flight.retain_outcome(outcome);
                    self.runtime_setup.refresh_command = true;
                    if let Some(picker) = &self.runtime_setup.picker {
                        picker.update(cx, |picker, cx| {
                            picker.set_pending_page_retry_allowed(true, cx)
                        });
                    }
                    self.setup_command_state("Refreshing…", false, cx);
                    self.refresh_setup_collections(window, cx);
                }
            }
            outcome @ RuntimeAdmissionOutcome::Indeterminate { .. } => {
                flight.retain_outcome(outcome);
                self.setup_command_state("Reconciling…", false, cx);
                if let Err(error) = flight.start_reconciliation() {
                    self.setup_failure(&error, true);
                    self.setup_command_state("Unavailable", true, cx);
                }
            }
        }
        cx.notify();
    }
}
