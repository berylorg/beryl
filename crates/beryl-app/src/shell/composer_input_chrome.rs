use gpui::{Div, Entity, IntoElement, Pixels, div, prelude::*, px};
use gpui_text_input::TextInput;

use super::layout;

pub(crate) fn composer_input_area(content: impl IntoElement) -> Div {
    div()
        .absolute()
        .top(px(layout::COMPOSER_OUTER_VERTICAL_PADDING / 2.0))
        .bottom(px(layout::COMPOSER_OUTER_VERTICAL_PADDING / 2.0))
        .left(px(layout::COMPOSER_OUTER_HORIZONTAL_PADDING / 2.0))
        .right(px(layout::COMPOSER_OUTER_HORIZONTAL_PADDING / 2.0))
        .min_h(px(0.0))
        .flex()
        .items_end()
        .child(content)
}

pub(crate) fn composer_input_scroll_region(
    input_render_height: Pixels,
    text_top_padding: Pixels,
    conversation_input: &Entity<TextInput>,
) -> Div {
    div()
        .relative()
        .flex_1()
        .min_w(px(0.0))
        .h_full()
        .min_h(px(0.0))
        .px(px(layout::COMPOSER_INPUT_PADDING_X))
        .pt(px(layout::COMPOSER_INPUT_PADDING_TOP))
        .pb(px(layout::COMPOSER_INPUT_PADDING_BOTTOM))
        .rounded(px(layout::ROUNDED_WIDGET_CORNER_RADIUS))
        .border(px(layout::COMPOSER_INPUT_BORDER_WIDTH))
        .child(
            div()
                .size_full()
                .min_h(px(0.0))
                .overflow_hidden()
                .flex()
                .flex_col()
                .child(
                    div()
                        .w_full()
                        .min_w(px(0.0))
                        .h(text_top_padding)
                        .min_h(text_top_padding),
                )
                .child(
                    div()
                        .w_full()
                        .min_w(px(0.0))
                        .h(input_render_height)
                        .min_h(input_render_height)
                        .child(conversation_input.clone()),
                ),
        )
}
