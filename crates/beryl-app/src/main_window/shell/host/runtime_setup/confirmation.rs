use super::*;
use crate::same_window_thread_acquisition::SameWindowThreadRequest;

impl MainWindowShellRoot {
    pub(in crate::main_window::shell::host) fn sync_thread_confirmation(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        if self.runtime_setup.pending() {
            return;
        }
        let current_source = self
            .runtime_setup
            .services
            .as_ref()
            .is_some_and(|services| services.current())
            && self.runtime_setup.bootstrap_ready;
        let eligible = current_source
            && self
                .runtime_setup
                .selected_root
                .as_ref()
                .is_some_and(|row| {
                    self.runtime_setup
                        .scope
                        .is_none_or(|scope| scope == row.runtime.runtime_id())
                        && row.root.runtime_id() == row.runtime.runtime_id()
                });
        let state = if eligible
            && self
                .controller()
                .is_some_and(|controller| !controller.is_threadless())
        {
            PickerCommandState::enabled("Confirm")
        } else {
            PickerCommandState::unavailable("Confirm", "Choose a root before confirming.")
        };
        if let Some(picker) = &self.runtime_setup.picker {
            picker.update(cx, |picker, pcx| {
                picker.configure_selection(PickerSelectionMode::Confirmed { confirm: state }, pcx);
                if let Some(row) = &self.runtime_setup.selected_root {
                    picker.set_selection_eligibility(
                        &PickerRowKey(format!("root:{}", row.root.root_id())),
                        None,
                        pcx,
                    );
                }
            });
        }
    }

    pub(in crate::main_window::shell::host) fn begin_thread_confirmation(
        &mut self,
        key: PickerRowKey,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let command = PickerCommand::Confirm(key.clone());
        let result = (|| {
            if !self.runtime_setup.bootstrap_ready
                || !self
                    .runtime_setup
                    .services
                    .as_ref()
                    .is_some_and(|services| services.current())
            {
                return Err("The root collection is awaiting a current source.".into());
            }
            let row = self
                .runtime_setup
                .selected_root
                .as_ref()
                .filter(|row| {
                    key == PickerRowKey(format!("root:{}", row.root.root_id()))
                        && self
                            .runtime_setup
                            .scope
                            .is_none_or(|scope| scope == row.runtime.runtime_id())
                })
                .ok_or("The selected root is no longer available in this scope.")?;
            let selected = self
                .cached_running_selection(cx)
                .ok_or("The invoking conversation is unavailable.")?
                .0;
            let mut thread = [0; 16];
            let mut draft = [0; 16];
            getrandom::fill(&mut thread).map_err(|error| error.to_string())?;
            getrandom::fill(&mut draft).map_err(|error| error.to_string())?;
            let at = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_err(|error| error.to_string())?
                .as_millis();
            let at = u64::try_from(at).map_err(|error| error.to_string())?;
            SameWindowThreadRequest::new(
                selected.window_id(),
                Some(selected.claim()),
                beryl_state::RememberedTarget::new(row.runtime.runtime_id(), row.root.root_id()),
                beryl_model::SyndicThreadId::from_bytes(thread),
                beryl_model::SyndicDraftId::from_bytes(draft),
                beryl_model::ExecutionBinding::new(
                    row.runtime.runtime_id(),
                    row.root.root_id(),
                    row.root.canonical_path().clone(),
                ),
                syndic_storage::SyndicTimestamp::from_unix_millis(at),
                syndic_storage::DraftEditHistoryPolicyV1::new(8 * 1024 * 1024, 1)
                    .ok_or("Thread history policy is invalid")?,
            )
            .map_err(|error| error.to_string())
        })();
        self.runtime_setup.command = Some(command);
        match result.and_then(|request| self.start_thread_creation(request, window, cx)) {
            Ok(()) => self.setup_command_state("Confirm", false, cx),
            Err(error) => {
                self.setup_failure(&error, false);
                self.finish_setup_command(window, cx);
                self.sync_thread_confirmation(cx);
            }
        }
        cx.notify();
    }

    pub(in crate::main_window::shell::host) fn finish_thread_confirmation(
        &mut self,
        success: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.finish_setup_command(window, cx);
        if success {
            self.runtime_setup.picker = None;
            self.runtime_setup.subscription = None;
            window.focus(&self.shell_focus);
        } else {
            self.sync_thread_confirmation(cx);
        }
    }
}
