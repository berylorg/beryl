use gpui::App;

#[derive(Clone, Default)]
pub(crate) struct SettingsCommandRoute {
    command: Option<crate::settings_window::SettingsWindowCommand>,
}

impl gpui::Global for SettingsCommandRoute {}

impl SettingsCommandRoute {
    pub(crate) fn install(command: crate::settings_window::SettingsWindowCommand, app: &mut App) {
        app.set_global(Self {
            command: Some(command),
        });
    }

    pub(crate) fn disabled_reason(app: &App) -> Option<&'static str> {
        app.try_global::<Self>()
            .and_then(|route| route.command.as_ref())
            .map_or(Some("Settings is unavailable."), |command| {
                command.disabled_reason(app)
            })
    }

    pub(crate) fn open(
        app: &mut App,
    ) -> Result<(), crate::settings_window::SettingsWindowMountError> {
        let command = app
            .try_global::<Self>()
            .and_then(|route| route.command.clone())
            .ok_or(crate::settings_window::SettingsWindowMountError::Window)?;
        command.open(app)
    }
}
