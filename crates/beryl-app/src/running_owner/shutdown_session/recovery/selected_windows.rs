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

impl RunningProcessOwner {
    pub(crate) fn retain_interrupted_exit_selected_windows(
        &mut self,
        request: &RunningExitRequest,
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
        request: &RunningExitRequest,
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
        owner: &Rc<RefCell<Self>>,
        request: &RunningExitRequest,
        retired: HomeGeneration,
        at: SyndicTimestamp,
        cancellation: CommandCancellation,
        failed: impl FnMut(preparation_retry::RecoveryPreparationFailure),
        cx: &mut AsyncApp,
    ) -> Result<(), String> {
        let retained = owner
            .borrow_mut()
            .interrupted_exit_selected_windows(request)?;
        let mut retained = retained
            .try_borrow_mut()
            .map_err(|_| "Interrupted Exit selected recovery is busy")?;
        let SelectedWindowRecovery {
            windows,
            appearance,
        } = &mut *retained;
        let configuration = owner.borrow().process.configuration.clone();
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
        owner: &Rc<RefCell<Self>>,
        request: &RunningExitRequest,
        retired: HomeGeneration,
        at: SyndicTimestamp,
        cancellation: CommandCancellation,
        failed: impl FnMut(preparation_retry::RecoveryPreparationFailure),
        cx: &mut AsyncApp,
    ) -> Result<(), String> {
        let retained = owner
            .borrow_mut()
            .interrupted_exit_selected_windows(request)?;
        let _driver = owner
            .borrow_mut()
            .reserve_interrupted_exit_driver(request)?;
        let mut retained = retained
            .try_borrow_mut()
            .map_err(|_| "Interrupted Exit selected recovery is busy")?;
        let SelectedWindowRecovery {
            windows,
            appearance,
        } = &mut *retained;
        let configuration = owner.borrow().process.configuration.clone();
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
        owner: &Rc<RefCell<Self>>,
        request: &RunningExitRequest,
        retired: HomeGeneration,
        cancellation: CommandCancellation,
        cx: &mut AsyncApp,
    ) -> Result<(), String> {
        let retained = owner
            .borrow_mut()
            .interrupted_exit_selected_windows(request)?;
        let _driver = owner
            .borrow_mut()
            .reserve_interrupted_exit_driver(request)?;
        let mut retained = retained
            .try_borrow_mut()
            .map_err(|_| "Interrupted Exit selected recovery is busy")?;
        let SelectedWindowRecovery {
            windows,
            appearance,
        } = &mut *retained;
        let configuration = owner.borrow().process.configuration.clone();
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

    pub(crate) async fn continue_interrupted_exit_selected_windows(
        owner: &Rc<RefCell<Self>>,
        request: &RunningExitRequest,
        retired: HomeGeneration,
        generation: HomeGeneration,
        cancellation: CommandCancellation,
        cx: &mut AsyncApp,
    ) -> Result<(), String> {
        let retained = owner
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
        request: &RunningExitRequest,
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
