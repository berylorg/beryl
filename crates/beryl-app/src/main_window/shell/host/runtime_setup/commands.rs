use super::*;
use beryl_model::{RuntimeId, WindowId};

mod admission;
mod native;

impl MainWindowShellRoot {
    pub(in crate::main_window::shell::host) fn setup_members(
        &self,
        cx: &App,
    ) -> Result<Vec<WindowId>, String> {
        #[cfg(all(test, feature = "test-faults"))]
        if !self.runtime_setup.fixture_members.is_empty() {
            return Ok(self.runtime_setup.fixture_members.clone());
        }
        #[cfg(target_os = "windows")]
        {
            crate::running_owner::RunningProcessOwner::mounted_owner(cx)
                .and_then(|owner| owner.upgrade())
                .ok_or("Runtime setup process is unavailable.")?
                .borrow()
                .runtime_setup_members(cx)
        }
        #[cfg(not(target_os = "windows"))]
        {
            let _ = cx;
            Err("Runtime setup process is unavailable.".into())
        }
    }

    pub(in crate::main_window::shell::host) fn setup_command(
        &mut self,
        command: PickerCommand,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.setup_enabled() || self.runtime_setup.pending() {
            return;
        }
        match command {
            PickerCommand::AddRuntime => {
                self.runtime_setup.command = Some(PickerCommand::AddRuntime);
                self.choose_runtime_form(window, cx);
            }
            PickerCommand::AddRoot(ref key) => {
                let runtime = self
                    .runtime_setup
                    .runtimes
                    .iter()
                    .find(|row| {
                        *key == PickerRowKey(format!("runtime:{}", row.runtime.runtime_id()))
                    })
                    .map(|row| row.runtime.runtime_id());
                if let Some(runtime) = runtime {
                    self.runtime_setup.command = Some(command);
                    self.prompt_setup_path(None, Some(runtime), window, cx);
                } else {
                    self.setup_failure("The selected runtime is no longer available.", false);
                    self.runtime_setup.command = Some(command);
                    self.finish_setup_command(window, cx);
                }
            }
            PickerCommand::BrowseRoots(ref key) => {
                if let Some(runtime) = self
                    .runtime_setup
                    .runtimes
                    .iter()
                    .find(|row| {
                        *key == PickerRowKey(format!("runtime:{}", row.runtime.runtime_id()))
                    })
                    .map(|row| row.runtime.runtime_id())
                {
                    self.setup_scope(Some(runtime), window, cx);
                }
            }
            PickerCommand::Return => self.setup_scope(None, window, cx),
            PickerCommand::Confirm(key) => self.begin_thread_confirmation(key, window, cx),
            PickerCommand::RetryCollection | PickerCommand::RetryRuntime => {}
        }
        cx.notify();
    }

    pub(in crate::main_window::shell::host) fn setup_scope(
        &mut self,
        scope: Option<RuntimeId>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.runtime_setup.pending() || self.runtime_setup.scope == scope {
            return;
        }
        self.runtime_setup.scope = scope;
        self.runtime_setup.query =
            beryl_state::CatalogNormalizedQuery::new("").expect("empty query");
        let keep_selected = self
            .runtime_setup
            .selected_root
            .as_ref()
            .is_some_and(|row| scope.is_none_or(|runtime| runtime == row.runtime.runtime_id()));
        if !keep_selected {
            self.runtime_setup.selected_root = None;
        }
        let heading = scope
            .and_then(|runtime| {
                self.runtime_setup
                    .runtimes
                    .iter()
                    .find(|row| row.runtime.runtime_id() == runtime)
                    .map(|row| format!("ROOTS FOR {}", row.runtime.environment_label()))
            })
            .unwrap_or_else(|| "ROOTS FOR ALL RUNTIMES".into());
        if let Some(picker) = &self.runtime_setup.picker {
            picker.update(cx, |picker, pcx| {
                picker.clear_search(pcx);
                picker.set_return_command(
                    scope.map(|_| PickerCommandState::enabled("All runtimes")),
                    pcx,
                );
                picker.set_collection_labels(
                    "Choose a root before confirming a new thread.".into(),
                    heading,
                    "No roots match this scope and search.".into(),
                    pcx,
                );
                if !keep_selected {
                    picker.set_selected_key(None, pcx);
                }
            });
        }
        self.refresh_setup_collections(window, cx);
        self.sync_thread_confirmation(cx);
    }

    pub(in crate::main_window::shell::host) fn setup_command_state(
        &mut self,
        label: &str,
        terminal: bool,
        cx: &mut Context<Self>,
    ) {
        if let (Some(picker), Some(command)) =
            (self.setup_command_picker(), &self.runtime_setup.command)
        {
            let state = if terminal {
                PickerCommandState::unavailable(
                    label,
                    self.runtime_setup
                        .unavailable
                        .as_deref()
                        .unwrap_or("This request cannot be repeated."),
                )
            } else {
                let mut state = PickerCommandState::enabled(label);
                state.pending = true;
                state
            };
            picker.update(cx, |picker, pcx| {
                if terminal {
                    picker.set_pending_page_retry_allowed(false, pcx);
                }
                picker.update_command_state(command, state, pcx)
            });
        }
    }
}
