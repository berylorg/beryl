use super::*;

impl MainWindowShellRoot {
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
        if !matches!(controller.content, ShellContent::Threadless { .. }) {
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
