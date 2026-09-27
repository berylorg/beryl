use super::*;
use crate::main_window::{
    MainWindowConversationComposerCloseAdvance, MainWindowConversationComposerCloseTicket,
    MainWindowConversationComposerMount,
};

pub struct MainWindowShutdownDraft {
    root: gpui::EntityId,
    composer: Option<(
        Entity<MainWindowConversationComposerMount>,
        MainWindowConversationComposerCloseTicket,
    )>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MainWindowShutdownDraftAdvance {
    Threadless,
    Resident(MainWindowConversationComposerCloseAdvance),
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
                let close = mount.update(cx, |mount, cx| mount.begin_window_close(window, cx))?;
                Some((mount, close.ticket))
            }
            None => None,
        };
        Ok(MainWindowShutdownDraft {
            root: cx.entity_id(),
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
            (Some(current), Some((mount, ticket))) if current.entity_id() == mount.entity_id() => {
                mount
                    .update(cx, |mount, cx| {
                        mount.advance_window_close(*ticket, window, cx)
                    })
                    .map(MainWindowShutdownDraftAdvance::Resident)
            }
            _ => Err("shutdown draft composer custody changed".into()),
        }
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
    #[cfg(feature = "test-faults")]
    pub fn test_ticket(&self) -> Option<MainWindowConversationComposerCloseTicket> {
        self.composer.as_ref().map(|(_, ticket)| *ticket)
    }
}
