use super::*;
use crate::{
    app_services::recovery_composer::PreparedComposerRecoveryAdapters,
    main_window::{
        MainWindowComposerRecoveryPreparation, MainWindowConversationComposerCloseTicket,
        MainWindowConversationComposerConfigurator,
    },
};

impl MainWindowShellRoot {
    pub(crate) fn adopt_interrupted_exit_shell(
        &mut self,
        draft: &mut MainWindowShutdownDraft,
        preparation: &mut MainWindowComposerRecoveryPreparation,
        adapters: &mut Option<PreparedComposerRecoveryAdapters>,
        configurator: &mut Option<MainWindowConversationComposerConfigurator>,
        current: gpui_text_input::RangePrepublicationCurrent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<
        (
            beryl_home_store::HomeRecoveryCandidate,
            MainWindowConversationComposerCloseTicket,
        ),
        String,
    > {
        if draft.root != cx.entity_id()
            || !self.shutdown_interaction_gated
            || self.startup_interaction_gated()
            || draft.retirement.is_some()
        {
            return Err("shell recovery lost its exact gated draft".into());
        }
        let controller = self
            .controller
            .as_mut()
            .ok_or("recovery shell lost its controller")?;
        let ShellContent::Retired {
            window_id,
            threadless: false,
            reservation: Some(_),
            ..
        } = &controller.content
        else {
            return Err("selected shell construction is not retired".into());
        };
        let (mount, editor, close) = draft
            .composer
            .as_mut()
            .ok_or("recovery draft lost its composer")?;
        if controller.composer_mount.as_ref() != Some(mount) {
            return Err("recovery draft belongs to another mount".into());
        }
        let resident = mount
            .read(cx)
            .contribution()
            .ok_or("recovery shell lost its resident")?;
        if resident.entity_id() != *editor {
            return Err("recovery draft belongs to another resident".into());
        }
        let (_, selection) = preparation
            .authenticated_source()?
            .ok_or("recovery source is unavailable")?;
        let record = preparation.authenticated_window()?;
        let previous = close.selection();
        if selection.window_id() != *window_id
            || record.window_id() != *window_id
            || record.selected_thread() != Some(selection.claim())
            || record.remembered_target().is_none()
            || previous.window_id() != *window_id
            || previous.claim() != selection.claim()
            || previous.binding().home_id() != selection.binding().home_id()
            || previous.binding().home_generation() == selection.binding().home_generation()
        {
            return Err("recovery window and resident identities disagree".into());
        }
        let adopted = mount.update(cx, |mount, cx| {
            mount.adopt_interrupted_exit_resident(
                &resident,
                *close,
                preparation,
                adapters,
                configurator,
                current,
                window,
                cx,
            )
        })?;
        let ShellContent::Retired { reservation, .. } = &mut controller.content else {
            unreachable!()
        };
        controller.content = ShellContent::Recovered {
            window: record,
            selection,
            reservation: reservation.take().unwrap(),
        };
        *close = adopted.1;
        cx.notify();
        Ok(adopted)
    }
}

impl MainWindowShellController {
    pub(super) fn validate_recovered_appearance(&self) -> Result<(), String> {
        if let ShellContent::Recovered { selection, .. } = &self.content {
            let home = self.appearance.generation.prepared().home();
            if home.home_id() != selection.binding().home_id()
                || home.home_generation() != selection.binding().home_generation()
            {
                return Err(
                    "recovered shell requires fresh appearance before interaction release".into(),
                );
            }
        }
        Ok(())
    }
}

#[cfg(all(test, feature = "test-faults"))]
#[path = "../../../../tests/unit/recovery_shell.rs"]
mod tests;
