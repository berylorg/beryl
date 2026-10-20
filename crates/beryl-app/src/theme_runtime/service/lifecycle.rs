use super::*;

impl ThemeRuntime {
    pub fn start_unavailable_context(
        store: &HomeStore,
        service: ThemeService,
        context: ThemeRecoveryContext,
        config: ThemeRuntimeConfig,
    ) -> Result<Self, ThemeRuntimeStartError> {
        let subscription = service
            .subscribe_changes(
                store,
                config.watch_interval,
                config.watch_queue_capacity,
                config.watch_max_entries_per_poll,
                config.watch_max_file_bytes,
            )
            .map_err(|_| start_error(ThemeRuntimeFailureClass::Subscription))?;
        let mut runtime = Self::load_unavailable(service, context, config)?;
        runtime.subscription = Some(subscription);
        Ok(runtime)
    }

    pub(super) fn load_unavailable(
        service: ThemeService,
        context: ThemeRecoveryContext,
        config: ThemeRuntimeConfig,
    ) -> Result<Self, ThemeRuntimeStartError> {
        if context.home_id != service.home().home_id() {
            return Err(start_error(ThemeRuntimeFailureClass::Identity));
        }
        let appearance = AppearanceCoordinator::new_unavailable_context(
            config.appearance,
            service.home(),
            &context.appearance,
        );
        Ok(Self {
            config,
            retired_state: service.diagnostics(),
            service: Some(service),
            repository: None,
            subscription: None,
            appearance: Some(appearance),
            settings: None,
            pending_settings_publication: None,
            selection_available: Arc::new(AtomicBool::new(false)),
            preparation_admission: Arc::new(AtomicBool::new(true)),
            active: None,
            last_observed_document: None,
            refresh_reread_needed: false,
            pages_read: 0,
            watch_batches: 0,
            watch_hints: 0,
            app_coalesced_hints: 0,
            app_overflow_hints: 0,
            settings_outcomes: 0,
            repository_requests: 0,
            last_failure: Some(ThemeRuntimeFailureClass::Reconciliation),
        })
    }

    pub fn start(
        store: &HomeStore,
        service: ThemeService,
        domain_revision: DomainRevision,
        active_setting: Option<&SettingRecord>,
        config: ThemeRuntimeConfig,
    ) -> Result<Self, ThemeRuntimeStartError> {
        let subscription = service
            .subscribe_changes(
                store,
                config.watch_interval,
                config.watch_queue_capacity,
                config.watch_max_entries_per_poll,
                config.watch_max_file_bytes,
            )
            .map_err(|_| start_error(ThemeRuntimeFailureClass::Subscription))?;
        Self::load_started(
            store,
            service,
            domain_revision,
            active_setting,
            config,
            subscription,
        )
    }

    pub(super) fn load_started(
        store: &HomeStore,
        service: ThemeService,
        domain_revision: DomainRevision,
        active_setting: Option<&SettingRecord>,
        config: ThemeRuntimeConfig,
        subscription: ThemeChangeSubscription,
    ) -> Result<Self, ThemeRuntimeStartError> {
        let mut runtime = Self::load_initial(
            store.into(),
            service,
            domain_revision,
            active_setting,
            config,
        )?;
        runtime.subscription = Some(subscription);
        Ok(runtime)
    }

    pub(super) fn load_initial(
        access: load::ThemeLoadAccess<'_>,
        service: ThemeService,
        domain_revision: DomainRevision,
        active_setting: Option<&SettingRecord>,
        config: ThemeRuntimeConfig,
    ) -> Result<Self, ThemeRuntimeStartError> {
        let active = ThemeService::active_theme_from_setting(active_setting)
            .map_err(|_| start_error(ThemeRuntimeFailureClass::Identity))?;
        let settings = service.settings_identity(domain_revision, active_setting);
        let (repository, repository_failure) = match access.observe(&service, config) {
            Ok(repository) => (Some(repository), None),
            Err(error) => (None, Some(map_repository_load_failure(&error))),
        };
        let mut pages_read = 0;
        let loaded = match (&active, &repository) {
            (None, _) => Err(ThemeLoadFailure::ActiveIdentityMissing),
            (Some(_), None) => Err(ThemeLoadFailure::RepositoryUnavailable),
            (Some(active), Some(repository)) => load_prepared(
                &service,
                access,
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
        let last_failure =
            repository_failure.or_else(|| startup.failure().map(map_startup_failure));
        let initial = startup.appearance().clone();
        let last_observed_document = installed_identity(&initial).cloned();
        let appearance = AppearanceCoordinator::new(config.appearance, initial);
        Ok(Self {
            config,
            retired_state: service.diagnostics(),
            service: Some(service),
            repository,
            subscription: None,
            appearance: Some(appearance),
            settings: Some(settings),
            pending_settings_publication: None,
            selection_available: Arc::new(AtomicBool::new(true)),
            preparation_admission: Arc::new(AtomicBool::new(true)),
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

    pub fn selection_available(&self) -> bool {
        self.selection_available.load(Ordering::Acquire)
    }

    pub fn attach_publication_target(
        &mut self,
        target: Arc<dyn AppearancePublicationTarget>,
    ) -> Result<(), AppearancePublicationFailure> {
        self.appearance
            .as_mut()
            .ok_or(AppearancePublicationFailure::Unavailable)?
            .attach_publication_target(target)
    }

    pub fn stop_preview(
        &mut self,
        store: &HomeStore,
    ) -> Result<StopPreviewResult, PreviewPublicationError> {
        let intent = self
            .appearance
            .as_mut()
            .ok_or(PreviewPublicationError::CandidateMismatch)?
            .reserve_stop_preview()?;
        if !self.selection_available() {
            return Err(PreviewPublicationError::CandidateMismatch);
        }
        let was_preview = self
            .appearance
            .as_ref()
            .is_some_and(|appearance| appearance.current().is_preview());
        match self.retry_current_durable(store) {
            Ok(RepositoryAppearanceResult::Retained(_)) | Err(_) => {
                return Err(PreviewPublicationError::CandidateMismatch);
            }
            Ok(_) => {}
        }
        let result = self
            .appearance
            .as_mut()
            .ok_or(PreviewPublicationError::CandidateMismatch)?
            .finish_stop_preview(intent)?;
        Ok(
            if was_preview && matches!(result, StopPreviewResult::AlreadyStopped) {
                StopPreviewResult::Restored(self.current().expect("running appearance"))
            } else {
                result
            },
        )
    }

    pub fn begin_preview(
        &mut self,
        source: PreviewSource,
        candidate: PreviewCandidateIdentity,
    ) -> Result<PreviewPublicationRequest, PreviewPublicationError> {
        if !self.selection_available() {
            return Err(PreviewPublicationError::CandidateMismatch);
        }
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
        if !self.selection_available() {
            return Err(PreviewPublicationError::CandidateMismatch);
        }
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
        self.preparation_admission.store(false, Ordering::Release);
        self.selection_available.store(false, Ordering::Release);
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
