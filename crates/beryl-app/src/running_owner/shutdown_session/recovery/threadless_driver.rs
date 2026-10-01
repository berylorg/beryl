use super::*;
use crate::app_services::AppServiceConfiguration;
use crate::main_window::MainWindowShellRoot;
use crate::theme_runtime::{AppearancePublicationTarget, GpuiAppearanceWindowSet};
use beryl_home_store::{CommandCancellation, HomeGeneration};
use gpui::{AsyncApp, Entity, WindowHandle};
use std::num::NonZeroUsize;
use syndic_storage::SyndicTimestamp;

impl RunningProcessOwner {
    pub(crate) async fn recover_interrupted_exit_threadless(
        owner: &Rc<RefCell<Self>>,
        request: &RunningExitRequest,
        retired: HomeGeneration,
        window: WindowHandle<MainWindowShellRoot>,
        configuration: AppServiceConfiguration,
        at: SyndicTimestamp,
        cancellation: CommandCancellation,
        failed: impl FnMut(RecoveryPreparationFailure),
        appearance: &mut Option<Entity<GpuiAppearanceWindowSet>>,
        cx: &mut AsyncApp,
    ) -> Result<(), String> {
        let _driver = owner
            .borrow_mut()
            .reserve_interrupted_exit_driver(request)?;
        if appearance.is_some() {
            return Err("Interrupted Exit fresh appearance is already retained".into());
        }
        let (retired_home, previous, capacity) = cx
            .update(|app| -> Result<_, String> {
                let owner = owner.borrow();
                let shells = owner.process.windows.shells();
                if shells.len() != 1 || shells[0].window() != window {
                    return Err(
                        "Interrupted Exit requires the sole retained threadless window".into(),
                    );
                }
                let root = window.read(app).map_err(|error| error.to_string())?;
                if !root
                    .controller()
                    .is_some_and(|controller| controller.is_threadless())
                {
                    return Err("Interrupted Exit requires a threadless window".into());
                }
                let graph = owner
                    .process
                    .services
                    .as_ref()
                    .and_then(|services| services.graph())
                    .ok_or("Interrupted Exit original service graph is unavailable")?;
                let previous = owner.process.appearance.clone();
                let capacity = NonZeroUsize::new(previous.read(app).target().snapshot().capacity)
                    .ok_or("Interrupted Exit appearance capacity is unavailable")?;
                Ok((graph.home().home_id(), previous, capacity))
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
            configuration,
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
                let generation = prepared.prepared().home().home_generation();
                previous.update(app, |set, _| set.retire());
                *appearance = Some(GpuiAppearanceWindowSet::new(prepared, capacity, app));
                Ok(generation)
            })
            .map_err(|error| error.to_string())??;
        let appearance = appearance
            .as_ref()
            .expect("fresh recovery appearance retained");
        Self::attach_interrupted_exit_threadless_pass(
            owner,
            request,
            retired_home,
            retired,
            window,
            appearance,
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
        .await
    }
}
