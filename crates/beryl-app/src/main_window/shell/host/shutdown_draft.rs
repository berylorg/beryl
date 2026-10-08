use super::*;
use crate::main_window::{
    MainWindowConversationComposerCloseAdvance, MainWindowConversationComposerCloseRelease,
    MainWindowConversationComposerCloseTicket, MainWindowConversationComposerMount,
};

mod detached;
pub(super) mod recovery;

pub struct MainWindowShutdownDraft {
    pub(crate) failed: Option<FailedShutdownResident>,
    pub(super) root: gpui::EntityId,
    pub(super) retirement: Option<recovery::ResidentRetirement>,
    pub(super) detached_source: Option<syndic_storage::DetachedDraftReadSourceV1>,
    pub(super) detached_installed: bool,
    pub(super) composer: Option<(
        Entity<MainWindowConversationComposerMount>,
        gpui::EntityId,
        MainWindowConversationComposerCloseTicket,
    )>,
}

pub(crate) struct FailedShutdownResident {
    pub(crate) adoption: Option<crate::main_window::MainWindowFailedResidentAdoption>,
    pub(crate) capture: Option<crate::main_window::MainWindowFailedResidentCapture>,
    pub(crate) ticket: crate::main_window::MainWindowFailedResidentTicket,
    pub(crate) resources: Option<Box<crate::main_window::MainWindowFailedResidentMountResources>>,
    pub(crate) retired: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MainWindowShutdownDraftAdvance {
    Threadless,
    Resident(MainWindowConversationComposerCloseAdvance),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MainWindowShutdownDraftRelease {
    Pending,
    Released,
}

impl MainWindowShellRoot {
    fn shutdown_draft_mount(
        &self,
        cx: &App,
    ) -> Result<Option<Entity<MainWindowConversationComposerMount>>, String> {
        if !(self.shutdown_interaction_gated || self.ordinary_close_interaction_gated)
            || self.startup_interaction_gated()
        {
            return Err("shutdown draft requires the running shutdown interaction gate".into());
        }
        let controller = self
            .controller
            .as_ref()
            .ok_or("shutdown shell lost its controller")?;
        if matches!(
            controller.content,
            ShellContent::Threadless { .. } | ShellContent::RecoveredThreadless { .. }
        ) {
            if controller.composer_mount.is_some() {
                return Err("threadless shutdown shell has a composer".into());
            }
            return Ok(None);
        }
        let mount = controller
            .composer_mount
            .clone()
            .ok_or("shutdown shell lost its composer")?;
        let composer = mount
            .read(cx)
            .contribution()
            .ok_or("shutdown shell lost its resident editor")?;
        if !composer.read(cx).is_live() {
            return Err("shutdown resident editor is being released".into());
        }
        Ok(Some(mount))
    }

    pub(crate) fn begin_shutdown_draft(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<MainWindowShutdownDraft, String> {
        if self.running_threads.pending_activation.is_some() {
            return Err("a running thread selection is still settling".into());
        }
        self.suspend_running_thread_reads(window, cx);
        let composer = match self.shutdown_draft_mount(cx)? {
            Some(mount) => {
                let editor = mount
                    .read(cx)
                    .contribution()
                    .ok_or("shutdown shell lost its resident editor")?
                    .entity_id();
                let close = mount.update(cx, |mount, cx| mount.begin_window_close(window, cx))?;
                Some((mount, editor, close.ticket))
            }
            None => None,
        };
        Ok(MainWindowShutdownDraft {
            failed: None,
            root: cx.entity_id(),
            retirement: None,
            detached_source: None,
            detached_installed: false,
            composer,
        })
    }

    pub(crate) fn advance_shutdown_draft(
        &mut self,
        draft: &MainWindowShutdownDraft,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<MainWindowShutdownDraftAdvance, String> {
        if draft.root != cx.entity_id() {
            return Err("shutdown draft belongs to another shell".into());
        }
        if !self.retire_setup_first_mount(window, cx)? {
            return Ok(MainWindowShutdownDraftAdvance::Resident(
                MainWindowConversationComposerCloseAdvance::Preparing,
            ));
        }
        if !self.running_thread_reads_drained() || !self.release_suspended_running_thread_sources()
        {
            return Ok(MainWindowShutdownDraftAdvance::Resident(
                MainWindowConversationComposerCloseAdvance::Preparing,
            ));
        }
        match (self.shutdown_draft_mount(cx)?, &draft.composer) {
            (None, None) => Ok(MainWindowShutdownDraftAdvance::Threadless),
            (Some(current), Some((mount, editor, ticket)))
                if current.entity_id() == mount.entity_id()
                    && current
                        .read(cx)
                        .contribution()
                        .is_some_and(|resident| resident.entity_id() == *editor) =>
            {
                mount
                    .update(cx, |mount, cx| {
                        mount.advance_window_close(*ticket, window, cx)
                    })
                    .map(MainWindowShutdownDraftAdvance::Resident)
            }
            _ => Err("shutdown draft composer custody changed".into()),
        }
    }

    pub(crate) fn release_shutdown_draft(
        &mut self,
        draft: &MainWindowShutdownDraft,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<MainWindowShutdownDraftRelease, String> {
        if draft.root != cx.entity_id() {
            return Err("shutdown draft belongs to another shell".into());
        }
        if !self.retire_setup_first_mount(window, cx)? {
            return Ok(MainWindowShutdownDraftRelease::Pending);
        }
        if !self.running_thread_reads_drained() || !self.release_suspended_running_thread_sources()
        {
            return Ok(MainWindowShutdownDraftRelease::Pending);
        }
        match (self.shutdown_draft_mount(cx)?, &draft.composer) {
            (None, None) => Ok(MainWindowShutdownDraftRelease::Released),
            (Some(current), Some((mount, editor, ticket)))
                if current.entity_id() == mount.entity_id()
                    && current
                        .read(cx)
                        .contribution()
                        .is_some_and(|resident| resident.entity_id() == *editor) =>
            {
                match mount.update(cx, |mount, cx| {
                    mount.release_window_close_with_evidence(*ticket, window, cx)
                })? {
                    MainWindowConversationComposerCloseRelease::Pending => {
                        Ok(MainWindowShutdownDraftRelease::Pending)
                    }
                    MainWindowConversationComposerCloseRelease::Released => {
                        Ok(MainWindowShutdownDraftRelease::Released)
                    }
                    MainWindowConversationComposerCloseRelease::Stale => {
                        Err("shutdown draft release lost its exact close ticket".into())
                    }
                }
            }
            _ => Err("shutdown draft composer custody changed".into()),
        }
    }

    #[cfg(feature = "test-faults")]
    pub fn test_release_shutdown_draft(
        &mut self,
        draft: &MainWindowShutdownDraft,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<MainWindowShutdownDraftRelease, String> {
        self.release_shutdown_draft(draft, window, cx)
    }

    #[cfg(feature = "test-faults")]
    pub fn test_begin_shutdown_draft(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<MainWindowShutdownDraft, String> {
        self.begin_shutdown_draft(window, cx)
    }

    #[cfg(feature = "test-faults")]
    pub fn test_advance_shutdown_draft(
        &mut self,
        draft: &MainWindowShutdownDraft,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<MainWindowShutdownDraftAdvance, String> {
        self.advance_shutdown_draft(draft, window, cx)
    }
}

impl MainWindowShutdownDraft {
    pub(crate) fn retained_native_close_service(
        &self,
    ) -> Result<
        Option<(
            std::sync::Arc<crate::main_window::MainWindowConversationComposerService>,
            MainWindowConversationComposerCloseTicket,
        )>,
        String,
    > {
        if !self.detached_installed || self.failed.is_some() {
            return Err("nonfinal native recovery has no reversible draft".into());
        }
        match (&self.composer, &self.retirement) {
            (None, None) => Ok(None),
            (Some((_, _, close)), Some(recovery::ResidentRetirement::Detached(resources))) => {
                resources
                    .service
                    .as_ref()
                    .map(|service| Some((service.clone(), *close)))
                    .ok_or_else(|| "nonfinal native recovery service is unavailable".into())
            }
            _ => Err("nonfinal native recovery draft correspondence changed".into()),
        }
    }
    pub(crate) fn discard_detached_source(&mut self) {
        self.detached_source.take();
    }

    pub(crate) fn recovery_resident_identity(
        &self,
    ) -> Option<(gpui::EntityId, MainWindowConversationComposerCloseTicket)> {
        self.composer
            .as_ref()
            .map(|(_, resident, close)| (*resident, *close))
    }

    #[cfg(feature = "test-faults")]
    pub fn test_ticket(&self) -> Option<MainWindowConversationComposerCloseTicket> {
        self.composer.as_ref().map(|(_, _, ticket)| *ticket)
    }
}
