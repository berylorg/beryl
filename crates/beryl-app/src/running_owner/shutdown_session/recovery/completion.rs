use super::*;
use crate::{
    main_window::MainWindowShellRoot, running_owner::progress::RunningShutdownProgress,
    startup_owner::RunningExitGate, theme_runtime::AppearancePublicationTarget,
};

impl RunningProcessOwner {
    #[cfg(test)]
    pub(crate) fn test_interrupted_exit_start_gate(
        &self,
    ) -> std::sync::Arc<crate::cas_projection::initial_start::InitialStartGate> {
        self.interrupted_exit
            .as_ref()
            .unwrap()
            .publication
            .borrow()
            .as_ref()
            .unwrap()
            .as_ref()
            .unwrap()
            .gate()
    }

    pub(crate) fn complete_interrupted_exit(
        owner: &Rc<RefCell<Self>>,
        request: &impl RecoveryIdentity,
        app: &mut App,
    ) -> Result<bool, String> {
        let wake = {
            let mut owner = owner.borrow_mut();
            owner.interrupted_exit_theme_activation_result(request)?;
            let appearance = owner.process.appearance.clone();
            owner.validate_interrupted_exit_bindings(request, &appearance, app)?;
            if owner.confirmation.is_some()
                || owner.observing_initial_work
                || matches!(owner.progress, Some(RunningShutdownProgress::Polling))
            {
                return Err("Interrupted Exit process work has not settled".into());
            }
            let generation = appearance
                .read(app)
                .target()
                .snapshot()
                .current
                .prepared()
                .home()
                .home_generation();
            let publication = owner.interrupted_exit.as_ref().unwrap().publication.clone();
            let mut publication = publication
                .try_borrow_mut()
                .map_err(|_| "Interrupted Exit publication is busy")?;
            let drafts = owner.recovery_drafts()?;
            let windows = owner
                .process
                .windows
                .shells()
                .iter()
                .map(|shell| shell.window())
                .collect::<Vec<_>>();
            let creation = crate::main_window::MainWindowCreationOwner::prepare_recovered_process(
                owner
                    .process
                    .services
                    .as_ref()
                    .ok_or("Recovered creation services are unavailable")?,
                &appearance,
                app,
            )?;
            let released = {
                let drafts = drafts
                    .try_borrow()
                    .map_err(|_| "Interrupted Exit drafts are busy")?;
                let process = &mut owner.process;
                drafts.release_recovered_mounts_after(&process.windows, &appearance, app, || {
                    process
                        .services
                        .as_mut()
                        .ok_or("Published recovery services are unavailable")?
                        .reopen_recovery_admission(generation)?;
                    if let Some(creation) = &creation {
                        creation.require_live_source()?;
                    }
                    Ok(())
                })?
            };
            if !released {
                return Ok(false);
            }
            if let Some(creation) = creation {
                creation.apply(app);
            }
            // Prepared mounts are now live; no GUI turn intervenes before the remaining gates.
            #[cfg(target_os = "windows")]
            for shell in owner.process.windows.shells_mut() {
                shell
                    .release_settled_nonfinal_native_if_present()
                    .expect("prepared recovery native survival remains settled");
                shell
                    .release_pre_native_close_if_present(&request.identity(), app)
                    .expect("prepared recovery pre-native custody remains exact");
            }
            MainWindowShellRoot::release_shutdown_interaction_gates(&windows, app)
                .expect("prepared recovered shells remain live during synchronous completion");
            for window in &windows {
                window
                    .update(app, |root, window, cx| {
                        root.set_notices_inert(false, window, cx)
                    })
                    .expect("prepared recovery window remains live during synchronous completion");
            }
            for (handle, focus) in &owner.interrupted_exit.as_ref().unwrap().focus {
                handle
                    .update(app, |_, window, _| focus.focus(window))
                    .expect("captured recovered focus remains owned by the surviving shell");
            }
            if request.lifecycle().is_none() {
                if let Some(observed) = &mut owner.observed_home_failure {
                    observed.focus.clear();
                }
            }
            let start = publication
                .take()
                .expect("validated recovery publication")
                .expect("successful recovery publication");
            owner.shutdown = None;
            owner.progress = None;
            owner.interrupted_exit = None;
            owner
                .process
                .commands
                .set_gate(RunningExitGate::HomeUnavailable, false);
            let wake = request.lifecycle().and_then(|request| {
                owner
                    .process
                    .commands
                    .finish_exit_deferred_wake(request)
                    .expect("validated cancelled Exit remains active during synchronous completion")
            });
            assert!(
                start.release(),
                "published recovery workers retain their start gate"
            );
            wake
        };
        if let Some(wake) = wake {
            wake.wake();
        }
        Ok(true)
    }
}
