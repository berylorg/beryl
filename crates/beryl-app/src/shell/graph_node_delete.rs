use std::time::{Instant, SystemTime, UNIX_EPOCH};

use beryl_model::{
    provenance::{MutationProvenance, MutationSource},
    semantic_graph::{SemanticGraph, SemanticNodeId},
};
use gpui::{ClickEvent, Context, KeyDownEvent, KeyUpEvent, MouseDownEvent, MouseUpEvent, Window};

use crate::{
    NodeLeafDeleteRequest, NodeSubtreeDeleteRequest, node_leaf_delete_patch,
    node_subtree_delete_patch,
};

use super::{
    ShellView, SurfaceNotice,
    graph::GraphOptimisticMutation,
    graph_link_menu_state::GraphNodeDeleteHoldSource,
    graph_node_action_policy::{
        GRAPH_NODE_LEAF_DELETE_NON_LEAF_REASON, graph_node_action_keyboard_activation_key,
        graph_node_delete_blocked_by_graph_work,
    },
    graph_worker::{spawn_node_leaf_delete_worker, spawn_node_subtree_delete_worker},
};

impl ShellView {
    pub(crate) fn delete_graph_node_leaf_from_action_menu(
        &mut self,
        _: &ClickEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.start_graph_node_leaf_delete_from_action_menu(window, cx);
    }

    pub(crate) fn delete_graph_node_leaf_keyboard_from_action_menu(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if event.is_held || !graph_node_action_keyboard_activation_key(event.keystroke.key.as_str())
        {
            return;
        }

        self.start_graph_node_leaf_delete_from_action_menu(window, cx);
    }

    pub(crate) fn begin_graph_node_delete_hold_from_action_menu(
        &mut self,
        _: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.begin_graph_node_delete_hold_from_action_menu_source(
            GraphNodeDeleteHoldSource::Pointer,
            window,
            cx,
        );
    }

    pub(crate) fn cancel_graph_node_delete_hold_from_action_menu(
        &mut self,
        _: &MouseUpEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(surface) = self.conversation_surface_mut()
            && surface
                .graph_thread_link_menu_mut()
                .cancel_delete_hold_source(GraphNodeDeleteHoldSource::Pointer)
        {
            cx.stop_propagation();
            cx.notify();
        }
    }

    pub(crate) fn cancel_graph_node_delete_hold_on_hover_change(
        &mut self,
        hovered: &bool,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if *hovered {
            return;
        }

        if let Some(surface) = self.conversation_surface_mut()
            && surface.graph_thread_link_menu_mut().cancel_delete_hold()
        {
            cx.notify();
        }
    }

    pub(crate) fn begin_graph_node_delete_keyboard_hold_from_action_menu(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if event.is_held || !graph_node_action_keyboard_activation_key(event.keystroke.key.as_str())
        {
            return;
        }

        self.begin_graph_node_delete_hold_from_action_menu_source(
            GraphNodeDeleteHoldSource::Keyboard,
            window,
            cx,
        );
    }

    pub(crate) fn cancel_graph_node_delete_keyboard_hold_from_action_menu(
        &mut self,
        event: &KeyUpEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !graph_node_action_keyboard_activation_key(event.keystroke.key.as_str()) {
            return;
        }

        if let Some(surface) = self.conversation_surface_mut()
            && surface
                .graph_thread_link_menu_mut()
                .cancel_delete_hold_source(GraphNodeDeleteHoldSource::Keyboard)
        {
            cx.stop_propagation();
            cx.notify();
        }
    }

    pub(crate) fn handle_graph_thread_link_menu_key_up(
        &mut self,
        event: &KeyUpEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if !graph_node_action_keyboard_activation_key(event.keystroke.key.as_str()) {
            return false;
        }

        let cancelled = self.conversation_surface_mut().is_some_and(|surface| {
            surface
                .graph_thread_link_menu_mut()
                .cancel_delete_hold_source(GraphNodeDeleteHoldSource::Keyboard)
        });
        if cancelled {
            cx.notify();
        }
        cancelled
    }

    fn start_graph_node_leaf_delete_from_action_menu(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if graph_node_delete_blocked_by_graph_work(
            self.graph_receiver.is_some(),
            self.graph_thread_start_receiver.is_some(),
        ) {
            return;
        }

        let Some(node_id) = self.conversation_surface().and_then(|surface| {
            surface
                .graph_thread_link_menu()
                .active()
                .map(|open| open.node_id().clone())
        }) else {
            return;
        };

        let Some((workspace_id, request, optimistic_mutation)) =
            self.build_node_leaf_delete_request(&node_id)
        else {
            return;
        };
        let optimistic_mutation_id = optimistic_mutation.id();

        if let Some(surface) = self.conversation_surface_mut() {
            surface
                .graph_thread_link_menu_mut()
                .mark_leaf_delete_in_flight(node_id);
            if let Err(error) = surface.begin_optimistic_graph_mutation(optimistic_mutation) {
                surface.report_optimistic_graph_mutation_failure(None, error);
                cx.notify();
                return;
            }
        }
        let Some(persistence) = self.workspace_persistence_for_worker() else {
            return;
        };
        self.graph_receiver = Some(spawn_node_leaf_delete_worker(
            persistence,
            workspace_id,
            request,
            Some(optimistic_mutation_id),
        ));
        self.schedule_poll_if_needed(window, cx);
        cx.stop_propagation();
        cx.notify();
    }

    fn begin_graph_node_delete_hold_from_action_menu_source(
        &mut self,
        source: GraphNodeDeleteHoldSource,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if graph_node_delete_blocked_by_graph_work(
            self.graph_receiver.is_some(),
            self.graph_thread_start_receiver.is_some(),
        ) {
            return;
        }

        let Some(node_id) = self.conversation_surface().and_then(|surface| {
            let node_id = surface.graph_thread_link_menu().active()?.node_id().clone();
            surface
                .graph_overlay()
                .graph()
                .node(&node_id)
                .is_some()
                .then_some(node_id)
        }) else {
            return;
        };

        let started = self.conversation_surface_mut().is_some_and(|surface| {
            surface
                .graph_thread_link_menu_mut()
                .begin_delete_hold(node_id, source, Instant::now())
        });
        if !started {
            return;
        }

        self.schedule_poll_if_needed(window, cx);
        cx.stop_propagation();
        cx.notify();
    }

    pub(super) fn poll_graph_node_action_menu_hold(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let target_exists = self
            .conversation_surface()
            .and_then(|surface| {
                let node_id = surface.graph_thread_link_menu().active()?.node_id();
                Some(surface.graph_overlay().graph().node(node_id).is_some())
            })
            .unwrap_or(false);
        let graph_mutation_in_flight = graph_node_delete_blocked_by_graph_work(
            self.graph_receiver.is_some(),
            self.graph_thread_start_receiver.is_some(),
        );
        let now = Instant::now();
        let mut updated = false;
        let mut completed_node_id = None;

        if let Some(surface) = self.conversation_surface_mut() {
            let menu = surface.graph_thread_link_menu_mut();
            if !window.is_window_active() || graph_mutation_in_flight {
                updated |= menu.cancel_delete_hold();
            } else {
                updated |= menu.cancel_delete_hold_for_stale_target(target_exists);
                if menu.delete_hold_active() {
                    completed_node_id = menu.complete_delete_hold_if_ready(now);
                    updated = true;
                }
            }
        }

        if let Some(node_id) = completed_node_id {
            updated |= self.complete_graph_node_delete_hold_from_action_menu(node_id, window, cx);
        }

        updated
    }

    fn complete_graph_node_delete_hold_from_action_menu(
        &mut self,
        node_id: SemanticNodeId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if graph_node_delete_blocked_by_graph_work(
            self.graph_receiver.is_some(),
            self.graph_thread_start_receiver.is_some(),
        ) {
            return false;
        }

        let Some((workspace_id, request, optimistic_mutation)) =
            self.build_node_subtree_delete_request(&node_id)
        else {
            return false;
        };
        let optimistic_mutation_id = optimistic_mutation.id();

        if let Some(surface) = self.conversation_surface_mut() {
            surface
                .graph_thread_link_menu_mut()
                .mark_delete_in_flight(node_id);
            if let Err(error) = surface.begin_optimistic_graph_mutation(optimistic_mutation) {
                surface.report_optimistic_graph_mutation_failure(None, error);
                cx.notify();
                return true;
            }
        }
        let Some(persistence) = self.workspace_persistence_for_worker() else {
            return false;
        };
        self.graph_receiver = Some(spawn_node_subtree_delete_worker(
            persistence,
            workspace_id,
            request,
            Some(optimistic_mutation_id),
        ));
        self.schedule_poll_if_needed(window, cx);
        cx.notify();
        true
    }

    fn build_node_subtree_delete_request(
        &mut self,
        node_id: &SemanticNodeId,
    ) -> Option<(
        beryl_model::workspace::BerylWorkspaceId,
        NodeSubtreeDeleteRequest,
        GraphOptimisticMutation,
    )> {
        let workspace_id = self.loaded_workspace()?.workspace.id().clone();
        let (target_exists, graph_revision, affected_node_ids) = self
            .conversation_surface()
            .map(|surface| {
                let graph = surface.graph_overlay().graph();
                (
                    graph.node(node_id).is_some(),
                    surface.graph_overlay().revision(),
                    collect_hard_subtree_node_ids(graph, node_id),
                )
            })
            .unwrap_or_default();

        if !target_exists {
            if let Some(surface) = self.conversation_surface_mut() {
                surface.graph_thread_link_menu_mut().close();
                surface.set_notice(SurfaceNotice::new(
                    "Semantic node unavailable",
                    "The selected semantic node was already removed.",
                ));
            }
            return None;
        }

        let provenance = MutationProvenance::new(
            "operator",
            current_unix_millis(),
            MutationSource::workspace_action("delete_graph_node_subtree").ok()?,
            Some(100),
        )
        .ok()?;
        let mutation_id = self
            .conversation_surface_mut()?
            .reserve_optimistic_graph_mutation_id();
        let patch = node_subtree_delete_patch(node_id, &provenance);
        let optimistic_mutation = GraphOptimisticMutation::new(
            mutation_id,
            graph_revision,
            patch,
            affected_node_ids,
            "Deleting semantic node subtree",
        );

        Some((
            workspace_id.clone(),
            NodeSubtreeDeleteRequest {
                workspace_id,
                node_id: node_id.clone(),
                provenance,
                expected_base_revision: Some(optimistic_mutation.base_revision()),
            },
            optimistic_mutation,
        ))
    }

    fn build_node_leaf_delete_request(
        &mut self,
        node_id: &SemanticNodeId,
    ) -> Option<(
        beryl_model::workspace::BerylWorkspaceId,
        NodeLeafDeleteRequest,
        GraphOptimisticMutation,
    )> {
        let workspace_id = self.loaded_workspace()?.workspace.id().clone();
        let graph_revision = self.conversation_surface()?.graph_overlay().revision();
        let target_has_hard_children = self.conversation_surface().and_then(|surface| {
            let graph = surface.graph_overlay().graph();
            graph.node(node_id)?;
            Some(
                graph
                    .child_ids_of(node_id)
                    .is_some_and(|children| !children.is_empty()),
            )
        });

        match target_has_hard_children {
            None => {
                if let Some(surface) = self.conversation_surface_mut() {
                    surface.graph_thread_link_menu_mut().close();
                    surface.set_notice(SurfaceNotice::new(
                        "Semantic node unavailable",
                        "The selected semantic node was already removed.",
                    ));
                }
                return None;
            }
            Some(true) => {
                if let Some(surface) = self.conversation_surface_mut() {
                    surface.set_notice(SurfaceNotice::new(
                        "Semantic node has child nodes",
                        GRAPH_NODE_LEAF_DELETE_NON_LEAF_REASON,
                    ));
                }
                return None;
            }
            Some(false) => {}
        }

        let provenance = MutationProvenance::new(
            "operator",
            current_unix_millis(),
            MutationSource::workspace_action("delete_graph_node_leaf").ok()?,
            Some(100),
        )
        .ok()?;
        let mutation_id = self
            .conversation_surface_mut()?
            .reserve_optimistic_graph_mutation_id();
        let patch = node_leaf_delete_patch(node_id, &provenance);
        let optimistic_mutation = GraphOptimisticMutation::new(
            mutation_id,
            graph_revision,
            patch,
            [node_id.clone()],
            "Deleting semantic node",
        );

        Some((
            workspace_id.clone(),
            NodeLeafDeleteRequest {
                workspace_id,
                node_id: node_id.clone(),
                provenance,
                expected_base_revision: Some(optimistic_mutation.base_revision()),
            },
            optimistic_mutation,
        ))
    }
}

fn collect_hard_subtree_node_ids(
    graph: &SemanticGraph,
    node_id: &SemanticNodeId,
) -> Vec<SemanticNodeId> {
    let mut node_ids = Vec::new();
    collect_hard_subtree_node_ids_into(graph, node_id, &mut node_ids);
    node_ids
}

fn collect_hard_subtree_node_ids_into(
    graph: &SemanticGraph,
    node_id: &SemanticNodeId,
    node_ids: &mut Vec<SemanticNodeId>,
) {
    if graph.node(node_id).is_none() {
        return;
    }
    node_ids.push(node_id.clone());
    if let Some(children) = graph.child_ids_of(node_id) {
        for child_id in children {
            collect_hard_subtree_node_ids_into(graph, child_id, node_ids);
        }
    }
}

fn current_unix_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}
