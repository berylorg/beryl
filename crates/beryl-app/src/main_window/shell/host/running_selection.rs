use super::*;
use crate::main_window::{
    MainWindowComposerSelectionIdentity, MainWindowConversationComposerService,
};

impl MainWindowShellRoot {
    pub(in crate::main_window::shell::host) fn preflight_running_selection(
        &self,
        service: &Arc<MainWindowConversationComposerService>,
        cx: &App,
    ) -> Result<(), String> {
        let controller = self
            .controller
            .as_ref()
            .ok_or("running selection lost its controller")?;
        let mount = controller
            .composer_mount
            .as_ref()
            .ok_or("running selection lost its composer")?;
        if !Arc::ptr_eq(&mount.read(cx).claim_publication_service()?, service) {
            return Err("running selection composer service changed".into());
        }
        match &controller.content {
            ShellContent::Acquired { custody, .. } => custody
                .initial_composer
                .as_ref()
                .ok_or("running selection lost construction custody")?
                .validate_service_retirement(service),
            ShellContent::Restored { custody, .. } => {
                custody.composer.validate_service_retirement(service)
            }
            ShellContent::Selected { .. } => Ok(()),
            _ => Err("running selection requires a selected window".into()),
        }
    }

    pub(in crate::main_window::shell::host) fn sync_running_selected_cache(
        &mut self,
        previous: MainWindowComposerSelectionIdentity,
        next: MainWindowComposerSelectionIdentity,
        cx: &App,
    ) -> Result<(), String> {
        if previous.window_id() != next.window_id() || previous.claim() != next.claim() {
            return Err("running selection flush changed its claim".into());
        }
        let controller = self
            .controller
            .as_mut()
            .ok_or("running selection lost its controller")?;
        let composer = controller
            .composer_mount
            .as_ref()
            .and_then(|mount| mount.read(cx).contribution())
            .ok_or("running selection lost its resident composer")?;
        if composer.read(cx).selection_identity() != next {
            return Err("running selection flush presentation is stale".into());
        }
        let cached = match &mut controller.content {
            ShellContent::Acquired { selection, .. }
            | ShellContent::Restored { selection, .. }
            | ShellContent::Selected { selection, .. } => selection,
            _ => return Err("running selection lost its selected window".into()),
        };
        if cached.window_id() != previous.window_id() || cached.claim() != previous.claim() {
            return Err("running selection flush cache is stale".into());
        }
        *cached = next;
        Ok(())
    }

    pub(in crate::main_window::shell::host) fn publish_running_selection(
        &mut self,
        window: beryl_state::SessionWindowRecord,
        selection: MainWindowComposerSelectionIdentity,
        cx: &App,
    ) -> Result<(), String> {
        let controller = self
            .controller
            .as_ref()
            .ok_or("running selection lost its controller")?;
        if window.window_id() != controller.window_id()
            || selection.window_id() != window.window_id()
            || window.selected_thread() != Some(selection.claim())
        {
            return Err("running selection publication identity is stale".into());
        }
        let service = controller
            .composer_mount
            .as_ref()
            .ok_or("running selection lost its composer")?
            .read(cx)
            .claim_publication_service()?;
        self.preflight_running_selection(&service, cx)?;
        let controller = self.controller.as_mut().unwrap();
        match &mut controller.content {
            ShellContent::Acquired { custody, .. } => custody
                .initial_composer
                .as_mut()
                .ok_or("running selection lost construction custody")?
                .release_recovery_service(&service)?,
            ShellContent::Restored { custody, .. } => {
                custody.composer.release_recovery_service(&service)?
            }
            ShellContent::Selected { .. } => {}
            _ => return Err("running selection lost its selected window".into()),
        }
        controller.retire_construction()?;
        let ShellContent::Retired { reservation, .. } = &mut controller.content else {
            unreachable!()
        };
        let reservation = reservation
            .take()
            .ok_or("running selection lost its window reservation")?;
        controller.content = ShellContent::Selected {
            window,
            selection,
            reservation,
        };
        Ok(())
    }
}
