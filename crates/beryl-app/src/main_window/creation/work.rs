use super::*;

pub(super) enum CreationState {
    Request {
        target: RememberedTarget,
        reservation: RuntimeBackedWindowMainWindowReservation,
    },
    Acquire {
        request: RuntimeBackedWindowAcquisitionRequest,
        reservation: RuntimeBackedWindowMainWindowReservation,
    },
    AcquisitionPending {
        reconciliation: RuntimeBackedWindowAcquisitionReconciliation,
        reservation: RuntimeBackedWindowMainWindowReservation,
    },
    RepairPending {
        request: RuntimeBackedWindowAcquisitionRequest,
        reconciliation: RuntimeBackedWindowAcquisitionRepairReconciliation,
        reservation: RuntimeBackedWindowMainWindowReservation,
    },
    Acquired {
        acquisition: RuntimeBackedWindowAcquisition,
        reservation: RuntimeBackedWindowMainWindowReservation,
    },
    Initial(MainWindowInitialComposer),
    Retiring(MainWindowInitialComposer),
    Unpublished(MainWindowShellUnpublished),
    Abandonment(MainWindowShellAbandonment),
    AbandonmentPending(MainWindowShellAbandonmentReconciliation),
    Settled,
}

impl MainWindowCreation {
    pub fn advance(
        mut self,
        appearance: Arc<crate::theme_runtime::AppearanceGeneration>,
    ) -> MainWindowCreationOutcome {
        for _ in 0..16 {
            let state = std::mem::replace(&mut self.state, CreationState::Settled);
            match state {
                CreationState::Request {
                    target,
                    reservation,
                } => {
                    if self.cancellation.is_cancelled() {
                        self.error = Some("New Window creation was cancelled.".to_owned());
                        continue;
                    }
                    match (self.services.request_source)(self.window_id, target) {
                        Ok(request)
                            if request.window_id() == self.window_id
                                && request.target() == target =>
                        {
                            self.state = CreationState::Acquire {
                                request,
                                reservation,
                            };
                        }
                        Ok(_) => {
                            self.error =
                                Some("New Window request changed its captured target.".to_owned())
                        }
                        Err(error) => self.error = Some(error),
                    }
                }
                CreationState::Acquire {
                    request,
                    reservation,
                } => {
                    match self
                        .services
                        .acquisition
                        .acquire(request.clone(), self.cancellation.clone())
                    {
                        RuntimeBackedWindowAcquisitionOutcome::NotCommitted {
                            evidence, ..
                        } => {
                            self.error = Some(format!(
                                "New Window acquisition did not commit: {evidence:?}"
                            ));
                        }
                        RuntimeBackedWindowAcquisitionOutcome::Committed {
                            acquisition, ..
                        }
                        | RuntimeBackedWindowAcquisitionOutcome::ExactCommitted { acquisition } => {
                            self.state = CreationState::Acquired {
                                acquisition,
                                reservation,
                            };
                        }
                        RuntimeBackedWindowAcquisitionOutcome::Indeterminate {
                            reconciliation,
                            ..
                        } => {
                            self.state = CreationState::AcquisitionPending {
                                reconciliation,
                                reservation,
                            };
                            return MainWindowCreationOutcome::Pending(self);
                        }
                        RuntimeBackedWindowAcquisitionOutcome::RepairIndeterminate {
                            reconciliation,
                            ..
                        } => {
                            self.state = CreationState::RepairPending {
                                request,
                                reconciliation,
                                reservation,
                            };
                            return MainWindowCreationOutcome::Pending(self);
                        }
                    }
                }
                CreationState::AcquisitionPending {
                    reconciliation,
                    reservation,
                } => match reconciliation.reconcile(&self.services.store) {
                    RuntimeBackedWindowAcquisitionReconciliationOutcome::Pending {
                        reconciliation,
                        ..
                    } => {
                        self.state = CreationState::AcquisitionPending {
                            reconciliation,
                            reservation,
                        };
                        return MainWindowCreationOutcome::Pending(self);
                    }
                    RuntimeBackedWindowAcquisitionReconciliationOutcome::ExactNew {
                        acquisition,
                        ..
                    } => {
                        self.state = CreationState::Acquired {
                            acquisition,
                            reservation,
                        };
                    }
                    RuntimeBackedWindowAcquisitionReconciliationOutcome::ExactOld { .. }
                    | RuntimeBackedWindowAcquisitionReconciliationOutcome::Collision { .. } => {
                        self.error = Some("New Window acquisition was not established.".to_owned());
                    }
                },
                CreationState::RepairPending {
                    request,
                    reconciliation,
                    reservation,
                } => match reconciliation.reconcile(&self.services.store) {
                    RuntimeBackedWindowAcquisitionRepairReconciliationOutcome::Pending {
                        reconciliation,
                        ..
                    } => {
                        self.state = CreationState::RepairPending {
                            request,
                            reconciliation,
                            reservation,
                        };
                        return MainWindowCreationOutcome::Pending(self);
                    }
                    RuntimeBackedWindowAcquisitionRepairReconciliationOutcome::ExactOld {
                        ..
                    }
                    | RuntimeBackedWindowAcquisitionRepairReconciliationOutcome::ExactNew {
                        ..
                    } => {
                        self.state = CreationState::Acquire {
                            request,
                            reservation,
                        };
                    }
                    RuntimeBackedWindowAcquisitionRepairReconciliationOutcome::Collision {
                        ..
                    } => {
                        self.error = Some("New Window catalog repair collided.".to_owned());
                    }
                },
                CreationState::Acquired {
                    acquisition,
                    reservation,
                } => {
                    if self.cancellation.is_cancelled() {
                        self.error = Some("New Window creation was cancelled.".to_owned());
                        self.state = CreationState::Unpublished(
                            MainWindowShellUnpublished::from_acquired(acquisition, reservation),
                        );
                        continue;
                    }
                    let facts = (|| {
                        let bootstrap = self
                            .services
                            .state
                            .session()
                            .minimal_bootstrap(&self.services.store)
                            .map_err(|error| error.to_string())?
                            .ok_or_else(|| "New Window session is unavailable.".to_owned())?;
                        let claim = bootstrap
                            .windows()
                            .iter()
                            .find(|record| record.window_id() == self.window_id)
                            .and_then(|record| record.selected_thread())
                            .ok_or_else(|| "New Window claim is unavailable.".to_owned())?;
                        let (request, retirement) =
                            (self.services.activation_source)(&acquisition)?;
                        Ok::<_, String>((claim, request, retirement))
                    })();
                    let (claim, request, retirement) = match facts {
                        Ok(facts) => facts,
                        Err(error) => {
                            self.error = Some(error);
                            self.state = CreationState::Unpublished(
                                MainWindowShellUnpublished::from_acquired(acquisition, reservation),
                            );
                            continue;
                        }
                    };
                    match MainWindowInitialComposer::new(
                        acquisition,
                        reservation,
                        self.services.acquisition.clone(),
                        self.services.store.clone(),
                        self.services.storage.clone(),
                        claim,
                        request,
                        retirement,
                        MainWindowComposerMarkerMetadataAuthority::new(
                            self.services.state.assets(),
                        ),
                    ) {
                        Ok(initial) => self.state = CreationState::Initial(initial),
                        Err(failure) => {
                            self.error = Some(failure.error);
                            self.state = CreationState::Unpublished(
                                MainWindowShellUnpublished::from_acquired(
                                    failure.acquisition,
                                    failure.reservation,
                                ),
                            );
                        }
                    }
                }
                CreationState::Initial(mut initial) => {
                    #[cfg(feature = "test-faults")]
                    if let Some(before) = &self.services.test_before_initial_advance {
                        before(&mut initial);
                    }
                    match initial.advance(&self.cancellation) {
                        Ok(
                            MainWindowInitialComposerProgress::Pending
                            | MainWindowInitialComposerProgress::Retry,
                        ) => {
                            self.state = if self.cancellation.is_cancelled() {
                                CreationState::Retiring(initial)
                            } else {
                                CreationState::Initial(initial)
                            };
                            return MainWindowCreationOutcome::Pending(self);
                        }
                        Err(error) => {
                            self.error = Some(error);
                            self.state = CreationState::Retiring(initial);
                        }
                        Ok(MainWindowInitialComposerProgress::Activated) => {
                            let mut configurator = (self.services.configurator_source)();
                            let prepared =
                                initial.prepare(&mut configurator).and_then(|prepared| {
                                    prepared.into_shell(
                                        configurator,
                                        self.services.marker_seals.clone(),
                                        MainWindowComposerSubmissionRequestSource::new(
                                            self.services.turn_start_requirement,
                                        ),
                                        appearance.clone(),
                                    )
                                });
                            match prepared {
                                Ok(prepared) if !self.cancellation.is_cancelled() => {
                                    return MainWindowCreationOutcome::Prepared {
                                        prepared,
                                        cancellation: self.cancellation,
                                    };
                                }
                                Ok(prepared) => {
                                    self.error =
                                        Some("New Window creation was cancelled.".to_owned());
                                    self.state =
                                        CreationState::Unpublished(prepared.into_unpublished());
                                }
                                Err(failure) => {
                                    self.error = Some(failure.error);
                                    self.state = CreationState::Retiring(failure.custody);
                                }
                            }
                        }
                    }
                }
                CreationState::Retiring(initial) => {
                    match initial.retire(CommandCancellation::new()) {
                        MainWindowInitialComposerRetirement::Retired(unpublished) => {
                            self.state = CreationState::Unpublished(unpublished);
                        }
                        MainWindowInitialComposerRetirement::Pending(failure) => {
                            self.error = Some(failure.error);
                            self.state = CreationState::Retiring(failure.custody);
                            return MainWindowCreationOutcome::Pending(self);
                        }
                    }
                }
                CreationState::Unpublished(unpublished) => {
                    match unpublished
                        .prepare_abandonment(&self.services.acquisition, CommandCancellation::new())
                    {
                        MainWindowShellAbandonmentPreparationOutcome::ExactAcquired {
                            abandonment,
                        } => {
                            self.state = CreationState::Abandonment(abandonment);
                        }
                        MainWindowShellAbandonmentPreparationOutcome::InitialComposerPending {
                            unpublished,
                            error,
                        } => {
                            self.error = Some(error);
                            self.state = CreationState::Unpublished(unpublished);
                            return MainWindowCreationOutcome::Pending(self);
                        }
                        MainWindowShellAbandonmentPreparationOutcome::NotCommitted {
                            unpublished,
                            evidence,
                        } => {
                            self.error = Some(format!(
                                "New Window abandonment did not prepare: {evidence:?}"
                            ));
                            self.state = CreationState::Unpublished(unpublished);
                            return MainWindowCreationOutcome::Pending(self);
                        }
                        MainWindowShellAbandonmentPreparationOutcome::Collision { .. } => {
                            self.error = Some("New Window abandonment collided.".to_owned());
                        }
                    }
                }
                CreationState::Abandonment(abandonment) => {
                    match abandonment
                        .abandon(&self.services.acquisition, CommandCancellation::new())
                    {
                        MainWindowShellAbandonmentOutcome::NotCommitted {
                            abandonment,
                            evidence,
                        } => {
                            self.error = Some(format!(
                                "New Window abandonment did not commit: {evidence:?}"
                            ));
                            self.state = CreationState::Abandonment(abandonment);
                            return MainWindowCreationOutcome::Pending(self);
                        }
                        MainWindowShellAbandonmentOutcome::Committed { .. } => {}
                        MainWindowShellAbandonmentOutcome::Indeterminate {
                            reconciliation, ..
                        } => {
                            self.state = CreationState::AbandonmentPending(reconciliation);
                            return MainWindowCreationOutcome::Pending(self);
                        }
                    }
                }
                CreationState::AbandonmentPending(reconciliation) => {
                    match reconciliation.reconcile(&self.services.store) {
                        MainWindowShellAbandonmentReconciliationOutcome::Pending {
                            reconciliation,
                            ..
                        } => {
                            self.state = CreationState::AbandonmentPending(reconciliation);
                            return MainWindowCreationOutcome::Pending(self);
                        }
                        MainWindowShellAbandonmentReconciliationOutcome::ExactAcquired {
                            abandonment,
                        } => {
                            self.state = CreationState::Abandonment(abandonment);
                        }
                        MainWindowShellAbandonmentReconciliationOutcome::ExactAbandoned {
                            ..
                        } => {}
                        MainWindowShellAbandonmentReconciliationOutcome::Collision { .. } => {
                            self.error =
                                Some("New Window abandonment reconciliation collided.".to_owned());
                        }
                    }
                }
                CreationState::Settled => {
                    return MainWindowCreationOutcome::Settled {
                        window_id: self.window_id,
                        error: self.error,
                    };
                }
            }
        }
        MainWindowCreationOutcome::Pending(self)
    }
}
