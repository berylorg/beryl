use super::*;

impl MainWindowShell {
    pub(in crate::main_window) fn startup_composer_entity(
        &self,
        app: &App,
    ) -> Option<Entity<crate::main_window::MainWindowConversationComposer>> {
        self.root
            .read(app)
            .controller
            .as_ref()
            .and_then(|controller| controller.composer_mount.as_ref())
            .and_then(|mount| mount.read(app).contribution())
    }

    pub(in crate::main_window) fn startup_readiness(
        &self,
        expected: beryl_model::WindowId,
        app: &mut App,
    ) -> Result<bool, String> {
        use crate::theme_runtime::AppearancePublicationTarget;
        if self.published || !self.desktop_placement_ready() {
            return Err("startup member is published or desktop placement is not ready".to_owned());
        }
        if !self.desktop_native_publication_allowed(app) {
            return Err("startup native member was closed or lost".to_owned());
        }
        let appearance = self.appearance_owner.read(app).target().snapshot();
        self.window
            .read_with(app, |root, app| {
                if let Some(error) = &root.construction_error {
                    return Err(error.clone());
                }
                let controller = root
                    .controller
                    .as_ref()
                    .ok_or_else(|| "startup member lost its controller".to_owned())?;
                if controller.window_id() != expected
                    || !root.startup_interaction_gated()
                    || !root.startup_interaction_ready(app)
                {
                    return Err("startup member identity or interaction gate changed".to_owned());
                }
                if !appearance.active
                    || !Arc::ptr_eq(&appearance.current, &controller.appearance.generation)
                {
                    return Err("startup appearance generation changed".to_owned());
                }
                match &controller.content {
                    ShellContent::Retired { .. }
                    | ShellContent::Selected { .. }
                    | ShellContent::RecoveredThreadless { .. } => {
                        Err("retired shell cannot enter startup".into())
                    }
                    ShellContent::Threadless { source, .. } => {
                        source.validate_lifetime()?;
                        if controller.composer_mount.is_some() {
                            return Err("threadless startup member acquired an editor".to_owned());
                        }
                        Ok(true)
                    }
                    ShellContent::Acquired { selection, .. }
                    | ShellContent::Restored { selection, .. } => {
                        if let ShellContent::Restored { custody, .. } = &controller.content {
                            custody.composer.validate_shell_lifetime()?;
                        }
                        let mount = controller
                            .composer_mount
                            .as_ref()
                            .ok_or_else(|| "startup member lost its editor mount".to_owned())?
                            .read(app);
                        let composer = mount
                            .contribution()
                            .ok_or_else(|| "startup member lost its selected editor".to_owned())?;
                        let composer = composer.read(app);
                        if composer.selection_identity() != *selection {
                            return Err("startup editor selection changed".to_owned());
                        }
                        if let Some(error) = composer.last_error() {
                            return Err(error.to_owned());
                        }
                        Ok(mount.selected_first_presentable(app))
                    }
                }
            })
            .map_err(|error| error.to_string())?
    }
}
