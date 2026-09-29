use super::*;
use crate::main_window::{
    MainWindowConversationComposerCloseAdvance, MainWindowConversationComposerCloseRelease,
    MainWindowConversationComposerCloseTicket, MainWindowConversationComposerMount,
};

mod recovery;

pub struct MainWindowShutdownDraft {
    root: gpui::EntityId,
    retirement: Option<recovery::ResidentRetirement>,
    composer: Option<(
        Entity<MainWindowConversationComposerMount>,
        gpui::EntityId,
        MainWindowConversationComposerCloseTicket,
    )>,
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
        if !self.shutdown_interaction_gated || self.startup_interaction_gated() {
            return Err("shutdown draft requires the running shutdown interaction gate".into());
        }
        let controller = self
            .controller
            .as_ref()
            .ok_or("shutdown shell lost its controller")?;
        if matches!(controller.content, ShellContent::Threadless { .. }) {
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
            root: cx.entity_id(),
            retirement: None,
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
