use super::*;

impl ThemeRuntime {
    pub fn proven_selection(&self) -> Option<(ThemeSettingsIdentity, Option<InstalledThemeId>)> {
        if !self.selection_available() || self.service.is_none() {
            return None;
        }
        Some((self.settings?, self.active.clone()))
    }

    pub fn sync_applied_selection(
        &mut self,
        settings: ThemeSettingsIdentity,
        active: Option<InstalledThemeId>,
    ) -> Result<(), ThemeRuntimeFailureClass> {
        if self.appearance.is_none() || self.service.is_none() {
            return Err(ThemeRuntimeFailureClass::Retired);
        }
        if settings.home() != self.service.as_ref().expect("running service").home() {
            return Err(ThemeRuntimeFailureClass::Identity);
        }
        let current = self.settings.ok_or(ThemeRuntimeFailureClass::Settings)?;
        if !self.selection_available()
            || active != self.active
            || settings.active_record_revision() != current.active_record_revision()
            || settings.domain_revision() < current.domain_revision()
        {
            return Err(ThemeRuntimeFailureClass::Settings);
        }
        self.settings = Some(settings);
        Ok(())
    }

    pub fn mark_selection_unavailable(&mut self) {
        if self.service.is_none() {
            return;
        }
        self.selection_available.store(false, Ordering::Release);
        self.settings = None;
        self.pending_settings_publication = None;
        self.active = None;
        self.last_observed_document = None;
        self.last_failure = Some(ThemeRuntimeFailureClass::Reconciliation);
    }

    pub fn consume_repository_completion(
        &mut self,
        store: &HomeStore,
        completion: crate::theme_runtime::ThemeSettingsCompletion,
    ) -> Result<RepositoryAppearanceResult, ThemeRuntimeFailureClass> {
        self.ensure_running()?;
        let home = self.service.as_ref().expect("running service").home();
        if completion.home != home {
            return Err(ThemeRuntimeFailureClass::Identity);
        }
        let mut original = None;
        if let Some(outcome) = completion.repository.as_ref() {
            if let ThemeRepositoryOperationOutcome::Committed { publication, .. } = outcome.as_ref()
            {
                original = Some(publication);
            }
        }
        if let Some(outcome) = completion.reconciliation.as_ref() {
            if let ThemeReconciliation::ExactNew(publication) = outcome.as_ref() {
                if original.is_some_and(|original| original != publication) {
                    return Err(ThemeRuntimeFailureClass::Reconciliation);
                }
                original = Some(publication);
            }
        }
        if original != completion.publication.as_ref() {
            return Err(ThemeRuntimeFailureClass::Reconciliation);
        }
        let Some(publication) = original else {
            return Ok(if completion.unresolved.is_some() {
                RepositoryAppearanceResult::Retained(ThemeRuntimeFailureClass::Reconciliation)
            } else {
                RepositoryAppearanceResult::Unchanged
            });
        };
        if publication.manifest().is_some()
            || completion.appearance_target.as_ref() != self.active.as_ref()
            || !self.active.as_ref().is_some_and(|active| {
                publication
                    .affected_documents()
                    .iter()
                    .any(|document| document.theme_id() == active)
            })
        {
            return Ok(RepositoryAppearanceResult::Unchanged);
        }
        if publication
            .manifest()
            .is_some_and(|manifest| manifest.home() != home)
            || publication
                .affected_documents()
                .iter()
                .any(|document| document.manifest().home() != home)
        {
            return Err(ThemeRuntimeFailureClass::Identity);
        }
        let Some(prepared) = completion.prepared else {
            let failure = completion
                .preparation_failure
                .unwrap_or(ThemeRuntimeFailureClass::Reconciliation);
            self.last_failure = Some(failure);
            return Ok(RepositoryAppearanceResult::Retained(failure));
        };
        if !self.selection_available()
            || Some(prepared.settings()) != self.settings
            || prepared.home() != home
            || !prepared_matches_active(&prepared, self.active.as_ref())
        {
            self.refresh_reread_needed = true;
            return Ok(RepositoryAppearanceResult::Retained(
                ThemeRuntimeFailureClass::Settings,
            ));
        }
        if let Err(failure) =
            self.settings_source()?
                .revalidate_prepared(store, &prepared, self.active.as_ref())
        {
            self.refresh_reread_needed = true;
            return Ok(self.retain(failure));
        }
        let identity = DurablePublicationIdentity::RepositoryRefresh(prepared.source().clone());
        self.last_observed_document = installed_identity(&prepared).cloned();
        self.repository = None;
        self.publish(identity, prepared)
    }

    pub fn prepare_settings_selection(
        &mut self,
        store: &HomeStore,
        draft: ThemeDraftIdentity,
        revision: ThemeDraftRevision,
        intended: ThemeSettingsIdentity,
        active: Option<InstalledThemeId>,
    ) -> Result<ConfirmedSettingsTheme, ThemeRuntimeFailureClass> {
        self.settings_source()?
            .prepare_selection(store, draft, revision, intended, active)
    }

    pub fn consume_settings_outcome(
        &mut self,
        store: &HomeStore,
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
        if let Err(failure) = self.settings_source()?.verify_current_selection(
            store,
            confirmed.committed,
            confirmed.active.as_ref(),
        ) {
            self.last_failure = Some(failure);
            return Err(failure);
        }
        let identity = DurablePublicationIdentity::Settings {
            draft: confirmed.draft,
            revision: confirmed.revision,
            committed: confirmed.committed,
        };
        self.settings = Some(confirmed.committed);
        self.pending_settings_publication = Some((confirmed.draft, confirmed.revision));
        self.refresh_reread_needed = true;
        self.active = confirmed.active;
        self.selection_available.store(true, Ordering::Release);
        if let Err(failure) = self.settings_source()?.revalidate_prepared(
            store,
            &confirmed.prepared,
            self.active.as_ref(),
        ) {
            self.refresh_reread_needed = true;
            self.last_failure = Some(failure);
            return Ok(SettingsThemeResult::Retained);
        }
        self.last_observed_document = installed_identity(&confirmed.prepared).cloned();
        let result = match self.publish(identity, confirmed.prepared) {
            Ok(result) => result,
            Err(failure) => {
                self.last_failure = Some(failure);
                return Err(failure);
            }
        };
        self.pending_settings_publication = None;
        self.refresh_reread_needed = false;
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
        store: &HomeStore,
    ) -> Result<RepositoryAppearanceResult, ThemeRuntimeFailureClass> {
        self.ensure_running()?;
        if !self.selection_available() {
            return Err(ThemeRuntimeFailureClass::Reconciliation);
        }
        let settings = self.settings.ok_or(ThemeRuntimeFailureClass::Settings)?;
        let prepared = match self.settings_source()?.prepare_current_bound(
            store,
            settings,
            self.active.as_ref(),
            self.last_observed_document.as_ref(),
        ) {
            Ok(prepared) => prepared,
            Err(failure) => return Ok(self.retain(failure)),
        };
        let appearance = self.appearance.as_ref().expect("running appearance");
        if self.pending_settings_publication.is_none()
            && !appearance.diagnostics().pending_durable_application()
            && appearance.durable_base().prepared() == &prepared
        {
            self.last_failure = None;
            self.refresh_reread_needed = false;
            return Ok(RepositoryAppearanceResult::Unchanged);
        }
        let identity = self.pending_settings_publication.map_or_else(
            || DurablePublicationIdentity::RepositoryRefresh(prepared.source().clone()),
            |(draft, revision)| DurablePublicationIdentity::Settings {
                draft,
                revision,
                committed: settings,
            },
        );
        let observed = installed_identity(&prepared).cloned();
        let publication = if self.pending_settings_publication.is_some() {
            self.publish(identity, prepared)
        } else {
            self.publish_external(identity, prepared)
        };
        match publication {
            Ok(result) => {
                self.pending_settings_publication = None;
                self.last_observed_document = observed;
                self.refresh_reread_needed = false;
                Ok(result)
            }
            Err(failure) => Ok(self.retain(failure)),
        }
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
            .map_err(|error| {
                let failure = map_repository_execution_failure(&error);
                self.last_failure = Some(failure);
                failure
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
