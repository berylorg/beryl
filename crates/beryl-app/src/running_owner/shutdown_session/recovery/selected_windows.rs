use super::*;
use crate::main_window::MainWindowShellRoot;
use crate::theme_runtime::GpuiAppearanceWindowSet;
use beryl_home_store::{CommandCancellation, HomeGeneration};
use gpui::{AsyncApp, Entity, WindowHandle};
use resident_windows_driver::ResidentRecoveryConfigurator;
use syndic_storage::SyndicTimestamp;

pub(super) struct SelectedWindowRecovery {
    windows: Vec<ResidentRecoveryWindow>,
    appearance: Option<Entity<GpuiAppearanceWindowSet>>,
}

impl SelectedWindowRecovery {
    pub(super) async fn detach_unpublished(
        &mut self,
        owner: &impl RecoveryOwnerAccess,
        home: beryl_model::BerylHomeId,
        generation: HomeGeneration,
        cx: &mut AsyncApp,
    ) -> Result<(), String> {
        for entry in &mut self.windows {
            let original_creation = owner
                .recovery_owner()?
                .borrow()
                .recovery_drafts()?
                .borrow()
                .has_captured_thread_creation_window(entry.window)?;
            if entry.preparation.is_some() {
                if owner
                    .recovery_owner()?
                    .borrow()
                    .failed_resident_preparation(entry.preparation.as_ref().unwrap())?
                {
                    RunningProcessOwner::cancel_and_drain_failed_resident(
                        owner,
                        &mut entry.preparation,
                        cx,
                    )
                    .await?;
                } else {
                    match RunningProcessOwner::cancel_and_drain_interrupted_exit_resident(
                        owner,
                        &mut entry.preparation,
                        cx,
                    )
                    .await?
                    {
                        Ok(source) => drop(source),
                        Err((retired, _)) => entry.retirement = Some(retired),
                    }
                }
            }
            let creation = loop {
                let handled = cx
                    .update(|app| {
                        owner
                            .recovery_owner()?
                            .borrow()
                            .recovery_drafts()?
                            .borrow_mut()
                            .detach_recovered_thread_creation_window(
                                entry.window,
                                home,
                                generation,
                                app,
                            )
                    })
                    .map_err(|error| error.to_string())??;
                match handled {
                    Some(false) => {
                        cx.background_executor()
                            .timer(std::time::Duration::from_millis(50))
                            .await
                    }
                    Some(true) => break true,
                    None => break false,
                }
            };
            if creation {
                entry.retain_creation_configuration_after_detach();
            } else if let Some((close, _)) = entry.attached {
                loop {
                    let detached = cx
                        .update(|app| {
                            let mount = entry
                                .window
                                .read(app)
                                .map_err(|error| error.to_string())?
                                .controller()
                                .ok_or("unpublished recovery controller is unavailable")?
                                .composer_mount()
                                .ok_or("unpublished recovery mount is unavailable")?;
                            mount.update(app, |mount, cx| {
                                mount.detach_unpublished_recovery(close, home, generation, cx)
                            })
                        })
                        .map_err(|error| error.to_string())??;
                    if detached {
                        break;
                    }
                    cx.background_executor()
                        .timer(std::time::Duration::from_millis(50))
                        .await;
                }
            }
            entry.adapters.take();
            if original_creation {
                entry.retain_creation_configuration_after_detach();
            } else {
                entry.configurator.take();
            }
        }
        if let Some(appearance) = self.appearance.take() {
            cx.update(|app| appearance.update(app, |appearance, _| appearance.retire()))
                .map_err(|error| error.to_string())?;
        }
        Ok(())
    }

    pub(super) fn has_appearance(&self) -> bool {
        self.appearance.is_some()
    }
}

impl RunningProcessOwner {
    pub(crate) fn retain_interrupted_exit_selected_windows(
        &mut self,
        request: &impl RecoveryIdentity,
        app: &App,
        configure: impl FnMut(
            WindowHandle<MainWindowShellRoot>,
        ) -> Result<ResidentRecoveryConfigurator, String>,
    ) -> Result<(), String> {
        {
            let _driver = self.reserve_interrupted_exit_driver(request)?;
            if self
                .interrupted_exit
                .as_ref()
                .unwrap()
                .selected_windows
                .is_some()
            {
                return Err("Interrupted Exit selected recovery is already retained".into());
            }
        }
        let windows = self.interrupted_exit_resident_windows(request, app, configure)?;
        self.interrupted_exit.as_mut().unwrap().selected_windows =
            Some(Rc::new(RefCell::new(SelectedWindowRecovery {
                windows,
                appearance: None,
            })));
        Ok(())
    }

    fn interrupted_exit_selected_windows(
        &mut self,
        request: &impl RecoveryIdentity,
    ) -> Result<Rc<RefCell<SelectedWindowRecovery>>, String> {
        let _driver = self.reserve_interrupted_exit_driver(request)?;
        self.interrupted_exit
            .as_ref()
            .unwrap()
            .selected_windows
            .clone()
            .ok_or_else(|| "Interrupted Exit selected recovery is not retained".into())
    }

    pub(crate) async fn recover_interrupted_exit_selected_windows(
        owner: &impl RecoveryOwnerAccess,
        request: &impl RecoveryIdentity,
        retired: HomeGeneration,
        at: SyndicTimestamp,
        cancellation: CommandCancellation,
        failed: impl FnMut(preparation_retry::RecoveryPreparationFailure),
        cx: &mut AsyncApp,
    ) -> Result<(), String> {
        let retained = owner
            .recovery_owner()?
            .borrow_mut()
            .interrupted_exit_selected_windows(request)?;
        let mut retained = retained
            .try_borrow_mut()
            .map_err(|_| "Interrupted Exit selected recovery is busy")?;
        let SelectedWindowRecovery {
            windows,
            appearance,
        } = &mut *retained;
        let configuration = owner
            .recovery_owner()?
            .borrow()
            .process
            .configuration
            .clone();
        Self::recover_interrupted_exit_resident_windows(
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

    pub(crate) async fn prepare_retired_interrupted_exit_selected_windows(
        owner: &impl RecoveryOwnerAccess,
        request: &impl RecoveryIdentity,
        retired: HomeGeneration,
        at: SyndicTimestamp,
        cancellation: CommandCancellation,
        failed: impl FnMut(preparation_retry::RecoveryPreparationFailure),
        cx: &mut AsyncApp,
    ) -> Result<(), String> {
        let retained = owner
            .recovery_owner()?
            .borrow_mut()
            .interrupted_exit_selected_windows(request)?;
        let _driver = owner
            .recovery_owner()?
            .borrow_mut()
            .reserve_interrupted_exit_driver(request)?;
        let mut retained = retained
            .try_borrow_mut()
            .map_err(|_| "Interrupted Exit selected recovery is busy")?;
        let SelectedWindowRecovery {
            windows,
            appearance,
        } = &mut *retained;
        let configuration = owner
            .recovery_owner()?
            .borrow()
            .process
            .configuration
            .clone();
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

    pub(super) async fn complete_prepared_interrupted_exit_selected_windows(
        owner: &impl RecoveryOwnerAccess,
        request: &impl RecoveryIdentity,
        retired: HomeGeneration,
        cancellation: CommandCancellation,
        cx: &mut AsyncApp,
    ) -> Result<(), String> {
        let retained = owner
            .recovery_owner()?
            .borrow_mut()
            .interrupted_exit_selected_windows(request)?;
        let _driver = owner
            .recovery_owner()?
            .borrow_mut()
            .reserve_interrupted_exit_driver(request)?;
        let mut retained = retained
            .try_borrow_mut()
            .map_err(|_| "Interrupted Exit selected recovery is busy")?;
        let SelectedWindowRecovery {
            windows,
            appearance,
        } = &mut *retained;
        let configuration = owner
            .recovery_owner()?
            .borrow()
            .process
            .configuration
            .clone();
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

    pub(super) async fn continue_interrupted_exit_selected_windows(
        owner: &impl RecoveryOwnerAccess,
        request: &impl RecoveryIdentity,
        retired: HomeGeneration,
        generation: HomeGeneration,
        cancellation: CommandCancellation,
        cx: &mut AsyncApp,
    ) -> Result<(), String> {
        let retained = owner
            .recovery_owner()?
            .borrow_mut()
            .interrupted_exit_selected_windows(request)?;
        let mut retained = retained
            .try_borrow_mut()
            .map_err(|_| "Interrupted Exit selected recovery is busy")?;
        let SelectedWindowRecovery {
            windows,
            appearance,
        } = &mut *retained;
        let appearance = appearance
            .as_ref()
            .ok_or("Interrupted Exit selected recovery appearance is not prepared")?;
        Self::prepare_and_complete_interrupted_exit_resident_windows(
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

    pub(super) fn interrupted_exit_selected_appearance(
        &mut self,
        request: &impl RecoveryIdentity,
    ) -> Result<Entity<GpuiAppearanceWindowSet>, String> {
        self.interrupted_exit_selected_windows(request)?
            .try_borrow()
            .map_err(|_| "Interrupted Exit selected recovery is busy")?
            .appearance
            .clone()
            .ok_or_else(|| "Interrupted Exit selected recovery appearance is not prepared".into())
    }

    #[cfg(test)]
    pub(crate) fn test_selected_recovery_appearance(
        &self,
    ) -> Option<Entity<GpuiAppearanceWindowSet>> {
        self.interrupted_exit
            .as_ref()?
            .selected_windows
            .as_ref()?
            .borrow()
            .appearance
            .clone()
    }
}
