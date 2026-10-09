use super::*;
use crate::app_services::RunningThreadsObservation;
use crate::main_window::creation::command::theme_color;
use beryl_state::ThemePropertyId as Property;
use gpui::{
    AnyElement, AnyView, InteractiveElement, StatefulInteractiveElement, prelude::FluentBuilder,
};

pub(super) struct ThreadNavigationControls {
    pub(super) focus: [gpui::FocusHandle; 2],
    pub(super) status: [Option<NavigationAvailability>; 2],
}

pub(super) struct NavigationAvailability {
    pub(super) thread: beryl_model::SyndicThreadId,
    pub(super) observation: RunningThreadsObservation,
    pub(super) reason: Option<String>,
}

impl ThreadNavigationControls {
    pub(super) fn new(cx: &Context<MainWindowShellRoot>) -> Self {
        Self {
            focus: [cx.focus_handle(), cx.focus_handle()],
            status: [None, None],
        }
    }
}

impl MainWindowShellRoot {
    pub(in crate::main_window::shell) fn focused_thread_navigation_owner(
        &self,
        window: &Window,
    ) -> Option<gpui::FocusHandle> {
        self.thread_navigation
            .focus
            .iter()
            .find(|focus| focus.is_focused(window))
            .cloned()
    }

    pub(super) fn navigation_disabled_reason(
        &self,
        forward: bool,
        cx: &gpui::App,
    ) -> Option<String> {
        if let Some(reason) = self.switcher_disabled_reason(cx) {
            return Some(reason);
        }
        if self.running_threads.has_activation_custody() {
            return Some("This window is waiting for its original thread selection.".into());
        }
        let Some(thread) = self.running_threads.navigation_history.target(forward) else {
            return Some(
                if forward {
                    "No later thread in this window."
                } else {
                    "No earlier thread in this window."
                }
                .into(),
            );
        };
        let Some(reader) = self
            .running_threads
            .reader
            .as_ref()
            .filter(|reader| reader.current())
        else {
            return Some("Thread source is unavailable.".into());
        };
        let Some(status) = self.thread_navigation.status[usize::from(forward)]
            .as_ref()
            .filter(|status| status.thread == thread)
        else {
            return Some("Checking the recorded thread.".into());
        };
        if reader.elect(&status.observation, || ()).is_err() {
            return Some("Checking the recorded thread.".into());
        }
        status.reason.clone()
    }

    pub(crate) fn navigate_thread_history(
        &mut self,
        forward: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.navigation_disabled_reason(forward, cx).is_some() {
            #[cfg(all(test, feature = "test-faults"))]
            self.running_threads.fixture_selection_lease.take();
            return;
        }
        let Some(target) = self
            .running_threads
            .navigation_history
            .begin_movement(forward)
        else {
            #[cfg(all(test, feature = "test-faults"))]
            self.running_threads.fixture_selection_lease.take();
            return;
        };
        if self
            .request_ordinary_thread_activation(target, window, cx, |result, _, cx| {
                if !matches!(
                    result,
                    Ok(running_threads::OrdinaryThreadActivationAcceptance::Admitted)
                ) {
                    cx.notify();
                }
            })
            .is_err()
        {
            self.running_threads.navigation_history.cancel();
            #[cfg(all(test, feature = "test-faults"))]
            self.running_threads.fixture_selection_lease.take();
        }
        cx.notify();
    }
}

pub(super) fn render(
    root: &MainWindowShellRoot,
    forward: bool,
    cx: &mut Context<MainWindowShellRoot>,
) -> AnyElement {
    let label = if forward { "Forward" } else { "Back" };
    let id = if forward {
        "main-window-thread-forward"
    } else {
        "main-window-thread-back"
    };
    let reason = root.navigation_disabled_reason(forward, cx);
    let enabled = reason.is_none();
    let appearance = root.controller().expect("mounted controller").appearance();
    let color = |role, property, fallback| theme_color(appearance, role, property, fallback);
    let border = color("command-button", Property::Border, 0x334155);
    let hover = color("command-button.hover", Property::Background, 0x1e293b);
    let pressed = color("command-button.pressed", Property::Background, 0x263449);
    let ring = color("command-button.focused", Property::Color, 0x38bdf8);
    let source = cx.weak_entity();
    div()
        .id(id)
        .debug_selector(move || id.into())
        .flex_none()
        .w(px(32.))
        .h(px(32.))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(6.))
        .border_1()
        .border_color(border)
        .bg(color("command-button", Property::Background, 0x172033))
        .text_color(color("command-button", Property::Foreground, 0xf1f5f9))
        .text_size(px(18.))
        .track_focus(&root.thread_navigation.focus[usize::from(forward)])
        .tab_stop(true)
        .focus(move |style| style.border_color(ring))
        .when(!enabled, |button| button.opacity(0.55))
        .when(enabled, |button| {
            button
                .cursor_pointer()
                .hover(move |style| style.bg(hover))
                .active(move |style| style.bg(pressed))
                .on_click(cx.listener(move |root, _, window, cx| {
                    root.navigate_thread_history(forward, window, cx)
                }))
                .on_key_down(
                    cx.listener(move |root, event: &gpui::KeyDownEvent, window, cx| {
                        if root.thread_navigation.focus[usize::from(forward)].is_focused(window)
                            && matches!(event.keystroke.key.as_str(), "enter" | "space")
                            && !event.is_held
                        {
                            root.navigate_thread_history(forward, window, cx);
                            cx.stop_propagation();
                        }
                    }),
                )
        })
        .tooltip(move |_, cx| -> AnyView {
            cx.new(|_| NavigationTooltip {
                root: source.clone(),
                forward,
                label,
            })
            .into()
        })
        .child(if forward { "→" } else { "←" })
        .into_any_element()
}

struct NavigationTooltip {
    root: gpui::WeakEntity<MainWindowShellRoot>,
    forward: bool,
    label: &'static str,
}

impl Render for NavigationTooltip {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let text = self
            .root
            .upgrade()
            .map(|root| {
                root.read(cx)
                    .navigation_tooltip(self.forward, self.label, cx)
            })
            .unwrap_or_else(|| self.label.into());
        div()
            .max_w(px(420.))
            .px(px(8.))
            .py(px(6.))
            .rounded(px(5.))
            .bg(gpui::rgb(0x172033))
            .text_color(gpui::rgb(0xf1f5f9))
            .text_size(px(12.))
            .child(text)
    }
}

impl MainWindowShellRoot {
    fn navigation_tooltip(&self, forward: bool, label: &str, cx: &gpui::App) -> String {
        self.navigation_disabled_reason(forward, cx)
            .unwrap_or_else(|| label.into())
    }

    #[cfg(test)]
    pub(crate) fn test_thread_navigation_reason(
        &self,
        forward: bool,
        app: &gpui::App,
    ) -> Option<String> {
        self.navigation_disabled_reason(forward, app)
    }

    #[cfg(test)]
    pub(crate) fn test_thread_navigation_focus(&self, forward: bool) -> gpui::FocusHandle {
        self.thread_navigation.focus[usize::from(forward)].clone()
    }
}
