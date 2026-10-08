use super::*;

impl ThreadRootPicker {
    pub(super) fn tab_focus(
        &mut self,
        backwards: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.collection.cancel_navigation();
        if let Some(runtime) = &mut self.full.runtime {
            runtime.rows.collection.cancel_navigation();
        }
        if let Some(pending) = &self.full.in_flight {
            if !self.full.pending_page_retry_allowed {
                return;
            }
            let handles: Vec<_> = self
                .full
                .command_focus
                .iter()
                .filter_map(|(command, focus)| {
                    (command == pending
                        || (self.full.rendered_commands.contains(command)
                            && matches!(
                                command,
                                PickerCommand::RetryCollection | PickerCommand::RetryRuntime
                            )))
                    .then(|| focus.clone())
                })
                .collect();
            if handles.is_empty() {
                return;
            }
            let index = handles.iter().position(|focus| focus.is_focused(window));
            let next = match (index, backwards) {
                (Some(index), true) => (index + handles.len() - 1) % handles.len(),
                (Some(index), false) => (index + 1) % handles.len(),
                (None, true) => handles.len() - 1,
                (None, false) => 0,
            };
            handles[next].focus(window);
            cx.notify();
            return;
        }
        let mut handles = Vec::new();
        if let Some((_, handle)) = self
            .full
            .command_focus
            .iter()
            .find(|(command, _)| *command == PickerCommand::Return)
        {
            handles.push(handle.clone());
        }
        handles.push(self.search.read(cx).focus_handle(cx));
        handles.push(self.collection_focus.clone());
        for (command, handle) in &self.full.command_focus {
            if *command == PickerCommand::RetryCollection
                && self.full.rendered_commands.contains(command)
            {
                handles.push(handle.clone());
            }
        }
        if let Some(runtime) = &self.full.runtime {
            handles.push(runtime.focus.clone());
            for command in &self.full.rendered_commands {
                if matches!(
                    command,
                    PickerCommand::BrowseRoots(_)
                        | PickerCommand::AddRoot(_)
                        | PickerCommand::RetryRuntime
                ) {
                    if let Some((_, handle)) = self
                        .full
                        .command_focus
                        .iter()
                        .find(|(key, _)| key == command)
                    {
                        handles.push(handle.clone());
                    }
                }
            }
        }
        for command in &self.full.rendered_commands {
            if matches!(
                command,
                PickerCommand::AddRuntime | PickerCommand::Confirm(_)
            ) {
                if let Some((_, handle)) = self
                    .full
                    .command_focus
                    .iter()
                    .find(|(key, _)| key == command)
                {
                    handles.push(handle.clone());
                }
            }
        }
        let index = handles
            .iter()
            .position(|handle| handle.is_focused(window))
            .unwrap_or(0);
        let next = if backwards {
            (index + handles.len() - 1) % handles.len()
        } else {
            (index + 1) % handles.len()
        };
        handles[next].focus(window);
        if handles[next] == self.collection_focus && self.collection.focused_key().is_none() {
            self.collection.focus_position(self.ranges().0.start);
        }
        let runtime_initial = self.runtime_ranges().0.start;
        if let Some(runtime) = &mut self.full.runtime {
            if handles[next] == runtime.focus && runtime.rows.focused_key().is_none() {
                runtime.rows.focus_position(runtime_initial);
            }
        }
        cx.notify();
    }

    pub(super) fn full_key_down(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if let Some(command) = self
            .full
            .command_focus
            .iter()
            .find(|(_, focus)| focus.is_focused(window))
            .map(|(command, _)| command.clone())
        {
            if matches!(event.keystroke.key.as_str(), "enter" | "space") && !event.is_held {
                self.dispatch_command(command, cx);
                cx.stop_propagation();
            }
            return true;
        }
        let Some(runtime) = &mut self.full.runtime else {
            return false;
        };
        if !runtime.focus.is_focused(window) {
            return false;
        }
        if self.full.in_flight.is_some() {
            return true;
        }
        let direction = match event.keystroke.key.as_str() {
            "up" => Some(PickerNavigation::Up),
            "down" => Some(PickerNavigation::Down),
            "home" => Some(PickerNavigation::Home),
            "end" => Some(PickerNavigation::End),
            "pageup" => Some(PickerNavigation::PageUp),
            "pagedown" => Some(PickerNavigation::PageDown),
            _ => None,
        };
        if let Some(direction) = direction {
            let page = (self.config.style.runtime_viewport_height
                / self.config.style.runtime_row_stride())
            .floor() as usize;
            let (position, request) = runtime.rows.collection.navigate(direction, page);
            if let Some(request) = request {
                cx.emit(PickerEvent::RequestRuntimePage(request));
            }
            if let Some(position) = position {
                self.reveal_runtime(position);
            }
            cx.stop_propagation();
            cx.notify();
            return true;
        }
        if event.keystroke.key.as_str() == "enter" && !event.is_held {
            let key = runtime.rows.focused_key().cloned();
            if let Some(key) = key {
                self.dispatch_command(PickerCommand::BrowseRoots(key), cx);
            }
            cx.stop_propagation();
        }
        true
    }
}
