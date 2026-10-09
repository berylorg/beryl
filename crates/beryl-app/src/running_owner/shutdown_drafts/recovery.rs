use super::*;
mod adopted_return;
mod claim_widgets;
use std::sync::Arc;

pub(super) struct NoncommittedClaimProvenance {
    window: gpui::WindowHandle<MainWindowShellRoot>,
    selection: crate::main_window::MainWindowComposerSelectionIdentity,
    ticket: crate::main_window::MainWindowFailedResidentTicket,
}

#[cfg(test)]
impl RunningProcessOwner {
    pub(crate) fn test_replace_recovery_drafts(
        &mut self,
        drafts: Option<Rc<RefCell<RunningShutdownDrafts>>>,
    ) -> Option<Rc<RefCell<RunningShutdownDrafts>>> {
        std::mem::replace(&mut self.shutdown.as_mut().unwrap().drafts, drafts)
    }
}

impl RunningShutdownDrafts {
    #[cfg(test)]
    pub(crate) fn test_claim_retirement_diagnostics(&self) -> Vec<String> {
        self.windows
            .iter()
            .map(|(window, draft)| match draft {
                Err(error) => format!("window={window:?};capture_error={error}"),
                Ok(draft) => {
                    let claim = draft.claim_operation.as_ref().map(|claim| claim.test_retirement_state());
                    let failed = draft.failed.as_ref().map(|failed| {
                        let service = failed.resources.as_ref().and_then(|resources| {
                            resources.service.as_ref().or(resources.resident.service.as_ref())
                        }).map(|service| service.test_failed_claim_retirement_diagnostics());
                        format!(
                            "ticket={:?},retired={},capture={},resources={},adoption={},captured_selection={:?},captured_protection={:?},service={service:?}",
                            failed.ticket,
                            failed.retired,
                            failed.capture.is_some(),
                            failed.resources.is_some(),
                            failed.adoption.is_some(),
                            failed.capture.as_ref().map(|capture| capture.selection()),
                            failed.capture.as_ref().map(|capture| capture.protection())
                        )
                    });
                    format!("window={window:?};claim={claim:?};failed_resident={failed:?}")
                }
            })
            .collect()
    }

    pub(crate) fn has_captured_claim_operation_window(
        &self,
        window: gpui::WindowHandle<MainWindowShellRoot>,
    ) -> Result<bool, String> {
        self.has_captured_claim_operation()?;
        let draft = self
            .windows
            .iter()
            .find(|(captured, _)| *captured == window)
            .ok_or("New Thread original captured window is missing")?
            .1
            .as_ref()
            .map_err(Clone::clone)?;
        Ok(draft.claim_operation.is_some()
            || self
                .noncommitted_claims
                .iter()
                .any(|prior| prior.window == window))
    }

    pub(crate) fn has_captured_claim_operation(&self) -> Result<bool, String> {
        self.require_complete_capture()?;
        for prior in &self.noncommitted_claims {
            let draft = self
                .windows
                .iter()
                .find(|(window, _)| *window == prior.window)
                .ok_or("noncommitted New Thread preserved window changed")?
                .1
                .as_ref()
                .map_err(Clone::clone)?;
            if draft
                .failed
                .as_ref()
                .is_none_or(|failed| failed.ticket != prior.ticket)
                || draft
                    .failed
                    .as_ref()
                    .and_then(|failed| failed.capture.as_ref())
                    .is_some_and(|capture| capture.selection() != prior.selection)
            {
                return Err("noncommitted New Thread original resident identity changed".into());
            }
        }
        Ok(!self.noncommitted_claims.is_empty()
            || self.windows.iter().any(|(_, draft)| {
                draft
                    .as_ref()
                    .is_ok_and(|draft| draft.claim_operation.is_some())
            }))
    }

    pub(crate) fn detach_recovered_claim_target_window(
        &mut self,
        handle: gpui::WindowHandle<MainWindowShellRoot>,
        home: beryl_model::BerylHomeId,
        generation: beryl_home_store::HomeGeneration,
        app: &mut App,
    ) -> Result<Option<bool>, String> {
        if !self.prepared || self.driving || self.releasing || self.released {
            return Err("New Thread candidate cleanup draft set is unavailable".into());
        }
        let draft = self
            .windows
            .iter_mut()
            .find(|(window, _)| *window == handle)
            .ok_or("New Thread candidate cleanup window is missing")?
            .1
            .as_mut()
            .map_err(|error| error.clone())?;
        if draft.claim_operation.is_none() {
            return Ok(None);
        }
        handle
            .update(app, |root, window, cx| {
                root.detach_recovered_claim_target_shell(draft, home, generation, window, cx)
            })
            .map_err(|error| error.to_string())?
            .map(Some)
    }

    pub(crate) fn adopt_recovered_claim_target(
        &mut self,
        root: &mut MainWindowShellRoot,
        preparation: &mut crate::main_window::MainWindowFreshComposerPreparation,
        retirement: &mut crate::main_window::MainWindowFailedClaimRetirement,
        adapters: crate::app_services::recovery_composer::PreparedComposerRecoveryAdapters,
        configure: crate::main_window::MainWindowShellComposerConfigurator,
        transcript: crate::syndic_transcript::PreparedTranscriptActivation,
        window: &mut gpui::Window,
        cx: &mut gpui::Context<MainWindowShellRoot>,
    ) -> Result<crate::main_window::MainWindowConversationComposerCloseTicket, String> {
        if !self.prepared || self.driving || self.releasing || self.released {
            return Err("New Thread recovery draft set is unavailable".into());
        }
        let draft = self
            .windows
            .iter_mut()
            .find(|(handle, _)| gpui::AnyWindowHandle::from(*handle) == window.window_handle())
            .ok_or("New Thread recovery window is missing")?
            .1
            .as_mut()
            .map_err(|error| error.clone())?;
        let close = root.adopt_recovered_claim_target_shell(
            draft,
            preparation,
            retirement,
            adapters,
            configure,
            transcript,
            window,
            cx,
        )?;
        self.ready = false;
        Ok(close)
    }

    pub(crate) fn advance_recovered_claim_target(
        &self,
        root: &mut MainWindowShellRoot,
        home: beryl_model::BerylHomeId,
        generation: beryl_home_store::HomeGeneration,
        window: &mut gpui::Window,
        cx: &mut gpui::Context<MainWindowShellRoot>,
    ) -> Result<bool, String> {
        let draft = self
            .windows
            .iter()
            .find(|(handle, _)| gpui::AnyWindowHandle::from(*handle) == window.window_handle())
            .ok_or("New Thread recovery window is missing")?
            .1
            .as_ref()
            .map_err(Clone::clone)?;
        root.advance_first_conversation_shell(draft, home, generation, window, cx)
    }
    pub(crate) fn adopt_first_conversation(
        &mut self,
        root: &mut MainWindowShellRoot,
        facts: &crate::runtime_admission::recovery::FirstConversationFacts,
        preparation: &mut crate::main_window::MainWindowFreshComposerPreparation,
        adapters: crate::app_services::recovery_composer::PreparedComposerRecoveryAdapters,
        configure: crate::main_window::MainWindowShellComposerConfigurator,
        transcript: crate::syndic_transcript::PreparedTranscriptActivation,
        window: &mut gpui::Window,
        cx: &mut gpui::Context<MainWindowShellRoot>,
    ) -> Result<(), String> {
        if !self.prepared || self.driving || self.releasing || self.released {
            return Err("first conversation draft set is unavailable".into());
        }
        let draft = self
            .windows
            .iter_mut()
            .find(|(handle, _)| gpui::AnyWindowHandle::from(*handle) == window.window_handle())
            .ok_or("first conversation window is not preserved")?
            .1
            .as_mut()
            .map_err(|e| e.clone())?;
        root.adopt_first_conversation_shell(
            draft,
            facts,
            preparation,
            adapters,
            configure,
            transcript,
            window,
            cx,
        )?;
        self.ready = false;
        Ok(())
    }

    pub(crate) fn advance_first_conversation(
        &self,
        root: &mut MainWindowShellRoot,
        home: beryl_model::BerylHomeId,
        generation: beryl_home_store::HomeGeneration,
        window: &mut gpui::Window,
        cx: &mut gpui::Context<MainWindowShellRoot>,
    ) -> Result<bool, String> {
        if !self.prepared || self.driving || self.releasing || self.released {
            return Err("first conversation preparation draft set is unavailable".into());
        }
        let draft = self
            .windows
            .iter()
            .find(|(handle, _)| gpui::AnyWindowHandle::from(*handle) == window.window_handle())
            .ok_or("first conversation preparation window is not preserved")?
            .1
            .as_ref()
            .map_err(|e| e.clone())?;
        root.advance_first_conversation_shell(draft, home, generation, window, cx)
    }

    pub(crate) fn detach_first_conversation(
        &mut self,
        home: beryl_model::BerylHomeId,
        generation: beryl_home_store::HomeGeneration,
        app: &mut App,
    ) -> Result<bool, String> {
        let mut done = true;
        for (window, draft) in &mut self.windows {
            let draft = draft.as_mut().map_err(|e| e.clone())?;
            done &= window
                .update(app, |root, native, cx| {
                    root.detach_first_conversation_shell(draft, home, generation, native, cx)
                })
                .map_err(|e| e.to_string())??;
        }
        self.ready = false;
        Ok(done)
    }
    pub(crate) fn adopt_failed_recovered_shell<C: Send + 'static>(
        &mut self,
        root: &mut MainWindowShellRoot,
        resident: gpui::EntityId,
        close: crate::main_window::MainWindowConversationComposerCloseTicket,
        preparation: &mut crate::main_window::MainWindowFailedResidentPreparation<C>,
        adapters: &mut Option<
            crate::app_services::recovery_composer::PreparedComposerRecoveryAdapters,
        >,
        configurator: &mut Option<crate::main_window::MainWindowConversationComposerConfigurator>,
        current: gpui_text_input::RangePrepublicationCurrent,
        window: &mut gpui::Window,
        cx: &mut gpui::Context<MainWindowShellRoot>,
    ) -> Result<
        (
            C,
            crate::main_window::MainWindowConversationComposerCloseTicket,
        ),
        String,
    > {
        if !self.prepared || self.driving || self.releasing || self.released {
            return Err("failed recovery draft set is unavailable".into());
        }
        let draft = self
            .windows
            .iter_mut()
            .find(|(handle, _)| gpui::AnyWindowHandle::from(*handle) == window.window_handle())
            .ok_or("failed recovery window is missing")?
            .1
            .as_mut()
            .map_err(|e| e.clone())?;
        if draft.recovery_resident_identity() != Some((resident, close)) {
            return Err("failed recovery resident differs from captured draft".into());
        }
        let adopted = root.adopt_failed_interrupted_exit_shell(
            draft,
            preparation,
            adapters,
            configurator,
            current,
            window,
            cx,
        )?;
        self.ready = false;
        Ok(adopted)
    }

    pub(crate) fn release_recovered_mounts(
        &self,
        published: &crate::main_window::PublishedMainWindowRestoreSet,
        appearance: &gpui::Entity<crate::theme_runtime::GpuiAppearanceWindowSet>,
        app: &mut App,
    ) -> Result<bool, String> {
        self.release_recovered_mounts_after(published, appearance, app, || Ok(()))
    }

    pub(crate) fn release_recovered_mounts_after(
        &self,
        published: &crate::main_window::PublishedMainWindowRestoreSet,
        appearance: &gpui::Entity<crate::theme_runtime::GpuiAppearanceWindowSet>,
        app: &mut App,
        settle: impl FnOnce() -> Result<(), String>,
    ) -> Result<bool, String> {
        if !self.release_recovered_drafts(published, appearance, app)? {
            return Ok(false);
        }
        let target = appearance.read(app).target();
        self.release_prepared_recovered_mounts_after(&target, app, settle)
    }

    fn release_prepared_recovered_mounts_after(
        &self,
        target: &Arc<crate::theme_runtime::GpuiAppearancePublicationTarget>,
        app: &mut App,
        settle: impl FnOnce() -> Result<(), String>,
    ) -> Result<bool, String> {
        for (window, draft) in &self.windows {
            let draft = draft.as_ref().map_err(Clone::clone)?;
            if !window
                .update(app, |root, _, cx| {
                    root.prepare_interrupted_exit_mount(draft, target, cx)
                })
                .map_err(|error| error.to_string())??
            {
                return Ok(false);
            }
        }
        // Settlement cannot reenter GUI state or invalidate the prepared bindings.
        settle()?;
        for (window, draft) in &self.windows {
            let draft = draft.as_ref().expect("validated recovered draft");
            let released = window
                .update(app, |root, _, cx| {
                    root.release_interrupted_exit_mount(draft, target, cx)
                })
                .expect("prepared recovery window remains live during synchronous release")
                .expect("prepared recovery binding remains current during synchronous release");
            assert!(
                released,
                "prepared recovered mount remains ready during release"
            );
        }
        Ok(true)
    }

    #[cfg(test)]
    pub(crate) fn test_release_prepared_recovered_mounts_after(
        &self,
        target: &Arc<crate::theme_runtime::GpuiAppearancePublicationTarget>,
        app: &mut App,
        settle: impl FnOnce() -> Result<(), String>,
    ) -> Result<bool, String> {
        self.release_prepared_recovered_mounts_after(target, app, settle)
    }

    pub(in crate::running_owner) fn release_recovered_drafts(
        &self,
        published: &crate::main_window::PublishedMainWindowRestoreSet,
        appearance: &gpui::Entity<crate::theme_runtime::GpuiAppearanceWindowSet>,
        app: &mut App,
    ) -> Result<bool, String> {
        self.validate_recovered_bindings(published, appearance, app)?;
        let target = appearance.read(app).target();
        let mut released = true;
        for (window, draft) in &self.windows {
            let draft = draft.as_ref().map_err(Clone::clone)?;
            released &= window
                .update(app, |root, _, cx| {
                    root.release_interrupted_exit_draft(draft, &target, cx)
                })
                .map_err(|error| error.to_string())??;
        }
        Ok(released)
    }

    pub(in crate::running_owner) fn validate_recovered_bindings(
        &self,
        published: &crate::main_window::PublishedMainWindowRestoreSet,
        appearance: &gpui::Entity<crate::theme_runtime::GpuiAppearanceWindowSet>,
        app: &mut App,
    ) -> Result<(), String> {
        if !self.prepared
            || self.driving
            || self.releasing
            || self.released
            || self.windows.is_empty()
            || self.windows.len() != published.shells().len()
        {
            return Err("Recovery draft set is incomplete or busy".into());
        }
        for shell in published.shells() {
            let mut entries = self
                .windows
                .iter()
                .filter(|(window, _)| *window == shell.window());
            let (_, draft) = entries.next().ok_or("Recovery draft is missing")?;
            if entries.next().is_some() {
                return Err("Recovery draft is duplicated".into());
            }
            shell.validate_interrupted_exit_binding(
                draft.as_ref().map_err(Clone::clone)?,
                appearance,
                app,
            )?;
        }
        Ok(())
    }

    pub(in crate::running_owner) fn require_recovery_window(
        &self,
        window: WindowHandle<MainWindowShellRoot>,
    ) -> Result<(), String> {
        if !self.prepared || self.driving || self.releasing || self.released {
            return Err("Interrupted Exit draft set is not available for binding".into());
        }
        self.windows
            .iter()
            .find(|(handle, _)| *handle == window)
            .ok_or("Recovery window is absent from retained drafts")?
            .1
            .as_ref()
            .map(|_| ())
            .map_err(Clone::clone)
    }

    pub(crate) fn adopt_recovered_threadless_shell(
        &mut self,
        root: &mut MainWindowShellRoot,
        source: &mut Option<crate::app_services::recovery_threadless::ThreadlessRecoveryWindow>,
        window: &gpui::Window,
        cx: &mut gpui::Context<MainWindowShellRoot>,
    ) -> Result<(), String> {
        if !self.prepared || self.driving || self.releasing || self.released {
            return Err("Interrupted Exit draft set is not available for adoption".into());
        }
        let (_, draft) = self
            .windows
            .iter()
            .find(|(handle, _)| gpui::AnyWindowHandle::from(*handle) == window.window_handle())
            .ok_or("Recovery window is absent from retained drafts")?;
        let draft = draft.as_ref().map_err(|error| error.clone())?;
        root.adopt_interrupted_exit_threadless_shell(draft, source, cx)?;
        self.ready = false;
        Ok(())
    }

    pub(crate) fn adopt_recovered_shell<C: Send + 'static>(
        &mut self,
        root: &mut MainWindowShellRoot,
        resident: gpui::EntityId,
        close: crate::main_window::MainWindowConversationComposerCloseTicket,
        preparation: &mut crate::main_window::MainWindowComposerRecoveryPreparation<C>,
        adapters: &mut Option<
            crate::app_services::recovery_composer::PreparedComposerRecoveryAdapters,
        >,
        configurator: &mut Option<crate::main_window::MainWindowConversationComposerConfigurator>,
        current: gpui_text_input::RangePrepublicationCurrent,
        window: &mut gpui::Window,
        cx: &mut gpui::Context<MainWindowShellRoot>,
    ) -> Result<
        (
            C,
            crate::main_window::MainWindowConversationComposerCloseTicket,
        ),
        String,
    > {
        if !self.prepared || self.driving || self.releasing || self.released {
            return Err("Interrupted Exit draft set is not available for adoption".into());
        }
        let (_, draft) = self
            .windows
            .iter_mut()
            .find(|(handle, _)| gpui::AnyWindowHandle::from(*handle) == window.window_handle())
            .ok_or("Recovery window is absent from retained drafts")?;
        let draft = draft.as_mut().map_err(|error| error.clone())?;
        if draft.recovery_resident_identity() != Some((resident, close)) {
            return Err("Recovery resident differs from retained draft".into());
        }
        let adopted = root.adopt_interrupted_exit_shell(
            draft,
            preparation,
            adapters,
            configurator,
            current,
            window,
            cx,
        )?;
        self.ready = false;
        Ok(adopted)
    }

    #[cfg(test)]
    pub(crate) fn test_recovery_drafts(
        window: WindowHandle<MainWindowShellRoot>,
        draft: MainWindowShutdownDraft,
    ) -> Self {
        Self {
            windows: vec![(window, Ok(draft))],
            noncommitted_claims: Vec::new(),
            driving: false,
            prepared: true,
            releasing: false,
            released: false,
            ready: true,
            detached_preparing: false,
            detached_prepared: false,
            #[cfg(test)]
            last_poll: None,
        }
    }

    #[cfg(test)]
    pub(crate) fn test_recovery_driving(&mut self, driving: bool) {
        self.driving = driving;
    }

    #[cfg(test)]
    pub(crate) fn test_recovery_ready(&self) -> bool {
        self.ready()
    }

    pub(in crate::running_owner) fn retire_residents(
        &mut self,
        app: &mut App,
    ) -> Result<bool, String> {
        if !self.ready() || self.released {
            return Err("Interrupted Exit draft set is not ready".into());
        }
        let mut ready = true;
        let mut failure = None;
        for (window, draft) in &mut self.windows {
            let result = match draft {
                Ok(draft) => window
                    .update(app, |root, _, cx| root.retire_shutdown_draft(draft, cx))
                    .map_err(|error| format!("Interrupted Exit window is unavailable: {error}"))
                    .and_then(|result| result),
                Err(error) => Err(error.clone()),
            };
            match result {
                Ok(retired) => ready &= retired,
                Err(error) => {
                    failure.get_or_insert(error);
                }
            }
        }
        match failure {
            Some(error) => Err(error),
            None => Ok(ready),
        }
    }

    pub(in crate::running_owner) fn retire_failed_residents(
        &mut self,
        services: &mut crate::app_services::ProcessServiceOwner,
        app: &mut App,
    ) -> Result<bool, String> {
        if !self.prepared || self.driving || self.releasing || self.released {
            return Err("failed resident capture is unavailable".into());
        }
        let mut ready = true;
        for (window, draft) in &mut self.windows {
            let draft = draft.as_mut().map_err(|e| e.clone())?;
            let retirement = window
                .update(app, |root, window, cx| {
                    root.retire_failed_shutdown_draft(draft, services, window, cx)
                })
                .map_err(|e| e.to_string())
                .and_then(|result| result);
            #[cfg(test)]
            if let Some(claim) = draft.claim_operation.as_mut() {
                claim.retirement_return_error = retirement.as_ref().err().cloned();
                claim.retirement_returned = retirement.as_ref().ok().copied();
            }
            ready &= retirement?;
        }
        #[cfg(test)]
        for (_, draft) in &mut self.windows {
            if let Ok(draft) = draft
                && let Some(claim) = draft.claim_operation.as_mut()
            {
                claim.retirement_set_ready = Some(ready);
            }
        }
        Ok(ready)
    }

    pub(in crate::running_owner) fn take_failed_capture(
        &mut self,
        window: WindowHandle<MainWindowShellRoot>,
    ) -> Result<Option<crate::main_window::MainWindowFailedResidentCapture>, String> {
        let draft = self
            .windows
            .iter_mut()
            .find(|(handle, _)| *handle == window)
            .ok_or("failed recovery window is unavailable")?
            .1
            .as_mut()
            .map_err(|e| e.clone())?;
        if let Some((selection, ticket)) = draft.adopt_noncommitted_claim_capture()? {
            self.noncommitted_claims.push(NoncommittedClaimProvenance {
                window,
                selection,
                ticket,
            });
        }
        Ok(draft
            .failed
            .as_mut()
            .map(|failed| {
                failed
                    .capture
                    .take()
                    .ok_or("failed resident capture is already owned")
            })
            .transpose()?)
    }

    pub(in crate::running_owner) fn has_failed_residents(&self) -> bool {
        self.windows.iter().any(|(_, draft)| {
            draft
                .as_ref()
                .is_ok_and(|draft| draft.failed.is_some() || draft.claim_operation.is_some())
        })
    }
    pub(in crate::running_owner) fn return_failed_capture(
        &mut self,
        window: WindowHandle<MainWindowShellRoot>,
        capture: crate::main_window::MainWindowFailedResidentCapture,
    ) {
        let draft = self
            .windows
            .iter_mut()
            .find(|(handle, _)| *handle == window)
            .unwrap()
            .1
            .as_mut()
            .unwrap();
        let failed = draft.failed.as_mut().unwrap();
        assert!(failed.capture.is_none() && capture.ticket() == failed.ticket);
        failed.capture = Some(capture);
    }
}
