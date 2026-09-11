use super::*;

impl MainWindowCreation {
    #[inline(never)]
    pub(super) fn prepare_initial(
        &mut self,
        acquisition: RuntimeBackedWindowAcquisition,
        reservation: RuntimeBackedWindowMainWindowReservation,
    ) -> CreationStep {
        if self.cancellation.is_cancelled() {
            self.error = Some("New Window creation was cancelled.".to_owned());
            self.state = CreationState::Unpublished(MainWindowShellUnpublished::from_acquired(
                acquisition,
                reservation,
            ));
            return CreationStep::Continue;
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
            let (request, retirement) = (self.services.activation_source)(&acquisition)?;
            Ok::<_, String>((claim, request, retirement))
        })();
        let (claim, request, retirement) = match facts {
            Ok(facts) => facts,
            Err(error) => {
                self.error = Some(error);
                self.state = CreationState::Unpublished(MainWindowShellUnpublished::from_acquired(
                    acquisition,
                    reservation,
                ));
                return CreationStep::Continue;
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
            MainWindowComposerMarkerMetadataAuthority::new(self.services.state.assets()),
        ) {
            Ok(initial) => self.state = CreationState::Initial(initial),
            Err(failure) => {
                self.error = Some(failure.error);
                self.state = CreationState::Unpublished(MainWindowShellUnpublished::from_acquired(
                    failure.acquisition,
                    failure.reservation,
                ));
            }
        }

        CreationStep::Continue
    }

    #[inline(never)]
    pub(super) fn advance_initial(
        &mut self,
        mut initial: MainWindowInitialComposer,
        appearance: &Arc<crate::theme_runtime::AppearanceGeneration>,
    ) -> CreationStep {
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
                return CreationStep::Pending;
            }
            Err(error) => {
                self.error = Some(error);
                self.state = CreationState::Retiring(initial);
            }
            Ok(MainWindowInitialComposerProgress::Activated) => {
                let mut configurator = (self.services.configurator_source)();
                let prepared = initial.prepare(&mut configurator).and_then(|prepared| {
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
                        return CreationStep::Prepared(prepared);
                    }
                    Ok(prepared) => {
                        self.error = Some("New Window creation was cancelled.".to_owned());
                        self.state = CreationState::Unpublished(prepared.into_unpublished());
                    }
                    Err(failure) => {
                        self.error = Some(failure.error);
                        self.state = CreationState::Retiring(failure.custody);
                    }
                }
            }
        }

        CreationStep::Continue
    }

    #[inline(never)]
    pub(super) fn retire_initial(&mut self, initial: MainWindowInitialComposer) -> CreationStep {
        match initial.retire(CommandCancellation::new()) {
            MainWindowInitialComposerRetirement::Retired(unpublished) => {
                self.state = CreationState::Unpublished(unpublished);
            }
            MainWindowInitialComposerRetirement::Pending(failure) => {
                self.error = Some(failure.error);
                self.state = CreationState::Retiring(failure.custody);
                return CreationStep::Pending;
            }
        }

        CreationStep::Continue
    }
}
