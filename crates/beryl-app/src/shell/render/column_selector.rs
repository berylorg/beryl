use std::rc::Rc;

use gpui::{
    AnyElement, Context, InteractiveElement, ScrollHandle, StatefulInteractiveElement, div,
    prelude::*, px,
};

use crate::shell::{ScrollbarRegion, ShellView, column_selector::ColumnSelectorSurface};

use super::scrollbars::{ScrollbarActivityCallback, ScrollbarAxis, render_div_scrollbar};

pub(super) fn column_selector_trail_width(
    column_count: usize,
    column_width: f32,
    column_gap: f32,
) -> gpui::Pixels {
    if column_count == 0 {
        px(0.0)
    } else {
        px((column_count as f32 * column_width)
            + (column_count.saturating_sub(1) as f32 * column_gap))
    }
}

pub(super) fn render_column_selector_trail(
    surface: ColumnSelectorSurface,
    id: &'static str,
    column_width: f32,
    column_gap: f32,
    columns: Vec<AnyElement>,
    scroll_handle: ScrollHandle,
    scrollbar_opacity: f32,
    cx: &mut Context<ShellView>,
) -> impl IntoElement {
    let scrollbar_activity: ScrollbarActivityCallback = {
        let entity = cx.entity();
        let region = match surface {
            ColumnSelectorSurface::GraphOverlay => ScrollbarRegion::GraphColumns,
            ColumnSelectorSurface::ThreadSelector => ScrollbarRegion::ThreadSelectorColumns,
        };
        Rc::new(move |_: &mut gpui::Window, cx: &mut gpui::App| {
            entity.update(cx, |view, cx| {
                view.note_scrollbar_activity(region.clone(), cx);
            });
        })
    };
    let columns_width = column_selector_trail_width(columns.len(), column_width, column_gap);
    let mut column_row = div().h_full().min_h(px(0.0)).flex().gap_4();
    column_row.style().align_items = Some(gpui::AlignItems::Stretch);
    for column in columns {
        column_row = column_row.child(column);
    }

    let mut scroller = div()
        .id(id)
        .size_full()
        .min_h(px(0.0))
        .track_scroll(&scroll_handle)
        .overflow_x_scroll();
    scroller.style().restrict_scroll_to_axis = Some(true);

    let mut scroll_region = div()
        .relative()
        .size_full()
        .min_h(px(0.0))
        .on_mouse_move(cx.listener(move |view, event, window, cx| {
            view.note_column_selector_horizontal_scrollbar_motion(surface, event, window, cx);
        }))
        .on_scroll_wheel(cx.listener(move |view, event, window, cx| {
            view.note_column_selector_horizontal_scrollbar_scroll(surface, event, window, cx);
        }))
        .child(
            scroller.child(
                div()
                    .h_full()
                    .min_h(px(0.0))
                    .w(columns_width)
                    .min_w_full()
                    .pr_4()
                    .pb_2()
                    .child(column_row),
            ),
        );
    if let Some(scrollbar) = render_div_scrollbar(
        (gpui::ElementId::from(id), "horizontal-scrollbar"),
        &scroll_handle,
        ScrollbarAxis::Horizontal,
        scrollbar_opacity,
        Some(scrollbar_activity),
    ) {
        scroll_region = scroll_region.child(scrollbar);
    }
    scroll_region
}
