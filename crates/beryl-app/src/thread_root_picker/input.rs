use super::*;

impl ThreadRootPicker {
    pub(super) fn change_query(&mut self, query: String, cx: &mut Context<Self>) {
        if self.dismissed
            || self.query == query
            || self.full.native_dialog_open
            || self.full.in_flight.is_some()
        {
            if self.query != query {
                let retained = self.query.clone();
                self.search
                    .update(cx, |input, cx| input.set_text(retained, cx));
            }
            return;
        }
        let Some(revision) = self.collection.revision.checked_add(1) else {
            self.collection.failure = Some("Search revision capacity was exhausted.".into());
            cx.notify();
            return;
        };
        self.collection
            .replace(self.collection.key.clone(), revision, self.collection.total);
        self.query = query.clone();
        self.invalidate_selection_eligibility();
        self.activation_in_flight = None;
        cx.emit(PickerEvent::QueryChanged {
            collection_key: self.collection.key.clone(),
            query_revision: revision,
            query,
        });
        self.request_initial_page(cx);
        cx.notify();
    }

    pub(super) fn key_down(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.full.native_dialog_open {
            return;
        }
        match event.keystroke.key.as_str() {
            "escape" => {
                cx.stop_propagation();
                self.dismiss(window, cx);
            }
            "tab" => {
                if self.full.in_flight.is_some() && !self.full.pending_page_retry_allowed {
                    cx.stop_propagation();
                    return;
                }
                self.tab_focus(event.keystroke.modifiers.shift, window, cx);
                cx.stop_propagation();
                cx.notify();
            }
            _ if self.full_key_down(event, window, cx) => {}
            key if self.collection_focus.is_focused(window) => {
                let direction = match key {
                    "up" => Some(PickerNavigation::Up),
                    "down" => Some(PickerNavigation::Down),
                    "home" => Some(PickerNavigation::Home),
                    "end" => Some(PickerNavigation::End),
                    "pageup" => Some(PickerNavigation::PageUp),
                    "pagedown" => Some(PickerNavigation::PageDown),
                    _ => None,
                };
                if let Some(direction) = direction {
                    cx.stop_propagation();
                    self.navigate(direction, window, cx);
                } else if key == "enter" && !event.is_held {
                    cx.stop_propagation();
                    if let Some(key) = self.collection.focused_key().cloned() {
                        self.activate(&key, cx);
                    }
                }
            }
            _ => {}
        }
    }
}
