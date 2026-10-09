use super::*;
use beryl_model::{HomeRevision, RootId, RuntimeId};
use beryl_state::RuntimeRecord;

impl MainWindowShellRoot {
    pub(in crate::main_window::shell::host) fn setup_command_picker(
        &self,
    ) -> Option<Entity<ThreadRootPicker>> {
        if let Some(owner) = &self.runtime_setup.switcher_command_picker {
            let owner = owner.upgrade()?;
            return self
                .thread_switcher
                .picker
                .as_ref()
                .filter(|picker| picker.entity_id() == owner.entity_id())
                .cloned();
        }
        self.runtime_setup.picker.clone()
    }

    pub(in crate::main_window::shell::host) fn begin_switcher_setup_command(
        &mut self,
        command: PickerCommand,
        runtime: Option<RuntimeRecord>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        if !self.setup_enabled() || self.runtime_setup.pending() {
            return Err(
                "Runtime configuration is unavailable while another command is pending.".into(),
            );
        }
        if self
            .controller()
            .is_none_or(|controller| controller.is_threadless())
        {
            return Err("Thread Switcher configuration requires the selected conversation.".into());
        }
        let runtime = match (&command, runtime) {
            (PickerCommand::AddRuntime, None) => None,
            (PickerCommand::AddRoot(key), Some(runtime))
                if *key == PickerRowKey(format!("runtime:{}", runtime.runtime_id())) =>
            {
                Some(runtime.runtime_id())
            }
            _ => return Err("The configuration command does not match its exact runtime.".into()),
        };
        self.sync_runtime_setup(window, cx);
        if !self
            .runtime_setup
            .services
            .as_ref()
            .is_some_and(|services| services.current())
        {
            return Err("Runtime setup services are unavailable.".into());
        }
        let picker = self
            .thread_switcher
            .picker
            .as_ref()
            .ok_or("The invoking Thread Switcher is no longer open.")?;
        self.runtime_setup.switcher_command_picker = Some(picker.downgrade());
        self.runtime_setup.switcher_refresh_target = None;
        self.runtime_setup.command = Some(command);
        if let Some(runtime) = runtime {
            self.prompt_setup_path(None, Some(runtime), window, cx);
        } else {
            self.choose_runtime_form(window, cx);
        }
        Ok(())
    }

    pub(in crate::main_window::shell::host) fn switcher_setup_refresh_target(
        &self,
    ) -> Option<(RuntimeId, Option<RootId>, Option<HomeRevision>)> {
        self.runtime_setup.switcher_refresh_target
    }

    pub(in crate::main_window::shell::host) fn refresh_switcher_setup_outcome(
        &mut self,
        outcome: RuntimeAdmissionOutcome,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let target = match &outcome {
            RuntimeAdmissionOutcome::Existing {
                runtime_id,
                root_id,
            } => (*runtime_id, *root_id, None),
            RuntimeAdmissionOutcome::Committed {
                admission, receipt, ..
            } => (
                admission.facts().runtime_id(),
                Some(admission.facts().root_id()),
                Some(receipt.home_revision()),
            ),
            _ => unreachable!("only settled configuration outcomes request refresh"),
        };
        self.runtime_setup
            .flight
            .as_ref()
            .expect("settled original configuration flight")
            .retain_outcome(outcome);
        self.runtime_setup.switcher_refresh_target = Some(target);
        self.runtime_setup.refresh_command = true;
        self.setup_command_state("Refreshing…", false, cx);
        self.finish_switcher_configuration(true, window, cx);
    }

    pub(in crate::main_window::shell::host) fn complete_switcher_setup_refresh(
        &mut self,
        home_revision: HomeRevision,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        if self.setup_command_picker().is_none() || !self.runtime_setup.refresh_command {
            return Err("The original configuration presentation is no longer current.".into());
        }
        let (_, _, minimum) = self
            .runtime_setup
            .switcher_refresh_target
            .ok_or("The original configuration refresh target is absent.")?;
        if minimum.is_some_and(|minimum| home_revision < minimum) {
            return Err("The frozen collection predates the configuration receipt.".into());
        }
        let flight = self
            .runtime_setup
            .flight
            .clone()
            .ok_or("The original configuration flight is absent.")?;
        let outcome = flight
            .take_outcome()
            .ok_or("The original configuration outcome is absent.")?;
        if let RuntimeAdmissionOutcome::Committed { admission, .. } = &outcome {
            if let Err(error) = admission.validate_publication() {
                flight.retain_outcome(outcome);
                self.setup_failure(&error.to_string(), true);
                self.setup_command_state("Unavailable", true, cx);
                return Err(error.to_string());
            }
        }
        self.runtime_setup.refresh_command = false;
        self.runtime_setup.flight = None;
        self.finish_setup_command(window, cx);
        drop(outcome);
        Ok(())
    }
}
