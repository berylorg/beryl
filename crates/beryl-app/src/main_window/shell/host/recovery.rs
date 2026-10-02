use super::*;
use crate::{
    app_services::recovery_composer::PreparedComposerRecoveryAdapters,
    main_window::{
        MainWindowComposerRecoveryPreparation, MainWindowConversationComposerCloseTicket,
        MainWindowConversationComposerConfigurator,
    },
};

mod appearance;
mod bindings;

impl MainWindowShellRoot {
    pub(crate) fn adopt_failed_interrupted_exit_shell<C: Send + 'static>(
        &mut self,
        draft: &mut MainWindowShutdownDraft,
        preparation: &mut crate::main_window::MainWindowFailedResidentPreparation<C>,
        adapters: &mut Option<PreparedComposerRecoveryAdapters>,
        configurator: &mut Option<MainWindowConversationComposerConfigurator>,
        current: gpui_text_input::RangePrepublicationCurrent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<(C, MainWindowConversationComposerCloseTicket), String> {
        if draft.root != cx.entity_id()
            || !self.shutdown_interaction_gated
            || self.startup_interaction_gated()
            || draft.retirement.is_some()
        {
            return Err("failed shell recovery lost its exact gated draft".into());
        }
        let failed = draft
            .failed
            .as_mut()
            .ok_or("failed shell retirement is unavailable")?;
        if !failed.retired
            || failed.adoption.is_some()
            || failed.resources.is_some()
            || failed.capture.is_some()
            || preparation
                .capture()
                .is_none_or(|capture| capture.ticket() != failed.ticket)
        {
            return Err("failed shell capture or retirement changed".into());
        }
        let controller = self
            .controller
            .as_mut()
            .ok_or("failed shell controller is unavailable")?;
        let ShellContent::Retired {
            window_id,
            threadless: false,
            reservation: Some(_),
            ..
        } = &controller.content
        else {
            return Err("failed selected shell construction is not retired".into());
        };
        let (mount, editor, close) = draft
            .composer
            .as_mut()
            .ok_or("failed shell composer is unavailable")?;
        if controller.composer_mount.as_ref() != Some(mount)
            || !mount
                .read(cx)
                .contribution()
                .is_some_and(|resident| resident.entity_id() == *editor)
        {
            return Err("failed shell resident identity changed".into());
        }
        let (_, selection) = preparation
            .authenticated_source()
            .ok_or("failed shell source is unavailable")?;
        let record = preparation.authenticated_window()?;
        if selection.window_id() != *window_id
            || record.window_id() != *window_id
            || record.selected_thread() != Some(selection.claim())
            || record.remembered_target().is_none()
            || close.selection().binding().home_id() != selection.binding().home_id()
            || close.selection().binding().home_generation()
                == selection.binding().home_generation()
        {
            return Err("failed shell fresh window and resident correspondence changed".into());
        }
        let (graph, adoption, fresh) = mount.update(cx, |mount, cx| {
            mount.adopt_failed_recovery(
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
        failed.adoption = Some(adoption);
        *close = fresh;
        cx.notify();
        Ok((graph, fresh))
    }

    pub(crate) fn adopt_interrupted_exit_threadless_shell(
        &mut self,
        draft: &MainWindowShutdownDraft,
        source: &mut Option<crate::app_services::recovery_threadless::ThreadlessRecoveryWindow>,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        if draft.root != cx.entity_id()
            || !self.shutdown_interaction_gated
            || self.startup_interaction_gated()
            || draft.retirement.is_some()
            || draft.composer.is_some()
        {
            return Err("threadless recovery lost its exact gated draft".into());
        }
        let controller = self
            .controller
            .as_mut()
            .ok_or("recovery shell lost its controller")?;
        let ShellContent::Retired {
            window_id,
            threadless: true,
            reservation: Some(_),
            ..
        } = &controller.content
        else {
            return Err("threadless shell construction is not retired".into());
        };
        let authenticated = source
            .as_ref()
            .ok_or("threadless recovery source is unavailable")?;
        let previous = controller.appearance.generation.prepared().home();
        if controller.composer_mount.is_some()
            || authenticated.window().window_id() != *window_id
            || authenticated.home_id() != previous.home_id()
            || authenticated.generation() == previous.home_generation()
        {
            return Err("threadless recovery window and home identities disagree".into());
        }
        let ShellContent::Retired { reservation, .. } = &mut controller.content else {
            unreachable!()
        };
        controller.content = ShellContent::RecoveredThreadless {
            source: source.take().unwrap(),
            reservation: reservation.take().unwrap(),
        };
        cx.notify();
        Ok(())
    }

    pub(crate) fn adopt_interrupted_exit_shell<C: Send + 'static>(
        &mut self,
        draft: &mut MainWindowShutdownDraft,
        preparation: &mut MainWindowComposerRecoveryPreparation<C>,
        adapters: &mut Option<PreparedComposerRecoveryAdapters>,
        configurator: &mut Option<MainWindowConversationComposerConfigurator>,
        current: gpui_text_input::RangePrepublicationCurrent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<(C, MainWindowConversationComposerCloseTicket), String> {
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
            || previous.claim().thread_id() != selection.claim().thread_id()
            || previous.claim().generation() != selection.claim().generation()
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
        let identity = match &self.content {
            ShellContent::Recovered { selection, .. } => Some((
                selection.binding().home_id(),
                selection.binding().home_generation(),
            )),
            ShellContent::RecoveredThreadless { source, .. } => {
                Some((source.home_id(), source.generation()))
            }
            _ => None,
        };
        if let Some((home_id, generation)) = identity {
            let home = self.appearance.generation.prepared().home();
            if home.home_id() != home_id || home.home_generation() != generation {
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

#[cfg(all(test, feature = "test-faults"))]
#[path = "../../../../tests/unit/recovery_threadless_shell.rs"]
mod threadless_tests;
