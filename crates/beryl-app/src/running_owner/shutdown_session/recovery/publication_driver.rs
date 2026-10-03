use super::*;
use crate::main_window::MainWindowShellRoot;
use beryl_home_store::{CommandCancellation, HomeGeneration};
use gpui::{AsyncApp, Entity};
use std::time::Duration;

impl RunningProcessOwner {
    pub(crate) async fn attach_and_complete_interrupted_exit_threadless(
        owner: &impl RecoveryOwnerAccess,
        request: &impl RecoveryIdentity,
        retired_home: beryl_model::BerylHomeId,
        retired: HomeGeneration,
        generation: HomeGeneration,
        window: gpui::WindowHandle<MainWindowShellRoot>,
        appearance: &Entity<crate::theme_runtime::GpuiAppearanceWindowSet>,
        cancellation: CommandCancellation,
        cx: &mut AsyncApp,
    ) -> Result<(), String> {
        let _driver = owner
            .recovery_owner()?
            .borrow_mut()
            .reserve_interrupted_exit_driver(request)?;
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

    pub(crate) async fn publish_and_complete_interrupted_exit(
        owner: &impl RecoveryOwnerAccess,
        request: &impl RecoveryIdentity,
        retired: HomeGeneration,
        generation: HomeGeneration,
        appearance: &Entity<crate::theme_runtime::GpuiAppearanceWindowSet>,
        cancellation: CommandCancellation,
        cx: &mut AsyncApp,
    ) -> Result<(), String> {
        let _driver = owner
            .recovery_owner()?
            .borrow_mut()
            .reserve_interrupted_exit_driver(request)?;
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

    pub(super) async fn publish_and_complete_interrupted_exit_pass(
        owner: &impl RecoveryOwnerAccess,
        request: &impl RecoveryIdentity,
        retired: HomeGeneration,
        generation: HomeGeneration,
        appearance: &Entity<crate::theme_runtime::GpuiAppearanceWindowSet>,
        cancellation: CommandCancellation,
        cx: &mut AsyncApp,
    ) -> Result<(), String> {
        loop {
            let released = cx
                .update(|app| {
                    if cancellation.is_cancelled() {
                        return Err("Interrupted Exit publication was cancelled".into());
                    }
                    owner
                        .recovery_owner()?
                        .borrow()
                        .release_interrupted_exit_drafts(request, appearance, app)
                })
                .map_err(|error| error.to_string())??;
            if released {
                break;
            }
            cx.background_executor()
                .timer(Duration::from_millis(50))
                .await;
        }
        let (sender, receiver) = futures_channel::oneshot::channel();
        cx.update(|app| {
            Self::publish_interrupted_exit_services(
                &owner.recovery_owner()?,
                request,
                retired,
                generation,
                appearance,
                cancellation.clone(),
                app,
                move |_, result, _| {
                    let _ = sender.send(result);
                },
            )
        })
        .map_err(|error| error.to_string())??;
        receiver
            .await
            .map_err(|_| "Interrupted Exit publication delivery is unavailable")??;

        Self::activate_and_complete_interrupted_exit_pass(
            owner,
            request,
            appearance,
            cancellation,
            cx,
        )
        .await
    }

    pub(crate) async fn activate_and_complete_interrupted_exit(
        owner: &impl RecoveryOwnerAccess,
        request: &impl RecoveryIdentity,
        appearance: &Entity<crate::theme_runtime::GpuiAppearanceWindowSet>,
        cancellation: CommandCancellation,
        cx: &mut AsyncApp,
    ) -> Result<(), String> {
        let _driver = owner
            .recovery_owner()?
            .borrow_mut()
            .reserve_interrupted_exit_driver(request)?;
        Self::activate_and_complete_interrupted_exit_pass(
            owner,
            request,
            appearance,
            cancellation,
            cx,
        )
        .await
    }

    async fn activate_and_complete_interrupted_exit_pass(
        owner: &impl RecoveryOwnerAccess,
        request: &impl RecoveryIdentity,
        appearance: &Entity<crate::theme_runtime::GpuiAppearanceWindowSet>,
        cancellation: CommandCancellation,
        cx: &mut AsyncApp,
    ) -> Result<(), String> {
        let (sender, receiver) = futures_channel::oneshot::channel();
        cx.update(|app| {
            Self::activate_interrupted_exit_theme(
                &owner.recovery_owner()?,
                request,
                appearance,
                cancellation.clone(),
                app,
                move |_, result, _| {
                    let _ = sender.send(result);
                },
            )
        })
        .map_err(|error| error.to_string())??;
        receiver
            .await
            .map_err(|_| "Interrupted Exit theme activation delivery is unavailable")??;

        Self::bind_and_complete_interrupted_exit_pass(owner, request, appearance, cancellation, cx)
            .await
    }

    pub(crate) async fn bind_and_complete_interrupted_exit(
        owner: &impl RecoveryOwnerAccess,
        request: &impl RecoveryIdentity,
        appearance: &Entity<crate::theme_runtime::GpuiAppearanceWindowSet>,
        cancellation: CommandCancellation,
        cx: &mut AsyncApp,
    ) -> Result<(), String> {
        let _driver = owner
            .recovery_owner()?
            .borrow_mut()
            .reserve_interrupted_exit_driver(request)?;
        Self::bind_and_complete_interrupted_exit_pass(owner, request, appearance, cancellation, cx)
            .await
    }

    async fn bind_and_complete_interrupted_exit_pass(
        owner: &impl RecoveryOwnerAccess,
        request: &impl RecoveryIdentity,
        appearance: &Entity<crate::theme_runtime::GpuiAppearanceWindowSet>,
        cancellation: CommandCancellation,
        cx: &mut AsyncApp,
    ) -> Result<(), String> {
        cx.update(|app| {
            if cancellation.is_cancelled() {
                return Err("Interrupted Exit completion was cancelled".into());
            }
            let retained_owner = owner.recovery_owner()?;
            let mut running = retained_owner.borrow_mut();
            running.interrupted_exit_theme_activation_result(request)?;
            running.bind_interrupted_exit_process(request, appearance, app)
        })
        .map_err(|error| error.to_string())??;
        Self::await_interrupted_exit_completion_pass(owner, request, cancellation, cx).await
    }

    pub(crate) async fn await_interrupted_exit_completion(
        owner: &impl RecoveryOwnerAccess,
        request: &impl RecoveryIdentity,
        cancellation: CommandCancellation,
        cx: &mut AsyncApp,
    ) -> Result<(), String> {
        let _driver = owner
            .recovery_owner()?
            .borrow_mut()
            .reserve_interrupted_exit_driver(request)?;
        Self::await_interrupted_exit_completion_pass(owner, request, cancellation, cx).await
    }

    async fn await_interrupted_exit_completion_pass(
        owner: &impl RecoveryOwnerAccess,
        request: &impl RecoveryIdentity,
        cancellation: CommandCancellation,
        cx: &mut AsyncApp,
    ) -> Result<(), String> {
        loop {
            let complete = cx
                .update(|app| {
                    if cancellation.is_cancelled() {
                        return Err("Interrupted Exit completion was cancelled".into());
                    }
                    Self::complete_interrupted_exit(&owner.recovery_owner()?, request, app)
                })
                .map_err(|error| error.to_string())??;
            if complete {
                return Ok(());
            }
            cx.background_executor()
                .timer(Duration::from_millis(50))
                .await;
        }
    }
}
