use std::{
    sync::mpsc::{self, Receiver, TryRecvError},
    thread,
};

use beryl_model::{conversation::WorkspaceConversationState, workspace::BerylWorkspaceId};

use crate::{
    BerylWorkspacePersistence,
    member_thread_inventory::{
        MemberThreadInventoryEvent, MemberThreadInventorySnapshot,
        build_workspace_syndic_catalog_snapshot,
    },
};

use super::ShellView;

pub(super) enum MemberThreadInventoryRefreshUpdate {
    Finished(Result<MemberThreadInventorySnapshot, String>),
}

impl ShellView {
    pub(super) fn poll_member_thread_inventory_updates(&mut self) -> bool {
        let Some(receiver) = self.member_thread_inventory_receiver.as_ref() else {
            return false;
        };

        let outcome = match receiver.try_recv() {
            Ok(MemberThreadInventoryRefreshUpdate::Finished(outcome)) => outcome,
            Err(TryRecvError::Empty) => return false,
            Err(TryRecvError::Disconnected) => {
                Err("Thread catalog refresh worker stopped unexpectedly.".to_string())
            }
        };
        self.member_thread_inventory_receiver = None;

        let changed = if let Some(surface) = self.conversation_surface_mut() {
            let changed = match outcome {
                Ok(snapshot) => surface
                    .member_thread_inventory_mut()
                    .apply_refresh_success(snapshot),
                Err(error) => surface
                    .member_thread_inventory_mut()
                    .apply_refresh_failure(error),
            };
            surface.reconcile_thread_selector_state();
            changed
        } else {
            false
        };
        changed
    }

    pub(super) fn begin_member_thread_inventory_refresh_if_needed(&mut self) -> bool {
        if self.member_thread_inventory_receiver.is_some() {
            return false;
        }
        let Some((workspace_id, workspace_state)) = self.loaded_workspace().map(|loaded| {
            (
                loaded.workspace.id().clone(),
                loaded.workspace_state.clone(),
            )
        }) else {
            return false;
        };
        let Some(persistence) = self.workspace_persistence_for_worker() else {
            if let Some(surface) = self.conversation_surface_mut() {
                surface
                    .member_thread_inventory_mut()
                    .apply_refresh_failure("Workspace persistence is unavailable.");
            }
            return false;
        };
        let Some(surface) = self.conversation_surface_mut() else {
            return false;
        };
        if !surface.member_thread_inventory_mut().begin_refresh() {
            return false;
        }
        self.member_thread_inventory_receiver = Some(spawn_member_thread_inventory_refresh_worker(
            persistence,
            workspace_id,
            workspace_state,
        ));
        true
    }

    pub(super) fn reset_member_thread_inventory_for_workspace_state(&mut self) {
        self.apply_member_thread_inventory_event(MemberThreadInventoryEvent::MemberSetChanged);
    }

    pub(super) fn mark_member_thread_inventory_refresh_needed(&mut self) {
        self.apply_member_thread_inventory_event(
            MemberThreadInventoryEvent::InventoryContentsChanged,
        );
    }

    pub(super) fn apply_member_thread_inventory_event(
        &mut self,
        event: MemberThreadInventoryEvent,
    ) {
        let Some((workspace_id, workspace_state)) = self.loaded_workspace().map(|loaded| {
            (
                loaded.workspace.id().clone(),
                loaded.workspace_state.clone(),
            )
        }) else {
            return;
        };
        if let Some(surface) = self.conversation_surface_mut() {
            surface.member_thread_inventory_mut().apply_event(
                event,
                workspace_id,
                &workspace_state,
            );
            surface.reconcile_thread_selector_state();
        }
    }
}

fn spawn_member_thread_inventory_refresh_worker(
    persistence: BerylWorkspacePersistence,
    workspace_id: BerylWorkspaceId,
    workspace_state: WorkspaceConversationState,
) -> Receiver<MemberThreadInventoryRefreshUpdate> {
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || {
        let storage_dir = persistence.workspace_syndic_storage_dir(&workspace_id);
        let outcome =
            build_workspace_syndic_catalog_snapshot(&storage_dir, workspace_id, &workspace_state);
        let _ = sender.send(MemberThreadInventoryRefreshUpdate::Finished(outcome));
    });
    receiver
}
