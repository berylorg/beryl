use gpui::{div, prelude::*, rgb};

use crate::shell::syntax_highlighting::SyntaxHighlight;

use super::super::code_panel::{
    self, CodePanelChrome, CodePanelDisplayProjectionInput, CodePanelHeader, CodePanelResize,
    CodePanelScrollChrome, CodePanelSelection, CodePanelSyntaxTheme, CodePanelWrapMode,
};

pub(super) fn labeled_code_block(
    label: &str,
    panel_id: Option<String>,
    language: Option<&str>,
    text: &str,
    wrap_mode: CodePanelWrapMode,
    display_projection: CodePanelDisplayProjectionInput,
    background: gpui::Rgba,
    foreground: gpui::Rgba,
    syntax_theme: CodePanelSyntaxTheme,
    syntax_highlight: Option<&SyntaxHighlight>,
    header: Option<CodePanelHeader>,
    scroll_chrome: Option<CodePanelScrollChrome>,
    resize: Option<CodePanelResize>,
    selection: Option<CodePanelSelection>,
) -> impl IntoElement {
    let mut block = div().flex().flex_col().gap_2();

    if !label.is_empty() {
        block = block.child(
            div()
                .text_xs()
                .text_color(rgb(0x94a3b8))
                .child(label.to_string()),
        );
    }

    block.child(code_panel::render_code_panel(
        panel_id,
        text,
        language,
        wrap_mode,
        display_projection,
        CodePanelChrome::Bordered {
            background,
            border: rgb(0x1f2937),
        },
        foreground,
        Some(syntax_theme),
        syntax_highlight,
        header,
        scroll_chrome,
        resize,
        selection,
    ))
}

pub(super) fn empty_state(show_existing_thread_message: bool) -> impl IntoElement {
    let message = if show_existing_thread_message {
        "The selected thread has no persisted transcript turns yet."
    } else {
        "Start a new thread to build the transcript here."
    };

    div()
        .rounded_md()
        .bg(rgb(0x111827))
        .border_1()
        .border_color(rgb(0x1f2937))
        .p_3()
        .text_sm()
        .text_color(rgb(0x94a3b8))
        .child(message)
}

pub(super) fn pending_thread_activation_state(label: &str) -> impl IntoElement {
    div()
        .rounded_md()
        .bg(rgb(0x0f172a))
        .border_1()
        .border_color(rgb(0x2563eb))
        .p_3()
        .flex()
        .items_center()
        .gap_2()
        .child(div().w_2().h_2().rounded_full().bg(rgb(0x38bdf8)))
        .child(
            div()
                .min_w(gpui::px(0.0))
                .text_sm()
                .text_color(rgb(0xbfdbfe))
                .whitespace_nowrap()
                .truncate()
                .child(format!("Opening {label}")),
        )
}

pub(super) fn older_history_loading_state() -> impl IntoElement {
    div()
        .rounded_md()
        .bg(rgb(0x111827))
        .border_1()
        .border_color(rgb(0x334155))
        .px_3()
        .py_2()
        .flex()
        .items_center()
        .gap_2()
        .child(div().w_2().h_2().rounded_full().bg(rgb(0x93c5fd)))
        .child(
            div()
                .text_xs()
                .text_color(rgb(0xcbd5e1))
                .child("Loading older history"),
        )
}

pub(super) fn released_history_placeholder_state() -> impl IntoElement {
    div()
        .rounded_md()
        .bg(rgb(0x111827))
        .border_1()
        .border_color(rgb(0x334155))
        .px_3()
        .py_2()
        .flex()
        .items_center()
        .gap_2()
        .child(div().w_2().h_2().rounded_full().bg(rgb(0x93c5fd)))
        .child(
            div()
                .text_xs()
                .text_color(rgb(0xcbd5e1))
                .child("Loading transcript history"),
        )
}
