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
use std::num::NonZeroUsize;
use syndic_storage::SyndicTimestamp;

impl RunningProcessOwner {
    pub(crate) async fn recover_interrupted_exit_resident_window(
        owner: &Rc<RefCell<Self>>,
        request: &RunningExitRequest,
        retired: HomeGeneration,
        window: WindowHandle<MainWindowShellRoot>,
        configuration: AppServiceConfiguration,
        at: SyndicTimestamp,
        preparation: &mut Option<resident::ResidentPreparationKey>,
        admit: impl FnOnce(HomeGeneration, &mut App) -> Result<resident::ResidentPreparationKey, String>,
        appearance: &mut Option<Entity<GpuiAppearanceWindowSet>>,
        adapters: &mut Option<PreparedComposerRecoveryAdapters>,
        configurator: &mut Option<MainWindowConversationComposerConfigurator>,
        current: impl FnOnce(
            &gpui::Window,
            &mut App,
        ) -> Result<gpui_text_input::RangePrepublicationCurrent, String>,
        cancellation: CommandCancellation,
        failed: impl FnMut(preparation_retry::RecoveryPreparationFailure),
        cx: &mut AsyncApp,
    ) -> Result<
        (
            MainWindowConversationComposerCloseTicket,
            beryl_state::SessionWindowRecord,
        ),
        String,
    > {
        let _driver = owner
            .borrow_mut()
            .reserve_interrupted_exit_driver(request)?;
        if preparation.is_some() || appearance.is_some() || adapters.is_some() {
            return Err("Interrupted Exit selected recovery inputs are already retained".into());
        }
        if cancellation.is_cancelled() {
            return Err("Interrupted Exit preparation was cancelled".into());
        }
        let (previous, capacity) = cx
            .update(|app| -> Result<_, String> {
                let owner = owner.borrow();
                let shells = owner.process.windows.shells();
                if shells.len() != 1 || shells[0].window() != window {
                    return Err(
                        "Interrupted Exit requires the sole retained selected window".into(),
                    );
                }
                let root = window.read(app).map_err(|error| error.to_string())?;
                if !root
                    .controller()
                    .is_some_and(|controller| controller.composer_mount().is_some())
                {
                    return Err("Interrupted Exit requires a selected window".into());
                }
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
                let capacity = NonZeroUsize::new(previous.read(app).target().snapshot().capacity)
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
                let fresh_adapters = owner.borrow().interrupted_exit_composer_adapters(
                    request,
                    home.home_id(),
                    generation,
                    configuration.projection.turn_start_admission_requirement(),
                )?;
                previous.update(app, |set, _| set.retire());
                *appearance = Some(GpuiAppearanceWindowSet::new(prepared, capacity, app));
                *adapters = Some(fresh_adapters);
                Ok(generation)
            })
            .map_err(|error| error.to_string())??;
        let appearance = appearance
            .as_ref()
            .expect("fresh recovery appearance retained");
        let attached = Self::prepare_and_attach_interrupted_exit_resident_pass(
            owner,
            request,
            preparation,
            |app| admit(generation, app),
            window,
            appearance,
            adapters,
            configurator,
            current,
            cancellation.clone(),
            cx,
        )
        .await?;
        Self::publish_and_complete_interrupted_exit_pass(
            owner,
            request,
            retired,
            generation,
            appearance,
            cancellation,
            cx,
        )
        .await?;
        Ok(attached)
    }

    pub(crate) async fn prepare_and_complete_interrupted_exit_resident_window(
        owner: &Rc<RefCell<Self>>,
        request: &RunningExitRequest,
        retired: HomeGeneration,
        generation: HomeGeneration,
        preparation: &mut Option<resident::ResidentPreparationKey>,
        admit: impl FnOnce(&mut App) -> Result<resident::ResidentPreparationKey, String>,
        window: WindowHandle<MainWindowShellRoot>,
        appearance: &Entity<GpuiAppearanceWindowSet>,
        adapters: &mut Option<PreparedComposerRecoveryAdapters>,
        configurator: &mut Option<MainWindowConversationComposerConfigurator>,
        current: impl FnOnce(
            &gpui::Window,
            &mut App,
        ) -> Result<gpui_text_input::RangePrepublicationCurrent, String>,
        cancellation: CommandCancellation,
        cx: &mut AsyncApp,
    ) -> Result<
        (
            MainWindowConversationComposerCloseTicket,
            beryl_state::SessionWindowRecord,
        ),
        String,
    > {
        let _driver = owner
            .borrow_mut()
            .reserve_interrupted_exit_driver(request)?;
        let attached = Self::prepare_and_attach_interrupted_exit_resident_pass(
            owner,
            request,
            preparation,
            admit,
            window,
            appearance,
            adapters,
            configurator,
            current,
            cancellation.clone(),
            cx,
        )
        .await?;
        Self::publish_and_complete_interrupted_exit_pass(
            owner,
            request,
            retired,
            generation,
            appearance,
            cancellation,
            cx,
        )
        .await?;
        Ok(attached)
    }
}
