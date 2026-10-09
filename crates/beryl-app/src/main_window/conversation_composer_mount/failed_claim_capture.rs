use super::*;
use crate::main_window::{
    MainWindowFailedClaimCapture, MainWindowFailedClaimGuiAuthority,
    MainWindowFailedClaimResources, MainWindowFailedClaimRetirement,
};

pub(crate) struct MainWindowFailedClaimMountCapture {
    pub(crate) authority: MainWindowFailedClaimGuiAuthority,
    pub(crate) selected: Entity<MainWindowConversationComposer>,
    pub(crate) selected_capture: Option<MainWindowFailedClaimCapture>,
    pending: Option<Entity<MainWindowConversationComposer>>,
    pending_capture: Option<MainWindowFailedClaimCapture>,
    pub(crate) resources: Option<MainWindowFailedClaimResources>,
    selected_release_work: Option<Vec<gpui_text_input::RangeTextInputRequest>>,
    pending_release_work: Option<Vec<gpui_text_input::RangeTextInputRequest>>,
    detached_pending: bool,
    pending_resources_detached: bool,
    pub(crate) detached: bool,
    widgets_released: bool,
}

impl MainWindowFailedClaimMountCapture {
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

    pub(crate) fn unpublished_target_release(&self) -> Option<MainWindowComposerWidgetRelease> {
        self.pending_capture
            .as_ref()
            .and_then(|capture| capture.accepted_release())
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
    pub(crate) fn release_unpublished_claim_target(
        &mut self,
        capture: &mut MainWindowFailedClaimMountCapture,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        if !capture.detached {
            return Err("original claim resources remain attached".into());
        }
        let Some(pending) = capture.pending.as_ref() else {
            return Ok(());
        };
        let pending_capture = capture
            .pending_capture
            .as_ref()
            .ok_or("original unpublished target protection is missing")?;
        if capture.selected.read(cx).selection_identity().claim() != capture.authority.prior.claim()
            || Some(pending_capture.selection) != capture.authority.successor
            || capture.authority.receipt.is_none()
        {
            return Err("original unpublished target source changed".into());
        }
        if capture.pending_release_work.is_none() {
            capture.pending_release_work = Some(pending.update(cx, |pending, cx| {
                pending.release_failed_claim_widget(pending_capture, window, cx)
            })?);
        }
        if capture
            .pending_release_work
            .as_ref()
            .unwrap()
            .iter()
            .any(|request| {
                !crate::main_window::MainWindowComposerSlot::widget_release_request_is_settled(
                    request,
                )
            })
        {
            return Err("original unpublished target widget work remains retained".into());
        }
        let release = MainWindowComposerWidgetRelease::new(pending_capture.selection);
        pending.update(cx, |pending, _| {
            pending.accept_failed_claim_widget_release(pending_capture, release)
        })?;
        capture
            .pending_capture
            .as_mut()
            .unwrap()
            .accept_closed_release(release)?;
        Ok(())
    }
    pub(crate) fn gate_failed_claim_cleanup(
        &mut self,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        self.contribution
            .as_ref()
            .ok_or("original failed creation prior editor is missing")?
            .update(cx, |resident, cx| resident.gate_failed_claim_cleanup(cx))?;
        if let Some(pending) = self.pending_presentation.as_ref() {
            pending
                .contribution
                .update(cx, |resident, cx| resident.gate_failed_claim_cleanup(cx))?;
        }
        Ok(())
    }
    pub(crate) fn claim_recovery_close_ticket(
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

    pub(crate) fn begin_failed_claim_capture(
        &mut self,
        authority: MainWindowFailedClaimGuiAuthority,
        service: &Arc<MainWindowConversationComposerService>,
        cx: &mut Context<Self>,
    ) -> Result<MainWindowFailedClaimMountCapture, String> {
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
        Ok(MainWindowFailedClaimMountCapture {
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

    pub(crate) fn advance_failed_claim_capture(
        &mut self,
        capture: &mut MainWindowFailedClaimMountCapture,
        service: &Arc<MainWindowConversationComposerService>,
        window: &mut Window,
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
        if !self.failed_claim_mount_drained() {
            return Ok(false);
        }
        self.autosave.recovery_adapters()?;
        self.submission.recovery_source()?;
        let selected = service
            .selected_identity()
            .ok_or("original failed service selection is missing")?;
        service.authenticate_failed_claim_cleanup_selection(
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
                selected.capture_failed_claim_cleanup(&capture.authority, service, cx)
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
                    pending.capture_failed_claim_cleanup(&capture.authority, service, cx)
                })?;
                if capture.pending_capture.is_none() {
                    return Ok(false);
                }
            }
            if capture.pending_release_work.is_none()
                && capture
                    .pending_capture
                    .as_ref()
                    .unwrap()
                    .is_unpublished_target()
            {
                capture.pending_release_work = Some(pending.update(cx, |pending, cx| {
                    pending.release_failed_claim_widget(
                        capture.pending_capture.as_ref().unwrap(),
                        window,
                        cx,
                    )
                })?);
            }
        }
        if capture.resources.is_none() {
            capture.resources = Some(capture.selected.update(cx, |selected, cx| {
                selected
                    .detach_failed_claim_resources(capture.selected_capture.as_ref().unwrap(), cx)
            })?);
        }
        if let Some(pending) = capture
            .pending
            .as_ref()
            .filter(|_| !capture.pending_resources_detached)
        {
            let resources = pending.update(cx, |pending, cx| {
                pending.detach_failed_claim_resources(capture.pending_capture.as_ref().unwrap(), cx)
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

    fn failed_claim_mount_drained(&self) -> bool {
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

    #[cfg(test)]
    pub(crate) fn test_failed_claim_capture_diagnostics(
        &self,
        capture: &MainWindowFailedClaimMountCapture,
        cx: &App,
    ) -> String {
        format!(
            "mount_drained={},window_close_task={},window_close_workers={},autosave_drained={},submission_drained={},native_workers={},pending_cleanup_workers={},native_disposal_workers={},native_disposal_task={},native_validation_task={},submission_active={},native_snapshot={},native_disposal_active={},native_disposal_flush={},native_config={},native_environment={},native_session={},native_candidate={},native_effects={},native_cleanup={},native_source={},native_host_result={},detached_pending={},selected_capture={},pending_capture={},selected=[{}],pending={:?}",
            self.failed_claim_mount_drained(),
            self.window_close_task.is_some(),
            self.window_close_workers.retained(),
            self.autosave.workers_drained(),
            self.submission.workers_drained(),
            self.native_lineage_workers.retained(),
            self.pending_cleanup_workers.retained(),
            self.native_disposal_workers.retained(),
            self.native_lineage_disposal_task.is_some(),
            self.native_lineage_validation_task.is_some(),
            self.submission.is_active(),
            self.native_lineage_snapshot.is_some(),
            self.native_lineage_disposal_active,
            self.native_lineage_disposal_flush.is_some(),
            self.native_lineage_config.is_some(),
            self.native_lineage_environment.is_some(),
            self.native_lineage_session.is_some(),
            self.native_lineage_candidate.is_some(),
            self.native_lineage_effects.len(),
            self.native_lineage_cleanup.is_some(),
            self.native_lineage_source.is_some(),
            self.native_lineage_host_result.is_some(),
            capture.detached_pending,
            capture.selected_capture.is_some(),
            capture.pending_capture.is_some(),
            capture
                .selected
                .read(cx)
                .test_failed_editor_drain_diagnostics(cx),
            capture
                .pending
                .as_ref()
                .map(|pending| pending.read(cx).test_failed_editor_drain_diagnostics(cx)),
        )
    }

    pub(crate) fn release_failed_claim_widgets(
        &mut self,
        capture: &mut MainWindowFailedClaimMountCapture,
        retired: &mut MainWindowFailedClaimRetirement,
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
                selected.release_failed_claim_widget(selected_capture, window, cx)
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
            selected.accept_failed_claim_widget_release(selected_capture, release)
        })?;
        if let Some(pending) = capture.pending.as_ref() {
            let pending_capture = capture.pending_capture.as_ref().unwrap();
            if capture.pending_release_work.is_none() {
                capture.pending_release_work = Some(pending.update(cx, |pending, cx| {
                    pending.release_failed_claim_widget(pending_capture, window, cx)
                })?);
            }
            let release = retired.accept_successor_widget_release(
                pending_capture.selection,
                capture.pending_release_work.as_ref().unwrap(),
            )?;
            pending.update(cx, |pending, _| {
                pending.accept_failed_claim_widget_release(pending_capture, release)
            })?;
        }
        capture.widgets_released = true;
        Ok(())
    }
}
