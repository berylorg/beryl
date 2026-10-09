use super::*;
use crate::main_window::{MainWindowFreshClaimWidgetBatch, MainWindowFreshClaimWidgetReply};

impl RunningShutdownDrafts {
    fn claim_widget_draft(
        &self,
        window: &gpui::Window,
    ) -> Result<&crate::main_window::MainWindowShutdownDraft, String> {
        if !self.prepared || self.driving || self.releasing || self.released {
            return Err("fresh ordinary widget draft set is unavailable".into());
        }
        self.windows
            .iter()
            .find(|(handle, _)| gpui::AnyWindowHandle::from(*handle) == window.window_handle())
            .ok_or("fresh ordinary widget window is missing")?
            .1
            .as_ref()
            .map_err(Clone::clone)
    }

    pub(crate) fn claim_widget_batch(
        &self,
        root: &MainWindowShellRoot,
        home: beryl_model::BerylHomeId,
        generation: beryl_home_store::HomeGeneration,
        window: &gpui::Window,
        cx: &gpui::Context<MainWindowShellRoot>,
    ) -> Result<Option<std::sync::Arc<MainWindowFreshClaimWidgetBatch>>, String> {
        root.fresh_claim_widget_batch(self.claim_widget_draft(window)?, home, generation, cx)
    }

    pub(crate) fn accept_claim_widget_reply(
        &self,
        root: &MainWindowShellRoot,
        home: beryl_model::BerylHomeId,
        generation: beryl_home_store::HomeGeneration,
        window: &gpui::Window,
        reply: Box<MainWindowFreshClaimWidgetReply>,
        cx: &mut gpui::Context<MainWindowShellRoot>,
    ) -> Result<(), (Box<MainWindowFreshClaimWidgetReply>, String)> {
        let draft = match self.claim_widget_draft(window) {
            Ok(draft) => draft,
            Err(error) => return Err((reply, error)),
        };
        root.accept_fresh_claim_widget_reply(draft, home, generation, reply, cx)
    }
}
