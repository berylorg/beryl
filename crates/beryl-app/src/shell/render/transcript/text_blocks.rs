use gpui::{div, prelude::*};

use crate::shell::syntax_highlighting::SyntaxHighlight;

use super::super::code_panel::{
    self, CodePanelChrome, CodePanelDisplayProjectionInput, CodePanelHeader, CodePanelResize,
    CodePanelScrollChrome, CodePanelSelection, CodePanelWrapMode,
};
use super::TranscriptTheme;

pub(super) fn labeled_code_block(
    label: &str,
    panel_id: Option<String>,
    language: Option<&str>,
    text: &str,
    wrap_mode: CodePanelWrapMode,
    display_projection: CodePanelDisplayProjectionInput,
    theme: &TranscriptTheme,
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
                .text_color(theme.code_panel_header.foreground())
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
            background: theme.code_panel_container.background(),
            border: theme.code_panel_border.border(),
            content_background: theme.code_panel_body.background(),
            header_foreground: theme.code_panel_header.foreground(),
            button: theme.code_panel_button,
            resize_handle: theme.code_panel_resize_handle.color(),
        },
        theme.code_panel_body.foreground(),
        Some(theme.syntax.clone()),
        syntax_highlight,
        header,
        scroll_chrome,
        resize,
        selection,
    ))
}

pub(super) fn empty_state(
    show_existing_thread_message: bool,
    theme: &TranscriptTheme,
) -> impl IntoElement {
    let message = if show_existing_thread_message {
        "The selected thread has no persisted transcript turns yet."
    } else {
        "Start a new thread to build the transcript here."
    };

    div()
        .rounded_md()
        .bg(theme.unavailable.background())
        .border_1()
        .border_color(theme.unavailable.border())
        .p_3()
        .text_sm()
        .text_color(theme.unavailable.foreground())
        .child(message)
}

pub(super) fn pending_thread_activation_state(
    label: &str,
    theme: &TranscriptTheme,
) -> impl IntoElement {
    div()
        .rounded_md()
        .bg(theme.pending.background())
        .border_1()
        .border_color(theme.pending.border())
        .p_3()
        .flex()
        .items_center()
        .gap_2()
        .child(div().w_2().h_2().rounded_full().bg(theme.pending.border()))
        .child(
            div()
                .min_w(gpui::px(0.0))
                .text_sm()
                .text_color(theme.pending.foreground())
                .whitespace_nowrap()
                .truncate()
                .child(format!("Opening {label}")),
        )
}

pub(super) fn older_history_loading_state(theme: &TranscriptTheme) -> impl IntoElement {
    div()
        .rounded_md()
        .bg(theme.pending.background())
        .border_1()
        .border_color(theme.pending.border())
        .px_3()
        .py_2()
        .flex()
        .items_center()
        .gap_2()
        .child(div().w_2().h_2().rounded_full().bg(theme.pending.border()))
        .child(
            div()
                .text_xs()
                .text_color(theme.pending.foreground())
                .child("Loading older history"),
        )
}

pub(super) fn released_history_placeholder_state(theme: &TranscriptTheme) -> impl IntoElement {
    div()
        .rounded_md()
        .bg(theme.unavailable.background())
        .border_1()
        .border_color(theme.unavailable.border())
        .px_3()
        .py_2()
        .flex()
        .items_center()
        .gap_2()
        .child(
            div()
                .w_2()
                .h_2()
                .rounded_full()
                .bg(theme.unavailable.border()),
        )
        .child(
            div()
                .text_xs()
                .text_color(theme.unavailable.foreground())
                .child("Loading transcript history"),
        )
}
