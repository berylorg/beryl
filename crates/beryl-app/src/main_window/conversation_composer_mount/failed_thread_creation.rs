use super::*;
use crate::main_window::{
    MainWindowFailedThreadCreationCapture, MainWindowFailedThreadCreationGuiAuthority,
    MainWindowFailedThreadCreationResources, MainWindowFailedThreadCreationRetirement,
};

pub(crate) struct MainWindowFailedThreadCreationMountCapture {
    pub(crate) authority: MainWindowFailedThreadCreationGuiAuthority,
    pub(crate) selected: Entity<MainWindowConversationComposer>,
    pub(crate) selected_capture: Option<MainWindowFailedThreadCreationCapture>,
    pending: Option<Entity<MainWindowConversationComposer>>,
    pending_capture: Option<MainWindowFailedThreadCreationCapture>,
    pub(crate) resources: Option<MainWindowFailedThreadCreationResources>,
    selected_release_work: Option<Vec<gpui_text_input::RangeTextInputRequest>>,
    pending_release_work: Option<Vec<gpui_text_input::RangeTextInputRequest>>,
    detached_pending: bool,
    pending_resources_detached: bool,
    pub(crate) detached: bool,
    widgets_released: bool,
}

impl MainWindowFailedThreadCreationMountCapture {
    pub(crate) fn mounted_successor(
        &self,
        cx: &App,
    ) -> Option<MainWindowComposerSelectionIdentity> {
        let selected = self.selected.read(cx).selection_identity();
        if selected.claim() != self.authority.prior.claim() {
            Some(selected)
        } else {
            self.pending
                .as_ref()
                .map(|pending| pending.read(cx).selection_identity())
        }
    }

    pub(crate) fn widgets_released(&self) -> bool {
        self.widgets_released
    }

    #[cfg(all(test, feature = "test-faults"))]
    pub(crate) fn widget_release_counts(&self) -> (usize, usize) {
        (
            self.selected_release_work.as_ref().map_or(0, Vec::len),
            self.pending_release_work.as_ref().map_or(0, Vec::len),
        )
    }
}

impl MainWindowConversationComposerMount {
    pub(crate) fn gate_failed_thread_creation(
        &mut self,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        self.contribution
            .as_ref()
            .ok_or("original failed creation prior editor is missing")?
            .update(cx, |resident, cx| resident.gate_failed_thread_creation(cx))?;
        if let Some(pending) = self.pending_presentation.as_ref() {
            pending
                .contribution
                .update(cx, |resident, cx| resident.gate_failed_thread_creation(cx))?;
        }
        Ok(())
    }
    pub(crate) fn thread_creation_recovery_close_ticket(
        &mut self,
        cx: &mut Context<Self>,
    ) -> Result<crate::main_window::MainWindowConversationComposerCloseTicket, String> {
        let selected = self
            .contribution
            .as_ref()
            .ok_or("original thread creation editor is missing")?
            .read(cx)
            .selection_identity();
        let generation = self
            .window_close_generation
            .checked_add(1)
            .ok_or("original thread creation close identity exhausted")?;
        self.window_close_generation = generation;
        Ok(
            crate::main_window::MainWindowConversationComposerCloseTicket::for_recovery(
                cx.entity_id(),
                generation,
                selected,
            ),
        )
    }

    pub(crate) fn begin_failed_thread_creation_capture(
        &mut self,
        authority: MainWindowFailedThreadCreationGuiAuthority,
        service: &Arc<MainWindowConversationComposerService>,
        cx: &mut Context<Self>,
    ) -> Result<MainWindowFailedThreadCreationMountCapture, String> {
        if self
            .service
            .as_ref()
            .is_none_or(|bound| !Arc::ptr_eq(bound, service))
        {
            return Err("original thread creation mount service changed".into());
        }
        self.suspend_autosave()?;
        let selected = self
            .contribution
            .as_ref()
            .ok_or("original thread creation editor is missing")?
            .clone();
        let pending = self
            .pending_presentation
            .as_ref()
            .map(|pending| {
                if Some(pending.receipt) != authority.receipt
                    || Some(pending.contribution.read(cx).selection_identity())
                        != authority.successor
                {
                    return Err("original thread creation pending editor changed".to_owned());
                }
                Ok(pending.contribution.clone())
            })
            .transpose()?;
        Ok(MainWindowFailedThreadCreationMountCapture {
            authority,
            selected,
            pending,
            selected_capture: None,
            pending_capture: None,
            resources: None,
            selected_release_work: None,
            pending_release_work: None,
            detached_pending: false,
            pending_resources_detached: false,
            detached: false,
            widgets_released: false,
        })
    }

    pub(crate) fn advance_failed_thread_creation_capture(
        &mut self,
        capture: &mut MainWindowFailedThreadCreationMountCapture,
        service: &Arc<MainWindowConversationComposerService>,
        cx: &mut Context<Self>,
    ) -> Result<bool, String> {
        if capture.detached {
            return Ok(true);
        }
        if self.contribution.as_ref() != Some(&capture.selected)
            || self
                .service
                .as_ref()
                .is_none_or(|bound| !Arc::ptr_eq(bound, service))
        {
            return Err("original thread creation mount identity changed".into());
        }
        if !self.failed_thread_creation_mount_drained() {
            return Ok(false);
        }
        self.autosave.recovery_adapters()?;
        self.submission.recovery_source()?;
        let selected = service
            .selected_identity()
            .ok_or("original failed service selection is missing")?;
        service.authenticate_failed_thread_creation_selection(
            selected,
            capture.authority.receipt,
            capture.authority.prior,
        )?;
        if !capture.detached_pending {
            if let Some(receipt) = capture.authority.receipt {
                let detached = self.detach_pending_presentation(receipt, cx)?;
                if detached.as_ref() != capture.pending.as_ref() {
                    return Err("original thread creation pending mount custody changed".into());
                }
            }
            capture.detached_pending = true;
        }
        if capture.selected_capture.is_none() {
            capture.selected_capture = capture.selected.update(cx, |selected, cx| {
                selected.capture_failed_thread_creation(&capture.authority, service, cx)
            })?;
            if capture.selected_capture.is_none() {
                return Ok(false);
            }
        }
        if let Some(pending) = capture
            .pending
            .as_ref()
            .filter(|_| !capture.pending_resources_detached)
        {
            if capture.pending_capture.is_none() {
                pending.update(cx, |pending, cx| pending.admit_pending_surface(cx));
                capture.pending_capture = pending.update(cx, |pending, cx| {
                    pending.capture_failed_thread_creation(&capture.authority, service, cx)
                })?;
                if capture.pending_capture.is_none() {
                    return Ok(false);
                }
            }
        }
        if capture.resources.is_none() {
            capture.resources = Some(capture.selected.update(cx, |selected, cx| {
                selected.detach_failed_thread_creation_resources(
                    capture.selected_capture.as_ref().unwrap(),
                    cx,
                )
            })?);
        }
        if let Some(pending) = capture
            .pending
            .as_ref()
            .filter(|_| !capture.pending_resources_detached)
        {
            let resources = pending.update(cx, |pending, cx| {
                pending.detach_failed_thread_creation_resources(
                    capture.pending_capture.as_ref().unwrap(),
                    cx,
                )
            })?;
            if resources
                .service
                .as_ref()
                .is_some_and(|bound| !Arc::ptr_eq(bound, service))
            {
                return Err("original thread creation pending service changed".into());
            }
            drop(resources);
            capture.pending_resources_detached = true;
        }
        self.autosave.detach_recovery_adapters()?;
        self.submission.detach_recovery_source()?;
        self.configurator.take();
        self.native_lineage_recovery.take();
        self.native_lineage_refresh_task.take();
        self.service.take();
        capture.resources.as_mut().unwrap().clipboard_writer.take();
        if let Some(prior) = capture
            .selected_capture
            .as_ref()
            .and_then(|selected| selected.prior.as_ref())
        {
            self.failed_resident = Some(prior.ticket());
        }
        self.failed_resident_detached = true;
        capture.detached = true;
        Ok(true)
    }

    fn failed_thread_creation_mount_drained(&self) -> bool {
        self.window_close_task.is_none()
            && self.window_close_workers.retained() == 0
            && self.autosave.workers_drained()
            && self.submission.workers_drained()
            && self.native_lineage_workers.retained() == 0
            && self.pending_cleanup_workers.retained() == 0
            && self.native_disposal_workers.retained() == 0
            && self.native_lineage_disposal_task.is_none()
            && self.native_lineage_validation_task.is_none()
            && !self.submission.is_active()
            && self.native_lineage_snapshot.is_none()
            && !self.native_lineage_disposal_active
            && self.native_lineage_disposal_flush.is_none()
            && self.native_lineage_config.is_none()
            && self.native_lineage_environment.is_none()
            && self.native_lineage_session.is_none()
            && self.native_lineage_candidate.is_none()
            && self.native_lineage_effects.is_empty()
            && self.native_lineage_cleanup.is_none()
            && self.native_lineage_source.is_none()
            && self.native_lineage_host_result.is_none()
    }

    pub(crate) fn release_failed_thread_creation_widgets(
        &mut self,
        capture: &mut MainWindowFailedThreadCreationMountCapture,
        retired: &mut MainWindowFailedThreadCreationRetirement,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        if !capture.detached {
            return Err("original thread creation widget resources remain attached".into());
        }
        if capture.widgets_released {
            return Ok(());
        }
        let selected_capture = capture.selected_capture.as_ref().unwrap();
        if capture.selected_release_work.is_none() {
            capture.selected_release_work = Some(capture.selected.update(cx, |selected, cx| {
                selected.release_failed_thread_creation_widget(selected_capture, window, cx)
            })?);
        }
        let release = if selected_capture.selection.claim() == capture.authority.prior.claim() {
            retired.accept_predecessor_widget_release(
                selected_capture.selection,
                capture.selected_release_work.as_ref().unwrap(),
            )?
        } else {
            retired.accept_successor_widget_release(
                selected_capture.selection,
                capture.selected_release_work.as_ref().unwrap(),
            )?
        };
        capture.selected.update(cx, |selected, _| {
            selected.accept_failed_thread_creation_widget_release(selected_capture, release)
        })?;
        if let Some(pending) = capture.pending.as_ref() {
            let pending_capture = capture.pending_capture.as_ref().unwrap();
            if capture.pending_release_work.is_none() {
                capture.pending_release_work = Some(pending.update(cx, |pending, cx| {
                    pending.release_failed_thread_creation_widget(pending_capture, window, cx)
                })?);
            }
            let release = retired.accept_successor_widget_release(
                pending_capture.selection,
                capture.pending_release_work.as_ref().unwrap(),
            )?;
            pending.update(cx, |pending, _| {
                pending.accept_failed_thread_creation_widget_release(pending_capture, release)
            })?;
        }
        capture.widgets_released = true;
        Ok(())
    }
}
