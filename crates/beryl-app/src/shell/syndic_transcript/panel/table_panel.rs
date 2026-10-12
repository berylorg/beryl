use super::super::provider::resource_index as index;
use super::super::{
    IndexedResourceValue, RealizedFrameWindow, ResidentTranscriptSnapshot, ResourceKind,
};
use super::{
    SyndicTranscriptPanel,
    table_appearance::TableAppearance,
    table_viewport::{TableCellId, TableViewport},
};
use gpui::{
    AnyElement, AppContext, Context, MouseButton, SharedString, Window, canvas, div, point,
    prelude::*, px, size,
};
use gpui_scrollbar::{
    Axis, ScrollbarInteraction, ScrollbarMountGeneration, ScrollbarOwnerId, ScrollbarOwnerKey,
    ScrollbarScrollState, ScrollbarState, ScrollbarVisibilityPolicy, render_scrollbar,
};
use std::{
    cell::Cell,
    rc::Rc,
    sync::atomic::{AtomicU64, Ordering},
};
static NEXT_TABLE: AtomicU64 = AtomicU64::new(1);
#[cfg(test)]
#[path = "table_panel/testing.rs"]
mod testing;

pub(super) struct MountedTablePanel {
    pub viewport: TableViewport,
    pub id: u64,
    pub actions: super::table_source_actions::TableSourceActions,
    pub focus: gpui::FocusHandle,
    pub copy_focus: gpui::FocusHandle,
    pub save_focus: gpui::FocusHandle,
    owner: ScrollbarOwnerKey,
    scrollbar: ScrollbarState,
    visibility: ScrollbarVisibilityPolicy,
    interaction: ScrollbarInteraction,
    #[cfg(test)]
    command_bounds: Rc<Cell<Option<(gpui::Bounds<gpui::Pixels>, gpui::Bounds<gpui::Pixels>)>>>,
    #[cfg(test)]
    fallback_bounds:
        Rc<std::cell::RefCell<Vec<(u64, gpui::Bounds<gpui::Pixels>, gpui::Bounds<gpui::Pixels>)>>>,
}
impl MountedTablePanel {
    fn new(
        viewport: TableViewport,
        wheel: Rc<Cell<Option<u64>>>,
        cx: &mut Context<SyndicTranscriptPanel>,
    ) -> Self {
        let id = NEXT_TABLE.fetch_add(1, Ordering::Relaxed);
        let owner =
            ScrollbarOwnerKey::new(ScrollbarOwnerId::new(id), ScrollbarMountGeneration::new(1));
        let scrollbar = ScrollbarState::new(owner);
        let weak = cx.entity().downgrade();
        let visibility = scrollbar.managed(Rc::new(move |_, _, app| {
            let _ = weak.update(app, |_, cx| cx.notify());
        }));
        let scroll = viewport.scroll.clone();
        let set = scroll.clone();
        let page = scroll.clone();
        let acquire = wheel.clone();
        let weak = cx.entity().downgrade();
        let interaction = ScrollbarInteraction::new(
            move || {
                let scroll = scroll.borrow();
                Some(ScrollbarScrollState {
                    owner,
                    viewport_bounds: scroll.bounds,
                    content_size: size(
                        px(scroll.columns as f32 * scroll.column_width),
                        px(scroll.rows as f32 * scroll.row_height),
                    ),
                    scroll_offset: point(px(scroll.horizontal), px(scroll.vertical)),
                    page_distance: scroll.bounds.size,
                })
            },
            move |axis, value| set.borrow_mut().set_offset(axis.axis, f32::from(value)),
            move |axis, direction, distance| {
                let mut scroll = page.borrow_mut();
                let value = if direction == gpui_scrollbar::ScrollDirection::Forward {
                    f32::from(distance)
                } else {
                    -f32::from(distance)
                };
                match axis.axis {
                    Axis::Vertical => scroll.scroll(0.0, value),
                    Axis::Horizontal => scroll.scroll(value, 0.0),
                };
                wheel.set(Some(id));
            },
            move |_| acquire.set(Some(id)),
            |_| {},
            move |_, _, app| {
                let _ = weak.update(app, |_, cx| cx.notify());
            },
        );
        Self {
            viewport,
            id,
            actions: Default::default(),
            focus: cx.focus_handle(),
            copy_focus: cx.focus_handle(),
            save_focus: cx.focus_handle(),
            owner,
            scrollbar,
            visibility,
            interaction,
            #[cfg(test)]
            command_bounds: Rc::default(),
            #[cfg(test)]
            fallback_bounds: Rc::default(),
        }
    }
    pub(super) fn retire(&mut self, window: &mut Window, cx: &mut gpui::App) {
        self.actions.cancel("Table retired");
        self.scrollbar.unmount_viewport(self.owner, window, cx);
    }
}
impl SyndicTranscriptPanel {
    fn table_wheel(
        &mut self,
        id: u64,
        event: &gpui::ScrollWheelEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let delta = event.delta.pixel_delta(window.line_height());
        let selected = self.table_wheel_owner.get() == Some(id);
        if selected || delta.x != px(0.0) {
            if let Some(table) = self.table_panels.iter_mut().find(|table| table.id == id) {
                table.viewport.scroll.borrow_mut().scroll(
                    -f32::from(delta.x),
                    if selected { -f32::from(delta.y) } else { 0.0 },
                );
                table
                    .visibility
                    .record_viewport_activity(table.owner, window, cx);
            }
            cx.notify();
            if selected || delta.y == px(0.0) {
                cx.stop_propagation();
            }
        }
    }
    fn table_drag(&mut self, id: u64, event: &gpui::MouseMoveEvent, cx: &mut Context<Self>) {
        let Some(table) = self.table_panels.iter_mut().find(|table| table.id == id) else {
            return;
        };
        if event.pressed_button != Some(MouseButton::Left) {
            table.viewport.dragging = false;
            return;
        }
        if !table.viewport.dragging {
            return;
        }
        let Some(cell) = table.viewport.hit(event.position) else {
            return;
        };
        table.viewport.request_cell(cell, true);
        self.keyboard_copy.gesture.reset();
        cx.notify();
    }
    pub(super) fn reconcile_table_panels(
        &mut self,
        snapshot: &ResidentTranscriptSnapshot,
        frame: &RealizedFrameWindow,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        for mut table in self.retired_table_panels.drain(..) {
            table.retire(window, cx);
        }
        self.table_panels.retain_mut(|table|{
            let retained=self.host.selected_view_id()==Some(&table.viewport.owner.view_id)
                && frame.records.iter().any(|record|record.record_id==table.viewport.owner.record_id)
                && snapshot.records.iter().any(|record|record.id==table.viewport.owner.record_id
                    && matches!(&record.kind,super::super::ResidentPresentationRecordKind::ResourceReference{resource_id,resource_kind:ResourceKind::Table,..} if resource_id==&table.viewport.owner.resource_id || super::code_panel::saved_code_successor(&table.viewport.owner,record)));
            if !retained{table.retire(window,cx);self.host.reserve_indexed_layout(&table.viewport.owner,0);self.host.set_indexed_resource_demand(&table.viewport.owner,None);if self.table_wheel_owner.get()==Some(table.id){self.table_wheel_owner.set(None);}}
            retained
        });
        if self.staged.is_some() {
            return;
        }
        for demand in self.host.table_panel_demands() {
            if demand.indexed.is_some()
                || !frame
                    .records
                    .iter()
                    .any(|record| record.record_id == demand.record_id)
            {
                continue;
            }
            let Some(metadata) = &demand.metadata else {
                continue;
            };
            let Ok(revision) = beryl_model::ProjectionRevision::new(metadata.revision.0) else {
                continue;
            };
            if let Some(table) = self
                .table_panels
                .iter_mut()
                .find(|table| table.viewport.owner.record_id == demand.record_id)
            {
                table.actions.cancel("Table source changed");
                table.viewport.rebind(demand, revision);
                if self.host.set_indexed_resource_demand(
                    &table.viewport.owner,
                    Some(table.viewport.query(None)),
                ) {
                    cx.notify();
                    cx.emit(super::ResourceViewportDemandChanged);
                }
                continue;
            }
            let viewport = TableViewport::new(demand, revision);
            if self
                .host
                .set_indexed_resource_demand(&viewport.owner, Some(viewport.query(None)))
            {
                let mut table =
                    MountedTablePanel::new(viewport, self.table_wheel_owner.clone(), cx);
                table.actions.save_admission = self.save_admission.clone();
                self.table_panels.push(table);
                cx.notify();
                cx.emit(super::ResourceViewportDemandChanged);
            }
        }
        let row_height = self.table_appearance.row_height(cx);
        for table in &mut self.table_panels {
            let resource = table.viewport.owner.resource_id.clone();
            let Some(owner) = self.host.indexed_resource_owner(&resource) else {
                continue;
            };
            let response = self.host.indexed_resource_response(&resource);
            let pages = self.host.indexed_table_window(&resource);
            table.viewport.owner = owner;
            let (available, height) = {
                let scroll = table.viewport.scroll.borrow();
                (
                    f32::from(scroll.bounds.size.width),
                    row_height.unwrap_or(scroll.row_height),
                )
            };
            if table
                .viewport
                .set_tracks(height, self.table_appearance.column_width(available))
            {
                self.keyboard_copy.gesture.reset();
            }
            table.viewport.adopt(response.as_deref(), pages);
            self.host
                .acknowledge_table_window(&table.viewport.owner, table.viewport.source_pages());
            table.actions.observe(
                &table.viewport.owner,
                table.viewport.revision,
                response.as_deref(),
            );
            let charge = table
                .viewport
                .charge()
                .saturating_add(table.actions.charge());
            if !self
                .host
                .reserve_indexed_layout(&table.viewport.owner, charge)
            {
                table.actions.cancel("Table resource capacity refused");
                table.viewport.feedback = "Table resource capacity refused";
                continue;
            }
            let query = if table.actions.pending() {
                table
                    .actions
                    .query()
                    .or_else(|| table.viewport.owner.indexed.clone())
                    .unwrap_or_else(|| table.viewport.query(response.as_deref()))
            } else {
                table.viewport.query(response.as_deref())
            };
            if !response
                .as_ref()
                .is_some_and(|response| matches!(response.value, IndexedResourceValue::Pending))
                && table
                    .viewport
                    .owner
                    .indexed
                    .as_ref()
                    .is_none_or(|current| *current != query)
                && self
                    .host
                    .set_indexed_resource_demand(&table.viewport.owner, Some(query))
            {
                table.viewport.feedback = "Table range loading";
                cx.notify();
                cx.emit(super::ResourceViewportDemandChanged);
            }
            if table.viewport.feedback == "Table range loading"
                && table.viewport.realized_ranges() == table.viewport.scroll.borrow().ranges()
            {
                table.viewport.feedback = "";
            }
            let id = table.id;
            cx.defer_in(window, move |panel, _, cx| {
                panel.advance_table_source_actions(id, cx);
            });
        }
    }
    pub(super) fn table_navigate(
        &mut self,
        id: u64,
        event: &gpui::KeyDownEvent,
        cx: &mut Context<Self>,
    ) {
        let Some(table) = self.table_panels.iter_mut().find(|table| table.id == id) else {
            return;
        };
        let Some(root) = &table.viewport.root else {
            return;
        };
        let Some(cell) = table.viewport.focus else {
            return;
        };
        let row = cell.ordinal();
        let column = cell.column;
        let page = (f32::from(table.viewport.scroll.borrow().bounds.size.height)
            / table.viewport.scroll.borrow().row_height)
            .floor()
            .max(1.0) as u64;
        let target = match event.keystroke.key.as_str() {
            "up" => TableCellId::at(row.saturating_sub(1), column),
            "down" => TableCellId::at((row + 1).min(root.body_rows), column),
            "left" => TableCellId::at(row, column.saturating_sub(1)),
            "right" => TableCellId::at(row, (column + 1).min(root.columns.saturating_sub(1))),
            "home" => TableCellId::at(row, 0),
            "end" => TableCellId::at(row, root.columns.saturating_sub(1)),
            "pageup" => TableCellId::at(row.saturating_sub(page), column),
            "pagedown" => TableCellId::at((row + page).min(root.body_rows), column),
            "escape" => {
                table.viewport.selection = None;
                self.keyboard_copy.gesture.reset();
                cx.stop_propagation();
                cx.notify();
                return;
            }
            _ => return,
        };
        let target = if matches!(
            event.keystroke.key.as_str(),
            "up" | "down" | "pageup" | "pagedown"
        ) && !event.keystroke.modifiers.shift
        {
            let Some(target) = table.viewport.skip_fallback(
                target,
                matches!(event.keystroke.key.as_str(), "down" | "pagedown"),
            ) else {
                cx.stop_propagation();
                return;
            };
            target
        } else {
            target
        };
        table
            .viewport
            .request_cell(target, event.keystroke.modifiers.shift);
        table.viewport.reveal(target);
        self.keyboard_copy.gesture.reset();
        cx.stop_propagation();
        cx.notify();
    }
}

#[path = "table_panel/render.rs"]
mod rendering;
#[cfg(test)]
pub(super) use rendering::RenderSource;
pub(super) use rendering::{render, shell};
