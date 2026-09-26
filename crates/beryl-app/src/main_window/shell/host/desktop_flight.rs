use super::{App, CommandCancellation, MainWindowShell};
use crate::main_window::{WindowsDesktopPlacementOutcome, prepare_windows_desktop_placement};

#[derive(Debug)]
pub enum MainWindowDesktopPlacementRejection {
    Cancelled,
    AlreadyPublished,
    AlreadyEnrolled,
    MissingController,
    LeaseAdmission(String),
    NativeClose,
    NativeWindowLost,
    NativeSettlement(String),
}

pub struct MainWindowDesktopPlacementStartFailure {
    pub shell: MainWindowShell,
    pub reason: MainWindowDesktopPlacementRejection,
}

pub enum MainWindowDesktopPlacementCompletion {
    Ready {
        shell: MainWindowShell,
        outcome: WindowsDesktopPlacementOutcome,
    },
    Rejected {
        shell: MainWindowShell,
        reason: MainWindowDesktopPlacementRejection,
    },
}

enum PlacementAdmissionState {
    Pending,
    Ready,
    Rejected { settlement_proven: bool },
}

pub(super) struct ShellDesktopPlacementAdmission {
    cancellation: CommandCancellation,
    state: PlacementAdmissionState,
}

impl MainWindowShell {
    pub fn desktop_placement_ready(&self) -> bool {
        self.desktop_placement.as_ref().is_some_and(|admission| {
            matches!(admission.state, PlacementAdmissionState::Ready)
                && !admission.cancellation.is_cancelled()
        })
    }

    pub(super) fn desktop_publication_allowed(&self) -> bool {
        self.desktop_placement.is_none() || self.desktop_placement_ready()
    }

    pub(super) fn desktop_native_publication_allowed(&self, app: &mut App) -> bool {
        self.desktop_placement.is_none()
            || self
                .window
                .update(app, |_, window, _| !window.windows_native_close_requested())
                .unwrap_or(false)
    }

    pub(super) fn desktop_cleanup_allowed(&self) -> bool {
        self.desktop_placement.as_ref().is_none_or(|admission| {
            matches!(
                admission.state,
                PlacementAdmissionState::Ready
                    | PlacementAdmissionState::Rejected {
                        settlement_proven: true
                    }
            )
        })
    }

    #[cfg(feature = "test-faults")]
    pub fn test_hold_desktop_worker(&mut self, gate: std::sync::mpsc::Receiver<()>) {
        self.desktop_worker_gate = Some(gate);
    }

    pub fn start_desktop_placement(
        mut self,
        cancellation: CommandCancellation,
        completion: impl FnOnce(MainWindowDesktopPlacementCompletion, &mut App) + 'static,
        app: &mut App,
    ) -> Result<(), MainWindowDesktopPlacementStartFailure> {
        let failure = |shell, reason| MainWindowDesktopPlacementStartFailure { shell, reason };
        if self.published {
            return Err(failure(
                self,
                MainWindowDesktopPlacementRejection::AlreadyPublished,
            ));
        }
        if self.desktop_placement.is_some() {
            return Err(failure(
                self,
                MainWindowDesktopPlacementRejection::AlreadyEnrolled,
            ));
        }
        if cancellation.is_cancelled() {
            self.desktop_placement = Some(ShellDesktopPlacementAdmission {
                cancellation,
                state: PlacementAdmissionState::Rejected {
                    settlement_proven: true,
                },
            });
            return Err(failure(
                self,
                MainWindowDesktopPlacementRejection::Cancelled,
            ));
        }
        let Some(saved) = self
            .root
            .read(app)
            .controller
            .as_ref()
            .map(|controller| controller.placement().virtual_desktop())
        else {
            return Err(failure(
                self,
                MainWindowDesktopPlacementRejection::MissingController,
            ));
        };
        let lease = self
            .window
            .update(app, |_, window, _| window.lease_hidden_windows_window());
        let (lease, released) = match lease {
            Ok(Ok(lease)) => lease,
            Ok(Err(error)) => {
                return Err(failure(
                    self,
                    MainWindowDesktopPlacementRejection::LeaseAdmission(error.to_string()),
                ));
            }
            Err(_) => {
                return Err(failure(
                    self,
                    MainWindowDesktopPlacementRejection::NativeWindowLost,
                ));
            }
        };
        self.desktop_placement = Some(ShellDesktopPlacementAdmission {
            cancellation,
            state: PlacementAdmissionState::Pending,
        });
        #[cfg(feature = "test-faults")]
        let gate = self.desktop_worker_gate.take();
        let worker = app.background_executor().spawn(async move {
            #[cfg(feature = "test-faults")]
            if let Some(gate) = gate {
                let _ = gate.recv_timeout(std::time::Duration::from_secs(10));
            }
            prepare_windows_desktop_placement(lease, saved)
        });
        app.spawn(async move |cx| {
            let outcome = worker.await;
            let settlement = released.await;
            cx.update(move |app| {
                let reason = match settlement {
                    Err(error) => Some(MainWindowDesktopPlacementRejection::NativeSettlement(
                        error.to_string(),
                    )),
                    Ok(settled) => {
                        if self
                            .desktop_placement
                            .as_ref()
                            .expect("admitted desktop flight")
                            .cancellation
                            .is_cancelled()
                        {
                            Some(MainWindowDesktopPlacementRejection::Cancelled)
                        } else if settled.close_requested {
                            Some(MainWindowDesktopPlacementRejection::NativeClose)
                        } else if settled.native_destroyed {
                            Some(MainWindowDesktopPlacementRejection::NativeWindowLost)
                        } else {
                            match self
                                .window
                                .update(app, |_, window, _| window.windows_native_close_requested())
                            {
                                Ok(false) => None,
                                Ok(true) => Some(MainWindowDesktopPlacementRejection::NativeClose),
                                Err(_) => {
                                    Some(MainWindowDesktopPlacementRejection::NativeWindowLost)
                                }
                            }
                        }
                    }
                };
                let admission = self
                    .desktop_placement
                    .as_mut()
                    .expect("admitted desktop flight");
                let result = if let Some(reason) = reason {
                    admission.state = PlacementAdmissionState::Rejected {
                        settlement_proven: !matches!(
                            reason,
                            MainWindowDesktopPlacementRejection::NativeSettlement(_)
                        ),
                    };
                    MainWindowDesktopPlacementCompletion::Rejected {
                        shell: self,
                        reason,
                    }
                } else {
                    admission.state = PlacementAdmissionState::Ready;
                    MainWindowDesktopPlacementCompletion::Ready {
                        shell: self,
                        outcome,
                    }
                };
                completion(result, app);
            })
            .expect("desktop flight retains a live GUI executor through completion");
        })
        .detach();
        Ok(())
    }
}
