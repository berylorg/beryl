use super::*;
use std::rc::Rc;

pub(super) struct PreNativeCloseCustody {
    request: Rc<()>,
    root: gpui::EntityId,
    controller: Rc<()>,
    window: beryl_model::WindowId,
    resident: Option<(gpui::EntityId, gpui::EntityId)>,
}

impl MainWindowShell {
    #[cfg(test)]
    pub(crate) fn test_pre_native_close_identity(&self) -> Option<Rc<()>> {
        self.pre_native_close
            .as_ref()
            .map(|proof| proof.request.clone())
    }
    fn pre_native_close_resident(
        &self,
        app: &App,
    ) -> Result<Option<(gpui::EntityId, gpui::EntityId)>, String> {
        let root = self.root.read(app);
        let controller = root
            .controller
            .as_ref()
            .ok_or("pre-native shell controller is unavailable")?;
        if !self.published
            || !root.shutdown_interaction_gated
            || root.startup_interaction_gated()
            || !self.desktop_cleanup_allowed()
            || self
                .startup_disposal
                .as_ref()
                .is_some_and(|admission| admission.started)
            || self.nonfinal_native_destruction.is_some()
            || matches!(controller.content, ShellContent::Retired { .. })
        {
            return Err(
                "pre-native close requires the exact published gated unretired shell".into(),
            );
        }
        match controller.composer_mount.as_ref() {
            Some(mount) => {
                let resident = mount
                    .read(app)
                    .contribution()
                    .ok_or("pre-native resident is unavailable")?;
                Ok(Some((mount.entity_id(), resident.entity_id())))
            }
            None if controller.is_threadless() => Ok(None),
            None => Err("pre-native selected shell lost its resident".into()),
        }
    }

    pub(crate) fn retain_pre_native_close(
        &mut self,
        request: Rc<()>,
        window: beryl_model::WindowId,
        app: &App,
    ) -> Result<(), String> {
        if self.pre_native_close.is_some() || self.retained_window_id(app)? != window {
            return Err(
                "pre-native close custody is already retained or belongs to another window".into(),
            );
        }
        let resident = self.pre_native_close_resident(app)?;
        let controller = self
            .root
            .read(app)
            .controller
            .as_ref()
            .unwrap()
            .identity
            .clone();
        self.pre_native_close = Some(PreNativeCloseCustody {
            request,
            root: self.root.entity_id(),
            controller,
            window,
            resident,
        });
        Ok(())
    }

    pub(crate) fn require_pre_native_close(
        &self,
        request: &Rc<()>,
        app: &App,
    ) -> Result<(), String> {
        let proof = self
            .pre_native_close
            .as_ref()
            .ok_or("pre-native close proof is unavailable")?;
        if !Rc::ptr_eq(&proof.request, request)
            || proof.root != self.root.entity_id()
            || proof.window != self.retained_window_id(app)?
            || !self
                .root
                .read(app)
                .controller
                .as_ref()
                .is_some_and(|controller| Rc::ptr_eq(&proof.controller, &controller.identity))
            || proof.resident != self.pre_native_close_resident(app)?
        {
            return Err("pre-native close proof no longer matches its request and shell".into());
        }
        Ok(())
    }

    pub(crate) fn release_pre_native_close(
        &mut self,
        request: &Rc<()>,
        app: &App,
    ) -> Result<(), String> {
        self.require_pre_native_close(request, app)?;
        self.pre_native_close = None;
        Ok(())
    }

    pub(crate) fn release_pre_native_close_if_present(
        &mut self,
        request: &Rc<()>,
        app: &App,
    ) -> Result<(), String> {
        if self.pre_native_close.is_some() {
            self.release_pre_native_close(request, app)?;
        }
        Ok(())
    }
}
