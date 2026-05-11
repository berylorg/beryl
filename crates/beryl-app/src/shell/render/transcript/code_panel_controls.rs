use std::{
    cell::RefCell,
    collections::{HashMap, HashSet},
    rc::Rc,
    sync::Arc,
};

use gpui::{App, ClipboardItem, Entity, Pixels, ScrollHandle, px};

use crate::shell::transcript_markdown::markdown_code_panel_id;

use super::super::code_panel::{
    CodePanelHeader, CodePanelHeaderAction, CodePanelResize, CodePanelScrollChrome,
    CodePanelVerticalWheelOwnership, CodePanelWrapMode,
};
use super::{TRANSCRIPT_CODE_PANEL_MIN_HEIGHT, TranscriptCodeLayout, TranscriptPanel};

#[derive(Clone)]
pub(super) struct TranscriptCodePanelState {
    entity: Entity<TranscriptPanel>,
    soft_wrapped_panel_keys: Arc<HashSet<String>>,
    resized_panel_heights: Arc<HashMap<String, Pixels>>,
    scroll_handles: Rc<RefCell<HashMap<String, ScrollHandle>>>,
    scrollbar_opacities: Arc<HashMap<String, f32>>,
    selected_nested_code_panel_id: Arc<Option<String>>,
}

#[derive(Clone)]
pub(super) struct TranscriptCodePanelControls {
    state: TranscriptCodePanelState,
    row_identity: String,
    block_path: String,
}

impl TranscriptCodePanelState {
    pub(super) fn new(
        entity: Entity<TranscriptPanel>,
        soft_wrapped_panel_keys: Arc<HashSet<String>>,
        resized_panel_heights: Arc<HashMap<String, Pixels>>,
        scroll_handles: Rc<RefCell<HashMap<String, ScrollHandle>>>,
        scrollbar_opacities: Arc<HashMap<String, f32>>,
        selected_nested_code_panel_id: Arc<Option<String>>,
    ) -> Self {
        Self {
            entity,
            soft_wrapped_panel_keys,
            resized_panel_heights,
            scroll_handles,
            scrollbar_opacities,
            selected_nested_code_panel_id,
        }
    }

    pub(super) fn entity(&self) -> Entity<TranscriptPanel> {
        self.entity.clone()
    }

    pub(super) fn controls_for(
        &self,
        row_identity: impl Into<String>,
        block_path: impl Into<String>,
    ) -> TranscriptCodePanelControls {
        TranscriptCodePanelControls {
            state: self.clone(),
            row_identity: row_identity.into(),
            block_path: block_path.into(),
        }
    }
}

impl TranscriptCodePanelControls {
    pub(super) fn panel_id_for(&self, code_path: &str) -> String {
        markdown_code_panel_id(
            self.row_identity.as_str(),
            self.block_path.as_str(),
            code_path,
        )
    }

    pub(super) fn wrap_mode(
        &self,
        panel_id: &str,
        code_layout: TranscriptCodeLayout,
    ) -> CodePanelWrapMode {
        if self.state.soft_wrapped_panel_keys.contains(panel_id) {
            CodePanelWrapMode::Smart {
                columns: code_layout.transcript_bordered_panel_columns,
            }
        } else {
            CodePanelWrapMode::NoWrap
        }
    }

    pub(super) fn header(&self, panel_id: &str, source: &str) -> CodePanelHeader {
        CodePanelHeader {
            title: None,
            leading_actions: Vec::new(),
            trailing_actions: vec![
                self.soft_wrap_action(panel_id),
                self.copy_action(source.to_string()),
            ],
        }
    }

    pub(super) fn scroll_chrome(&self, panel_id: &str) -> CodePanelScrollChrome {
        let handle = self.scroll_handle(panel_id);
        let panel_key = panel_id.to_string();
        let activity_panel_key = panel_key.clone();
        let activity_entity = self.state.entity.clone();
        let select_panel_key = panel_key.clone();
        let select_entity = self.state.entity.clone();

        CodePanelScrollChrome {
            handle,
            scrollbar_opacity: self
                .state
                .scrollbar_opacities
                .get(panel_id)
                .copied()
                .unwrap_or(0.0),
            on_activity: Some(Arc::new(move |cx: &mut App| {
                activity_entity.update(cx, |view, cx| {
                    view.note_code_panel_scrollbar_activity(activity_panel_key.clone(), cx);
                });
            })),
            on_select: Some(Arc::new(move |cx: &mut App| {
                select_entity.update(cx, |view, cx| {
                    view.select_nested_code_panel(select_panel_key.clone(), cx);
                });
            })),
            vertical_wheel_ownership: self.vertical_wheel_ownership(panel_id),
        }
    }

    pub(super) fn resize(
        &self,
        panel_id: &str,
        code_layout: TranscriptCodeLayout,
    ) -> CodePanelResize {
        let panel_key = panel_id.to_string();
        let entity = self.state.entity.clone();
        CodePanelResize {
            current_height: self.state.resized_panel_heights.get(panel_id).copied(),
            min_height: px(TRANSCRIPT_CODE_PANEL_MIN_HEIGHT),
            max_height: Some(code_layout.resizable_panel_max_height),
            on_resize_start: Arc::new(move |panel_top, current_height, event, cx| {
                entity.update(cx, |view, cx| {
                    view.begin_code_panel_resize(
                        panel_key.clone(),
                        panel_top,
                        current_height,
                        event,
                        cx,
                    );
                });
            }),
        }
    }

    fn soft_wrap_action(&self, panel_id: &str) -> CodePanelHeaderAction {
        let panel_key = panel_id.to_string();
        let entity = self.state.entity.clone();
        CodePanelHeaderAction {
            key: "soft-wrap".to_string(),
            label: "Soft Wrap".to_string(),
            active: self.state.soft_wrapped_panel_keys.contains(panel_id),
            on_click: Arc::new(move |cx: &mut App| {
                entity.update(cx, |view, cx| {
                    view.toggle_code_panel_soft_wrap(panel_key.clone(), cx);
                });
            }),
        }
    }

    fn copy_action(&self, source: String) -> CodePanelHeaderAction {
        CodePanelHeaderAction {
            key: "copy".to_string(),
            label: "Copy".to_string(),
            active: false,
            on_click: Arc::new(move |cx: &mut App| {
                cx.write_to_clipboard(ClipboardItem::new_string(source.clone()));
            }),
        }
    }

    fn scroll_handle(&self, panel_id: &str) -> ScrollHandle {
        self.state
            .scroll_handles
            .borrow_mut()
            .entry(panel_id.to_string())
            .or_insert_with(ScrollHandle::new)
            .clone()
    }

    fn vertical_wheel_ownership(&self, panel_id: &str) -> CodePanelVerticalWheelOwnership {
        if self.state.selected_nested_code_panel_id.as_ref().as_deref() == Some(panel_id) {
            CodePanelVerticalWheelOwnership::Panel
        } else {
            CodePanelVerticalWheelOwnership::Parent
        }
    }
}
