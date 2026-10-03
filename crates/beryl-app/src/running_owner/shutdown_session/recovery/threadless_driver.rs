use super::*;
use crate::main_window::MainWindowShellRoot;
use crate::theme_runtime::{AppearancePublicationTarget, GpuiAppearanceWindowSet};
use beryl_home_store::{CommandCancellation, HomeGeneration};
use gpui::{AsyncApp, Entity, WindowHandle};
use std::num::NonZeroUsize;
use syndic_storage::SyndicTimestamp;

impl RunningProcessOwner {
    pub(super) fn interrupted_exit_threadless_appearance(
        &mut self,
        request: &impl RecoveryIdentity,
    ) -> Result<Entity<GpuiAppearanceWindowSet>, String> {
        let _driver = self.reserve_interrupted_exit_driver(request)?;
        self.interrupted_exit
            .as_ref()
            .unwrap()
            .threadless_appearance
            .clone()
            .ok_or_else(|| "Interrupted Exit threadless appearance is not prepared".into())
    }

    pub(crate) async fn recover_interrupted_exit_threadless(
        owner: &impl RecoveryOwnerAccess,
        request: &impl RecoveryIdentity,
        retired: HomeGeneration,
        window: WindowHandle<MainWindowShellRoot>,
        at: SyndicTimestamp,
        cancellation: CommandCancellation,
        failed: impl FnMut(RecoveryPreparationFailure),
        cx: &mut AsyncApp,
    ) -> Result<(), String> {
        let _driver = owner
            .recovery_owner()?
            .borrow_mut()
            .reserve_interrupted_exit_driver(request)?;
        if owner
            .recovery_owner()?
            .borrow()
            .interrupted_exit
            .as_ref()
            .unwrap()
            .threadless_appearance
            .is_some()
        {
            return Err("Interrupted Exit fresh appearance is already retained".into());
        }
        cx.update(|app| -> Result<_, String> {
            let retained_owner = owner.recovery_owner()?;
            let owner = retained_owner.borrow();
            let shells = owner.process.windows.shells();
            if shells.len() != 1 || shells[0].window() != window {
                return Err("Interrupted Exit requires the sole retained threadless window".into());
            }
            let root = window.read(app).map_err(|error| error.to_string())?;
            if !root
                .controller()
                .is_some_and(|controller| controller.is_threadless())
            {
                return Err("Interrupted Exit requires a threadless window".into());
            }
            owner
                .process
                .services
                .as_ref()
                .and_then(|services| services.graph())
                .ok_or("Interrupted Exit original service graph is unavailable")?;
            NonZeroUsize::new(
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
        Self::retire_interrupted_exit_for_preparation(
            owner,
            request,
            retired,
            cancellation.clone(),
            cx,
        )
        .await?;
        Self::prepare_retired_interrupted_exit_threadless_pass(
            owner,
            request,
            retired,
            window,
            at,
            cancellation,
            failed,
            cx,
        )
        .await
    }

    pub(crate) async fn prepare_retired_interrupted_exit_threadless(
        owner: &impl RecoveryOwnerAccess,
        request: &impl RecoveryIdentity,
        retired: HomeGeneration,
        window: WindowHandle<MainWindowShellRoot>,
        at: SyndicTimestamp,
        cancellation: CommandCancellation,
        failed: impl FnMut(RecoveryPreparationFailure),
        cx: &mut AsyncApp,
    ) -> Result<(), String> {
        let _driver = owner
            .recovery_owner()?
            .borrow_mut()
            .reserve_interrupted_exit_driver(request)?;
        Self::prepare_retired_interrupted_exit_threadless_pass(
            owner,
            request,
            retired,
            window,
            at,
            cancellation,
            failed,
            cx,
        )
        .await
    }

    async fn prepare_retired_interrupted_exit_threadless_pass(
        owner: &impl RecoveryOwnerAccess,
        request: &impl RecoveryIdentity,
        retired: HomeGeneration,
        window: WindowHandle<MainWindowShellRoot>,
        at: SyndicTimestamp,
        cancellation: CommandCancellation,
        failed: impl FnMut(RecoveryPreparationFailure),
        cx: &mut AsyncApp,
    ) -> Result<(), String> {
        if cancellation.is_cancelled() {
            return Err("Interrupted Exit preparation was cancelled".into());
        }
        cx.update(|app| -> Result<(), String> {
            let retained_owner = owner.recovery_owner()?;
            let owner = retained_owner.borrow();
            owner.validate_interrupted_exit_preparation(request, retired)?;
            if owner
                .interrupted_exit
                .as_ref()
                .unwrap()
                .threadless_appearance
                .is_some()
            {
                return Err("Interrupted Exit fresh appearance is already retained".into());
            }
            let shells = owner.process.windows.shells();
            if shells.len() != 1 || shells[0].window() != window {
                return Err("Interrupted Exit requires the sole retained threadless window".into());
            }
            if !window
                .read(app)
                .map_err(|error| error.to_string())?
                .controller()
                .is_some_and(|controller| controller.is_threadless())
            {
                return Err("Interrupted Exit requires a threadless window".into());
            }
            NonZeroUsize::new(
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
        let configuration = owner
            .recovery_owner()?
            .borrow()
            .process
            .configuration
            .clone();
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
        Self::complete_prepared_interrupted_exit_threadless_pass(
            owner,
            request,
            retired,
            window,
            cancellation,
            cx,
        )
        .await
    }

    pub(super) async fn complete_prepared_interrupted_exit_threadless(
        owner: &impl RecoveryOwnerAccess,
        request: &impl RecoveryIdentity,
        retired: HomeGeneration,
        window: WindowHandle<MainWindowShellRoot>,
        cancellation: CommandCancellation,
        cx: &mut AsyncApp,
    ) -> Result<(), String> {
        let _driver = owner
            .recovery_owner()?
            .borrow_mut()
            .reserve_interrupted_exit_driver(request)?;
        Self::complete_prepared_interrupted_exit_threadless_pass(
            owner,
            request,
            retired,
            window,
            cancellation,
            cx,
        )
        .await
    }

    async fn complete_prepared_interrupted_exit_threadless_pass(
        owner: &impl RecoveryOwnerAccess,
        request: &impl RecoveryIdentity,
        retired: HomeGeneration,
        window: WindowHandle<MainWindowShellRoot>,
        cancellation: CommandCancellation,
        cx: &mut AsyncApp,
    ) -> Result<(), String> {
        let (retired_home, generation, appearance) = cx
            .update(|app| -> Result<_, String> {
                if cancellation.is_cancelled() {
                    return Err("Interrupted Exit preparation was cancelled".into());
                }
                let retained_owner = owner.recovery_owner()?;
                let mut owner = retained_owner.borrow_mut();
                let prepared = owner.interrupted_exit_appearance(request)?;
                if owner
                    .interrupted_exit
                    .as_ref()
                    .unwrap()
                    .threadless_appearance
                    .is_some()
                {
                    return Err("Interrupted Exit fresh appearance is already retained".into());
                }
                let shells = owner.process.windows.shells();
                if shells.len() != 1 || shells[0].window() != window {
                    return Err(
                        "Interrupted Exit requires the sole retained threadless window".into(),
                    );
                }
                if !window
                    .read(app)
                    .map_err(|error| error.to_string())?
                    .controller()
                    .is_some_and(|controller| controller.is_threadless())
                {
                    return Err("Interrupted Exit requires a threadless window".into());
                }
                let retired_home = prepared.prepared().home().home_id();
                owner
                    .process
                    .services
                    .as_ref()
                    .ok_or("The complete service owner is on a worker")?
                    .validate_retired_service_home_return(retired, Some(retired_home))
                    .map_err(|error| error.to_string())?;
                let generation = prepared.prepared().home().home_generation();
                let previous = owner.process.appearance.clone();
                let capacity = NonZeroUsize::new(previous.read(app).target().snapshot().capacity)
                    .ok_or("Interrupted Exit appearance capacity is unavailable")?;
                previous.update(app, |set, _| set.retire());
                let appearance = GpuiAppearanceWindowSet::new(prepared, capacity, app);
                owner
                    .interrupted_exit
                    .as_mut()
                    .unwrap()
                    .threadless_appearance = Some(appearance.clone());
                Ok((retired_home, generation, appearance))
            })
            .map_err(|error| error.to_string())??;
        Self::attach_interrupted_exit_threadless_pass(
            owner,
            request,
            retired_home,
            retired,
            window,
            &appearance,
            cancellation.clone(),
            cx,
        )
        .await?;
        Self::publish_and_complete_interrupted_exit_pass(
            owner,
            request,
            retired,
            generation,
            &appearance,
            cancellation,
            cx,
        )
        .await
    }

    pub(super) async fn continue_interrupted_exit_threadless(
        owner: &impl RecoveryOwnerAccess,
        request: &impl RecoveryIdentity,
        retired_home: beryl_model::BerylHomeId,
        retired: HomeGeneration,
        generation: HomeGeneration,
        window: WindowHandle<MainWindowShellRoot>,
        cancellation: CommandCancellation,
        cx: &mut AsyncApp,
    ) -> Result<(), String> {
        let appearance = owner
            .recovery_owner()?
            .borrow_mut()
            .interrupted_exit_threadless_appearance(request)?;
        Self::attach_and_complete_interrupted_exit_threadless(
            owner,
            request,
            retired_home,
            retired,
            generation,
            window,
            &appearance,
            cancellation,
            cx,
        )
        .await
    }

    #[cfg(test)]
    pub(crate) fn test_threadless_recovery_appearance(
        &self,
    ) -> Option<Entity<GpuiAppearanceWindowSet>> {
        self.interrupted_exit
            .as_ref()?
            .threadless_appearance
            .clone()
    }
}
