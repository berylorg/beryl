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
mod configuration;
pub(crate) use configuration::ResidentRecoveryConfigurator;
use configuration::ResidentWindowConfiguration;

pub(crate) struct ResidentRecoveryWindow {
    pub(super) window: WindowHandle<MainWindowShellRoot>,
    pub(super) preparation: Option<resident::ResidentPreparationKey>,
    pub(super) adapters: Option<PreparedComposerRecoveryAdapters>,
    pub(super) configurator: Option<MainWindowConversationComposerConfigurator>,
    configuration: Rc<RefCell<ResidentWindowConfiguration>>,
    pub(super) retirement: Option<crate::main_window::MainWindowComposerRetiredClose>,
    pub(super) attached: Option<(
        MainWindowConversationComposerCloseTicket,
        beryl_state::SessionWindowRecord,
    )>,
    bound: bool,
}

impl ResidentRecoveryWindow {
    pub(super) fn retain_creation_configuration_after_detach(&mut self) {
        self.attached.take();
        self.bound = false;
        self.configurator = Some(ResidentWindowConfiguration::mount_configurator(
            &self.configuration,
        ));
    }

    pub(crate) fn unprepared(
        window: WindowHandle<MainWindowShellRoot>,
        configure: ResidentRecoveryConfigurator,
    ) -> Self {
        let (configuration, configurator) = ResidentWindowConfiguration::new(configure);
        Self {
            window,
            preparation: None,
            adapters: None,
            configurator: Some(configurator),
            configuration,
            retirement: None,
            attached: None,
            bound: false,
        }
    }

    pub(crate) fn new(
        window: WindowHandle<MainWindowShellRoot>,
        adapters: PreparedComposerRecoveryAdapters,
        configure: ResidentRecoveryConfigurator,
    ) -> Self {
        let mut entry = Self::unprepared(window, configure);
        entry.adapters = Some(adapters);
        entry
    }
}

impl RunningProcessOwner {
    pub(crate) fn interrupted_exit_resident_windows(
        &mut self,
        request: &impl RecoveryIdentity,
        app: &App,
        mut configure: impl FnMut(
            WindowHandle<MainWindowShellRoot>,
        ) -> Result<ResidentRecoveryConfigurator, String>,
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
        owner: &impl RecoveryOwnerAccess,
        request: &impl RecoveryIdentity,
        retired: HomeGeneration,
        windows: &mut [ResidentRecoveryWindow],
        configuration: AppServiceConfiguration,
        at: SyndicTimestamp,
        appearance: &mut Option<Entity<GpuiAppearanceWindowSet>>,
        cancellation: CommandCancellation,
        failed: impl FnMut(preparation_retry::RecoveryPreparationFailure),
        cx: &mut AsyncApp,
    ) -> Result<(), String> {
        let _driver = owner
            .recovery_owner()?
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
        cx.update(|app| -> Result<_, String> {
            let retained_owner = owner.recovery_owner()?;
            let owner = retained_owner.borrow();
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
            std::num::NonZeroUsize::new(previous.read(app).target().snapshot().capacity)
                .ok_or("Interrupted Exit appearance capacity is unavailable")?;
            Ok(())
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
        Self::prepare_retired_interrupted_exit_resident_windows_pass(
            owner,
            request,
            retired,
            windows,
            configuration,
            at,
            appearance,
            cancellation,
            failed,
            cx,
        )
        .await
    }

    pub(super) async fn prepare_retired_interrupted_exit_resident_windows_pass(
        owner: &impl RecoveryOwnerAccess,
        request: &impl RecoveryIdentity,
        retired: HomeGeneration,
        windows: &mut [ResidentRecoveryWindow],
        configuration: AppServiceConfiguration,
        at: SyndicTimestamp,
        appearance: &mut Option<Entity<GpuiAppearanceWindowSet>>,
        cancellation: CommandCancellation,
        failed: impl FnMut(preparation_retry::RecoveryPreparationFailure),
        cx: &mut AsyncApp,
    ) -> Result<(), String> {
        if cancellation.is_cancelled() {
            return Err("Interrupted Exit preparation was cancelled".into());
        }
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
        cx.update(|app| -> Result<(), String> {
            let retained_owner = owner.recovery_owner()?;
            let owner = retained_owner.borrow();
            owner.validate_interrupted_exit_resident_windows(windows, app)?;
            owner.validate_interrupted_exit_preparation(request, retired)?;
            std::num::NonZeroUsize::new(
                owner
                    .process
                    .appearance
                    .read(app)
                    .target()
                    .snapshot()
                    .capacity,
            )
            .ok_or("Interrupted Exit appearance capacity is unavailable")?;
            Ok(())
        })
        .map_err(|error| error.to_string())??;
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
        Self::complete_prepared_interrupted_exit_resident_windows_pass(
            owner,
            request,
            retired,
            windows,
            configuration,
            appearance,
            cancellation,
            cx,
        )
        .await
    }

    pub(super) async fn complete_prepared_interrupted_exit_resident_windows_pass(
        owner: &impl RecoveryOwnerAccess,
        request: &impl RecoveryIdentity,
        retired: HomeGeneration,
        windows: &mut [ResidentRecoveryWindow],
        configuration: AppServiceConfiguration,
        appearance: &mut Option<Entity<GpuiAppearanceWindowSet>>,
        cancellation: CommandCancellation,
        cx: &mut AsyncApp,
    ) -> Result<(), String> {
        if cancellation.is_cancelled() {
            return Err("Interrupted Exit preparation was cancelled".into());
        }
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
        let generation = cx
            .update(|app| -> Result<_, String> {
                let retained_owner = owner.recovery_owner()?;
                let owner = retained_owner.borrow();
                owner.validate_interrupted_exit_resident_windows(windows, app)?;
                let prepared = owner.interrupted_exit_appearance(request)?;
                let home = prepared.prepared().home();
                owner
                    .process
                    .services
                    .as_ref()
                    .ok_or("Interrupted Exit service owner is unavailable")?
                    .validate_retired_service_home_return(retired, Some(home.home_id()))
                    .map_err(|error| {
                        format!("resident recovery retired-home correspondence: {error:?}")
                    })?;
                let generation = home.home_generation();
                let previous = owner.process.appearance.clone();
                let capacity =
                    std::num::NonZeroUsize::new(previous.read(app).target().snapshot().capacity)
                        .ok_or("Interrupted Exit appearance capacity is unavailable")?;
                let adapters = windows
                    .iter()
                    .map(|_| {
                        owner.interrupted_exit_composer_adapters(
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
            cancellation,
            cx,
        )
        .await
    }

    pub(crate) async fn prepare_and_complete_interrupted_exit_resident_windows(
        owner: &impl RecoveryOwnerAccess,
        request: &impl RecoveryIdentity,
        retired: HomeGeneration,
        generation: HomeGeneration,
        windows: &mut [ResidentRecoveryWindow],
        appearance: &Entity<GpuiAppearanceWindowSet>,
        cancellation: CommandCancellation,
        cx: &mut AsyncApp,
    ) -> Result<(), String> {
        let _driver = owner
            .recovery_owner()?
            .borrow_mut()
            .reserve_interrupted_exit_driver(request)?;
        Self::prepare_and_complete_interrupted_exit_resident_windows_pass(
            owner,
            request,
            retired,
            generation,
            windows,
            appearance,
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
        owner: &impl RecoveryOwnerAccess,
        request: &impl RecoveryIdentity,
        retired: HomeGeneration,
        generation: HomeGeneration,
        windows: &mut [ResidentRecoveryWindow],
        appearance: &Entity<GpuiAppearanceWindowSet>,
        cancellation: CommandCancellation,
        cx: &mut AsyncApp,
    ) -> Result<(), String> {
        if cancellation.is_cancelled() {
            return Err("Interrupted Exit preparation was cancelled".into());
        }
        cx.update(|app| {
            owner
                .recovery_owner()?
                .borrow()
                .validate_interrupted_exit_resident_windows(windows, app)
        })
        .map_err(|error| error.to_string())??;
        for entry in windows.iter_mut() {
            let creation = if entry.preparation.is_none() && entry.attached.is_none() {
                cx.update(|app| -> Result<bool, String> {
                    let window = entry
                        .window
                        .read(app)
                        .map_err(|error| error.to_string())?
                        .controller()
                        .ok_or("Recovery controller is missing")?
                        .window_id();
                    owner
                        .recovery_owner()?
                        .borrow()
                        .committed_thread_creation_window(request, window)
                })
                .map_err(|error| error.to_string())??
            } else {
                false
            };
            if creation && entry.attached.is_none() {
                entry.attached = Some(
                    Self::attach_recovered_thread_creation_pass(
                        owner,
                        request,
                        entry.window,
                        appearance,
                        cancellation.clone(),
                        cx,
                    )
                    .await?,
                );
                entry.adapters.take();
                entry.configurator.take();
                entry.bound = true;
            }
            if entry.attached.is_none() {
                let configuration = entry.configuration.clone();
                let current_configuration = configuration.clone();
                let retirement = &mut entry.retirement;
                let window = entry.window;
                Self::attach_and_retain_interrupted_exit_resident_pass(
                    owner,
                    request,
                    &mut entry.preparation,
                    |app| {
                        ResidentWindowConfiguration::prepare(
                            configuration,
                            &owner.recovery_owner()?,
                            request,
                            window,
                            generation,
                            retirement,
                            app,
                        )
                    },
                    entry.window,
                    appearance,
                    &mut entry.attached,
                    &mut entry.adapters,
                    &mut entry.configurator,
                    |_, _| current_configuration.borrow_mut().current(),
                    cancellation.clone(),
                    cx,
                )
                .await?;
                entry.bound = true;
            }
            if !entry.bound {
                cx.update(|app| {
                    owner
                        .recovery_owner()?
                        .borrow_mut()
                        .bind_interrupted_exit_appearance(request, entry.window, appearance, app)
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
