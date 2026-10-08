use super::*;
use crate::main_window::{
    MainWindowComposerMountRecoveryResources, MainWindowComposerRetiredClose,
};

pub(in crate::main_window::shell::host) enum ResidentRetirement {
    Detached(MainWindowComposerMountRecoveryResources),
    Retired(MainWindowComposerRetiredClose),
}

impl MainWindowShellController {
    pub(in crate::main_window::shell::host) fn retire_construction(
        &mut self,
    ) -> Result<(), String> {
        match &self.content {
            ShellContent::Retired { .. } => return Ok(()),
            ShellContent::Acquired { custody, .. } => {
                custody
                    .initial_composer
                    .as_ref()
                    .ok_or("acquired shell lost its construction custody")?
                    .validate_recovery_retirement()?;
            }
            ShellContent::Restored { custody, .. } => {
                custody.composer.validate_recovery_retirement()?;
            }
            ShellContent::Threadless { .. }
            | ShellContent::Selected { .. }
            | ShellContent::RecoveredThreadless { .. } => {}
        }
        let retired = ShellContent::Retired {
            window_id: self.window_id(),
            placement: self.placement().clone(),
            threadless: self.is_threadless(),
            reservation: None,
        };
        let reservation = match std::mem::replace(&mut self.content, retired) {
            ShellContent::Acquired { custody, .. } => custody.reservation,
            ShellContent::Restored { custody, .. } => custody.reservation,
            ShellContent::Threadless { reservation, .. } => reservation,
            ShellContent::Selected { reservation, .. } => reservation,
            ShellContent::RecoveredThreadless { reservation, .. } => reservation,
            ShellContent::Retired { .. } => unreachable!(),
        };
        let ShellContent::Retired {
            reservation: retained,
            ..
        } = &mut self.content
        else {
            unreachable!()
        };
        *retained = Some(reservation);
        Ok(())
    }
}

impl MainWindowShellRoot {
    pub(crate) fn begin_failed_shutdown_draft(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<MainWindowShutdownDraft, String> {
        if !self.shutdown_interaction_gated || self.startup_interaction_gated() {
            return Err("failed shutdown capture requires exact running shell gate".into());
        }
        if !self.retire_setup_first_mount(window, cx)? {
            return Err("original first conversation widget release is still settling".into());
        }
        self.suspend_running_thread_reads(window, cx);
        if self.has_failed_thread_creation_entrance() {
            return self.begin_failed_thread_creation_shutdown_draft(cx);
        }
        if self.running_threads.has_activation_custody() {
            return Err("running thread selection custody prevents failed-home capture".into());
        }
        let controller = self
            .controller
            .as_ref()
            .ok_or("failed shutdown shell controller is unavailable")?;
        let mount = if controller.is_threadless() {
            if controller.composer_mount.is_some() {
                return Err("failed threadless shell has a composer".into());
            }
            None
        } else {
            Some(
                controller
                    .composer_mount
                    .clone()
                    .ok_or("failed selected shell composer is unavailable")?,
            )
        };
        let (composer, failed) = if let Some(mount) = mount {
            let resident = mount
                .read(cx)
                .contribution()
                .ok_or("failed shutdown resident is unavailable")?
                .entity_id();
            let (ticket, close) = mount.update(cx, |mount, cx| {
                let ticket = mount.begin_failed_resident(cx)?;
                Ok::<_, String>((ticket, mount.failed_recovery_close_ticket(cx)?))
            })?;
            (
                Some((mount, resident, close)),
                Some(super::FailedShutdownResident {
                    adoption: None,
                    capture: None,
                    ticket,
                    resources: None,
                    retired: false,
                }),
            )
        } else {
            (None, None)
        };
        Ok(MainWindowShutdownDraft {
            thread_creation: None,
            prepublication_cleanup: std::cell::RefCell::new(None),
            root: cx.entity_id(),
            failed,
            retirement: None,
            detached_source: None,
            detached_installed: false,
            composer,
        })
    }

    pub(crate) fn retire_failed_shutdown_draft(
        &mut self,
        draft: &mut MainWindowShutdownDraft,
        services: &mut crate::app_services::ProcessServiceOwner,
        cx: &mut Context<Self>,
    ) -> Result<bool, String> {
        if draft.root != cx.entity_id()
            || !self.shutdown_interaction_gated
            || self.startup_interaction_gated()
        {
            return Err("failed shutdown draft lost its exact gated shell".into());
        }
        if draft.thread_creation.is_some() {
            return self.retire_failed_thread_creation_shutdown_draft(draft, services, cx);
        }
        if !self.running_thread_reads_drained() || !self.release_suspended_running_thread_sources()
        {
            return Ok(false);
        }
        let Some(failed) = draft.failed.as_mut() else {
            return self.retire_shutdown_draft(draft, cx);
        };
        if failed.retired {
            return Ok(true);
        }
        let (mount, editor, _) = draft
            .composer
            .as_ref()
            .ok_or("failed shutdown composer is unavailable")?;
        let controller = self
            .controller
            .as_mut()
            .ok_or("failed shutdown controller is unavailable")?;
        if controller.composer_mount.as_ref() != Some(mount)
            || !mount
                .read(cx)
                .contribution()
                .is_some_and(|resident| resident.entity_id() == *editor)
        {
            return Err("failed shutdown resident identity changed".into());
        }
        if failed.capture.is_none() {
            failed.capture = mount.update(cx, |mount, cx| {
                mount.capture_failed_resident(failed.ticket, cx)
            })?;
            if failed.capture.is_none() {
                return Ok(false);
            }
        }
        let custody = services
            .failed_marker_custody()
            .ok_or("failed marker custody is unavailable")?;
        if failed.resources.is_none() {
            failed.resources = Some(Box::new(mount.update(cx, |mount, cx| {
                mount.detach_failed_resident_resources_with_marker_custody(
                    failed.capture.as_ref().unwrap(),
                    custody,
                    cx,
                )
            })?));
        }
        let service = failed
            .resources
            .as_ref()
            .unwrap()
            .service
            .as_ref()
            .or(failed.resources.as_ref().unwrap().resident.service.as_ref())
            .ok_or("failed resident service custody is unavailable")?;
        match &mut controller.content {
            ShellContent::Acquired { custody, .. } => {
                if let Some(candidate) = custody.initial_composer.as_mut() {
                    candidate.release_recovery_service(service)?;
                }
            }
            ShellContent::Restored { custody, .. } => {
                custody.composer.release_recovery_service(service)?
            }
            ShellContent::Selected { .. } => {}
            _ => return Err("failed selected construction custody changed".into()),
        }
        let resources = *failed.resources.take().unwrap();
        match resources.retire_with_marker_custody(custody) {
            Err(resources) => {
                failed.resources = Some(Box::new(resources));
                Ok(false)
            }
            Ok(retired) => {
                services
                    .retain_failed_resident(retired, failed.capture.as_ref().unwrap().restoration())
                    .map_err(|_| "failed resident retirement is duplicated")?;
                controller.retire_construction()?;
                failed.retired = true;
                Ok(true)
            }
        }
    }

    pub(crate) fn retire_shutdown_draft(
        &mut self,
        draft: &mut MainWindowShutdownDraft,
        cx: &mut Context<Self>,
    ) -> Result<bool, String> {
        if draft.root != cx.entity_id()
            || !self.shutdown_interaction_gated
            || self.startup_interaction_gated()
        {
            return Err("resident retirement lost its exact gated shell".into());
        }
        if !self.running_thread_reads_drained() || !self.release_suspended_running_thread_sources()
        {
            return Ok(false);
        }
        let controller = self
            .controller
            .as_mut()
            .ok_or("shutdown shell lost its controller")?;
        let Some((mount, editor, close)) = &draft.composer else {
            return if controller.is_threadless() && controller.composer_mount.is_none() {
                controller.retire_construction()?;
                Ok(true)
            } else {
                Err("threadless shutdown draft changed".into())
            };
        };
        if controller.is_threadless()
            || controller.composer_mount.as_ref() != Some(mount)
            || !mount
                .read(cx)
                .contribution()
                .is_some_and(|resident| resident.entity_id() == *editor)
        {
            return Err("resident retirement composer custody changed".into());
        }
        let ready = mount.update(cx, |mount, cx| {
            if !mount.fence_interrupted_exit_resident(*close, cx)? {
                return Ok(false);
            }
            if draft.retirement.is_none() {
                if let Some(resources) = mount.detach_interrupted_exit_resources(*close, cx)? {
                    draft.retirement = Some(ResidentRetirement::Detached(resources));
                } else {
                    return mount.interrupted_exit_retirement_ready(*close, cx);
                }
            }
            if let Some(ResidentRetirement::Detached(resources)) = draft.retirement.as_ref() {
                let service = resources
                    .service
                    .as_ref()
                    .or(resources.resident.service.as_ref())
                    .ok_or("resident retirement lost its service custody")?;
                match &mut controller.content {
                    ShellContent::Acquired { custody, .. } => {
                        if let Some(candidate) = custody.initial_composer.as_mut() {
                            candidate.release_recovery_service(service)?;
                        }
                    }
                    ShellContent::Restored { custody, .. } => {
                        custody.composer.release_recovery_service(service)?;
                    }
                    ShellContent::Selected { .. } => {}
                    ShellContent::Threadless { .. }
                    | ShellContent::RecoveredThreadless { .. }
                    | ShellContent::Retired { .. } => {
                        unreachable!()
                    }
                }
                let Some(ResidentRetirement::Detached(resources)) = draft.retirement.take() else {
                    unreachable!()
                };
                match resources.retire() {
                    Ok(retired) => draft.retirement = Some(ResidentRetirement::Retired(retired)),
                    Err(resources) => {
                        draft.retirement = Some(ResidentRetirement::Detached(resources));
                        return Ok(false);
                    }
                }
            }
            if let Some(ResidentRetirement::Retired(retired)) = draft.retirement.take() {
                if let Err(retired) = mount.accept_interrupted_exit_retirement(*close, retired, cx)
                {
                    draft.retirement = Some(ResidentRetirement::Retired(retired));
                    return Err("resident retirement evidence was refused".into());
                }
            }
            mount.interrupted_exit_retirement_ready(*close, cx)
        })?;
        if ready {
            controller.retire_construction()?;
        }
        Ok(ready)
    }

    #[cfg(feature = "test-faults")]
    pub fn test_shell_construction_retired(&self) -> bool {
        self.controller.as_ref().is_some_and(|controller| {
            matches!(
                controller.content,
                ShellContent::Retired {
                    reservation: Some(_),
                    ..
                }
            )
        })
    }

    #[cfg(feature = "test-faults")]
    pub fn test_retire_shutdown_draft(
        &mut self,
        draft: &mut MainWindowShutdownDraft,
        cx: &mut Context<Self>,
    ) -> Result<bool, String> {
        self.retire_shutdown_draft(draft, cx)
    }
}
