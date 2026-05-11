use std::time::{SystemTime, UNIX_EPOCH};

use beryl_model::{
    conversation::ConversationThreadId,
    provenance::{MutationProvenance, MutationSource},
    semantic_graph::{SemanticGraph, SemanticNodeId, ThreadRefDraft, ThreadRefId},
};
use gpui::{Bounds, Context, KeyDownEvent, MouseDownEvent, Pixels, Window};

use crate::{
    ThreadRefUpsertRequest,
    member_thread_inventory::{MemberThreadInventoryMemberKey, MemberThreadInventoryThread},
    thread_ref_upsert_patch,
};

use super::{
    ShellView, SurfaceNotice, graph::GraphOptimisticMutation,
    graph_node_action_policy::graph_node_delete_blocked_by_graph_work,
    spawn_thread_ref_link_worker,
};

pub(crate) use super::graph_link_menu_state::{GraphThreadLinkMenuState, GraphThreadLinkMenuView};

impl ShellView {
    pub(crate) fn open_graph_node_thread_link_menu(
        &mut self,
        column_index: usize,
        node_id: SemanticNodeId,
        event: &MouseDownEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let changed = self.conversation_surface_mut().is_some_and(|surface| {
            let changed = surface.select_graph_node(column_index, &node_id);
            surface.transcript_branch_menu_mut().close();
            surface.status_line_operations_mut().close();
            surface.checklist_thread_start_menu_mut().close();
            surface
                .graph_thread_link_menu_mut()
                .open_node(node_id.clone(), event.position);
            changed
        });
        if changed {
            self.prune_graph_scrollbar_activity();
            self.notify_checklist_sidebar_panel(cx);
        }
        cx.stop_propagation();
        cx.notify();
    }

    pub(crate) fn show_graph_thread_link_menu(
        &mut self,
        _: &gpui::ClickEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(surface) = self.conversation_surface_mut() {
            surface.graph_thread_link_menu_mut().set_link_threads_view();
            cx.notify();
        }
    }

    pub(crate) fn open_graph_thread_link_member(
        &mut self,
        member: MemberThreadInventoryMemberKey,
        _: &gpui::ClickEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(surface) = self.conversation_surface_mut() {
            surface
                .graph_thread_link_menu_mut()
                .set_member_threads_view(member);
            cx.notify();
        }
    }

    pub(crate) fn show_graph_thread_link_members(
        &mut self,
        _: &gpui::ClickEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(surface) = self.conversation_surface_mut() {
            surface.graph_thread_link_menu_mut().set_link_threads_view();
            cx.notify();
        }
    }

    pub(crate) fn show_graph_node_action_menu(
        &mut self,
        _: &gpui::ClickEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(surface) = self.conversation_surface_mut() {
            surface.graph_thread_link_menu_mut().set_root_view();
            cx.notify();
        }
    }

    pub(crate) fn handle_graph_thread_link_menu_mouse_down(
        &mut self,
        event: &MouseDownEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let should_dismiss = self.conversation_surface().is_some_and(|surface| {
            surface
                .graph_thread_link_menu()
                .should_dismiss_for_mouse_down(event.position)
        });
        if should_dismiss && let Some(surface) = self.conversation_surface_mut() {
            surface.graph_thread_link_menu_mut().close();
            cx.notify();
        }
    }

    pub(crate) fn handle_graph_thread_link_menu_key_down(
        &mut self,
        event: &KeyDownEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if event.keystroke.key.as_str() != "escape" {
            return false;
        }
        if let Some(surface) = self.conversation_surface_mut()
            && surface.graph_thread_link_menu().is_open()
        {
            surface.graph_thread_link_menu_mut().close();
            cx.notify();
            return true;
        }
        false
    }

    pub(crate) fn record_graph_thread_link_menu_bounds(
        &mut self,
        bounds: Option<Bounds<Pixels>>,
        _: &mut Context<Self>,
    ) {
        if let Some(surface) = self.conversation_surface_mut() {
            surface.graph_thread_link_menu_mut().set_bounds(bounds);
        }
    }

    pub(crate) fn link_graph_thread_to_node(
        &mut self,
        thread: MemberThreadInventoryThread,
        _: &gpui::ClickEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if graph_node_delete_blocked_by_graph_work(
            self.graph_receiver.is_some(),
            self.graph_thread_start_receiver.is_some(),
        ) {
            return;
        }

        let Some((workspace_id, request, optimistic_mutation)) =
            self.build_thread_ref_upsert_request(&thread)
        else {
            return;
        };
        let optimistic_mutation_id = optimistic_mutation.id();

        if let Some(surface) = self.conversation_surface_mut() {
            surface.graph_thread_link_menu_mut().close();
            if let Err(error) = surface.begin_optimistic_graph_mutation(optimistic_mutation) {
                surface.report_optimistic_graph_mutation_failure(None, error);
                cx.notify();
                return;
            }
        }

        let touched_manifest = self.loaded_workspace_mut().is_some_and(|loaded| {
            loaded
                .workspace_state
                .remember_thread(thread.to_registered_thread())
        });
        if touched_manifest {
            self.persist_current_workspace_state(true);
        }

        let Some(persistence) = self.workspace_persistence_for_worker() else {
            return;
        };
        self.graph_receiver = Some(spawn_thread_ref_link_worker(
            persistence,
            workspace_id,
            request,
            Some(optimistic_mutation_id),
        ));
        cx.notify();
    }

    fn build_thread_ref_upsert_request(
        &mut self,
        thread: &MemberThreadInventoryThread,
    ) -> Option<(
        beryl_model::workspace::BerylWorkspaceId,
        ThreadRefUpsertRequest,
        GraphOptimisticMutation,
    )> {
        let workspace_id = self.loaded_workspace()?.workspace.id().clone();
        let surface = self.conversation_surface()?;
        let node_id = surface.graph_thread_link_menu().active()?.node_id().clone();
        let graph = surface.graph_overlay().graph();
        let graph_revision = surface.graph_overlay().revision();

        if graph
            .thread_refs_for_node(&node_id)
            .any(|thread_ref| thread_ref.thread_id() == thread.thread_id())
        {
            if let Some(surface) = self.conversation_surface_mut() {
                surface.graph_thread_link_menu_mut().close();
                surface.set_notice(SurfaceNotice::new(
                    "Thread already linked",
                    "The selected thread is already attached to this semantic node.",
                ));
            }
            return None;
        }

        let thread_ref = ThreadRefDraft::new(
            next_thread_ref_id(graph, &node_id, thread.thread_id()),
            node_id.clone(),
            thread.thread_id().clone(),
            thread.execution_target().clone(),
            thread.title(),
        );
        let provenance = MutationProvenance::new(
            "operator",
            current_unix_millis(),
            MutationSource::workspace_action("link_thread").ok()?,
            Some(100),
        )
        .ok()?;

        let mutation_id = self
            .conversation_surface_mut()?
            .reserve_optimistic_graph_mutation_id();
        let patch = thread_ref_upsert_patch(&thread_ref, &provenance);
        let optimistic_mutation = GraphOptimisticMutation::new(
            mutation_id,
            graph_revision,
            patch,
            [node_id],
            "Linking thread to semantic node",
        );

        Some((
            workspace_id.clone(),
            ThreadRefUpsertRequest {
                workspace_id,
                thread_ref,
                provenance,
                expected_base_revision: Some(optimistic_mutation.base_revision()),
            },
            optimistic_mutation,
        ))
    }
}

fn next_thread_ref_id(
    graph: &SemanticGraph,
    node_id: &SemanticNodeId,
    thread_id: &ConversationThreadId,
) -> ThreadRefId {
    let base = format!(
        "thread_ref_{}_{}",
        sanitize_id_part(node_id.as_str()),
        sanitize_id_part(thread_id.as_str())
    );
    for suffix in 0usize.. {
        let candidate = if suffix == 0 {
            base.clone()
        } else {
            format!("{base}_{suffix}")
        };
        let Ok(thread_ref_id) = ThreadRefId::new(candidate) else {
            continue;
        };
        if graph.thread_ref(&thread_ref_id).is_none() {
            return thread_ref_id;
        }
    }

    unreachable!("usize suffix space is non-empty")
}

fn sanitize_id_part(value: &str) -> String {
    let sanitized: String = value
        .chars()
        .map(|ch| match ch {
            'a'..='z' | '0'..='9' | '-' | '_' => ch,
            'A'..='Z' => ch.to_ascii_lowercase(),
            _ => '_',
        })
        .collect();
    if sanitized.is_empty() {
        "untitled".to_string()
    } else {
        sanitized
    }
}

fn current_unix_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}
