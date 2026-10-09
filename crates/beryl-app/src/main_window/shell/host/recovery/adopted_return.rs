use super::*;

impl MainWindowShellRoot {
    pub(crate) fn return_adopted_failed_resident(
        &mut self,
        draft: &mut MainWindowShutdownDraft,
        home: beryl_model::BerylHomeId,
        generation: beryl_home_store::HomeGeneration,
        cx: &mut Context<Self>,
    ) -> Result<
        Option<Option<Box<crate::main_window::MainWindowFailedResidentAdoptionReturn>>>,
        String,
    > {
        if draft.root != cx.entity_id()
            || !self.shutdown_interaction_gated
            || self.startup_interaction_gated()
            || draft.retirement.is_some()
        {
            return Err("adopted return lost its exact gated draft".into());
        }
        let Some(failed) = draft.failed.as_ref() else {
            return Ok(None);
        };
        let Some(adoption) = failed.adoption.as_ref() else {
            return Ok(None);
        };
        let window_id = adoption.selection().window_id();
        if !draft.prepare_adopted_cleanup_return(window_id)? {
            return Ok(Some(None));
        }
        let failed = draft.failed.as_mut().unwrap();
        let adoption = failed.adoption.as_ref().unwrap();
        if !failed.retired || failed.resources.is_some() || failed.capture.is_some() {
            return Err("adopted return original capture custody changed".into());
        }
        let controller = self
            .controller
            .as_mut()
            .ok_or("adopted return controller is unavailable")?;
        let (mount, editor, close) = draft
            .composer
            .as_ref()
            .ok_or("adopted return composer is unavailable")?;
        let ShellContent::Selected {
            window, selection, ..
        } = &controller.content
        else {
            return Err("adopted return construction is not the selected native member".into());
        };
        if *selection != adoption.selection()
            || window.window_id() != adoption.selection().window_id()
            || window.selected_thread() != Some(adoption.selection().claim())
            || close.selection() != adoption.selection()
            || controller.composer_mount.as_ref() != Some(mount)
            || close.selection().binding().home_id() != home
            || close.selection().binding().home_generation() != generation
        {
            return Err("adopted return native member correspondence changed".into());
        }
        let resident = mount
            .read(cx)
            .contribution()
            .ok_or("adopted return resident is unavailable")?;
        if resident.entity_id() != *editor {
            return Err("adopted return resident identity changed".into());
        }
        let Some(service) = resident
            .read(cx)
            .authenticate_adoption_return(adoption, *close, cx)?
        else {
            return Ok(Some(None));
        };
        if !mount.update(cx, |mount, cx| {
            mount.detach_adopted_resident_return(*close, home, generation, cx)
        })? {
            return Ok(Some(None));
        }
        let retired = ShellContent::Retired {
            window_id: window.window_id(),
            placement: window.placement().clone(),
            threadless: false,
            reservation: None,
        };
        let ShellContent::Selected { reservation, .. } =
            std::mem::replace(&mut controller.content, retired)
        else {
            unreachable!()
        };
        let ShellContent::Retired {
            reservation: retained,
            ..
        } = &mut controller.content
        else {
            unreachable!()
        };
        *retained = Some(reservation);
        let returned = Box::new(failed.adoption.take().unwrap()).into_return(service);
        cx.notify();
        Ok(Some(Some(returned)))
    }
}
