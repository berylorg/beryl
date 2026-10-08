use super::super::running_threads::{
    CapturedThreadCreationOperation, RetiringThreadCreationOperation,
};
use super::*;
use crate::main_window::{
    MainWindowFailedThreadCreationMountCapture, MainWindowFailedThreadCreationRetirement,
};

pub(crate) struct FailedShutdownThreadCreation {
    original: Option<CapturedThreadCreationOperation>,
    retiring: Option<RetiringThreadCreationOperation>,
    pub(crate) capture: Option<MainWindowFailedThreadCreationMountCapture>,
    retired_host: Option<Box<MainWindowFailedThreadCreationRetirement>>,
    pub(crate) retired: bool,
    pub(crate) attachment: Option<RetainedThreadCreationMount>,
    #[cfg(test)]
    pub(crate) retirement_diagnostic: String,
}

pub(crate) struct RetainedThreadCreationMount {
    pub(crate) original: (
        Entity<MainWindowConversationComposerMount>,
        gpui::EntityId,
        crate::main_window::MainWindowConversationComposerCloseTicket,
    ),
    pub(crate) fresh: (
        Entity<MainWindowConversationComposerMount>,
        gpui::EntityId,
        crate::main_window::MainWindowConversationComposerCloseTicket,
    ),
    pub(crate) transcript: Entity<crate::syndic_transcript::SyndicTranscriptPanel>,
    pub(crate) transcript_claim: Option<beryl_state::WindowClaimSelection>,
}

impl MainWindowShutdownDraft {
    #[cfg(test)]
    pub(in crate::main_window::shell::host) fn accepted_prepublication_page_release_acknowledgements(
        &self,
    ) -> Result<usize, String> {
        let retained = self
            .prepublication_cleanup
            .try_borrow()
            .map_err(|_| "original prepublication cleanup custody is busy")?;
        Ok(retained.as_ref().map_or(0, |capsules| {
            capsules
                .iter()
                .map(|capsule| capsule.accepted_page_release_acknowledgements())
                .sum()
        }))
    }

    pub(in crate::main_window::shell::host) fn advance_original_prepublication_cleanup(
        &self,
        window: beryl_model::WindowId,
    ) -> Result<bool, String> {
        let mut retained = self
            .prepublication_cleanup
            .try_borrow_mut()
            .map_err(|_| "original prepublication cleanup custody is busy")?;
        let Some(capsules) = retained.as_mut() else {
            return Ok(true);
        };
        if capsules
            .iter()
            .any(|capsule| capsule.selection().window_id() != window)
        {
            return Err("original prepublication cleanup window changed".into());
        }
        let mut drained = true;
        for capsule in capsules {
            drained &= capsule.advance(64)?;
        }
        Ok(drained)
    }

    pub(crate) fn adopt_noncommitted_thread_creation_capture(
        &mut self,
    ) -> Result<
        Option<(
            crate::main_window::MainWindowComposerSelectionIdentity,
            crate::main_window::MainWindowFailedResidentTicket,
        )>,
        String,
    > {
        if self.failed.is_some() {
            return Ok(None);
        }
        if let Some(creation) = self.thread_creation.as_mut() {
            if !creation.retired {
                return Err("original New Thread retirement is not complete".into());
            }
            let prior = creation
                .capture
                .as_mut()
                .and_then(|capture| capture.selected_capture.as_mut())
                .and_then(|capture| capture.prior.take());
            if let Some(capture) = prior {
                let ticket = capture.ticket();
                let selection = capture.selection();
                self.failed = Some(FailedShutdownResident {
                    adoption: None,
                    capture: Some(capture),
                    ticket,
                    resources: None,
                    retired: true,
                });
                self.thread_creation.take();
                return Ok(Some((selection, ticket)));
            }
        }
        Ok(None)
    }
}

impl MainWindowShellRoot {
    pub(in crate::main_window::shell::host) fn begin_failed_thread_creation_shutdown_draft(
        &mut self,
        cx: &mut Context<Self>,
    ) -> Result<MainWindowShutdownDraft, String> {
        let controller = self
            .controller
            .as_ref()
            .ok_or("original creation controller is missing")?;
        let mount = controller
            .composer_mount
            .clone()
            .ok_or("original creation mount is missing")?;
        let editor = mount
            .read(cx)
            .contribution()
            .ok_or("original creation editor is missing")?
            .entity_id();
        let close = mount.update(cx, |mount, cx| {
            mount.thread_creation_recovery_close_ticket(cx)
        })?;
        Ok(MainWindowShutdownDraft {
            failed: None,
            prepublication_cleanup: std::cell::RefCell::new(None),
            thread_creation: Some(Box::new(FailedShutdownThreadCreation {
                original: None,
                retiring: None,
                capture: None,
                retired_host: None,
                retired: false,
                attachment: None,
                #[cfg(test)]
                retirement_diagnostic: "awaiting original capture".into(),
            })),
            root: cx.entity_id(),
            retirement: None,
            detached_source: None,
            detached_installed: false,
            composer: Some((mount, editor, close)),
        })
    }

    pub(in crate::main_window::shell::host) fn retire_failed_thread_creation_shutdown_draft(
        &mut self,
        draft: &mut MainWindowShutdownDraft,
        services: &mut crate::app_services::ProcessServiceOwner,
        cx: &mut Context<Self>,
    ) -> Result<bool, String> {
        let failed = draft.thread_creation.as_mut().unwrap();
        if failed.retired {
            return Ok(true);
        }
        let (mount, editor, _) = draft
            .composer
            .as_ref()
            .ok_or("original creation composer custody is missing")?;
        if self
            .controller
            .as_ref()
            .and_then(|controller| controller.composer_mount.as_ref())
            != Some(mount)
            || mount
                .read(cx)
                .contribution()
                .is_none_or(|resident| resident.entity_id() != *editor)
        {
            return Err("original creation window or editor identity changed".into());
        }
        if failed.original.is_none() && failed.retiring.is_none() {
            match self.capture_failed_thread_creation_operation() {
                Ok(Some(original)) => failed.original = Some(original),
                Ok(None) => {
                    return Err(
                        "original creation entrance is missing from its captured window".into(),
                    );
                }
                Err(error) => {
                    #[cfg(test)]
                    {
                        failed.retirement_diagnostic = format!("original capture: {error}");
                    }
                    let _ = error;
                    return Ok(false);
                }
            }
        }
        if let Some(original) = failed.original.as_ref() {
            if !original.stop_transcript_reads()? {
                #[cfg(test)]
                {
                    failed.retirement_diagnostic = "original transcript reads draining".into();
                }
                return Ok(false);
            }
            if original.mount() != *mount {
                return Err("original creation mount custody changed".into());
            }
            let service = original.service()?;
            if failed.capture.is_none() {
                let authority = original.authority()?;
                failed.capture = Some(mount.update(cx, |mount, cx| {
                    mount.begin_failed_thread_creation_capture(authority, &service, cx)
                })?);
            }
            if !mount.update(cx, |mount, cx| {
                mount.advance_failed_thread_creation_capture(
                    failed.capture.as_mut().unwrap(),
                    &service,
                    cx,
                )
            })? {
                #[cfg(test)]
                {
                    failed.retirement_diagnostic = format!(
                        "original GUI capture draining; {}",
                        service.test_failed_thread_creation_retirement_diagnostics()
                    );
                }
                return Ok(false);
            }
            let controller = self.controller.as_mut().unwrap();
            match &mut controller.content {
                ShellContent::Acquired { custody, .. } => {
                    if let Some(candidate) = custody.initial_composer.as_mut() {
                        candidate.release_recovery_service(&service)?;
                    }
                }
                ShellContent::Restored { custody, .. } => {
                    custody.composer.release_recovery_service(&service)?
                }
                ShellContent::Selected { .. } => {}
                _ => return Err("original creation shell construction custody changed".into()),
            }
            failed
                .capture
                .as_mut()
                .unwrap()
                .resources
                .as_mut()
                .unwrap()
                .service
                .take();
            drop(service);
            let original = failed.original.take().unwrap();
            let mounted_successor = failed.capture.as_ref().unwrap().mounted_successor(cx);
            match original.into_retirement_parts(mounted_successor) {
                Ok(retiring) => failed.retiring = Some(retiring),
                Err((original, error)) => {
                    failed.original = Some(original);
                    return Err(error);
                }
            }
        }
        if !self.running_thread_reads_drained() || !self.release_suspended_running_thread_sources()
        {
            #[cfg(test)]
            {
                failed.retirement_diagnostic =
                    "original Running reads or suspended sources draining".into();
            }
            return Ok(false);
        }
        let retiring = failed.retiring.as_mut().unwrap();
        if draft.prepublication_cleanup.get_mut().is_none() {
            let service = retiring
                .service
                .as_ref()
                .ok_or("original failed creation service custody is missing")?;
            let source = retiring
                .source
                .as_ref()
                .ok_or("original failed creation retirement source is missing")?;
            match service.take_failed_thread_creation_prepublication_cleanup(source) {
                Ok(capsules) => *draft.prepublication_cleanup.get_mut() = Some(capsules),
                Err(error) => {
                    #[cfg(test)]
                    {
                        failed.retirement_diagnostic = format!(
                            "original prepublication transfer: {error}; {}",
                            service.test_failed_thread_creation_retirement_diagnostics()
                        );
                    }
                    let _ = error;
                    return Ok(false);
                }
            }
        }
        retiring.operation.retire_selection_exclusion(
            services
                .graph()
                .ok_or("original failed graph is missing")?
                .home(),
            self.controller.as_ref().unwrap().window_id(),
        )?;
        if failed.retired_host.is_none() {
            let service = retiring
                .service
                .take()
                .ok_or("original failed creation service custody is missing")?;
            let source = retiring
                .source
                .take()
                .ok_or("original failed creation retirement source is missing")?;
            let markers = services
                .failed_marker_custody()
                .ok_or("original failed marker custody is missing")?;
            match service.retire_failed_thread_creation(source, markers) {
                Ok(retired) => failed.retired_host = Some(retired),
                Err((service, source, error)) => {
                    #[cfg(test)]
                    {
                        failed.retirement_diagnostic = format!(
                            "original service conversion: {error}; {}",
                            service.test_failed_thread_creation_retirement_diagnostics()
                        );
                    }
                    let _ = error;
                    retiring.service = Some(service);
                    retiring.source = Some(source);
                    return Ok(false);
                }
            }
        }
        let retiring = failed.retiring.take().unwrap();
        let retirement = failed.retired_host.take().unwrap();
        let seed = failed
            .capture
            .as_ref()
            .unwrap()
            .selected_capture
            .as_ref()
            .and_then(|capture| capture.restoration());
        match services.retain_failed_thread_creation(retiring.operation, retirement, seed) {
            Ok(()) => {}
            Err((operation, retirement)) => {
                failed.retiring = Some(RetiringThreadCreationOperation {
                    operation,
                    service: None,
                    source: None,
                });
                failed.retired_host = Some(retirement);
                return Err("original failed creation retirement is duplicated".into());
            }
        }
        self.controller.as_mut().unwrap().retire_construction()?;
        failed.retired = true;
        Ok(true)
    }
}
