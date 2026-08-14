use gpui::{Pixels, px};

#[path = "code_panel/projection.rs"]
mod projection;
#[path = "code_panel/styled_text.rs"]
mod styled_text;
#[path = "code_panel/syntax_projection.rs"]
mod syntax_projection;
#[allow(unused_imports)]
pub(crate) use projection::{
    CodePanelDisplayLine, CodePanelDisplayProjection, CodePanelDisplayWindow,
    code_panel_display_lines, code_panel_display_window, smart_wrap_for_columns,
};
#[allow(unused_imports)]
pub(crate) use styled_text::apply_selected_text_style;
pub(crate) use styled_text::{
    CodePanelSyntaxTheme, SelectedTextStyle, code_panel_styled_text_parts,
};
#[allow(unused_imports)]
pub(crate) use syntax_projection::{
    CodePanelDisplaySpan, CodePanelDisplaySyntaxSpans, code_panel_display_line_syntax_spans,
    code_panel_display_line_syntax_spans_for_window,
};

pub(crate) const DEFAULT_CODE_FONT_FAMILY: &str = "Consolas";
pub(crate) const DEFAULT_CODE_FONT_SIZE: f32 = 13.0;
const DEFAULT_CODE_PANEL_LINE_HEIGHT: f32 = 20.0;
const CODE_PANEL_VISIBLE_LINE_CAP: usize = 12;
const CODE_PANEL_CONTENT_VERTICAL_PADDING: f32 = 24.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum CodePanelWrapMode {
    Smart { columns: usize },
    NoWrap,
}

#[allow(dead_code)]
pub(crate) fn estimated_resizable_code_panel_height(
    display_text: &str,
    min_height: Pixels,
    max_height: Option<Pixels>,
) -> Pixels {
    let visible_line_count = display_text.replace("\r\n", "\n").lines().count().max(1);
    estimated_resizable_code_panel_height_for_line_count(
        visible_line_count,
        min_height,
        max_height,
        px(DEFAULT_CODE_PANEL_LINE_HEIGHT),
    )
}

fn estimated_resizable_code_panel_height_for_line_count(
    display_line_count: usize,
    min_height: Pixels,
    max_height: Option<Pixels>,
    line_height: Pixels,
) -> Pixels {
    let visible_line_count = display_line_count.max(1).min(CODE_PANEL_VISIBLE_LINE_CAP);
    let estimated_height =
        px(CODE_PANEL_CONTENT_VERTICAL_PADDING) + (line_height * visible_line_count as f32);
    clamp_resizable_code_panel_height(estimated_height, min_height, max_height)
}

pub(crate) fn clamp_resizable_code_panel_height(
    height: Pixels,
    min_height: Pixels,
    max_height: Option<Pixels>,
) -> Pixels {
    let mut clamped = height.max(min_height.max(Pixels::ZERO));
    if let Some(max_height) = max_height {
        clamped = clamped.min(max_height.max(min_height));
    }
    clamped
}
