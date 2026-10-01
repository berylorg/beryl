use super::*;
use crate::app_services::AppServiceConfiguration;
use crate::app_services::recovery_composer::PreparedComposerRecoveryAdapters;
use crate::main_window::{
    MainWindowConversationComposerCloseTicket, MainWindowConversationComposerConfigurator,
    MainWindowShellRoot,
};
use crate::theme_runtime::{AppearancePublicationTarget, GpuiAppearanceWindowSet};
use beryl_home_store::{CommandCancellation, HomeGeneration};
use gpui::{AsyncApp, Entity, WindowHandle};
use syndic_storage::SyndicTimestamp;

pub(crate) struct ResidentRecoveryWindow {
    window: WindowHandle<MainWindowShellRoot>,
    preparation: Option<resident::ResidentPreparationKey>,
    adapters: Option<PreparedComposerRecoveryAdapters>,
    configurator: Option<MainWindowConversationComposerConfigurator>,
    attached: Option<(
        MainWindowConversationComposerCloseTicket,
        beryl_state::SessionWindowRecord,
    )>,
    bound: bool,
}

impl ResidentRecoveryWindow {
    pub(crate) fn unprepared(
        window: WindowHandle<MainWindowShellRoot>,
        configurator: MainWindowConversationComposerConfigurator,
    ) -> Self {
        Self {
            window,
            preparation: None,
            adapters: None,
            configurator: Some(configurator),
            attached: None,
            bound: false,
        }
    }

    pub(crate) fn new(
        window: WindowHandle<MainWindowShellRoot>,
        adapters: PreparedComposerRecoveryAdapters,
        configurator: MainWindowConversationComposerConfigurator,
    ) -> Self {
        Self {
            window,
            preparation: None,
            adapters: Some(adapters),
            configurator: Some(configurator),
            attached: None,
            bound: false,
        }
    }
}

impl RunningProcessOwner {
    pub(crate) fn interrupted_exit_resident_windows(
        &mut self,
        request: &RunningExitRequest,
        app: &App,
        mut configure: impl FnMut(
            WindowHandle<MainWindowShellRoot>,
        ) -> Result<MainWindowConversationComposerConfigurator, String>,
    ) -> Result<Vec<ResidentRecoveryWindow>, String> {
        let _driver = self.reserve_interrupted_exit_driver(request)?;
        let shells = self.process.windows.shells();
        if shells.is_empty() {
            return Err(
                "Interrupted Exit requires the complete retained selected window set".into(),
            );
        }
        for shell in shells {
            Self::validate_interrupted_exit_selected_window(shell.window(), app)?;
        }
        shells
            .iter()
            .map(|shell| {
                let window = shell.window();
                Ok(ResidentRecoveryWindow::unprepared(
                    window,
                    configure(window)?,
                ))
            })
            .collect()
    }

    pub(crate) async fn recover_interrupted_exit_resident_windows(
        owner: &Rc<RefCell<Self>>,
        request: &RunningExitRequest,
        retired: HomeGeneration,
        windows: &mut [ResidentRecoveryWindow],
        configuration: AppServiceConfiguration,
        at: SyndicTimestamp,
        appearance: &mut Option<Entity<GpuiAppearanceWindowSet>>,
        mut admit: impl FnMut(
            usize,
            HomeGeneration,
            &mut App,
        ) -> Result<resident::ResidentPreparationKey, String>,
        current: impl FnMut(
            usize,
            &gpui::Window,
            &mut App,
        ) -> Result<gpui_text_input::RangePrepublicationCurrent, String>,
        cancellation: CommandCancellation,
        failed: impl FnMut(preparation_retry::RecoveryPreparationFailure),
        cx: &mut AsyncApp,
    ) -> Result<(), String> {
        let _driver = owner
            .borrow_mut()
            .reserve_interrupted_exit_driver(request)?;
        if appearance.is_some()
            || windows.iter().any(|entry| {
                entry.preparation.is_some()
                    || entry.adapters.is_some()
                    || entry.attached.is_some()
                    || entry.bound
                    || entry.configurator.is_none()
            })
        {
            return Err("Interrupted Exit selected recovery inputs are already retained".into());
        }
        if cancellation.is_cancelled() {
            return Err("Interrupted Exit preparation was cancelled".into());
        }
        let (previous, capacity) = cx
            .update(|app| -> Result<_, String> {
                let owner = owner.borrow();
                owner.validate_interrupted_exit_resident_windows(windows, app)?;
                if owner
                    .process
                    .services
                    .as_ref()
                    .and_then(|services| services.graph())
                    .is_none()
                {
                    return Err("Interrupted Exit original service graph is unavailable".into());
                }
                let previous = owner.process.appearance.clone();
                let capacity =
                    std::num::NonZeroUsize::new(previous.read(app).target().snapshot().capacity)
                        .ok_or("Interrupted Exit appearance capacity is unavailable")?;
                Ok((previous, capacity))
            })
            .map_err(|error| error.to_string())??;
        Self::retire_interrupted_exit_for_preparation(
            owner,
            request,
            retired,
            cancellation.clone(),
            cx,
        )
        .await?;
        Self::retry_interrupted_exit_preparation_attempts(
            owner,
            request,
            retired,
            configuration.clone(),
            at,
            cancellation.clone(),
            failed,
            cx,
        )
        .await?;
        if cancellation.is_cancelled() {
            Self::dispose_cancelled_interrupted_exit_preparation(owner, request, retired, cx)
                .await?;
            return Err("Interrupted Exit preparation was cancelled".into());
        }
        let generation = cx
            .update(|app| -> Result<_, String> {
                let prepared = owner.borrow().interrupted_exit_appearance(request)?;
                let home = prepared.prepared().home();
                let generation = home.home_generation();
                let adapters = windows
                    .iter()
                    .map(|_| {
                        owner.borrow().interrupted_exit_composer_adapters(
                            request,
                            home.home_id(),
                            generation,
                            configuration.projection.turn_start_admission_requirement(),
                        )
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                previous.update(app, |set, _| set.retire());
                *appearance = Some(GpuiAppearanceWindowSet::new(prepared, capacity, app));
                for (entry, adapters) in windows.iter_mut().zip(adapters) {
                    entry.adapters = Some(adapters);
                }
                Ok(generation)
            })
            .map_err(|error| error.to_string())??;
        Self::prepare_and_complete_interrupted_exit_resident_windows_pass(
            owner,
            request,
            retired,
            generation,
            windows,
            appearance
                .as_ref()
                .expect("fresh recovery appearance retained"),
            |index, app| admit(index, generation, app),
            current,
            cancellation,
            cx,
        )
        .await
    }

    pub(crate) async fn prepare_and_complete_interrupted_exit_resident_windows(
        owner: &Rc<RefCell<Self>>,
        request: &RunningExitRequest,
        retired: HomeGeneration,
        generation: HomeGeneration,
        windows: &mut [ResidentRecoveryWindow],
        appearance: &Entity<GpuiAppearanceWindowSet>,
        admit: impl FnMut(usize, &mut App) -> Result<resident::ResidentPreparationKey, String>,
        current: impl FnMut(
            usize,
            &gpui::Window,
            &mut App,
        ) -> Result<gpui_text_input::RangePrepublicationCurrent, String>,
        cancellation: CommandCancellation,
        cx: &mut AsyncApp,
    ) -> Result<(), String> {
        let _driver = owner
            .borrow_mut()
            .reserve_interrupted_exit_driver(request)?;
        Self::prepare_and_complete_interrupted_exit_resident_windows_pass(
            owner,
            request,
            retired,
            generation,
            windows,
            appearance,
            admit,
            current,
            cancellation,
            cx,
        )
        .await
    }

    fn validate_interrupted_exit_resident_windows(
        &self,
        windows: &[ResidentRecoveryWindow],
        app: &App,
    ) -> Result<(), String> {
        let shells = self.process.windows.shells();
        if windows.is_empty()
            || windows.len() != shells.len()
            || windows.iter().enumerate().any(|(index, entry)| {
                !shells.iter().any(|shell| shell.window() == entry.window)
                    || windows[..index]
                        .iter()
                        .any(|earlier| earlier.window == entry.window)
            })
        {
            return Err(
                "Interrupted Exit requires the complete retained selected window set".into(),
            );
        }
        for entry in windows {
            Self::validate_interrupted_exit_selected_window(entry.window, app)?;
        }
        Ok(())
    }

    fn validate_interrupted_exit_selected_window(
        window: WindowHandle<MainWindowShellRoot>,
        app: &App,
    ) -> Result<(), String> {
        let root = window.read(app).map_err(|error| error.to_string())?;
        if !root
            .controller()
            .is_some_and(|controller| controller.composer_mount().is_some())
        {
            return Err("Interrupted Exit requires selected windows".into());
        }
        Ok(())
    }

    async fn prepare_and_complete_interrupted_exit_resident_windows_pass(
        owner: &Rc<RefCell<Self>>,
        request: &RunningExitRequest,
        retired: HomeGeneration,
        generation: HomeGeneration,
        windows: &mut [ResidentRecoveryWindow],
        appearance: &Entity<GpuiAppearanceWindowSet>,
        mut admit: impl FnMut(usize, &mut App) -> Result<resident::ResidentPreparationKey, String>,
        mut current: impl FnMut(
            usize,
            &gpui::Window,
            &mut App,
        ) -> Result<gpui_text_input::RangePrepublicationCurrent, String>,
        cancellation: CommandCancellation,
        cx: &mut AsyncApp,
    ) -> Result<(), String> {
        if cancellation.is_cancelled() {
            return Err("Interrupted Exit preparation was cancelled".into());
        }
        cx.update(|app| {
            owner
                .borrow()
                .validate_interrupted_exit_resident_windows(windows, app)
        })
        .map_err(|error| error.to_string())??;
        for (index, entry) in windows.iter_mut().enumerate() {
            if entry.attached.is_none() {
                Self::attach_and_retain_interrupted_exit_resident_pass(
                    owner,
                    request,
                    &mut entry.preparation,
                    |app| admit(index, app),
                    entry.window,
                    appearance,
                    &mut entry.attached,
                    &mut entry.adapters,
                    &mut entry.configurator,
                    |window, app| current(index, window, app),
                    cancellation.clone(),
                    cx,
                )
                .await?;
                entry.bound = true;
            }
            if !entry.bound {
                cx.update(|app| {
                    owner.borrow_mut().bind_interrupted_exit_appearance(
                        request,
                        entry.window,
                        appearance,
                        app,
                    )
                })
                .map_err(|error| error.to_string())??;
                entry.bound = true;
            }
        }
        Self::publish_and_complete_interrupted_exit_pass(
            owner,
            request,
            retired,
            generation,
            appearance,
            cancellation,
            cx,
        )
        .await
    }
}
