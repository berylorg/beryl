use beryl_model::semantic_graph::SemanticNodeId;
use gpui::{Bounds, Context, MouseDownEvent, Pixels, Point, Window};

use super::ShellView;

#[derive(Clone, Debug, Default)]
pub(crate) struct ChecklistThreadStartMenuState {
    open: Option<ChecklistThreadStartMenuOpen>,
}

#[derive(Clone, Debug)]
pub(crate) struct ChecklistThreadStartMenuOpen {
    item_node_id: SemanticNodeId,
    position: Point<Pixels>,
    bounds: Option<Bounds<Pixels>>,
}

impl ChecklistThreadStartMenuState {
    pub(crate) fn is_open(&self) -> bool {
        self.open.is_some()
    }

    pub(crate) fn open_item(&mut self, item_node_id: SemanticNodeId, position: Point<Pixels>) {
        self.open = Some(ChecklistThreadStartMenuOpen {
            item_node_id,
            position,
            bounds: None,
        });
    }

    pub(crate) fn close(&mut self) {
        self.open = None;
    }

    pub(crate) fn active(&self) -> Option<&ChecklistThreadStartMenuOpen> {
        self.open.as_ref()
    }

    pub(crate) fn set_bounds(&mut self, bounds: Option<Bounds<Pixels>>) {
        if let Some(open) = self.open.as_mut() {
            open.bounds = bounds;
        }
    }

    pub(crate) fn should_dismiss_for_mouse_down(&self, position: Point<Pixels>) -> bool {
        self.open
            .as_ref()
            .is_some_and(|open| !open.bounds.is_some_and(|bounds| bounds.contains(&position)))
    }
}

impl ChecklistThreadStartMenuOpen {
    pub(crate) fn item_node_id(&self) -> &SemanticNodeId {
        &self.item_node_id
    }

    pub(crate) fn position(&self) -> Point<Pixels> {
        self.position
    }
}

impl ShellView {
    pub(crate) fn open_checklist_item_thread_start_menu(
        &mut self,
        item_node_id: SemanticNodeId,
        event: &MouseDownEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(surface) = self.conversation_surface_mut() {
            surface.graph_thread_link_menu_mut().close();
            surface.transcript_branch_menu_mut().close();
            surface
                .checklist_thread_start_menu_mut()
                .open_item(item_node_id, event.position);
        }
        cx.stop_propagation();
        cx.notify();
    }

    pub(crate) fn handle_checklist_thread_start_menu_mouse_down(
        &mut self,
        event: &MouseDownEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let should_dismiss = self.conversation_surface().is_some_and(|surface| {
            surface
                .checklist_thread_start_menu()
                .should_dismiss_for_mouse_down(event.position)
        });
        if should_dismiss && let Some(surface) = self.conversation_surface_mut() {
            surface.checklist_thread_start_menu_mut().close();
            cx.notify();
        }
    }

    pub(crate) fn handle_checklist_thread_start_menu_key_down(
        &mut self,
        event: &gpui::KeyDownEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if event.keystroke.key.as_str() != "escape" {
            return false;
        }
        if let Some(surface) = self.conversation_surface_mut()
            && surface.checklist_thread_start_menu().is_open()
        {
            surface.checklist_thread_start_menu_mut().close();
            cx.notify();
            return true;
        }
        false
    }

    pub(crate) fn record_checklist_thread_start_menu_bounds(
        &mut self,
        bounds: Option<Bounds<Pixels>>,
        _: &mut Context<Self>,
    ) {
        if let Some(surface) = self.conversation_surface_mut() {
            surface.checklist_thread_start_menu_mut().set_bounds(bounds);
        }
    }

    pub(crate) fn start_checklist_item_thread_from_menu(
        &mut self,
        _: &gpui::ClickEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(message) = self.new_thread_controls_disabled_message() {
            if let Some(surface) = self.conversation_surface_mut() {
                surface.set_notice(super::SurfaceNotice::new(
                    "Thread start unavailable",
                    message,
                ));
            }
            cx.notify();
            return;
        }
        let item_node_id = self.conversation_surface().and_then(|surface| {
            surface
                .checklist_thread_start_menu()
                .active()
                .map(|menu| menu.item_node_id().clone())
        });
        let Some(item_node_id) = item_node_id else {
            return;
        };

        if let Some(surface) = self.conversation_surface_mut() {
            surface.checklist_thread_start_menu_mut().close();
        }
        self.start_checklist_item_thread_from_node(item_node_id, window, cx);
    }
}
