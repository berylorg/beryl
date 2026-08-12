use super::*;

impl ThemeRuntime {
    pub fn start(
        store: &HomeStore,
        service: ThemeService,
        domain_revision: DomainRevision,
        active_setting: Option<&SettingRecord>,
        config: ThemeRuntimeConfig,
    ) -> Result<Self, ThemeRuntimeStartError> {
        let active = ThemeService::active_theme_from_setting(active_setting)
            .map_err(|_| start_error(ThemeRuntimeFailureClass::Identity))?;
        let settings = service.settings_identity(domain_revision, active_setting);
        let repository = service
            .observe_repository(store, config.max_manifest_bytes, config.manifest_read, None)
            .ok();
        let mut pages_read = 0;
        let loaded = match (&active, &repository) {
            (None, _) => Err(ThemeLoadFailure::ActiveIdentityMissing),
            (Some(_), None) => Err(ThemeLoadFailure::RepositoryUnavailable),
            (Some(active), Some(repository)) => load_prepared(
                &service,
                store,
                repository,
                active,
                settings,
                None,
                config,
                &mut pages_read,
            )
            .map(|(prepared, _)| prepared),
        };
        let startup = ThemeStartupOutcome::evaluate(settings, active.as_ref(), loaded)
            .map_err(|_| start_error(ThemeRuntimeFailureClass::Identity))?;
        let last_failure = startup.failure().map(map_startup_failure);
        let initial = startup.appearance().clone();
        let last_observed_document = installed_identity(&initial).cloned();
        let appearance = AppearanceCoordinator::new(config.appearance, initial);
        let subscription = service
            .subscribe_changes(
                store,
                config.watch_interval,
                config.watch_queue_capacity,
                config.watch_max_entries_per_poll,
                config.watch_max_file_bytes,
            )
            .map_err(|_| start_error(ThemeRuntimeFailureClass::Subscription))?;
        Ok(Self {
            config,
            retired_state: service.diagnostics(),
            service: Some(service),
            repository,
            subscription: Some(subscription),
            appearance: Some(appearance),
            settings,
            active,
            last_observed_document,
            refresh_reread_needed: false,
            pages_read,
            watch_batches: 0,
            watch_hints: 0,
            app_coalesced_hints: 0,
            app_overflow_hints: 0,
            settings_outcomes: 0,
            repository_requests: 0,
            last_failure,
        })
    }

    #[must_use]
    pub fn current(&self) -> Option<Arc<AppearanceGeneration>> {
        self.appearance.as_ref().map(AppearanceCoordinator::current)
    }

    pub fn register_adapter(
        &mut self,
        adapter: Arc<dyn AppearanceWindowAdapter>,
    ) -> Result<(), AdapterRegistrationError> {
        self.appearance
            .as_mut()
            .ok_or(AdapterRegistrationError::CapacityReached)?
            .register_adapter(adapter)
    }

    pub fn unregister_adapter(
        &mut self,
        id: WindowAdapterId,
    ) -> Result<bool, WindowEpochExhausted> {
        self.appearance
            .as_mut()
            .map_or(Ok(false), |value| value.unregister_adapter(id))
    }

    pub fn stop_preview(&mut self) -> Result<StopPreviewResult, PreviewPublicationError> {
        self.appearance
            .as_mut()
            .ok_or(PreviewPublicationError::CandidateMismatch)?
            .stop_preview()
    }

    pub fn begin_preview(
        &mut self,
        source: PreviewSource,
        candidate: PreviewCandidateIdentity,
    ) -> Result<PreviewPublicationRequest, PreviewPublicationError> {
        self.appearance
            .as_mut()
            .ok_or(PreviewPublicationError::CandidateMismatch)?
            .begin_preview(source, candidate)
    }

    pub fn publish_preview(
        &mut self,
        request: PreviewPublicationRequest,
        prepared: PreparedPreviewAppearance,
    ) -> Result<PreviewPublicationResult, PreviewPublicationError> {
        self.appearance
            .as_mut()
            .ok_or(PreviewPublicationError::CandidateMismatch)?
            .publish_preview(request, prepared)
    }

    #[must_use]
    pub fn diagnostics(&self) -> ThemeRuntimeDiagnostics {
        ThemeRuntimeDiagnostics {
            home_generation_present: self.service.is_some(),
            repository_generation_present: self.repository.is_some(),
            repository_generation: self
                .repository
                .as_ref()
                .map(|repository| repository.manifest().generation()),
            active_document_revision: self
                .last_observed_document
                .as_ref()
                .map(ThemeDocumentIdentity::revision),
            refresh_reread_needed: self.refresh_reread_needed,
            retired: self.appearance.is_none(),
            worker_count: 0,
            pages_read: self.pages_read,
            watch_batches: self.watch_batches,
            watch_hints: self.watch_hints,
            app_coalesced_hints: self.app_coalesced_hints,
            app_overflow_hints: self.app_overflow_hints,
            settings_outcomes: self.settings_outcomes,
            repository_requests: self.repository_requests,
            last_failure: self.last_failure,
            appearance: self
                .appearance
                .as_ref()
                .map(AppearanceCoordinator::diagnostics),
            state: self
                .service
                .as_ref()
                .map_or(self.retired_state, ThemeService::diagnostics),
        }
    }

    pub fn retire(&mut self) {
        if let Some(subscription) = self.subscription.take() {
            subscription.shutdown();
        }
        if let Some(appearance) = self.appearance.take() {
            appearance.retire();
        }
        self.repository = None;
        self.last_observed_document = None;
        self.retired_state = self
            .service
            .take()
            .map_or(self.retired_state, |service| service.diagnostics());
        self.last_failure = Some(ThemeRuntimeFailureClass::Retired);
    }
}
