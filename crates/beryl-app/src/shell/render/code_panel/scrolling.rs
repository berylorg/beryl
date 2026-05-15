use gpui::{
    AnyElement, IsZero, MouseButton, Overflow, Pixels, Point, Size, StatefulInteractiveElement,
    div, point, prelude::*, px,
};

use super::super::scrollbars::{ScrollbarAxis, render_div_scrollbar};
use super::{CodePanelScrollChrome, CodePanelScrollOverflow, CodePanelVerticalWheelOwnership};

#[derive(Clone, Copy)]
pub(crate) struct ScrollbarAxes {
    pub(crate) horizontal: bool,
    pub(crate) vertical: bool,
}

pub(super) fn render_scrollable_code_panel(
    element_key: u64,
    content: impl IntoElement,
    axes: ScrollbarAxes,
    scroll_chrome: Option<CodePanelScrollChrome>,
    content_height: Option<Pixels>,
    selection_enabled: bool,
) -> AnyElement {
    let mut scrollable = div()
        .id(("code-panel-scroll", element_key))
        .w_full()
        .min_w(px(0.0));

    if let Some(content_height) = content_height {
        scrollable = scrollable.h(content_height);
    }

    let vertical_wheel_ownership = scroll_chrome
        .as_ref()
        .map_or(CodePanelVerticalWheelOwnership::Panel, |scroll_chrome| {
            scroll_chrome.vertical_wheel_ownership
        });
    let overflow = code_panel_scroll_overflow(axes, vertical_wheel_ownership);
    scrollable.style().overflow.x = Some(overflow.horizontal);
    scrollable.style().overflow.y = Some(overflow.vertical);

    if axes.horizontal {
        scrollable.style().restrict_scroll_to_axis = Some(true);
    }

    match scroll_chrome {
        Some(scroll_chrome) => {
            let CodePanelScrollChrome {
                handle,
                scrollbar_opacity,
                on_activity,
                on_select,
                vertical_wheel_ownership,
            } = scroll_chrome;
            let stop_scroll_wheel_propagation =
                code_panel_stops_scroll_wheel_propagation(axes, vertical_wheel_ownership);
            let mut scroll_region = div()
                .relative()
                .w_full()
                .min_w(px(0.0))
                .when_some(content_height, |this, content_height| {
                    this.h(content_height)
                })
                .on_mouse_move({
                    let on_activity = on_activity.clone();
                    move |_, _, cx| {
                        if let Some(on_activity) = on_activity.as_ref() {
                            on_activity(cx);
                        }
                    }
                })
                .when_some(on_select, |this, on_select| {
                    this.on_mouse_down(MouseButton::Left, move |_, _, cx| {
                        on_select(cx);
                        if !selection_enabled {
                            cx.stop_propagation();
                        }
                    })
                })
                .on_scroll_wheel({
                    let on_activity = on_activity.clone();
                    move |_, _, cx| {
                        if let Some(on_activity) = on_activity.as_ref() {
                            on_activity(cx);
                        }
                        if stop_scroll_wheel_propagation {
                            cx.stop_propagation();
                        }
                    }
                })
                .child(scrollable.track_scroll(&handle).child(content));
            if axes.vertical {
                if let Some(scrollbar) =
                    render_div_scrollbar(&handle, ScrollbarAxis::Vertical, scrollbar_opacity)
                {
                    scroll_region = scroll_region.child(scrollbar);
                }
            }
            if axes.horizontal {
                if let Some(scrollbar) =
                    render_div_scrollbar(&handle, ScrollbarAxis::Horizontal, scrollbar_opacity)
                {
                    scroll_region = scroll_region.child(scrollbar);
                }
            }
            scroll_region.into_any_element()
        }
        None => scrollable.child(content).into_any_element(),
    }
}

pub(crate) fn code_panel_scroll_overflow(
    axes: ScrollbarAxes,
    vertical_wheel_ownership: CodePanelVerticalWheelOwnership,
) -> CodePanelScrollOverflow {
    CodePanelScrollOverflow {
        horizontal: if axes.horizontal {
            Overflow::Scroll
        } else {
            Overflow::Visible
        },
        vertical: if axes.vertical {
            match vertical_wheel_ownership {
                CodePanelVerticalWheelOwnership::Panel => Overflow::Scroll,
                CodePanelVerticalWheelOwnership::Parent => Overflow::Hidden,
            }
        } else {
            Overflow::Visible
        },
    }
}

pub(crate) fn code_panel_stops_scroll_wheel_propagation(
    axes: ScrollbarAxes,
    vertical_wheel_ownership: CodePanelVerticalWheelOwnership,
) -> bool {
    axes.vertical && vertical_wheel_ownership == CodePanelVerticalWheelOwnership::Panel
}

pub(crate) fn code_panel_offset_after_scroll_delta(
    current_offset: Point<Pixels>,
    max_offset: Size<Pixels>,
    delta: Point<Pixels>,
) -> Point<Pixels> {
    let mut delta_x = delta.x;
    let mut delta_y = delta.y;
    if !delta_x.is_zero() && !delta_y.is_zero() {
        if delta_x.abs() > delta_y.abs() {
            delta_y = Pixels::ZERO;
        } else {
            delta_x = Pixels::ZERO;
        }
    }

    point(
        (current_offset.x + delta_x).clamp(-max_offset.width, px(0.0)),
        (current_offset.y + delta_y).clamp(-max_offset.height, px(0.0)),
    )
}
