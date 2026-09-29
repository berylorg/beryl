use super::*;
use crate::main_window::{
    MainWindowComposerMountRecoveryResources, MainWindowComposerRetiredClose,
};

pub(super) enum ResidentRetirement {
    Detached(MainWindowComposerMountRecoveryResources),
    Retired(MainWindowComposerRetiredClose),
}

impl MainWindowShellRoot {
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
        let controller = self
            .controller
            .as_mut()
            .ok_or("shutdown shell lost its controller")?;
        let Some((mount, editor, close)) = &draft.composer else {
            return if controller.is_threadless() && controller.composer_mount.is_none() {
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
        mount.update(cx, |mount, cx| {
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
                    ShellContent::Threadless { .. } => unreachable!(),
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
