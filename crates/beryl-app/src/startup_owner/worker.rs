use super::*;
use crate::{
    app_services::AppServiceShutdownProgress,
    cas_projection::ProjectionCancellationToken,
    main_window::{
        MainWindowRestoreSet, MainWindowRestoreSetOutcome, PreparedNativeMainWindowRestoreSet,
    },
    theme_runtime::AppearanceGeneration,
};
use beryl_home_store::HomeHealthState;
use std::{
    sync::Mutex,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use syndic_storage::SyndicTimestamp;

#[derive(Clone)]
pub(super) struct PreparationCancellation(Arc<Mutex<CommandCancellation>>);

impl PreparationCancellation {
    pub(super) fn new() -> Self {
        Self(Arc::new(Mutex::new(CommandCancellation::new())))
    }

    pub(super) fn cancel(&self) {
        self.0.lock().expect("startup cancellation").cancel();
    }

    fn current(&self) -> CommandCancellation {
        self.0.lock().expect("startup cancellation").clone()
    }

    fn replace(&self, next: CommandCancellation) {
        let mut current = self.0.lock().expect("startup cancellation");
        if current.is_cancelled() {
            next.cancel();
        }
        *current = next;
    }
}

pub(super) struct Worker {
    pub(super) services: Option<ProcessServiceOwner>,
    pub(super) failed_open: Option<beryl_home_store::HomeCloseError>,
    pub(super) retained_restore: Option<MainWindowRestoreSet>,
}

pub(super) enum Preparation {
    Ready {
        native: PreparedNativeMainWindowRestoreSet,
        appearance: Arc<AppearanceGeneration>,
    },
    Busy,
    Failed {
        detail: String,
        blocked: bool,
    },
}

impl Worker {
    pub(super) fn new() -> Self {
        Self {
            services: None,
            failed_open: None,
            retained_restore: None,
        }
    }

    pub(super) fn prepare(
        &mut self,
        input: &StartupConfiguration,
        cancellation: &PreparationCancellation,
    ) -> Preparation {
        if cancellation.current().is_cancelled() {
            return self.failure("Startup cancelled".to_owned());
        }
        let (candidate, state, syndic) = match (input.open)(&input.home, cancellation.current()) {
            StartupHomeOpen::Ready {
                candidate,
                state,
                syndic,
            } => (candidate, state, syndic),
            StartupHomeOpen::Busy => return Preparation::Busy,
            StartupHomeOpen::Failed { detail, retained } => {
                self.failed_open = retained;
                return self.failure(detail);
            }
        };
        let owner = self.services.get_or_insert_with(|| {
            ProcessServiceOwner::new(
                candidate.home_id(),
                input.enrollment_slots,
                input.settlement_slots,
            )
        });
        #[cfg(test)]
        if let Some(hook) = &input.service_hook {
            hook(owner);
        }
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default();
        let at =
            SyndicTimestamp::from_unix_millis(u64::try_from(now.as_millis()).unwrap_or(u64::MAX));
        if let Err(failure) = owner.open_initial(
            candidate,
            state,
            syndic,
            input.services.clone(),
            at,
            cancellation.current(),
        ) {
            let detail = failure.to_string();
            if let Some(candidate) = failure.rejected_candidate {
                if let Err(error) = candidate.close() {
                    self.failed_open = Some(error);
                }
            }
            return self.failure(detail);
        }
        let (appearance, mut work) = match self.restore(input) {
            Ok(value) => value,
            Err(error) => return self.failure(error),
        };
        cancellation.replace(work.cancellation());
        let mut native_preparation_error = None;
        loop {
            match work.advance() {
                MainWindowRestoreSetOutcome::Pending(next) => {
                    if let Some(error) = next.cleanup_error() {
                        let detail = error.to_owned();
                        self.retained_restore = Some(next);
                        return self.failure(detail);
                    }
                    work = next;
                    std::thread::sleep(Duration::from_millis(10));
                }
                MainWindowRestoreSetOutcome::Retained { custody, reason } => {
                    self.retained_restore = Some(custody);
                    return self.failure(format!("Startup command cleanup is blocked: {reason:?}"));
                }
                MainWindowRestoreSetOutcome::Failed { error } => {
                    return self.failure(native_preparation_error.unwrap_or(error));
                }
                MainWindowRestoreSetOutcome::Prepared(prepared) => {
                    if cancellation.current().is_cancelled() {
                        work = prepared.dispose();
                        continue;
                    }
                    match prepared.prepare_native() {
                        Ok(native) => {
                            #[cfg(test)]
                            let native = {
                                let mut native = native;
                                if let Some(hook) = &input.native_hook {
                                    hook(&mut native);
                                }
                                native
                            };
                            return Preparation::Ready { native, appearance };
                        }
                        Err(failure) => {
                            native_preparation_error = Some(failure.error);
                            work = failure.prepared.dispose();
                        }
                    }
                }
            }
        }
    }

    fn restore(
        &mut self,
        input: &StartupConfiguration,
    ) -> Result<(Arc<AppearanceGeneration>, MainWindowRestoreSet), String> {
        let services = self.services.as_mut().expect("published startup services");
        let graph = services.graph_mut().expect("published startup graph");
        graph
            .release_theme()
            .map_err(|e| format!("Theme loading failed: {e:?}"))?;
        let appearance = graph
            .theme()
            .and_then(|theme| theme.current())
            .ok_or("Startup appearance is unavailable")?;
        let work = services
            .window_services(input.windows.clone())?
            .into_restore_set(
                appearance.clone(),
                input.initial_window,
                input.initial_placement.clone(),
            )?;
        Ok((appearance, work))
    }

    pub(super) fn failure(&mut self, mut detail: String) -> Preparation {
        let blocked = match self.close() {
            Ok(()) => false,
            Err(error) => {
                detail.push_str("\nCleanup could not finish: ");
                detail.push_str(&error);
                true
            }
        };
        Preparation::Failed {
            detail: crate::startup_surface::bounded_detail(&detail),
            blocked,
        }
    }

    pub(super) fn close(&mut self) -> Result<(), String> {
        if self.failed_open.is_some() || self.retained_restore.is_some() {
            return Err("original home or restore-command custody remains retained".to_owned());
        }
        let Some(owner) = self.services.as_mut() else {
            return Ok(());
        };
        if owner.graph().is_none() {
            return if owner.initial_attempt_is_settled() {
                Ok(())
            } else {
                Err("service retirement remains unproven".to_owned())
            };
        }
        if owner.graph().unwrap().home().health().state() == HomeHealthState::Failed {
            return owner.retire_failed_startup().map_err(|e| e.to_string());
        }
        if let Err(error) = owner.begin_shutdown() {
            return Self::failed_during_shutdown(owner, error.to_string());
        }
        let cancellation = ProjectionCancellationToken::new();
        loop {
            match owner.poll_shutdown(&cancellation) {
                Ok(AppServiceShutdownProgress::Waiting) => {
                    std::thread::sleep(Duration::from_millis(10))
                }
                Ok(AppServiceShutdownProgress::Ready) => {
                    return owner.finish_shutdown().map_err(|e| e.to_string());
                }
                Ok(AppServiceShutdownProgress::Failed { reason, .. }) => {
                    return Self::failed_during_shutdown(owner, format!("{reason:?}"));
                }
                Err(error) => return Self::failed_during_shutdown(owner, error.to_string()),
            }
        }
    }

    fn failed_during_shutdown(
        owner: &mut ProcessServiceOwner,
        error: String,
    ) -> Result<(), String> {
        if owner
            .graph()
            .is_some_and(|graph| graph.home().health().state() == HomeHealthState::Failed)
        {
            owner.retire_failed_startup().map_err(|e| e.to_string())
        } else {
            Err(error)
        }
    }
}
