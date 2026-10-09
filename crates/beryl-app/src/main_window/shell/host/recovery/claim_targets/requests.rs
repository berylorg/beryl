use super::*;
use crate::main_window::{MainWindowFreshClaimWidgetBatch, MainWindowFreshClaimWidgetReply};

impl MainWindowShellRoot {
    fn fresh_claim_widget_owner(
        &self,
        draft: &MainWindowShutdownDraft,
        home: beryl_model::BerylHomeId,
        generation: beryl_home_store::HomeGeneration,
        cx: &Context<Self>,
    ) -> Result<
        (
            Entity<crate::main_window::MainWindowConversationComposer>,
            crate::main_window::MainWindowConversationComposerCloseTicket,
        ),
        String,
    > {
        if draft.root != cx.entity_id()
            || !self.shutdown_interaction_gated
            || self.startup_interaction_gated()
        {
            return Err("fresh ordinary widget shell fence changed".into());
        }
        let (mount, editor, close) = draft
            .composer
            .as_ref()
            .ok_or("fresh ordinary mount is missing")?;
        let controller = self
            .controller
            .as_ref()
            .ok_or("fresh ordinary controller is missing")?;
        let resident = mount
            .read(cx)
            .contribution()
            .ok_or("fresh ordinary editor is missing")?;
        if close.selection().binding().home_id() != home
            || close.selection().binding().home_generation() != generation
            || controller.composer_mount.as_ref() != Some(mount)
            || mount.read(cx).fresh_recovery_ticket() != Some(*close)
            || resident.entity_id() != *editor
            || !matches!(&controller.content, ShellContent::Selected { window, selection, .. } if window.selected_thread() == Some(close.selection().claim()) && *selection == close.selection())
        {
            return Err("fresh ordinary widget shell identity changed".into());
        }
        Ok((resident, *close))
    }

    pub(crate) fn fresh_claim_widget_batch(
        &self,
        draft: &MainWindowShutdownDraft,
        home: beryl_model::BerylHomeId,
        generation: beryl_home_store::HomeGeneration,
        cx: &Context<Self>,
    ) -> Result<Option<Arc<MainWindowFreshClaimWidgetBatch>>, String> {
        let (resident, close) = self.fresh_claim_widget_owner(draft, home, generation, cx)?;
        resident.read(cx).fresh_claim_widget_batch(close)
    }

    pub(crate) fn accept_fresh_claim_widget_reply(
        &self,
        draft: &MainWindowShutdownDraft,
        home: beryl_model::BerylHomeId,
        generation: beryl_home_store::HomeGeneration,
        reply: Box<MainWindowFreshClaimWidgetReply>,
        cx: &mut Context<Self>,
    ) -> Result<(), (Box<MainWindowFreshClaimWidgetReply>, String)> {
        let (resident, close) = match self.fresh_claim_widget_owner(draft, home, generation, cx) {
            Ok(owner) => owner,
            Err(error) => return Err((reply, error)),
        };
        resident.update(cx, |resident, _| {
            resident.accept_fresh_claim_widget_reply(close, reply)
        })
    }
}
