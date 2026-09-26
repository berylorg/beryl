use super::*;
use std::{future::Future, pin::Pin, task::Poll};

pub(super) struct ShellStartupDisposalAdmission {
    receipt: Option<Pin<Box<gpui::WindowsNativeWindowDestroyed>>>,
    preserve_records: bool,
    started: bool,
}

pub struct MainWindowStartupDisposalFailure {
    pub shell: MainWindowShell,
    pub error: String,
}

pub enum MainWindowStartupDisposalCompletion {
    Ready {
        retirement: MainWindowStartupRetirement,
    },
    Rejected(MainWindowStartupDisposalFailure),
}

pub enum MainWindowStartupRetirement {
    AcquiredUnpublished(MainWindowShellUnpublished),
    AcquiredPreserved(MainWindowShellRecordPreservingRetirement),
    Restored(RestoredWindowShellUnpublished),
    Threadless,
}

impl MainWindowShell {
    pub fn enroll_startup_disposal(&mut self, app: &mut App) -> Result<(), String> {
        if self.published
            || self.startup_disposal.is_some()
            || !self.root.read(app).startup_interaction_gated()
        {
            return Err("startup disposal requires an unenrolled hidden gated shell".to_owned());
        }
        self.startup_disposal = Some(ShellStartupDisposalAdmission {
            receipt: None,
            preserve_records: false,
            started: false,
        });
        let receipt = self
            .window
            .update(app, |_, window, _| {
                window.observe_windows_native_destruction()
            })
            .map_err(|error| error.to_string())?
            .map_err(|error| error.to_string())?;
        self.startup_disposal.as_mut().unwrap().receipt = Some(Box::pin(receipt));
        Ok(())
    }

    pub fn seal_startup_publication(&mut self) -> Result<(), String> {
        let Some(admission) = self.startup_disposal.as_mut() else {
            return Err("startup shell has no native destruction enrollment".to_owned());
        };
        if admission.started || admission.receipt.is_none() {
            return Err("startup shell has no available native destruction receipt".to_owned());
        }
        admission.preserve_records = true;
        Ok(())
    }

    pub(super) fn startup_publication_allowed(&self) -> bool {
        self.startup_disposal.as_ref().is_none_or(|admission| {
            admission.preserve_records && !admission.started && admission.receipt.is_some()
        })
    }

    pub fn start_startup_disposal(
        mut self,
        completion: impl FnOnce(MainWindowStartupDisposalCompletion, &mut App) + 'static,
        app: &mut App,
    ) -> Result<(), MainWindowStartupDisposalFailure> {
        let failure = |shell, error: &str| MainWindowStartupDisposalFailure {
            shell,
            error: error.to_owned(),
        };
        if !self.desktop_cleanup_allowed() {
            return Err(failure(self, "startup desktop work is not settled"));
        }
        if !self.root.read(app).startup_interaction_gated() {
            return Err(failure(
                self,
                "startup disposal requires closed interaction admission",
            ));
        }
        if self.startup_disposal.as_ref().is_none_or(|admission| {
            admission.started
                || admission.receipt.is_none()
                || (self.published && !admission.preserve_records)
        }) {
            return Err(failure(
                self,
                "startup disposal has no available exact destruction receipt",
            ));
        }
        let composer = match self.root.read(app).controller.as_ref() {
            Some(controller) if controller.is_threadless() => None,
            Some(controller) => {
                let composer = controller
                    .composer_mount
                    .as_ref()
                    .and_then(|mount| mount.read(app).contribution());
                let Some(composer) = composer else {
                    return Err(failure(self, "startup disposal lost its selected editor"));
                };
                Some(composer)
            }
            None => return Err(failure(self, "startup disposal lost its controller")),
        };
        let release = match composer {
            Some(composer) => {
                match self.window.update(app, |_, window, app| {
                    composer.update(app, |composer, cx| {
                        composer.release_startup_widget(window, cx)
                    })
                }) {
                    Ok(Ok(release)) => Some(Box::pin(release)),
                    Ok(Err(error)) => {
                        return Err(MainWindowStartupDisposalFailure { shell: self, error });
                    }
                    Err(error) => return Err(failure(self, &error.to_string())),
                }
            }
            None => None,
        };
        let admission = self.startup_disposal.as_mut().unwrap();
        admission.started = true;
        let preserve_records = admission.preserve_records;
        let mut receipt = admission.receipt.take().unwrap();
        app.spawn(async move |cx| {
            let mut native_settled = false;
            let release_result = match release {
                Some(mut release) => std::future::poll_fn(|task| {
                    if let Poll::Ready(result) = receipt.as_mut().poll(task) {
                        native_settled = true;
                        return Poll::Ready(Err(match result {
                            Ok(()) => "startup native window was destroyed before editor release"
                                .to_owned(),
                            Err(error) => format!(
                                "startup native destruction failed before editor release: {error}"
                            ),
                        }));
                    }
                    release.as_mut().poll(task).map(|result| result.map(|_| ()))
                })
                .await,
                None => Ok(()),
            };
            if let Err(error) = release_result {
                if !native_settled {
                    self.startup_disposal.as_mut().unwrap().receipt = Some(receipt);
                }
                cx.update(|app| {
                    completion(
                        MainWindowStartupDisposalCompletion::Rejected(
                            MainWindowStartupDisposalFailure { shell: self, error },
                        ),
                        app,
                    );
                })
                .expect("startup disposal retains a live GUI executor through completion");
                return;
            }
            let removal = cx
                .update(|app| {
                    self.appearance_owner
                        .update(app, |owner, _| owner.unregister(self.adapter_id))
                        .map_err(|error| error.to_string())?;
                    self.window
                        .update(app, |root, window, cx| {
                            root.retire_notices(window, cx);
                            window.remove_window();
                        })
                        .map_err(|error| error.to_string())
                })
                .expect("startup disposal retains a live GUI executor through removal");
            if let Err(error) = removal {
                self.startup_disposal.as_mut().unwrap().receipt = Some(receipt);
                cx.update(|app| {
                    completion(
                        MainWindowStartupDisposalCompletion::Rejected(
                            MainWindowStartupDisposalFailure { shell: self, error },
                        ),
                        app,
                    );
                })
                .expect("startup disposal retains a live GUI executor through failure");
                return;
            }
            let result = receipt.await;
            cx.update(|app| {
                let result = match result {
                    Err(error) => MainWindowStartupDisposalCompletion::Rejected(
                        MainWindowStartupDisposalFailure {
                            shell: self,
                            error: error.to_string(),
                        },
                    ),
                    Ok(()) => {
                        let controller = self.root.update(app, |root, _| root.controller.take());
                        match controller {
                            Some(controller) => {
                                let retirement = match controller.content {
                                    ShellContent::Acquired { custody, .. } if preserve_records => {
                                        MainWindowStartupRetirement::AcquiredPreserved(
                                            custody.preserve_records(),
                                        )
                                    }
                                    ShellContent::Acquired { custody, .. } => {
                                        MainWindowStartupRetirement::AcquiredUnpublished(custody)
                                    }
                                    ShellContent::Restored { custody, .. } => {
                                        MainWindowStartupRetirement::Restored(*custody)
                                    }
                                    ShellContent::Threadless { reservation, .. } => {
                                        drop(reservation);
                                        MainWindowStartupRetirement::Threadless
                                    }
                                };
                                MainWindowStartupDisposalCompletion::Ready { retirement }
                            }
                            None => MainWindowStartupDisposalCompletion::Rejected(
                                MainWindowStartupDisposalFailure {
                                    shell: self,
                                    error: "startup disposal lost its original controller"
                                        .to_owned(),
                                },
                            ),
                        }
                    }
                };
                completion(result, app);
            })
            .expect("startup disposal retains a live GUI executor through native completion");
        })
        .detach();
        Ok(())
    }
}
