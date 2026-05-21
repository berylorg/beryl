use beryl_model::{conversation::ConversationThreadId, semantic_graph::SemanticNodeId};
use gpui::{Context, Window};

use crate::{
    member_thread_inventory::resolved_thread_title,
    threaded_decision_graph_presentation::{
        active_decision_branch_record_for_item, archive_retry_record_for_item,
        checklist_update_retry_record_for_item, latest_handoff_record_for_item,
    },
};

use super::super::{
    PendingDecisionHandoffNavigation, ShellView, SurfaceNotice, ThreadActivationStart,
    thread_selector::ThreadSelectorActivationTarget,
};

impl ShellView {
    pub(crate) fn start_checklist_item_thread_from_graph_action_menu(
        &mut self,
        _: &gpui::ClickEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(item_node_id) = self.active_graph_menu_node_id() else {
            return;
        };

        if let Some(surface) = self.conversation_surface_mut() {
            surface.graph_thread_link_menu_mut().close();
        }
        self.start_checklist_item_thread_from_node(item_node_id, window, cx);
    }

    pub(crate) fn start_decision_branch_from_graph_action_menu(
        &mut self,
        _: &gpui::ClickEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(item_node_id) = self.active_graph_menu_node_id() else {
            return;
        };

        if let Some(surface) = self.conversation_surface_mut() {
            surface.graph_thread_link_menu_mut().close();
        }
        self.start_decision_branch_from_node(item_node_id, window, cx);
    }

    pub(crate) fn start_topic_decision_from_graph_action_menu(
        &mut self,
        _: &gpui::ClickEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(topic_node_id) = self.active_graph_menu_node_id() else {
            return;
        };

        if let Some(surface) = self.conversation_surface_mut() {
            surface.graph_thread_link_menu_mut().close();
        }
        self.start_topic_decision_from_node(topic_node_id, window, cx);
    }

    pub(crate) fn open_active_decision_branch_from_graph_row(
        &mut self,
        node_id: SemanticNodeId,
        _: &gpui::ClickEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_active_decision_branch_from_node(node_id, window, cx);
        cx.stop_propagation();
    }

    pub(crate) fn open_decision_handoff_from_graph_row(
        &mut self,
        node_id: SemanticNodeId,
        _: &gpui::ClickEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_decision_handoff_from_node(node_id, window, cx);
        cx.stop_propagation();
    }

    pub(crate) fn open_active_decision_branch_from_graph_action_menu(
        &mut self,
        _: &gpui::ClickEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(item_node_id) = self.active_graph_menu_node_id() else {
            return;
        };

        if let Some(surface) = self.conversation_surface_mut() {
            surface.graph_thread_link_menu_mut().close();
        }
        self.open_active_decision_branch_from_node(item_node_id, window, cx);
    }

    pub(crate) fn open_decision_handoff_from_graph_action_menu(
        &mut self,
        _: &gpui::ClickEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(item_node_id) = self.active_graph_menu_node_id() else {
            return;
        };

        if let Some(surface) = self.conversation_surface_mut() {
            surface.graph_thread_link_menu_mut().close();
        }
        self.open_decision_handoff_from_node(item_node_id, window, cx);
    }

    pub(crate) fn retry_decision_checklist_update_from_graph_action_menu(
        &mut self,
        _: &gpui::ClickEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(item_node_id) = self.active_graph_menu_node_id() else {
            return;
        };

        if let Some(surface) = self.conversation_surface_mut() {
            surface.graph_thread_link_menu_mut().close();
        }

        let Some((workspace_id, record_id)) = self.loaded_workspace().and_then(|loaded| {
            let record = checklist_update_retry_record_for_item(
                &loaded.threaded_decision_state,
                &item_node_id,
            )?;
            Some((loaded.workspace.id().clone(), record.record_id().clone()))
        }) else {
            self.set_graph_decision_notice(
                "Decision retry unavailable",
                "There is no failed decision checklist update to retry.",
                cx,
            );
            return;
        };

        self.queue_decision_resolution_job(workspace_id, record_id);
        self.begin_next_ready_decision_resolution_handoff(window, cx);
        cx.notify();
    }

    pub(crate) fn retry_decision_archive_from_graph_action_menu(
        &mut self,
        _: &gpui::ClickEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(item_node_id) = self.active_graph_menu_node_id() else {
            return;
        };

        if let Some(surface) = self.conversation_surface_mut() {
            surface.graph_thread_link_menu_mut().close();
        }

        let Some((workspace_id, record_id)) = self.loaded_workspace().and_then(|loaded| {
            let record =
                archive_retry_record_for_item(&loaded.threaded_decision_state, &item_node_id)?;
            Some((loaded.workspace.id().clone(), record.record_id().clone()))
        }) else {
            self.set_graph_decision_notice(
                "Decision close retry unavailable",
                "There is no failed decision branch close operation to retry.",
                cx,
            );
            return;
        };

        self.queue_decision_archive_job(workspace_id, record_id);
        self.begin_next_ready_decision_archive_cleanup(window, cx);
        cx.notify();
    }

    pub(crate) fn active_decision_branch_open_disabled_reason(
        &self,
        node_id: &SemanticNodeId,
    ) -> Option<String> {
        let record = self.loaded_workspace().and_then(|loaded| {
            active_decision_branch_record_for_item(&loaded.threaded_decision_state, node_id)
        })?;
        let child_thread_id = record.child_thread_id()?;
        self.registered_thread_activation_disabled_reason(child_thread_id)
    }

    pub(crate) fn decision_handoff_open_disabled_reason(
        &self,
        node_id: &SemanticNodeId,
    ) -> Option<String> {
        let record = self.loaded_workspace().and_then(|loaded| {
            latest_handoff_record_for_item(&loaded.threaded_decision_state, node_id)
        })?;
        self.registered_thread_activation_disabled_reason(record.parent_thread_id())
    }

    pub(crate) fn decision_checklist_update_retry_disabled_reason(
        &self,
        node_id: &SemanticNodeId,
    ) -> Option<String> {
        self.loaded_workspace().and_then(|loaded| {
            checklist_update_retry_record_for_item(&loaded.threaded_decision_state, node_id)
        })?;
        if self.graph_receiver.is_some() || self.graph_thread_start_receiver.is_some() {
            return Some("Retry after the current semantic graph operation finishes.".to_string());
        }
        None
    }

    pub(crate) fn decision_archive_retry_disabled_reason(
        &self,
        node_id: &SemanticNodeId,
    ) -> Option<String> {
        let child_thread_id = self.loaded_workspace().and_then(|loaded| {
            archive_retry_record_for_item(&loaded.threaded_decision_state, node_id)
                .and_then(|record| record.child_thread_id())
                .cloned()
        })?;
        if self.decision_archive_receiver.is_some() {
            return Some(
                "Retry after the current decision branch close operation finishes.".to_string(),
            );
        }
        self.registered_thread_activation_disabled_reason(&child_thread_id)
    }

    fn active_graph_menu_node_id(&self) -> Option<SemanticNodeId> {
        self.conversation_surface()
            .and_then(|surface| surface.graph_thread_link_menu().active())
            .map(|menu| menu.node_id().clone())
    }

    fn open_active_decision_branch_from_node(
        &mut self,
        node_id: SemanticNodeId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(child_thread_id) = self.loaded_workspace().and_then(|loaded| {
            active_decision_branch_record_for_item(&loaded.threaded_decision_state, &node_id)
                .and_then(|record| record.child_thread_id())
                .cloned()
        }) else {
            self.set_graph_decision_notice(
                "Decision branch unavailable",
                "This checklist item has no active decision branch to open.",
                cx,
            );
            return;
        };

        self.activate_decision_registered_thread(child_thread_id, "Decision branch", window, cx);
    }

    fn open_decision_handoff_from_node(
        &mut self,
        node_id: SemanticNodeId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some((record_id, parent_thread_id, handoff_turn_id)) =
            self.loaded_workspace().and_then(|loaded| {
                let record =
                    latest_handoff_record_for_item(&loaded.threaded_decision_state, &node_id)?;
                Some((
                    record.record_id().clone(),
                    record.parent_thread_id().clone(),
                    record.handoff_turn_id()?.clone(),
                ))
            })
        else {
            self.set_graph_decision_notice(
                "Decision handoff unavailable",
                "This decision does not have a recorded parent handoff turn yet.",
                cx,
            );
            return;
        };

        self.pending_decision_handoff_navigation = Some(PendingDecisionHandoffNavigation {
            record_id,
            parent_thread_id: parent_thread_id.clone(),
            handoff_turn_id: handoff_turn_id.clone(),
        });

        if self
            .conversation_surface()
            .and_then(|surface| surface.selected_thread_id())
            == Some(parent_thread_id.as_str())
        {
            self.finish_pending_decision_handoff_navigation_for_thread(
                parent_thread_id.as_str(),
                window,
                cx,
            );
            return;
        }

        match self.activate_decision_registered_thread(
            parent_thread_id.clone(),
            "Decision handoff",
            window,
            cx,
        ) {
            ThreadActivationStart::Started => {}
            ThreadActivationStart::AlreadySelected => {
                self.finish_pending_decision_handoff_navigation_for_thread(
                    parent_thread_id.as_str(),
                    window,
                    cx,
                );
            }
            ThreadActivationStart::Rejected { .. } => {
                self.pending_decision_handoff_navigation = None;
            }
        }
    }

    fn activate_decision_registered_thread(
        &mut self,
        thread_id: ConversationThreadId,
        fallback_label: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> ThreadActivationStart {
        let target = match self.registered_thread_activation_target(&thread_id, fallback_label) {
            Ok(target) => target,
            Err(message) => {
                self.set_graph_decision_notice("Thread activation unavailable", message, cx);
                return ThreadActivationStart::Rejected {
                    kind: "missing_thread",
                    message: "The bound conversation thread is no longer registered.".to_string(),
                };
            }
        };
        self.activate_thread_selector_target(target, window, cx)
    }

    fn registered_thread_activation_target(
        &self,
        thread_id: &ConversationThreadId,
        fallback_label: &str,
    ) -> Result<ThreadSelectorActivationTarget, String> {
        let loaded = self
            .loaded_workspace()
            .ok_or_else(|| "No workspace is loaded.".to_string())?;
        let registration = loaded
            .workspace_state
            .thread_registration(thread_id)
            .ok_or_else(|| "The bound conversation thread is no longer registered.".to_string())?;
        let execution_target = registration.execution_target().clone();
        let label = resolved_thread_title(
            &loaded.workspace_state,
            thread_id,
            &execution_target,
            registration.preview(),
            registration.backend_name(),
            registration.created_at_millis(),
            registration.updated_at_millis(),
        );
        Ok(ThreadSelectorActivationTarget {
            thread_id: thread_id.clone(),
            label: if label.trim().is_empty() {
                fallback_label.to_string()
            } else {
                label
            },
            execution_target,
        })
    }

    fn registered_thread_activation_disabled_reason(
        &self,
        thread_id: &ConversationThreadId,
    ) -> Option<String> {
        let target = match self.registered_thread_activation_target(thread_id, "Decision thread") {
            Ok(target) => target,
            Err(message) => return Some(message),
        };
        self.known_backend_unavailable_block_for_target(&target.execution_target)
            .map(|block| block.message)
    }

    fn set_graph_decision_notice(
        &mut self,
        title: &'static str,
        message: impl Into<String>,
        cx: &mut Context<Self>,
    ) {
        if let Some(surface) = self.conversation_surface_mut() {
            surface.set_notice(SurfaceNotice::new(title, message.into()));
        }
        cx.notify();
    }
}
