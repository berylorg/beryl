use super::render::PickerTooltip;
use super::*;
use beryl_state::ThemePropertyId as Property;
use gpui::{
    AnyElement, InteractiveElement, IntoElement, ParentElement, StatefulInteractiveElement, Styled,
    div, prelude::FluentBuilder,
};

impl ThreadRootPicker {
    pub(super) fn render_command(
        &mut self,
        command: PickerCommand,
        mut state: PickerCommandState,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        if let Some(current) = self.command_state(&command) {
            state = current;
        }
        let same_control = |existing: &PickerCommand| {
            existing == &command
                || matches!(
                    (existing, &command),
                    (PickerCommand::Confirm(_), PickerCommand::Confirm(_))
                )
        };
        let focus = if let Some((existing, focus)) = self
            .full
            .command_focus
            .iter_mut()
            .find(|(existing, _)| same_control(existing))
        {
            *existing = command.clone();
            focus.clone()
        } else {
            let focus = cx.focus_handle();
            self.full
                .command_focus
                .push((command.clone(), focus.clone()));
            focus
        };
        self.full.rendered_commands.push(command.clone());
        state.pending |= self.full.in_flight.as_ref() == Some(&command);
        let tooltip = state
            .unavailable_reason
            .clone()
            .or_else(|| state.pending.then(|| "This command is pending.".to_owned()));
        let disabled = !state.can_dispatch();
        let id = match &command {
            PickerCommand::Return => "thread-root-picker-return".into(),
            PickerCommand::AddRuntime => "thread-root-picker-add-runtime".into(),
            PickerCommand::Confirm(_) => "thread-root-picker-confirm".into(),
            PickerCommand::RetryCollection => "thread-root-picker-retry-collection".into(),
            PickerCommand::RetryRuntime => "thread-root-picker-retry-runtime".into(),
            PickerCommand::BrowseRoots(key) => format!("thread-root-picker-browse-roots-{}", key.0),
            PickerCommand::AddRoot(key) => format!("thread-root-picker-add-root-{}", key.0),
        };
        let id = gpui::SharedString::from(id);
        let pointer_focus = focus.clone();
        let page_retry = matches!(
            command,
            PickerCommand::RetryCollection | PickerCommand::RetryRuntime
        );
        let role = if disabled {
            "command-button.disabled"
        } else {
            "command-button"
        };
        let font = self.font("command-button", 13., 500.);
        let runtime_identity = matches!(
            command,
            PickerCommand::BrowseRoots(_) | PickerCommand::AddRoot(_)
        )
        .then(|| {
            self.full.runtime.as_ref().map(|runtime| {
                (
                    runtime.rows.collection.key.clone(),
                    runtime.rows.collection.revision,
                )
            })
        })
        .flatten();
        let mut button = div()
            .id(id.clone())
            .debug_selector(move || id.to_string())
            .track_focus(&focus)
            .tab_stop(true)
            .h(px(self.config.style.command_height))
            .flex_shrink_0()
            .px(px(12.))
            .rounded(px(6.))
            .border(px(1.))
            .border_color(self.color(role, Property::Border, 0xcbd5e1))
            .bg(self.color(
                role,
                Property::Background,
                if disabled { 0xf1f5f9 } else { 0xf8fafc },
            ))
            .text_color(self.color(
                role,
                Property::Foreground,
                if disabled { 0x94a3b8 } else { 0x1f2937 },
            ))
            .text_size(px(font.0))
            .font_weight(font.1)
            .when_some(font.2, |button, font| button.font_family(font))
            .flex()
            .items_center()
            .justify_center()
            .whitespace_nowrap()
            .when(!disabled, |button| {
                button.hover(|style| {
                    style.bg(self.color("command-button.hover", Property::Background, 0xeef2f7))
                })
            })
            .on_mouse_down(
                gpui::MouseButton::Left,
                cx.listener(move |this, _, window, cx| {
                    if this.full.native_dialog_open
                        || (this.full.in_flight.is_some()
                            && !(page_retry && this.full.pending_page_retry_allowed))
                    {
                        cx.stop_propagation();
                        return;
                    }
                    this.collection.cancel_navigation();
                    if let Some(runtime) = &mut this.full.runtime {
                        runtime.rows.collection.cancel_navigation();
                    }
                    pointer_focus.focus(window);
                    cx.stop_propagation();
                }),
            )
            .on_click(cx.listener(move |this, _, _, cx| {
                cx.stop_propagation();
                if runtime_identity.as_ref().is_some_and(|(key, revision)| {
                    this.full.runtime.as_ref().is_none_or(|runtime| {
                        runtime.rows.collection.key != *key
                            || runtime.rows.collection.revision != *revision
                    })
                }) {
                    return;
                }
                this.dispatch_command(command.clone(), cx);
            }))
            .child(if state.pending {
                format!("{}…", state.label)
            } else {
                state.label
            });
        if let Some(tooltip) = tooltip {
            button = button.tooltip(move |_, cx| cx.new(|_| PickerTooltip(tooltip.clone())).into());
        }
        button.into_any_element()
    }
}
