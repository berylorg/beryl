use super::render::PickerTooltip;
use super::*;
use beryl_state::ThemePropertyId as Property;
use gpui::{
    AnyElement, InteractiveElement, IntoElement, ParentElement, StatefulInteractiveElement, Styled,
    div, prelude::FluentBuilder,
};

impl ThreadRootPicker {
    pub(super) fn render_row(
        &self,
        position: usize,
        row: PickerRow,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let style = self.config.style;
        let unavailable = row.unavailable_reason.is_some();
        let pending =
            row.activation_pending || self.activation_in_flight.as_ref() == Some(&row.key);
        let focused = self.collection.focused_key() == Some(&row.key);
        let selected = self.full.selected.as_ref() == Some(&row.key);
        let role = if unavailable {
            "thread-root-picker.row.unavailable"
        } else if selected {
            "thread-root-picker.row.selected"
        } else if row.current {
            "thread-root-picker.row.current"
        } else {
            "thread-root-picker.row"
        };
        let primary_font = self.font("thread-root-picker.row-primary", 14., 520.);
        let secondary_font = self.font("thread-root-picker.row-secondary", 12., 400.);
        let status_font = self.font("thread-root-picker.row-status", 10., 650.);
        let key = row.key.clone();
        let focus_key = row.key.clone();
        let revision = self.collection.revision;
        let collection_key = self.collection.key.clone();
        let focus_collection_key = collection_key.clone();
        let id = gpui::SharedString::from(format!("thread-root-picker-row-{}", row.key.0));
        let tooltip = row.unavailable_reason.clone().or(row.tooltip.clone());
        let mut element = div()
            .id(id.clone())
            .debug_selector(move || id.to_string())
            .h(px(self.collection_row_height()))
            .flex_shrink_0()
            .w_full()
            .px(px(style.row_padding_x))
            .flex()
            .items_center()
            .gap(px(style.row_content_gap))
            .rounded(px(style.row_radius))
            .border(px(if focused {
                style.ring_width
            } else {
                style.row_border_width
            }))
            .border_color(if focused {
                self.color("thread-root-picker.row.focused", Property::Border, 0x38bdf8)
            } else {
                self.color(
                    role,
                    Property::Border,
                    if unavailable {
                        0x2b3547
                    } else if selected {
                        0x38bdf8
                    } else if row.current {
                        0x3b475d
                    } else {
                        0x2f3b52
                    },
                )
            })
            .bg(self.color(
                role,
                Property::Background,
                if selected {
                    0x173a5e
                } else if row.current {
                    0x131b29
                } else {
                    0x111827
                },
            ))
            .when(unavailable || !self.collection.is_current(), |el| {
                el.opacity(style.unavailable_opacity)
            })
            .hover(|el| {
                el.bg(self.color(
                    "thread-root-picker.row.hover",
                    Property::Background,
                    0x141d2d,
                ))
            })
            .on_mouse_down(
                gpui::MouseButton::Left,
                cx.listener(move |this, _, window, cx| {
                    if this.collection.revision == revision
                        && this.collection.key == focus_collection_key
                        && this
                            .collection
                            .row(position)
                            .is_some_and(|row| row.key == focus_key)
                    {
                        this.focus_row(position, window, cx);
                    }
                }),
            )
            .on_click(cx.listener(move |this, _, _, cx| {
                if this.collection.revision == revision && this.collection.key == collection_key {
                    this.activate(&key, cx);
                }
            }))
            .child(
                div()
                    .w(px(style.row_icon_size))
                    .h(px(style.row_icon_size))
                    .flex_shrink_0()
                    .relative()
                    .when(
                        self.full.presentation == PickerRowPresentation::Thread,
                        |icon| {
                            icon.child(
                                div()
                                    .absolute()
                                    .top_0()
                                    .w_full()
                                    .h(px(style.row_icon_size * 0.2))
                                    .bg(self.color(
                                        "thread-root-picker.row-icon",
                                        Property::Foreground,
                                        0x7dd3fc,
                                    )),
                            )
                            .child(
                                div()
                                    .absolute()
                                    .bottom_0()
                                    .w_full()
                                    .h(px(style.row_icon_size * 0.2))
                                    .bg(self.color(
                                        "thread-root-picker.row-icon",
                                        Property::Foreground,
                                        0x7dd3fc,
                                    )),
                            )
                            .child(
                                div()
                                    .absolute()
                                    .top(px(style.row_icon_size * 0.2))
                                    .left(px(style.row_icon_size * 0.25))
                                    .w(px(style.row_icon_size * 0.5))
                                    .h(px(style.row_icon_size * 0.6))
                                    .border_l(px(style.row_border_width))
                                    .border_r(px(style.row_border_width))
                                    .border_color(self.color(
                                        "thread-root-picker.row-icon",
                                        Property::Foreground,
                                        0x7dd3fc,
                                    )),
                            )
                        },
                    )
                    .when(
                        self.full.presentation == PickerRowPresentation::Root,
                        |icon| {
                            icon.child(
                                div()
                                    .absolute()
                                    .top(px(style.row_icon_size * 0.16))
                                    .left_0()
                                    .w(px(style.row_icon_size * 0.45))
                                    .h(px(style.row_icon_size * 0.25))
                                    .bg(self.color(
                                        "thread-root-picker.row-icon",
                                        Property::Foreground,
                                        0x7dd3fc,
                                    )),
                            )
                            .child(
                                div()
                                    .absolute()
                                    .top(px(style.row_icon_size * 0.3))
                                    .left_0()
                                    .w_full()
                                    .h(px(style.row_icon_size * 0.6))
                                    .border(px(style.row_border_width))
                                    .border_color(self.color(
                                        "thread-root-picker.row-icon",
                                        Property::Foreground,
                                        0x7dd3fc,
                                    )),
                            )
                        },
                    ),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .text_size(px(primary_font.0))
                            .font_weight(primary_font.1)
                            .when_some(primary_font.2, |el, font| el.font_family(font))
                            .text_color(self.color(
                                "thread-root-picker.row-primary",
                                Property::Foreground,
                                if unavailable { 0x7f8ea3 } else { 0xe9eef5 },
                            ))
                            .child(row.primary),
                    )
                    .child(
                        div()
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .text_size(px(secondary_font.0))
                            .font_weight(secondary_font.1)
                            .when_some(secondary_font.2, |el, font| el.font_family(font))
                            .text_color(self.color(
                                "thread-root-picker.row-secondary",
                                Property::Foreground,
                                0x92a2b7,
                            ))
                            .child(row.secondary),
                    ),
            )
            .child(
                div()
                    .flex_shrink_0()
                    .text_size(px(status_font.0))
                    .font_weight(status_font.1)
                    .when_some(status_font.2, |el, font| el.font_family(font))
                    .text_color(self.color(
                        "thread-root-picker.row-status",
                        Property::Foreground,
                        0x7dd3fc,
                    ))
                    .child(if pending {
                        "OPENING…".to_owned()
                    } else {
                        row.status
                    }),
            );
        if let Some(tooltip) = tooltip {
            element =
                element.tooltip(move |_, cx| cx.new(|_| PickerTooltip(tooltip.clone())).into());
        }
        element.into_any_element()
    }
}
