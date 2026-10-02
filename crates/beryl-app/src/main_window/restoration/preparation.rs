use super::*;

impl MainWindowRestoreSet {
    pub(super) fn prepare_step(&mut self) -> Result<PreparationStep, String> {
        if !self.discovered {
            return self.discover_step();
        }
        if self.replacement.is_some() {
            return self.replacement_step();
        }
        self.restored_step()
    }

    #[inline(never)]
    fn discover_step(&mut self) -> Result<PreparationStep, String> {
        let Some(discovered) = self.discovery.advance(
            &self.services.store,
            &self.services.state,
            &self.cancellation,
        )?
        else {
            return Ok(PreparationStep::Pending);
        };
        self.discovered = true;
        match discovered {
            DiscoveredRestoreSet::Restored(session) => {
                self.expected_revision = Some(session.header().revision());
                self.expected_windows = session.windows().iter().map(|w| w.window_id()).collect();
                self.remaining = session.windows().iter().cloned().collect();
            }
            DiscoveredRestoreSet::Threadless(session) => {
                if session.windows().len() != 1 {
                    return Err("threadless restore set is not a single member".to_owned());
                }
                let window = session.windows()[0].window_id();
                self.expected_revision = Some(session.header().revision());
                self.expected_windows.push(window);
                let source = self.attempt.begin_threadless(
                    session.header().revision(),
                    window,
                    self.services.state.runtime_roots(),
                )?;
                self.members.push(PreparedRestoreSetMember::Threadless(
                    ThreadlessWindowShellPrepared::new(
                        source,
                        &self.services.acquisition.process_registry(),
                        self.appearance.clone(),
                    )?,
                ));
            }
            DiscoveredRestoreSet::Replacement { session, target } => {
                if !session.windows().is_empty() {
                    return Err("replacement restore set is not empty".to_owned());
                }
                self.expected_revision = Some(session.header().revision());
                self.expected_windows.push(self.initial_window);
                self.replacement = Some(Box::new(
                    MainWindowCreation::admit(self.services.clone(), self.initial_window, target)
                        .map_err(|error| format!("replacement admission failed: {error:?}"))?,
                ));
            }
        }
        Ok(PreparationStep::Continue)
    }

    #[inline(never)]
    fn replacement_step(&mut self) -> Result<PreparationStep, String> {
        if let Some(work) = self.replacement.take() {
            match work.advance(self.appearance.clone()) {
                MainWindowCreationOutcome::Pending(work) => {
                    self.replacement = Some(Box::new(work));
                    return Ok(PreparationStep::Pending);
                }
                MainWindowCreationOutcome::Settled { error, .. } => {
                    return Err(error.unwrap_or_else(|| "replacement creation failed".to_owned()));
                }
                MainWindowCreationOutcome::Prepared { prepared, .. } => {
                    let actual = prepared.session_revision();
                    self.members
                        .push(PreparedRestoreSetMember::Replacement(Box::new(prepared)));
                    let expected = self
                        .expected_revision
                        .unwrap()
                        .checked_next()
                        .map_err(|e| e.to_string())?;
                    if actual != expected {
                        return Err(
                            "replacement session changed outside this startup attempt".to_owned()
                        );
                    }
                    self.expected_revision = Some(expected);
                    return Ok(PreparationStep::Continue);
                }
            }
        }
        unreachable!("replacement work was checked before dispatch")
    }

    #[inline(never)]
    fn restored_step(&mut self) -> Result<PreparationStep, String> {
        if self.current.is_none() && !self.begin_restored()? {
            return Ok(PreparationStep::Complete);
        }
        let current = self.current.as_mut().unwrap();
        if !self.current_ready {
            match current.advance(&self.attempt, &self.cancellation)? {
                MainWindowInitialComposerProgress::Pending
                | MainWindowInitialComposerProgress::Retry => {
                    return Ok(PreparationStep::Pending);
                }
                MainWindowInitialComposerProgress::Activated => self.current_ready = true,
            }
        }
        #[cfg(feature = "test-faults")]
        if let Some(fault) = self.before_claim_activation.take() {
            current.test_arm_before_claim_activation(fault);
        }
        match current.activate_claim(&self.attempt, &self.cancellation)? {
            RestoredClaimActivationProgress::Pending | RestoredClaimActivationProgress::Retry => {
                return Ok(PreparationStep::Pending);
            }
            RestoredClaimActivationProgress::Active => {}
        }
        self.expected_revision = Some(
            self.expected_revision
                .unwrap()
                .checked_next()
                .map_err(|e| e.to_string())?,
        );
        self.prepare_restored()
    }

    #[inline(never)]
    fn begin_restored(&mut self) -> Result<bool, String> {
        let Some(window) = self.remaining.front() else {
            return Ok(false);
        };
        let (request, retirement) = (self.activation_source)(window)?;
        self.current = Some(Box::new(self.attempt.begin(
            self.expected_revision.unwrap(),
            window.window_id(),
            request,
            retirement,
            MainWindowComposerMarkerMetadataAuthority::new(self.services.state.assets()),
        )?));
        self.remaining.pop_front();
        self.current_ready = false;
        Ok(true)
    }

    #[inline(never)]
    fn prepare_restored(&mut self) -> Result<PreparationStep, String> {
        let mut configurator = (self.services.configurator_source)();
        let current = *self.current.take().unwrap();
        let prepared = match current.prepare(&self.attempt, &mut configurator) {
            Ok(prepared) => prepared,
            Err(failure) => {
                self.current = Some(Box::new(failure.custody));
                return Err(failure.error);
            }
        };
        self.prepare_restored_shell(prepared, configurator)
    }

    #[inline(never)]
    fn prepare_restored_shell(
        &mut self,
        prepared: RestoredWindowComposerPrepared,
        configurator: MainWindowShellComposerConfigurator,
    ) -> Result<PreparationStep, String> {
        match RestoredWindowShellPrepared::prepare_owned(
            Box::new(prepared),
            &self.attempt,
            &self.services.acquisition.process_registry(),
            configurator,
            self.services.marker_seals.clone(),
            MainWindowComposerSubmissionRequestSource::new(
                self.services.submission_execution.clone(),
                self.services.turn_start_requirement,
            ),
            self.appearance.clone(),
        ) {
            Ok(prepared) => self
                .members
                .push(PreparedRestoreSetMember::Restored(prepared)),
            Err((prepared, error)) => {
                self.current = Some(Box::new(prepared.into_custody()));
                return Err(error);
            }
        }
        Ok(PreparationStep::Continue)
    }
}
