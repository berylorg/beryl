use super::*;

impl ThemeRuntime {
    pub fn consume_settings_outcome(
        &mut self,
        outcome: SettingsThemeOutcome,
    ) -> Result<SettingsThemeResult, ThemeRuntimeFailureClass> {
        self.ensure_running()?;
        self.settings_outcomes = self.settings_outcomes.saturating_add(1);
        let confirmed = match outcome {
            SettingsThemeOutcome::NotCommitted | SettingsThemeOutcome::ReconciledExactOld => {
                return Ok(SettingsThemeResult::Retained);
            }
            SettingsThemeOutcome::Indeterminate | SettingsThemeOutcome::ReconciliationCollision => {
                self.last_failure = Some(ThemeRuntimeFailureClass::Reconciliation);
                return Ok(SettingsThemeResult::Retained);
            }
            SettingsThemeOutcome::Committed(confirmed)
            | SettingsThemeOutcome::ReconciledExactNew(confirmed) => confirmed,
        };
        if confirmed.committed.home() != self.service.as_ref().expect("running service").home()
            || confirmed.prepared.settings() != confirmed.committed
            || !prepared_matches_active(&confirmed.prepared, confirmed.active.as_ref())
        {
            self.last_failure = Some(ThemeRuntimeFailureClass::Settings);
            return Err(ThemeRuntimeFailureClass::Settings);
        }
        let identity = DurablePublicationIdentity::Settings {
            draft: confirmed.draft,
            revision: confirmed.revision,
            committed: confirmed.committed,
        };
        self.settings = confirmed.committed;
        self.active = confirmed.active;
        self.last_observed_document = installed_identity(&confirmed.prepared).cloned();
        let result = match self.publish(identity, confirmed.prepared) {
            Ok(result) => result,
            Err(failure) => {
                self.last_failure = Some(failure);
                return Err(failure);
            }
        };
        self.last_failure = None;
        Ok(match result {
            RepositoryAppearanceResult::Published => SettingsThemeResult::Published,
            RepositoryAppearanceResult::HiddenBaseReplaced => {
                SettingsThemeResult::HiddenBaseReplaced
            }
            RepositoryAppearanceResult::Unchanged | RepositoryAppearanceResult::Retained(_) => {
                SettingsThemeResult::Retained
            }
        })
    }

    pub fn retry_current_durable(
        &mut self,
    ) -> Result<RepositoryAppearanceResult, ThemeRuntimeFailureClass> {
        self.ensure_running()?;
        let outcome = self
            .appearance
            .as_mut()
            .expect("running appearance")
            .retry_durable_publication()
            .map_err(|error| {
                let failure = map_publication_error(error);
                self.last_failure = Some(failure);
                failure
            })?;
        self.last_failure = None;
        Ok(match outcome {
            DurableRetryOutcome::NotPending => RepositoryAppearanceResult::Unchanged,
            DurableRetryOutcome::Published(_) => RepositoryAppearanceResult::Published,
            DurableRetryOutcome::HiddenBaseRetained(_) => {
                RepositoryAppearanceResult::HiddenBaseReplaced
            }
        })
    }

    pub fn execute_repository_request(
        &mut self,
        store: &HomeStore,
        request: ThemeRepositoryRequest,
        references: &dyn ThemeReferenceSnapshotProvider,
    ) -> Result<ThemeRepositoryRequestResult, ThemeRuntimeFailureClass> {
        self.ensure_running()?;
        self.repository_requests = self.repository_requests.saturating_add(1);
        let Some(repository) = self.repository.as_ref() else {
            self.last_failure = Some(ThemeRuntimeFailureClass::Repository);
            return Err(ThemeRuntimeFailureClass::Repository);
        };
        let outcome = self
            .service
            .as_ref()
            .expect("running service")
            .execute_command(
                store,
                repository,
                &request.command,
                self.config.max_manifest_bytes,
                references,
            )
            .map_err(|_| {
                self.last_failure = Some(ThemeRuntimeFailureClass::Repository);
                ThemeRuntimeFailureClass::Repository
            })?;
        match outcome {
            ThemeRepositoryOperationOutcome::NotCommitted { .. } => {
                Ok(ThemeRepositoryRequestResult::NotCommitted)
            }
            ThemeRepositoryOperationOutcome::Indeterminate(operation) => {
                self.last_failure = Some(ThemeRuntimeFailureClass::Reconciliation);
                Ok(ThemeRepositoryRequestResult::Indeterminate {
                    operation: operation.operation(),
                })
            }
            ThemeRepositoryOperationOutcome::Committed { publication, .. } => {
                let appearance = self.apply_repository_commit(store, &publication);
                Ok(ThemeRepositoryRequestResult::Committed {
                    publication,
                    appearance,
                })
            }
        }
    }

    pub fn reconcile_repository_request(
        &mut self,
        store: &HomeStore,
        operation: NonZeroU64,
    ) -> Result<ThemeRepositoryReconciliationResult, ThemeRuntimeFailureClass> {
        self.ensure_running()?;
        let outcome = self
            .service
            .as_ref()
            .expect("running service")
            .reconcile_operation(store, operation, self.config.max_manifest_bytes)
            .map_err(|_| {
                self.last_failure = Some(ThemeRuntimeFailureClass::Reconciliation);
                ThemeRuntimeFailureClass::Reconciliation
            })?;
        match outcome {
            ThemeReconciliation::ExactOld => Ok(ThemeRepositoryReconciliationResult::ExactOld),
            ThemeReconciliation::Collision => {
                self.last_failure = Some(ThemeRuntimeFailureClass::Reconciliation);
                Ok(ThemeRepositoryReconciliationResult::Collision)
            }
            ThemeReconciliation::ExactNew(publication) => {
                let appearance = self.apply_repository_commit(store, &publication);
                Ok(ThemeRepositoryReconciliationResult::ExactNew {
                    publication,
                    appearance,
                })
            }
        }
    }
}
