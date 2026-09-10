use gpui::{
    AppContext, Application, Bounds, Context, FocusHandle, InteractiveElement, IntoElement,
    KeyDownEvent, MouseButton, ParentElement, Render, SharedString, StatefulInteractiveElement,
    Styled, TitlebarOptions, Window, WindowBounds, WindowOptions, div, px, rgb, size,
};

#[cfg(all(target_os = "windows", not(test)))]
mod clipboard;

pub fn present(report: String) {
    if report.len() > super::record::MAX_REPORT_BYTES {
        return;
    }
    Application::new().run(move |cx| {
        let bounds = Bounds::centered(None, size(px(660.), px(460.)), cx);
        if cx
            .open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    titlebar: Some(TitlebarOptions {
                        title: Some("Beryl — Internal error".into()),
                        ..Default::default()
                    }),
                    is_resizable: false,
                    is_minimizable: false,
                    window_min_size: Some(bounds.size),
                    ..Default::default()
                },
                move |window, cx| {
                    window.on_window_should_close(cx, |_, cx| {
                        cx.quit();
                        true
                    });
                    cx.new(|cx| CrashReport::new(report, window, cx))
                },
            )
            .is_err()
        {
            cx.quit();
        }
        cx.activate(true);
    });
}

struct CrashReport {
    report: String,
    preview: Vec<SharedString>,
    omitted: bool,
    copy_focus: FocusHandle,
    exit_focus: FocusHandle,
    feedback: &'static str,
    #[cfg(test)]
    commands: tests::Commands,
}

impl CrashReport {
    fn new(report: String, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let (preview, omitted) = preview(&report);
        let copy_focus = cx.focus_handle();
        copy_focus.focus(window);
        Self {
            report,
            preview,
            omitted,
            copy_focus,
            exit_focus: cx.focus_handle(),
            feedback: "",
            #[cfg(test)]
            commands: tests::Commands::default(),
        }
    }

    fn copy(&mut self, cx: &mut Context<Self>) {
        #[cfg(all(target_os = "windows", not(test)))]
        let copied = clipboard::copy(&self.report).is_ok();
        #[cfg(all(not(target_os = "windows"), not(test)))]
        let copied = false;
        #[cfg(test)]
        let copied = {
            self.commands.copies.push(self.report.clone());
            self.commands.copy_success
        };
        self.feedback = if copied {
            "Report copied."
        } else {
            "Could not copy the report. Try again."
        };
        cx.notify();
    }

    fn exit(&mut self, cx: &mut Context<Self>) {
        #[cfg(test)]
        {
            self.commands.exits += 1;
            cx.notify();
        }
        #[cfg(not(test))]
        cx.quit();
    }

    fn key(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let modifiers = event.keystroke.modifiers;
        if modifiers.control || modifiers.alt || modifiers.platform || event.is_held {
            return;
        }
        match event.keystroke.key.as_str() {
            "tab" => {
                if self.copy_focus.is_focused(window) {
                    self.exit_focus.focus(window);
                } else {
                    self.copy_focus.focus(window);
                }
            }
            _ => return,
        }
        cx.stop_propagation();
        cx.notify();
    }
}

impl Render for CrashReport {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let copy_focus = self.copy_focus.clone();
        let exit_focus = self.exit_focus.clone();
        div()
            .id("crash-report")
            .debug_selector(|| "crash-report".to_owned())
            .size_full()
            .flex()
            .flex_col()
            .gap(px(14.))
            .p(px(24.))
            .bg(rgb(0xf8fafc))
            .text_color(rgb(0x1f2937))
            .font_family("Segoe UI")
            .text_size(px(14.))
            .on_key_down(cx.listener(Self::key))
            .child(
                div()
                    .text_size(px(20.))
                    .child("Beryl stopped because of an internal error"),
            )
            .child("The application has stopped. You can copy this report before closing it.")
            .child(
                div()
                    .id("crash-report-preview")
                    .debug_selector(|| "crash-report-preview".to_owned())
                    .flex_1()
                    .min_h_0()
                    .overflow_hidden()
                    .bg(rgb(0xffffff))
                    .border_1()
                    .border_color(rgb(0xcbd5e1))
                    .rounded_md()
                    .p(px(12.))
                    .font_family("Consolas")
                    .text_size(px(13.))
                    .children(
                        self.preview
                            .iter()
                            .cloned()
                            .map(|line| div().h(px(18.)).truncate().child(line)),
                    )
                    .child(
                        div()
                            .debug_selector(|| "crash-report-preview-omission".to_owned())
                            .h(px(18.))
                            .child(if self.omitted {
                                "[Preview shortened; Copy includes the full report]"
                            } else {
                                ""
                            }),
                    ),
            )
            .child(
                div()
                    .id("crash-report-copy-result")
                    .debug_selector(|| "crash-report-copy-result".to_owned())
                    .h(px(20.))
                    .child(self.feedback),
            )
            .child(
                div()
                    .flex()
                    .justify_end()
                    .gap(px(12.))
                    .child(
                        button(
                            "crash-report-copy",
                            "Copy to clipboard",
                            &self.copy_focus,
                            false,
                        )
                        .on_mouse_down(MouseButton::Left, move |_, window, _| {
                            copy_focus.focus(window)
                        })
                        .on_click(cx.listener(|this, _, _, cx| this.copy(cx))),
                    )
                    .child(
                        button("crash-report-exit", "Exit Beryl", &self.exit_focus, true)
                            .on_mouse_down(MouseButton::Left, move |_, window, _| {
                                exit_focus.focus(window)
                            })
                            .on_click(cx.listener(|this, _, _, cx| this.exit(cx))),
                    ),
            )
    }
}

fn button(
    id: &'static str,
    label: &'static str,
    focus: &FocusHandle,
    primary: bool,
) -> gpui::Stateful<gpui::Div> {
    div()
        .id(id)
        .debug_selector(move || id.to_owned())
        .track_focus(focus)
        .h(px(32.))
        .px(px(12.))
        .flex()
        .items_center()
        .justify_center()
        .border_1()
        .border_color(rgb(if primary { 0x2563eb } else { 0xcbd5e1 }))
        .rounded_md()
        .cursor_pointer()
        .bg(rgb(if primary { 0xdbeafe } else { 0xf8fafc }))
        .hover(|style| style.bg(rgb(0xe2e8f0)))
        .active(|style| style.bg(rgb(0xcbd5e1)))
        .focus(|style| style.border_color(rgb(0x2563eb)).border_2())
        .child(label)
}

fn preview(report: &str) -> (Vec<SharedString>, bool) {
    let mut omitted = false;
    let mut lines = Vec::new();
    for line in report.lines() {
        if lines.len() == 9 {
            omitted = true;
            break;
        }
        let mut chars = line.chars();
        let mut text: String = chars
            .by_ref()
            .take(100)
            .map(|c| if c.is_control() { ' ' } else { c })
            .collect();
        if chars.next().is_some() {
            text.push('…');
            omitted = true;
        }
        lines.push(text.into());
    }
    (lines, omitted)
}

#[cfg(test)]
#[path = "../../tests/unit/crash_report_surface.rs"]
mod tests;
