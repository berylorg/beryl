use super::*;
use crate::app_services::recovery_composer::PreparedComposerRecoveryAdapters;
use crate::main_window::{
    MainWindowConversationComposerPreparedSelection, MainWindowFreshComposerPreparation,
};

impl MainWindowConversationComposerMount {
    pub(crate) fn fresh_recovery_ticket(
        &self,
    ) -> Option<MainWindowConversationComposerCloseTicket> {
        self.window_close
            .filter(|close| close.recovery_fenced)
            .map(|close| close.ticket)
    }

    pub(crate) fn detach_fresh_recovery(
        &mut self,
        ticket: MainWindowConversationComposerCloseTicket,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<bool, String> {
        let close = self
            .window_close
            .filter(|close| {
                close.ticket == ticket && close.recovery_fenced && ticket.owner == cx.entity_id()
            })
            .ok_or("fresh recovery mount close identity changed")?;
        if close.resources_detached {
            return Ok(true);
        }
        if self.window_close_task.is_some()
            || self.window_close_workers.retained() != 0
            || !self.autosave.workers_drained()
            || !self.submission.workers_drained()
            || self.submission.is_active()
            || self.pending_presentation.is_some()
            || self.native_lineage_snapshot.is_some()
            || self.native_lineage_validation_task.is_some()
            || self.native_lineage_workers.retained() != 0
            || self.pending_cleanup_workers.retained() != 0
            || self.native_disposal_workers.retained() != 0
            || self.native_lineage_disposal_task.is_some()
        {
            return Ok(false);
        }
        self.validate_recovery_native_resources()?;
        let autosave = self.autosave.recovery_adapters()?;
        let submission = self.submission.recovery_source()?;
        if !self
            .contribution
            .as_ref()
            .ok_or("fresh recovery resident is missing")?
            .update(cx, |resident, cx| {
                resident.detach_fresh_recovery(ticket, window, cx)
            })?
        {
            return Ok(false);
        }
        autosave.take();
        submission.take();
        self.contribution_subscription.take();
        self.service.take();
        self.configurator.take();
        self.native_lineage_recovery.take();
        self.native_lineage_refresh_task.take();
        self.window_close.as_mut().unwrap().resources_detached = true;
        cx.notify();
        Ok(true)
    }

    pub(crate) fn advance_fresh_recovery(
        &mut self,
        ticket: MainWindowConversationComposerCloseTicket,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<bool, String> {
        if ticket.owner != cx.entity_id() || !self.recovery_binding_current(ticket) {
            return Err("fresh recovery mount preparation binding changed".into());
        }
        self.contribution
            .as_ref()
            .ok_or("fresh recovery mount resident is missing")?
            .update(cx, |resident, cx| {
                resident.advance_fresh_recovery(ticket, window, cx)
            })
    }

    pub(crate) fn validate_fresh_recovery_protection(
        &self,
        ticket: MainWindowConversationComposerCloseTicket,
        cx: &App,
    ) -> Result<(), String> {
        if !self.recovery_binding_current(ticket) {
            return Err("fresh recovery mount protection binding changed".into());
        }
        self.contribution
            .as_ref()
            .ok_or("fresh recovery mount resident is missing")?
            .read(cx)
            .validate_fresh_recovery_protection(ticket, cx)
    }

    pub(crate) fn from_fresh_recovery(
        prepared: MainWindowConversationComposerPreparedSelection,
        configurator: MainWindowConversationComposerConfigurator,
        adapters: PreparedComposerRecoveryAdapters,
        preparation: &MainWindowFreshComposerPreparation,
        window: &mut Window,
        cx: &mut App,
    ) -> Result<
        (Entity<Self>, MainWindowConversationComposerCloseTicket),
        (String, Option<Entity<Self>>),
    > {
        let selection = prepared.selection_identity();
        if preparation.selection() != Some(selection)
            || !adapters.matches(
                selection.binding().home_id(),
                selection.binding().home_generation(),
            )
        {
            return Err((
                "fresh recovery preparation and mount adapters differ".into(),
                None,
            ));
        }
        let service = prepared.service();
        let assets = prepared.assets();
        let private_clipboard = adapters.private_clipboard_owner();
        let (_, marker, submission, native) = adapters.into_parts();
        let contribution = prepared
            .mount(
                MainWindowConversationComposer::production_clipboard_writer(),
                window,
                cx,
            )
            .map_err(|error| (error, None))?;
        let mount = cx.new(|mount_cx| {
            Self::complete(
                service,
                configurator,
                assets,
                marker,
                submission,
                contribution,
                mount_cx,
            )
        });
        let setup = mount.update(cx, |mount, mount_cx| {
            let ticket = MainWindowConversationComposerCloseTicket::for_recovery(
                mount_cx.entity_id(),
                1,
                selection,
            );
            mount.window_close_generation = 1;
            mount.native_lineage_recovery = Some(native);
            mount.window_close = Some(ActiveWindowClose {
                ticket,
                flush: None,
                state: MainWindowConversationComposerCloseAdvance::Preparing,
                disposing: false,
                disposal_captured: false,
                release_requested: false,
                recovery_fenced: true,
                resources_detached: false,
                restore_enabled: None,
                #[cfg(feature = "test-faults")]
                cancel_disposal: false,
            });
            preparation.fence(ticket)?;
            let resident = mount.contribution.as_ref().unwrap().clone();
            if let Some(owner) = private_clipboard {
                mount
                    .bound_service()?
                    .set_private_clipboard_owner(owner.clone());
                resident.update(mount_cx, |resident, cx| {
                    resident.attach_private_clipboard_owner(owner, cx)
                });
            }
            resident.update(mount_cx, |resident, cx| {
                resident.fence_fresh_recovery(ticket, window, cx)
            })?;
            mount.advance_fresh_recovery(ticket, window, mount_cx)?;
            mount.subscribe_to_contribution(window, mount_cx)?;
            Ok(ticket)
        });
        match setup {
            Ok(ticket) => Ok((mount, ticket)),
            Err(error) => Err((error, Some(mount))),
        }
    }
}
