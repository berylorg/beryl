use std::{cell::RefCell, rc::Rc, sync::Arc};

#[path = "navigator/chrome.rs"]
mod chrome;
#[path = "navigator/layout.rs"]
mod layout;
#[path = "navigator/row_render.rs"]
mod row_render;
#[path = "navigator/scroll_state.rs"]
mod scroll_state;

use gpui::{AnyElement, App, ParentElement, ScrollHandle, SharedString, div, prelude::*, px};
use gpui_scrollbar::{Axis as ScrollbarAxis, ScrollbarVisibilityPolicy, ScrollbarVisibilityState};
use gpui_settings_window::{
    SettingsPageBodyRenderer, SettingsPageCustomBodyId, SettingsWindowOptions, SettingsWindowTheme,
};

use crate::StyleRoleId;

use super::{ThemeRoleNavigatorColumn, ThemeRoleNavigatorProjection, ThemeRoleNavigatorRow};

use chrome::{navigator_scrollbar_update_callback, render_navigator_scrollbar, theme_color};
use layout::{
    NAVIGATOR_COLUMN_GAP, NAVIGATOR_COLUMN_HEADER_HEIGHT, NAVIGATOR_COLUMN_WIDTH,
    NAVIGATOR_ROW_GAP, column_trail_width, role_row_visible_window, role_row_window, spacer,
};
use row_render::render_row;
use scroll_state::ThemeRoleNavigatorScrollState;

pub(crate) use layout::ThemeRoleNavigatorRenderStrategy;

const BODY_ID: &str = "themes.editor.role_navigator";

// The schema can grow without a local hard row-count bound, so each column
// renders a fixed-height visible row window with overscan instead of all rows.
type SelectRole = Arc<dyn Fn(StyleRoleId, &mut App)>;

#[derive(Clone)]
pub(crate) struct ThemeRoleNavigatorBodyRenderer {
    state: Rc<RefCell<ThemeRoleNavigatorRenderState>>,
    renderer: SettingsPageBodyRenderer,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ThemeRoleNavigatorRenderDiagnostics {
    pub(crate) total_schema_role_count: usize,
    pub(crate) visible_row_count: usize,
    pub(crate) rendered_row_count: usize,
    pub(crate) column_count: usize,
    pub(crate) selected_role_id: Option<StyleRoleId>,
    pub(crate) selected_role_path: Vec<StyleRoleId>,
    pub(crate) horizontal_scroll_surface_count: usize,
    pub(crate) column_scroll_surface_count: usize,
    pub(crate) strategy: ThemeRoleNavigatorRenderStrategy,
    pub(crate) column_keys: Vec<Option<StyleRoleId>>,
}

#[derive(Clone)]
struct ThemeRoleNavigatorRenderState {
    projection: Option<ThemeRoleNavigatorProjection>,
    visual_theme: SettingsWindowTheme,
    scroll_state: ThemeRoleNavigatorScrollState,
}

impl ThemeRoleNavigatorBodyRenderer {
    pub(crate) fn new(on_select_role: impl Fn(StyleRoleId, &mut App) + 'static) -> Self {
        let state = Rc::new(RefCell::new(ThemeRoleNavigatorRenderState::new()));
        let select_role = Arc::new(on_select_role);
        let renderer_state = state.clone();
        let renderer = SettingsPageBodyRenderer::new(move |body_id| {
            render_body(body_id, &renderer_state, select_role.clone())
        });
        Self { state, renderer }
    }

    pub(crate) fn update_projection(&self, projection: Option<ThemeRoleNavigatorProjection>) {
        self.state.borrow_mut().update_projection(projection);
    }

    pub(crate) fn options_with_renderer(
        &self,
        options: SettingsWindowOptions,
    ) -> SettingsWindowOptions {
        self.state
            .borrow_mut()
            .set_visual_theme(options.visual_theme().clone());
        options.with_page_body_renderer(self.renderer.clone())
    }

    pub(crate) fn diagnostics(&self) -> ThemeRoleNavigatorRenderDiagnostics {
        self.state.borrow().diagnostics()
    }
}

impl ThemeRoleNavigatorRenderState {
    fn new() -> Self {
        Self {
            projection: None,
            visual_theme: SettingsWindowTheme::default(),
            scroll_state: ThemeRoleNavigatorScrollState::new(),
        }
    }

    fn update_projection(&mut self, projection: Option<ThemeRoleNavigatorProjection>) {
        if let Some(projection) = projection.as_ref() {
            self.scroll_state.reconcile(projection.columns());
        } else {
            self.scroll_state.clear_columns();
        }
        self.projection = projection;
    }

    fn set_visual_theme(&mut self, visual_theme: SettingsWindowTheme) {
        self.visual_theme = visual_theme;
    }

    fn diagnostics(&self) -> ThemeRoleNavigatorRenderDiagnostics {
        let total_schema_role_count = self
            .projection
            .as_ref()
            .map(|projection| projection.rows().count())
            .unwrap_or_default();
        let visible_row_count = self
            .projection
            .as_ref()
            .map(|projection| {
                projection
                    .columns()
                    .iter()
                    .enumerate()
                    .map(|(index, column)| {
                        let handle = self
                            .scroll_state
                            .column_handle(index)
                            .unwrap_or_else(ScrollHandle::new);
                        role_row_visible_window(column.rows().len(), &handle)
                            .range
                            .len()
                    })
                    .sum()
            })
            .unwrap_or_default();
        let rendered_row_count = self
            .projection
            .as_ref()
            .map(|projection| {
                projection
                    .columns()
                    .iter()
                    .enumerate()
                    .map(|(index, column)| {
                        let handle = self
                            .scroll_state
                            .column_handle(index)
                            .unwrap_or_else(ScrollHandle::new);
                        role_row_window(column.rows().len(), &handle).range.len()
                    })
                    .sum()
            })
            .unwrap_or_default();
        let column_count = self
            .projection
            .as_ref()
            .map(|projection| projection.columns().len())
            .unwrap_or_default();
        ThemeRoleNavigatorRenderDiagnostics {
            total_schema_role_count,
            visible_row_count,
            rendered_row_count,
            column_count,
            selected_role_id: self
                .projection
                .as_ref()
                .map(|projection| projection.selected_role_id().clone()),
            selected_role_path: self
                .projection
                .as_ref()
                .map(|projection| projection.selected_path().to_vec())
                .unwrap_or_default(),
            horizontal_scroll_surface_count: usize::from(column_count > 0),
            column_scroll_surface_count: column_count,
            strategy: ThemeRoleNavigatorRenderStrategy::fixed_height_windowed(),
            column_keys: self.scroll_state.column_keys().cloned().collect(),
        }
    }
}

fn render_body(
    body_id: &SettingsPageCustomBodyId,
    state: &Rc<RefCell<ThemeRoleNavigatorRenderState>>,
    select_role: SelectRole,
) -> Option<AnyElement> {
    if body_id.as_str() != BODY_ID {
        return None;
    }

    let state = state.borrow();
    let Some(projection) = state.projection.as_ref() else {
        return Some(
            div()
                .id("theme-role-navigator-empty")
                .size_full()
                .into_any_element(),
        );
    };

    let columns = projection
        .columns()
        .iter()
        .enumerate()
        .map(|(index, column)| {
            let handle = state
                .scroll_state
                .column_handle(index)
                .unwrap_or_else(ScrollHandle::new);
            let visibility = state
                .scroll_state
                .column_visibility_policy(index)
                .unwrap_or_else(|| {
                    ScrollbarVisibilityState::new().managed(navigator_scrollbar_update_callback())
                });
            render_column(
                projection,
                column,
                index,
                handle,
                visibility,
                &state.visual_theme,
                select_role.clone(),
            )
        })
        .collect::<Vec<_>>();
    let trail_width = column_trail_width(columns.len());
    let horizontal = state.scroll_state.horizontal_handle();
    let horizontal_visibility = state.scroll_state.horizontal_visibility_policy();

    let mut horizontal_scroller = div()
        .id("theme-role-navigator-horizontal-scroll")
        .size_full()
        .track_scroll(&horizontal)
        .overflow_x_scroll();
    horizontal_scroller.style().restrict_scroll_to_axis = Some(true);

    let mut horizontal_scroll_region = div()
        .relative()
        .size_full()
        .on_mouse_move({
            let horizontal_visibility = horizontal_visibility.clone();
            move |_, window, cx| {
                horizontal_visibility.record_viewport_activity(window, cx);
            }
        })
        .on_scroll_wheel({
            let horizontal_visibility = horizontal_visibility.clone();
            move |_, window, cx| {
                horizontal_visibility.record_viewport_activity(window, cx);
            }
        })
        .child(
            horizontal_scroller.child(
                div()
                    .h_full()
                    .min_h(px(0.0))
                    .w(px(trail_width))
                    .min_w_full()
                    .p_2()
                    .child(
                        div()
                            .h_full()
                            .min_h(px(0.0))
                            .flex()
                            .gap(px(NAVIGATOR_COLUMN_GAP))
                            .children(columns),
                    ),
            ),
        );
    if let Some(scrollbar) = render_navigator_scrollbar(
        "theme-role-navigator-horizontal-scrollbar",
        &horizontal,
        ScrollbarAxis::Horizontal,
        &state.visual_theme,
        horizontal_visibility,
    ) {
        horizontal_scroll_region = horizontal_scroll_region.child(scrollbar);
    }

    Some(
        div()
            .id("theme-role-navigator")
            .size_full()
            .overflow_hidden()
            .border_1()
            .border_color(theme_color(state.visual_theme.panel.border))
            .bg(theme_color(state.visual_theme.panel.background))
            .child(horizontal_scroll_region)
            .into_any_element(),
    )
}

fn render_column(
    projection: &ThemeRoleNavigatorProjection,
    column: &ThemeRoleNavigatorColumn,
    column_index: usize,
    scroll_handle: ScrollHandle,
    scrollbar_visibility: ScrollbarVisibilityPolicy,
    theme: &SettingsWindowTheme,
    select_role: SelectRole,
) -> AnyElement {
    let header = column
        .parent_role_id()
        .and_then(|role_id| projection.row(role_id))
        .map(ThemeRoleNavigatorRow::label)
        .unwrap_or("Root");
    div()
        .id(SharedString::from(format!(
            "theme-role-navigator-column-{}",
            column_index
        )))
        .w(px(NAVIGATOR_COLUMN_WIDTH))
        .h_full()
        .min_h(px(0.0))
        .flex_none()
        .overflow_hidden()
        .border_1()
        .border_color(theme_color(theme.row.border))
        .bg(theme_color(theme.row.background))
        .child(
            div()
                .size_full()
                .min_h(px(0.0))
                .flex()
                .flex_col()
                .child(
                    div()
                        .flex_none()
                        .h(px(NAVIGATOR_COLUMN_HEADER_HEIGHT))
                        .px_2()
                        .flex()
                        .items_center()
                        .border_b_1()
                        .border_color(theme_color(theme.row.border))
                        .text_color(theme_color(theme.row.muted_foreground))
                        .text_size(px(12.0))
                        .child(div().truncate().child(header.to_owned())),
                )
                .child(render_column_rows(
                    projection,
                    column,
                    column_index,
                    scroll_handle,
                    scrollbar_visibility,
                    theme,
                    select_role,
                )),
        )
        .into_any_element()
}

fn render_column_rows(
    projection: &ThemeRoleNavigatorProjection,
    column: &ThemeRoleNavigatorColumn,
    column_index: usize,
    scroll_handle: ScrollHandle,
    scrollbar_visibility: ScrollbarVisibilityPolicy,
    theme: &SettingsWindowTheme,
    select_role: SelectRole,
) -> AnyElement {
    let window = role_row_window(column.rows().len(), &scroll_handle);
    let mut rows = div()
        .w_full()
        .h(px(window.total_height))
        .min_h(px(window.total_height))
        .flex()
        .flex_col()
        .child(spacer(window.top_spacer_height));

    for row_index in window.range.clone() {
        let Some(row) = column.rows().get(row_index) else {
            continue;
        };
        debug_assert!(projection.row(row.role_id()).is_some());
        rows = rows.child(render_row(
            projection,
            row,
            column_index,
            row_index,
            theme,
            select_role.clone(),
        ));
        if row_index + 1 < window.range.end {
            rows = rows.child(spacer(NAVIGATOR_ROW_GAP));
        }
    }
    rows = rows.child(spacer(window.bottom_spacer_height));

    let mut scroll_region = div()
        .flex_1()
        .min_h(px(0.0))
        .relative()
        .overflow_hidden()
        .on_mouse_move({
            let scrollbar_visibility = scrollbar_visibility.clone();
            move |_, window, cx| {
                scrollbar_visibility.record_viewport_activity(window, cx);
            }
        })
        .on_scroll_wheel({
            let scrollbar_visibility = scrollbar_visibility.clone();
            move |_, window, cx| {
                scrollbar_visibility.record_viewport_activity(window, cx);
            }
        })
        .child(
            div()
                .id(SharedString::from(format!(
                    "theme-role-navigator-column-scroll-{}",
                    column_index
                )))
                .size_full()
                .min_h(px(0.0))
                .track_scroll(&scroll_handle)
                .overflow_y_scroll()
                .child(rows),
        );
    let scrollbar_id = SharedString::from(format!(
        "theme-role-navigator-column-scrollbar-{}-{}",
        column_index,
        column.parent_role_id().map_or("root", StyleRoleId::as_str)
    ));
    if let Some(scrollbar) = render_navigator_scrollbar(
        scrollbar_id,
        &scroll_handle,
        ScrollbarAxis::Vertical,
        theme,
        scrollbar_visibility,
    ) {
        scroll_region = scroll_region.child(scrollbar);
    }
    scroll_region.into_any_element()
}

#[cfg(test)]
pub(crate) use layout::{
    theme_role_navigator_render_strategy_for_test, theme_role_navigator_row_window_for_test,
    theme_role_navigator_row_window_height_sum_for_test,
};
