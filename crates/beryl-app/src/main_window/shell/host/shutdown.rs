use super::*;

impl MainWindowShellRoot {
    pub(crate) fn set_ordinary_close_interaction_gated(
        &mut self,
        gated: bool,
        cx: &mut Context<Self>,
    ) {
        self.ordinary_close_interaction_gated = gated;
        cx.notify();
    }
    pub(crate) fn release_shutdown_interaction_gates(
        windows: &[WindowHandle<Self>],
        app: &mut App,
    ) -> Result<(), String> {
        Self::release_shutdown_interaction_gates_after(windows, app, || Ok(()))
    }

    pub(crate) fn release_shutdown_interaction_gates_after(
        windows: &[WindowHandle<Self>],
        app: &mut App,
        settle: impl FnOnce() -> Result<(), String>,
    ) -> Result<(), String> {
        let (first, rest) = windows
            .split_first()
            .ok_or("shutdown interaction release has no published windows")?;
        first
            .update(app, |root, _, cx| {
                root.validate_shutdown_interaction_release(cx)?;
                for window in rest {
                    window
                        .read(cx)
                        .map_err(|error| error.to_string())?
                        .validate_shutdown_interaction_release(cx)?;
                }
                settle()?;
                root.set_shutdown_interaction_gated(false, cx)
                    .expect("validated composer remains live during synchronous gate release");
                root.set_ordinary_close_interaction_gated(false, cx);
                for window in rest {
                    window
                        .update(cx, |root, _, cx| {
                            root.set_shutdown_interaction_gated(false, cx).expect(
                                "validated composer remains live during synchronous gate release",
                            );
                            root.set_ordinary_close_interaction_gated(false, cx);
                        })
                        .expect("validated window remains live during synchronous gate release");
                }
                Ok(())
            })
            .map_err(|error| error.to_string())?
    }

    fn validate_shutdown_interaction_release(&self, app: &App) -> Result<(), String> {
        let controller = self
            .controller
            .as_ref()
            .ok_or("shutdown shell lost its controller")?;
        if matches!(controller.content, ShellContent::Retired { .. }) {
            return Err("retired shell requires fresh bindings before interaction release".into());
        }
        controller.validate_recovered_appearance()?;
        if !controller.is_threadless() {
            let composer = controller
                .composer_mount
                .as_ref()
                .and_then(|mount| mount.read(app).contribution())
                .ok_or("shutdown shell lost its composer")?;
            if !composer.read(app).is_live() {
                return Err("conversation composer is being released".into());
            }
        }
        Ok(())
    }

    #[cfg(feature = "test-faults")]
    pub fn test_release_shutdown_interaction_gates(
        windows: &[WindowHandle<Self>],
        app: &mut App,
    ) -> Result<(), String> {
        Self::release_shutdown_interaction_gates(windows, app)
    }

    #[cfg(feature = "test-faults")]
    pub fn test_release_shutdown_interaction_gates_after(
        windows: &[WindowHandle<Self>],
        app: &mut App,
        settle: impl FnOnce() -> Result<(), String>,
    ) -> Result<(), String> {
        Self::release_shutdown_interaction_gates_after(windows, app, settle)
    }

    pub(crate) fn set_shutdown_interaction_gated(
        &mut self,
        gated: bool,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        self.shutdown_interaction_gated = true;
        cx.notify();
        let controller = self
            .controller
            .as_ref()
            .ok_or_else(|| "shutdown shell lost its controller".to_owned())?;
        if !gated && matches!(controller.content, ShellContent::Retired { .. }) {
            return Err("retired shell requires fresh bindings before interaction release".into());
        }
        if !gated {
            controller.validate_recovered_appearance()?;
        }
        if !controller.is_threadless() {
            let composer = controller
                .composer_mount
                .as_ref()
                .and_then(|mount| mount.read(cx).contribution())
                .ok_or_else(|| "shutdown shell lost its composer".to_owned())?;
            composer.update(cx, |composer, cx| {
                composer.set_shutdown_interaction_gated(gated, cx)
            })?;
        }
        self.shutdown_interaction_gated = gated;
        Ok(())
    }

    #[cfg(feature = "test-faults")]
    pub fn test_set_shutdown_interaction_gated(
        &mut self,
        gated: bool,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        self.set_shutdown_interaction_gated(gated, cx)
    }
}
