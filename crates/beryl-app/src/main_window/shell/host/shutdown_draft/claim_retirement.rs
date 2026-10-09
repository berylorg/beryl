use super::super::running_threads::{CapturedClaimOperation, RetiringClaimOperation};
use super::*;
use crate::main_window::{MainWindowFailedClaimMountCapture, MainWindowFailedClaimRetirement};
mod adopted_cleanup;

pub(crate) struct FailedShutdownClaim {
    original: Option<CapturedClaimOperation>,
    retiring: Option<RetiringClaimOperation>,
    pub(crate) capture: Option<MainWindowFailedClaimMountCapture>,
    retired_host: Option<Box<MainWindowFailedClaimRetirement>>,
    pub(crate) retired: bool,
    pub(crate) attachment: Option<RetainedClaimMount>,
    #[cfg(test)]
    pub(crate) retirement_diagnostic: String,
    #[cfg(test)]
    pub(crate) retirement_return_error: Option<String>,
    #[cfg(test)]
    pub(crate) retirement_returned: Option<bool>,
    #[cfg(test)]
    pub(crate) retirement_set_ready: Option<bool>,
}

#[cfg(test)]
impl FailedShutdownClaim {
    pub(crate) fn test_retirement_state(&self) -> String {
        format!(
            "retired={},original={},retiring={},capture={},retired_host={},attachment={},returned={:?},set_ready={:?},error={:?},stage={}",
            self.retired,
            self.original.is_some(),
            self.retiring.is_some(),
            self.capture.is_some(),
            self.retired_host.is_some(),
            self.attachment.is_some(),
            self.retirement_returned,
            self.retirement_set_ready,
            self.retirement_return_error,
            self.retirement_diagnostic
        )
    }
}

pub(crate) struct RetainedClaimMount {
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
    pub(in crate::main_window::shell::host) fn test_original_page_release_evidence(
        &self,
    ) -> Result<
        Vec<(
            crate::main_window::MainWindowComposerSelectionIdentity,
            u64,
            u64,
            usize,
        )>,
        String,
    > {
        let retained = self
            .prepublication_cleanup
            .try_borrow()
            .map_err(|_| "original prepublication cleanup custody is busy")?;
        let mut evidence = retained.as_ref().map_or_else(Vec::new, |capsules| {
            capsules
                .iter()
                .map(|capsule| capsule.test_page_release_evidence())
                .collect()
        });
        if let Some(failed) = &self.failed {
            let current = failed
                .prepublication_cleanup
                .try_borrow()
                .map_err(|_| "adopted prepublication cleanup custody is busy")?;
            evidence.extend(
                current
                    .iter()
                    .flatten()
                    .map(|capsule| capsule.test_page_release_evidence()),
            );
        }
        Ok(evidence)
    }

    #[cfg(test)]
    pub(in crate::main_window::shell::host) fn accepted_prepublication_page_release_acknowledgements(
        &self,
    ) -> Result<usize, String> {
        let retained = self
            .prepublication_cleanup
            .try_borrow()
            .map_err(|_| "original prepublication cleanup custody is busy")?;
        let mut accepted = retained.as_ref().map_or(0, |capsules| {
            capsules
                .iter()
                .map(|capsule| capsule.accepted_page_release_acknowledgements())
                .sum()
        });
        if let Some(failed) = &self.failed {
            let current = failed
                .prepublication_cleanup
                .try_borrow()
                .map_err(|_| "adopted prepublication cleanup custody is busy")?;
            accepted += current
                .iter()
                .flatten()
                .map(|capsule| capsule.accepted_page_release_acknowledgements())
                .sum::<usize>();
        }
        Ok(accepted)
    }

    pub(in crate::main_window::shell::host) fn advance_original_prepublication_cleanup(
        &self,
        window: beryl_model::WindowId,
    ) -> Result<bool, String> {
        let mut retained = self
            .prepublication_cleanup
            .try_borrow_mut()
            .map_err(|_| "original prepublication cleanup custody is busy")?;
        let mut drained = adopted_cleanup::advance_group(retained.as_mut(), window)?;
        if let Some(failed) = &self.failed {
            let mut current = failed
                .prepublication_cleanup
                .try_borrow_mut()
                .map_err(|_| "adopted prepublication cleanup custody is busy")?;
            drained &= adopted_cleanup::advance_group(current.as_mut(), window)?;
        }
        Ok(drained)
    }

    pub(crate) fn adopt_noncommitted_claim_capture(
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
        if let Some(creation) = self.claim_operation.as_mut() {
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
                    prepublication_cleanup: std::cell::RefCell::new(None),
                    adoption: None,
                    capture: Some(capture),
                    ticket,
                    resources: None,
                    retired: true,
                });
                self.claim_operation.take();
                return Ok(Some((selection, ticket)));
            }
        }
        Ok(None)
    }
}

impl MainWindowShellRoot {
    pub(in crate::main_window::shell::host) fn begin_failed_claim_shutdown_draft(
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
        let close = mount.update(cx, |mount, cx| mount.claim_recovery_close_ticket(cx))?;
        Ok(MainWindowShutdownDraft {
            failed: None,
            prepublication_cleanup: std::cell::RefCell::new(None),
            claim_operation: Some(Box::new(FailedShutdownClaim {
                original: None,
                retiring: None,
                capture: None,
                retired_host: None,
                retired: false,
                attachment: None,
                #[cfg(test)]
                retirement_diagnostic: "awaiting original capture".into(),
                #[cfg(test)]
                retirement_return_error: None,
                #[cfg(test)]
                retirement_returned: None,
                #[cfg(test)]
                retirement_set_ready: None,
            })),
            root: cx.entity_id(),
            retirement: None,
            detached_source: None,
            detached_installed: false,
            composer: Some((mount, editor, close)),
        })
    }

    pub(in crate::main_window::shell::host) fn retire_failed_claim_shutdown_draft(
        &mut self,
        draft: &mut MainWindowShutdownDraft,
        services: &mut crate::app_services::ProcessServiceOwner,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<bool, String> {
        let failed = draft.claim_operation.as_mut().unwrap();
        if failed.retired {
            return Ok(true);
        }
        #[cfg(test)]
        {
            failed.retirement_diagnostic = format!(
                "original capture entry: composer_present={},original={},retiring={},capture={}",
                draft.composer.is_some(),
                failed.original.is_some(),
                failed.retiring.is_some(),
                failed.capture.is_some()
            );
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
            #[cfg(test)]
            {
                let current_mount = self
                    .controller
                    .as_ref()
                    .and_then(|controller| controller.composer_mount.as_ref());
                let current_editor = mount
                    .read(cx)
                    .contribution()
                    .map(|resident| resident.entity_id());
                failed.retirement_diagnostic = format!(
                    "original capture initial identity refused: controller_present={},mount_matches={},editor_matches={},expected_mount={:?},current_mount={:?},expected_editor={:?},current_editor={:?}",
                    self.controller.is_some(),
                    current_mount == Some(mount),
                    current_editor == Some(*editor),
                    mount.entity_id(),
                    current_mount.map(|mount| mount.entity_id()),
                    editor,
                    current_editor
                );
            }
            return Err("original creation window or editor identity changed".into());
        }
        if failed.original.is_none() && failed.retiring.is_none() {
            let captured = if self.has_failed_thread_creation_entrance() {
                self.capture_failed_thread_creation_operation()
            } else {
                self.capture_failed_ordinary_selection_operation()
            };
            match captured {
                Ok(Some(original)) => failed.original = Some(original),
                Ok(None) => {
                    #[cfg(test)]
                    {
                        failed.retirement_diagnostic =
                            "original capture entrance refused: no matching original operation"
                                .into();
                    }
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
            #[cfg(test)]
            {
                failed.retirement_diagnostic = "original transcript retirement admission".into();
            }
            if !original.stop_transcript_reads()? {
                #[cfg(test)]
                {
                    failed.retirement_diagnostic = "original transcript reads draining".into();
                }
                return Ok(false);
            }
            if original.mount() != *mount {
                #[cfg(test)]
                {
                    failed.retirement_diagnostic = format!(
                        "original capture operation mount refused: expected={:?},operation={:?}",
                        mount.entity_id(),
                        original.mount().entity_id()
                    );
                }
                return Err("original creation mount custody changed".into());
            }
            #[cfg(test)]
            {
                failed.retirement_diagnostic = "original capture service admission".into();
            }
            let service = original.service()?;
            if failed.capture.is_none() {
                #[cfg(test)]
                {
                    failed.retirement_diagnostic = "original capture authority admission".into();
                }
                let authority = original.authority()?;
                #[cfg(test)]
                {
                    failed.retirement_diagnostic = "original GUI capture admission".into();
                }
                let capture = mount.update(cx, |mount, cx| {
                    mount.begin_failed_claim_capture(authority, &service, cx)
                });
                #[cfg(test)]
                if let Err(error) = &capture {
                    failed.retirement_diagnostic = format!("original GUI capture refused: {error}");
                }
                failed.capture = Some(capture?);
            }
            #[cfg(test)]
            {
                failed.retirement_diagnostic = "original GUI capture advancing".into();
            }
            let advanced = mount.update(cx, |mount, cx| {
                mount.advance_failed_claim_capture(
                    failed.capture.as_mut().unwrap(),
                    &service,
                    window,
                    cx,
                )
            });
            #[cfg(test)]
            if let Err(error) = &advanced {
                failed.retirement_diagnostic =
                    format!("original GUI capture advance refused: {error}");
            }
            if !advanced? {
                #[cfg(test)]
                {
                    failed.retirement_diagnostic = format!(
                        "original GUI capture draining; {}; {}",
                        service.test_failed_claim_retirement_diagnostics(),
                        mount.read(cx).test_failed_claim_capture_diagnostics(
                            failed.capture.as_ref().unwrap(),
                            cx
                        )
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
            if failed.original.as_ref().unwrap().is_ordinary_selection() {
                mount.update(cx, |mount, cx| {
                    mount.release_unpublished_claim_target(
                        failed.capture.as_mut().unwrap(),
                        window,
                        cx,
                    )
                })?;
            }
            if failed
                .original
                .as_ref()
                .unwrap()
                .original_source_drain()?
                .is_pending()
            {
                #[cfg(test)]
                {
                    failed.retirement_diagnostic = format!(
                        "original GUI source owners draining: {:?}",
                        failed
                            .original
                            .as_ref()
                            .unwrap()
                            .original_source_owner_counts()
                    );
                }
                return Ok(false);
            }
            let original = failed.original.take().unwrap();
            let mounted_successor = failed.capture.as_ref().unwrap().mounted_successor(cx);
            let successor_release = failed
                .capture
                .as_ref()
                .unwrap()
                .unpublished_target_release();
            match original.into_retirement_parts(mounted_successor, successor_release) {
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
            match service.take_failed_claim_prepublication_cleanup(source) {
                Ok(capsules) => *draft.prepublication_cleanup.get_mut() = Some(capsules),
                Err(error) => {
                    #[cfg(test)]
                    {
                        failed.retirement_diagnostic = format!(
                            "original prepublication transfer: {error}; {}",
                            service.test_failed_claim_retirement_diagnostics()
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
            match service.retire_failed_claim_cleanup(source, markers) {
                Ok(retired) => failed.retired_host = Some(retired),
                Err((service, source, error)) => {
                    #[cfg(test)]
                    {
                        failed.retirement_diagnostic = format!(
                            "original service conversion: {error}; {}",
                            service.test_failed_claim_retirement_diagnostics()
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
        match services.retain_failed_claim(retiring.operation, retirement, seed) {
            Ok(()) => {}
            Err((operation, retirement)) => {
                failed.retiring = Some(RetiringClaimOperation {
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
