use super::*;

impl ThreadRootPicker {
    pub fn dispatch_command(&mut self, command: PickerCommand, cx: &mut Context<Self>) {
        if self.dismissed
            || self.full.native_dialog_open
            || self.full.in_flight.as_ref() == Some(&command)
        {
            return;
        }
        let Some(state) = self.command_state(&command) else {
            return;
        };
        if !state.can_dispatch() {
            return;
        }
        match command {
            PickerCommand::RetryCollection => {
                if let Some(request) = self.collection.retry() {
                    cx.emit(PickerEvent::RequestPage(request));
                }
                cx.notify();
                return;
            }
            PickerCommand::RetryRuntime => {
                if let Some(request) = self
                    .full
                    .runtime
                    .as_mut()
                    .and_then(|runtime| runtime.rows.collection.retry())
                {
                    cx.emit(PickerEvent::RequestRuntimePage(request));
                }
                cx.notify();
                return;
            }
            _ => {}
        }
        if matches!(
            command,
            PickerCommand::AddRoot(_) | PickerCommand::AddRuntime | PickerCommand::Confirm(_)
        ) {
            if self.full.in_flight.is_some() {
                return;
            }
            self.full.in_flight = Some(command.clone());
            self.full.in_flight_state = Some(state);
            self.full.pending_page_retry_allowed = false;
            self.collection.cancel_navigation();
            if let Some(runtime) = &mut self.full.runtime {
                runtime.rows.collection.cancel_navigation();
            }
        }
        cx.emit(PickerEvent::Command(command));
        cx.notify();
    }

    pub fn finish_command(&mut self, command: &PickerCommand, cx: &mut Context<Self>) {
        if self.full.in_flight.as_ref() == Some(command) {
            self.full.in_flight = None;
            self.full.in_flight_state = None;
            self.full.pending_page_retry_allowed = false;
        }
        cx.notify();
    }

    pub fn set_pending_page_retry_allowed(&mut self, allowed: bool, cx: &mut Context<Self>) {
        if self.dismissed {
            return;
        }
        self.full.pending_page_retry_allowed = allowed && self.full.in_flight.is_some();
        cx.notify();
    }

    pub fn update_command_state(
        &mut self,
        command: &PickerCommand,
        state: PickerCommandState,
        cx: &mut Context<Self>,
    ) {
        if self.dismissed {
            return;
        }
        if self.full.in_flight.as_ref() == Some(command) {
            self.full.in_flight_state = Some(state.clone());
            if state.pending {
                cx.notify();
                return;
            }
        }
        match command {
            PickerCommand::Return => self.full.return_command = Some(state),
            PickerCommand::AddRuntime => {
                if let Some(runtime) = &mut self.full.runtime {
                    runtime.config.add_runtime = state;
                }
            }
            PickerCommand::BrowseRoots(key) | PickerCommand::AddRoot(key) => {
                if let Some(runtime) = &mut self.full.runtime {
                    runtime.rows.update_row_command(
                        key,
                        matches!(command, PickerCommand::BrowseRoots(_)),
                        state,
                    );
                }
            }
            PickerCommand::Confirm(_) => {
                if let PickerSelectionMode::Confirmed { confirm } = &mut self.full.selection {
                    *confirm = state;
                }
            }
            PickerCommand::RetryCollection => self.full.collection_retry = Some(state),
            PickerCommand::RetryRuntime => self.full.runtime_retry = Some(state),
        }
        cx.notify();
    }

    pub fn set_native_dialog_open(&mut self, open: bool, cx: &mut Context<Self>) {
        if self.dismissed {
            return;
        }
        self.full.native_dialog_open = open;
        cx.notify();
    }

    pub fn restore_command_focus(
        &mut self,
        command: &PickerCommand,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some((_, focus)) = self
            .full
            .command_focus
            .iter()
            .find(|(key, _)| key == command)
        {
            focus.focus(window);
        }
        cx.notify();
    }

    pub fn command_focus_handle(&self, command: &PickerCommand) -> Option<FocusHandle> {
        self.full
            .command_focus
            .iter()
            .find(|(key, _)| key == command)
            .map(|(_, focus)| focus.clone())
    }

    pub fn command_state(&self, command: &PickerCommand) -> Option<PickerCommandState> {
        let mut state = if self.full.in_flight.as_ref() == Some(command) {
            self.full.in_flight_state.clone()?
        } else {
            match command {
                PickerCommand::RetryCollection => {
                    self.collection.failed_request.as_ref()?;
                    let mut state = self.full.collection_retry.clone()?;
                    state.pending |= self.collection.retry_pending();
                    state
                }
                PickerCommand::RetryRuntime => {
                    let collection = &self.full.runtime.as_ref()?.rows.collection;
                    collection.failed_request.as_ref()?;
                    let mut state = self.full.runtime_retry.clone()?;
                    state.pending |= collection.retry_pending();
                    state
                }
                PickerCommand::Return => self.full.return_command.clone()?,
                PickerCommand::AddRuntime => self.full.runtime.as_ref()?.config.add_runtime.clone(),
                PickerCommand::BrowseRoots(key) | PickerCommand::AddRoot(key) => {
                    let runtime = self.full.runtime.as_ref()?;
                    if !runtime.rows.collection.is_current() {
                        return None;
                    }
                    let row = runtime
                        .rows
                        .collection
                        .pages
                        .iter()
                        .find_map(|page| {
                            page.rows
                                .iter()
                                .position(|row| &row.key == key)
                                .map(|index| page.request.range.start + index)
                        })
                        .and_then(|position| runtime.rows.row(position))?;
                    if matches!(command, PickerCommand::BrowseRoots(_)) {
                        row.browse_roots.clone()
                    } else {
                        row.add_root.clone()
                    }
                }
                PickerCommand::Confirm(key) => {
                    if self.full.selected.as_ref() != Some(key) {
                        return None;
                    }
                    let PickerSelectionMode::Confirmed { confirm } = &self.full.selection else {
                        return None;
                    };
                    let mut state = confirm.clone();
                    match &self.full.eligibility {
                        Some(eligibility)
                            if eligibility.key == *key
                                && eligibility.collection_key == self.collection.key
                                && eligibility.revision == self.collection.revision =>
                        {
                            if let Some(reason) = &eligibility.unavailable_reason {
                                state.unavailable_reason = Some(reason.clone());
                            }
                        }
                        _ => {
                            state.unavailable_reason =
                                Some("The selected root is awaiting validation.".into());
                        }
                    }
                    if !self.collection.is_current() {
                        state.unavailable_reason = Some("The root collection is loading.".into());
                    }
                    if let Some(row) = self
                        .collection
                        .pages
                        .iter()
                        .flat_map(|page| &page.rows)
                        .find(|row| &row.key == key)
                    {
                        if let Some(reason) = &row.unavailable_reason {
                            state.unavailable_reason = Some(reason.clone());
                        }
                        state.pending |= row.activation_pending;
                    }
                    state
                }
            }
        };
        state.pending |= self.full.in_flight.as_ref() == Some(command);
        if self.full.native_dialog_open {
            state.unavailable_reason = Some("A platform dialog is open.".into());
        } else if self.full.in_flight.is_some()
            && self.full.in_flight.as_ref() != Some(command)
            && !(self.full.pending_page_retry_allowed
                && matches!(
                    command,
                    PickerCommand::RetryCollection | PickerCommand::RetryRuntime
                ))
        {
            state.unavailable_reason = Some("Another setup command is pending.".into());
        }
        Some(state)
    }
}
