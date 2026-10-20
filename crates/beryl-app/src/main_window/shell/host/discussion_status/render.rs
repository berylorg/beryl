use super::*;
use crate::main_window::creation::command::{theme_color, theme_font};
use beryl_state::ThemePropertyId as Property;

pub(in crate::main_window::shell::host) const HEIGHT: f32 = 24.0;

pub(in crate::main_window::shell::host) fn render(
    root: &MainWindowShellRoot,
    cx: &mut Context<MainWindowShellRoot>,
) -> Option<AnyElement> {
    let snapshot = root.selected_discussion_snapshot(cx)?;
    let status = snapshot.status()?;
    let label = match status {
        DiscussionStatus::Open => "Open",
        DiscussionStatus::ResolutionPending => "Resolution pending",
        DiscussionStatus::HandingOff => "Handing off",
        DiscussionStatus::HandoffFailed => "Handoff failed",
        DiscussionStatus::Unavailable => "Unavailable",
        DiscussionStatus::Archived => "Archived",
    };
    let generation = &root.controller.as_ref()?.appearance.generation;
    let color = |role, property, fallback| theme_color(generation, role, property, fallback);
    let background = color("segmented-status-bar", Property::Background, 0xf8fafc);
    let foreground = color("segmented-status-bar", Property::Foreground, 0x334155);
    let divider = color(
        "segmented-status-bar.divider",
        Property::Background,
        0xcbd5e1,
    );
    let font = theme_font(generation, "segmented-status-bar", 12.0, 400.0);
    let key_font = theme_font(generation, "segmented-status-bar.key", font.size, 400.0);
    let value_font = theme_font(generation, "segmented-status-bar.value", font.size, 500.0);
    let passive = div()
        .flex()
        .items_center()
        .flex_1()
        .min_w_0()
        .h_full()
        .px(px(8.0))
        .gap(px(6.0))
        .overflow_hidden()
        .whitespace_nowrap()
        .child(
            div()
                .flex_none()
                .text_color(color(
                    "segmented-status-bar.key",
                    Property::Foreground,
                    0x64748b,
                ))
                .font_weight(gpui::FontWeight(key_font.weight))
                .child("DISCUSSION"),
        )
        .child(
            div()
                .min_w_0()
                .text_ellipsis()
                .overflow_hidden()
                .text_color(color(
                    "segmented-status-bar.value",
                    Property::Foreground,
                    0x0f172a,
                ))
                .font_weight(gpui::FontWeight(value_font.weight))
                .child(label),
        );
    let mut strip = div()
        .id("main-window-discussion-status")
        .debug_selector(move || format!("main-window-discussion-status:{label}"))
        .flex()
        .flex_none()
        .h(px(HEIGHT))
        .overflow_hidden()
        .border_t_1()
        .border_color(color(
            "segmented-status-bar",
            Property::BorderColor,
            0xcbd5e1,
        ))
        .bg(background)
        .text_color(foreground)
        .text_size(px(font.size))
        .child(passive);
    let pending_descriptor = root
        .discussion_status
        .pending
        .as_ref()
        .filter(|flight| {
            root.discussion_composer(cx).is_some_and(|composer| {
                composer.entity_id() == flight.owner
                    && composer.read(cx).selection_identity() == flight.selection
                    && Arc::ptr_eq(
                        &composer.read(cx).discussion_publication_epoch(),
                        &flight.epoch,
                    )
            })
        })
        .map(|flight| flight.descriptor.clone());
    if let Some(descriptor) = snapshot.retry().cloned().or(pending_descriptor) {
        let composer = root.discussion_composer(cx)?;
        let owner = composer.entity_id();
        let selection = composer.read(cx).selection_identity();
        let epoch = composer.read(cx).discussion_publication_epoch();
        let pending = root.discussion_status.pending.is_some() || snapshot.retry_pending();
        let blocked = root.status_mutation_gate().or_else(|| {
            root.running_selection_interaction_gated(cx)
                .then_some("The selected conversation is being changed.")
        });
        let enabled = !pending
            && !snapshot.retry_pending()
            && composer.read(cx).discussion_presentation_fresh()
            && snapshot.current()
            && blocked.is_none()
            && composer
                .read(cx)
                .discussion_presentation_reader()
                .is_some_and(|reader| reader.current());
        let reason = if pending {
            "The exact handoff retry is awaiting its outcome."
        } else {
            blocked.unwrap_or("The handoff authority is unavailable.")
        }
        .to_owned();
        let hover = color(
            "segmented-status-bar.segment.hover",
            Property::Background,
            0xeef2f7,
        );
        let pressed = color(
            "segmented-status-bar.segment.pressed",
            Property::Background,
            0xe2e8f0,
        );
        let disabled = color(
            "segmented-status-bar.segment.disabled",
            Property::Foreground,
            0x94a3b8,
        );
        let tooltip_background = color("tooltip", Property::Background, 0x111827);
        let tooltip_foreground = color("tooltip.text", Property::Foreground, 0xffffff);
        let action = div()
            .id("main-window-discussion-retry")
            .debug_selector(move || {
                format!(
                    "main-window-discussion-retry:{}",
                    if enabled { "enabled" } else { "disabled" }
                )
            })
            .flex_none()
            .min_w_0()
            .h_full()
            .px(px(8.0))
            .flex()
            .items_center()
            .overflow_hidden()
            .whitespace_nowrap()
            .text_ellipsis()
            .when(enabled, |action| {
                action
                    .cursor_pointer()
                    .hover(move |style| style.bg(hover))
                    .active(move |style| style.bg(pressed))
                    .on_mouse_down(gpui::MouseButton::Left, |_, window, _| {
                        window.prevent_default()
                    })
                    .on_click(cx.listener(move |root, _, window, cx| {
                        root.retry_discussion_handoff(
                            owner,
                            selection,
                            epoch.clone(),
                            descriptor.clone(),
                            window,
                            cx,
                        )
                    }))
            })
            .when(!enabled, |action| {
                action.text_color(disabled).tooltip(move |_, cx| {
                    cx.new(|_| DiscussionTooltip {
                        text: reason.clone(),
                        background: tooltip_background,
                        foreground: tooltip_foreground,
                    })
                    .into()
                })
            })
            .child("Retry handoff");
        strip = strip
            .child(div().flex_none().w(px(1.0)).h_full().bg(divider))
            .child(action);
    }
    Some(strip.into_any_element())
}

struct DiscussionTooltip {
    text: String,
    background: gpui::Rgba,
    foreground: gpui::Rgba,
}
impl Render for DiscussionTooltip {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .max_w(px(320.0))
            .px(px(8.0))
            .py(px(6.0))
            .rounded(px(5.0))
            .bg(self.background)
            .text_color(self.foreground)
            .text_size(px(12.0))
            .child(self.text.clone())
    }
}
