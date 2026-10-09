use super::*;
use std::sync::Arc;
use syndic_storage::DetachedDraftReadSourceV1;

impl MainWindowShutdownDraft {
    pub(crate) fn advance_destroyed_final_cleanup(
        &self,
        window: beryl_model::WindowId,
    ) -> Result<bool, String> {
        if !self.detached_installed
            || self.composer.is_some()
            || self.retirement.as_ref().is_some_and(|retirement| {
                !matches!(retirement, recovery::ResidentRetirement::Retired(retired)
                    if retired.selection().window_id() == window)
            })
        {
            return Err("final cleanup still retains its resident or foreign retirement".into());
        }
        self.advance_original_prepublication_cleanup(window)
    }
}

#[cfg(test)]
impl MainWindowShutdownDraft {
    pub(crate) fn test_final_retirement_diagnostics(&self) -> String {
        let (retirement, service) = match &self.retirement {
            Some(recovery::ResidentRetirement::Detached(resources)) => (
                "Detached",
                resources
                    .service
                    .as_ref()
                    .or(resources.resident.service.as_ref()),
            ),
            Some(recovery::ResidentRetirement::Retired(_)) => ("Retired", None),
            None => ("None", None),
        };
        format!(
            "detached_installed={},retirement={},service={:?}",
            self.detached_installed,
            retirement,
            service.map(|service| service.test_failed_claim_retirement_diagnostics())
        )
    }
}

impl MainWindowShellRoot {
    pub(crate) fn detached_export_service(
        &mut self,
        draft: &MainWindowShutdownDraft,
        cx: &mut Context<Self>,
    ) -> Result<
        Option<(
            Arc<crate::main_window::MainWindowConversationComposerService>,
            MainWindowConversationComposerCloseTicket,
            crate::main_window::MainWindowComposerSelectionIdentity,
        )>,
        String,
    > {
        let mount = self.shutdown_draft_mount(cx)?;
        match (mount, &draft.composer) {
            (None, None) if draft.root == cx.entity_id() => Ok(None),
            (Some(current), Some((mount, editor, close)))
                if draft.root == cx.entity_id()
                    && current == *mount
                    && mount
                        .read(cx)
                        .contribution()
                        .is_some_and(|resident| resident.entity_id() == *editor) =>
            {
                let selection = mount
                    .read(cx)
                    .contribution()
                    .unwrap()
                    .read(cx)
                    .selection_identity();
                let service =
                    mount.update(cx, |mount, cx| mount.detached_export_service(*close, cx))?;
                Ok(Some((service, *close, selection)))
            }
            _ => Err("detached export resident custody changed".into()),
        }
    }

    pub(crate) fn set_detached_shutdown_source(
        &mut self,
        draft: &mut MainWindowShutdownDraft,
        source: Option<DetachedDraftReadSourceV1>,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let work = self.detached_export_service(draft, cx)?;
        match (work, source.as_ref()) {
            (None, None) => {}
            (Some((_, _, selection)), Some(source))
                if source.binding() == selection.binding().candidate()
                    && source.root() == selection.binding().root() => {}
            _ => return Err("detached source does not match its shutdown resident".into()),
        }
        draft.detached_source = source;
        Ok(())
    }

    pub(crate) fn validate_detached_shutdown_install(
        &mut self,
        draft: &MainWindowShutdownDraft,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let work = self.detached_export_service(draft, cx)?;
        let Some((service, close, _)) = work else {
            return if draft.detached_source.is_none() {
                Ok(())
            } else {
                Err("threadless detached source changed".into())
            };
        };
        let source = draft
            .detached_source
            .as_ref()
            .ok_or("shutdown resident has no detached source")?;
        draft.composer.as_ref().unwrap().0.update(cx, |mount, cx| {
            mount.validate_detached_install(close, source, cx)
        })?;
        match &self.controller.as_ref().unwrap().content {
            ShellContent::Acquired { custody, .. } => custody
                .initial_composer
                .as_ref()
                .ok_or("initial composer custody missing")?
                .validate_service_retirement(&service),
            ShellContent::Restored { custody, .. } => {
                custody.composer.validate_service_retirement(&service)
            }
            ShellContent::Selected { .. } => Ok(()),
            _ => Err("selected shutdown construction custody changed".into()),
        }
    }

    pub(crate) fn install_detached_shutdown_source(
        &mut self,
        draft: &mut MainWindowShutdownDraft,
        cx: &mut Context<Self>,
    ) {
        if let Some((mount, _, close)) = &draft.composer {
            let source = draft
                .detached_source
                .take()
                .expect("preflighted detached shutdown source");
            let resources = mount.update(cx, |mount, cx| {
                mount.install_detached_source(*close, source, cx)
            });
            draft.retirement = Some(recovery::ResidentRetirement::Detached(resources));
        }
        draft.detached_installed = true;
    }

    pub(crate) fn retire_final_shutdown_draft(
        &mut self,
        draft: &mut MainWindowShutdownDraft,
        cx: &mut Context<Self>,
    ) -> Result<bool, String> {
        if draft.root != cx.entity_id()
            || !draft.detached_installed
            || !(self.shutdown_interaction_gated || self.ordinary_close_interaction_gated)
        {
            return Err("final resident retirement lost its installed source".into());
        }
        let controller = self
            .controller
            .as_mut()
            .ok_or("final shell controller missing")?;
        if let Some(recovery::ResidentRetirement::Detached(resources)) = draft.retirement.as_ref() {
            let (mount, editor, close) = draft
                .composer
                .as_ref()
                .ok_or("final cleanup composer custody missing")?;
            if controller.window_id() != close.selection().window_id()
                || controller.composer_mount.as_ref() != Some(mount)
                || !mount
                    .read(cx)
                    .contribution()
                    .is_some_and(|resident| resident.entity_id() == *editor)
            {
                return Err("final cleanup resident identity changed".into());
            }
            if self.shutdown_interaction_gated {
                let mut retained = draft
                    .prepublication_cleanup
                    .try_borrow_mut()
                    .map_err(|_| "final cleanup capsule custody is busy")?;
                if retained.is_none() {
                    let capsules = mount.update(cx, |mount, cx| {
                        mount.take_final_prepublication_cleanup(*close, resources, cx)
                    })?;
                    let Some(capsules) = capsules else {
                        return Ok(false);
                    };
                    *retained = Some(capsules);
                }
                if !mount.update(cx, |mount, cx| {
                    mount.stop_final_prepublication_cleanup(
                        *close,
                        resources,
                        retained.as_ref().unwrap(),
                        cx,
                    )
                })? {
                    return Ok(false);
                }
            }
            let service = resources
                .service
                .as_ref()
                .or(resources.resident.service.as_ref())
                .ok_or("final service custody missing")?;
            match &mut controller.content {
                ShellContent::Acquired { custody, .. } => custody
                    .initial_composer
                    .as_mut()
                    .unwrap()
                    .release_recovery_service(service)?,
                ShellContent::Restored { custody, .. } => {
                    custody.composer.release_recovery_service(service)?
                }
                ShellContent::Selected { .. } => {}
                _ => return Err("final construction custody changed".into()),
            }
            let Some(recovery::ResidentRetirement::Detached(resources)) = draft.retirement.take()
            else {
                unreachable!()
            };
            match resources.retire() {
                Ok(retired) => {
                    draft.retirement = Some(recovery::ResidentRetirement::Retired(retired));
                }
                Err(resources) => {
                    draft.retirement = Some(recovery::ResidentRetirement::Detached(resources));
                    return Ok(false);
                }
            }
        }
        controller.retire_construction()?;
        Ok(true)
    }

    pub(in crate::main_window::shell::host) fn release_destroyed_final_shutdown_resident(
        &mut self,
        draft: &mut MainWindowShutdownDraft,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        if draft.root != cx.entity_id()
            || !draft.detached_installed
            || !self.shutdown_interaction_gated
        {
            return Err("destroyed final resident lost its original shutdown source".into());
        }
        let controller = self
            .controller
            .as_mut()
            .ok_or("destroyed final controller missing")?;
        if !matches!(controller.content, ShellContent::Retired { .. }) {
            return Err("destroyed final controller is not retired".into());
        }
        match &draft.composer {
            None if controller.composer_mount.is_none() => return Ok(()),
            Some((mount, editor, close)) => {
                let Some(recovery::ResidentRetirement::Retired(retired)) = &draft.retirement else {
                    return Err(
                        "destroyed final resident still retains live service custody".into(),
                    );
                };
                if controller.window_id() != retired.selection().window_id()
                    || controller.composer_mount.as_ref() != Some(mount)
                    || retired.close_ticket() != *close
                    || !mount.read(cx).contribution().is_some_and(|resident| {
                        resident.entity_id() == *editor
                            && resident
                                .read(cx)
                                .validate_destroyed_detached_source(*close, cx)
                                .is_ok_and(|selection| selection == retired.selection())
                    })
                {
                    return Err("destroyed final resident correspondence changed".into());
                }
            }
            _ => return Err("destroyed final composer custody changed".into()),
        }
        // Both aliases outlive the native window; their release drops original widget custody.
        controller.composer_mount.take();
        draft.composer.take();
        Ok(())
    }

    pub(crate) fn drain_detached_shutdown_reads(&mut self, cx: &mut Context<Self>) -> bool {
        self.running_thread_reads_drained()
            && self.release_suspended_running_thread_sources()
            && self
                .controller
                .as_ref()
                .and_then(|controller| controller.composer_mount.clone())
                .is_none_or(|mount| mount.update(cx, |mount, cx| mount.drain_detached_reads(cx)))
    }

    pub(crate) fn resume_detached_shutdown_reads(
        &mut self,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(mount) = self
            .controller
            .as_ref()
            .and_then(|controller| controller.composer_mount.clone())
        {
            mount.update(cx, |mount, cx| mount.resume_detached_reads(window, cx));
        }
    }
}
