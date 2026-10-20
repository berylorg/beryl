use super::*;
use crate::theme_runtime::ThemeRuntime;

#[derive(Debug)]
pub(crate) enum AppThemeLoadError {
    AlreadyAttempted,
    Start(ThemeRuntimeStartError),
    Settings(crate::settings_service::SettingsServiceError),
    SettingsBinding,
    ThemeSource(crate::theme_runtime::ThemeRuntimeFailureClass),
    ThemeBroker(crate::theme_runtime::ThemeSettingsBridgeError),
}

impl ProcessServiceOwner {
    pub(crate) fn settings_theme_retirement(
        &self,
    ) -> Option<&crate::theme_runtime::ThemeSettingsRetirement> {
        self.settings_theme_retirement.as_ref()
    }
    pub(super) fn next_settings_generation(&self) -> Result<u64, AppServiceOpenError> {
        self.settings_generation
            .fetch_update(
                std::sync::atomic::Ordering::AcqRel,
                std::sync::atomic::Ordering::Acquire,
                |value| value.checked_add(1),
            )
            .map(|previous| previous + 1)
            .map_err(|_| crate::settings_service::SettingsServiceError::Stale.into())
    }
    pub(crate) fn activity_reader(&self) -> Option<crate::activity_service::ActivityWindowReader> {
        Some(
            self.graph()?
                .activity_reader()?
                .with_height_custody(self.activity_heights.clone()),
        )
    }
    pub(crate) fn graph_mut(&mut self) -> Option<&mut PublishedAppServices> {
        self.graph.as_mut()
    }
}

impl PublishedAppServices {
    pub(crate) fn settings_theme_failure(
        &self,
    ) -> Option<crate::theme_runtime::ThemeRuntimeFailureClass> {
        self.loaded_theme
            .as_ref()
            .and_then(|runtime| runtime.diagnostics().last_failure())
    }

    pub(crate) fn take_settings_theme_hints(
        &mut self,
    ) -> Result<Vec<beryl_state::ThemeChangeHint>, crate::theme_runtime::ThemeRuntimeFailureClass>
    {
        self.loaded_theme
            .as_mut()
            .ok_or(crate::theme_runtime::ThemeRuntimeFailureClass::Retired)?
            .take_change_hints()
    }

    pub(crate) fn consume_settings_theme_hints(
        &mut self,
        hints: &[beryl_state::ThemeChangeHint],
    ) -> Result<
        crate::theme_runtime::ThemeWatchDrainResult,
        crate::theme_runtime::ThemeRuntimeFailureClass,
    > {
        let home = self
            .home
            .as_ref()
            .ok_or(crate::theme_runtime::ThemeRuntimeFailureClass::Retired)?;
        self.loaded_theme
            .as_mut()
            .ok_or(crate::theme_runtime::ThemeRuntimeFailureClass::Retired)?
            .consume_change_hints(home, hints)
    }

    pub(crate) fn sync_settings_theme_identity(
        &mut self,
    ) -> Result<(), crate::theme_runtime::ThemeRuntimeFailureClass> {
        use crate::theme_runtime::ThemeRuntimeFailureClass;
        let baseline = self
            .settings
            .as_ref()
            .ok_or(ThemeRuntimeFailureClass::Settings)?
            .snapshot()
            .map_err(|_| ThemeRuntimeFailureClass::Settings)?;
        let runtime = self
            .loaded_theme
            .as_mut()
            .ok_or(ThemeRuntimeFailureClass::Retired)?;
        let home = runtime
            .current()
            .ok_or(ThemeRuntimeFailureClass::Retired)?
            .home();
        let source = baseline.identity();
        if source.home_id != home.home_id() || source.home_generation != home.home_generation() {
            return Err(ThemeRuntimeFailureClass::Identity);
        }
        match baseline.applied().active_theme() {
            Ok(active) => runtime.sync_applied_selection(
                beryl_state::ThemeSettingsIdentity::new(
                    home,
                    source.settings_revision,
                    baseline.record_revision(beryl_state::SettingKey::ActiveThemeId),
                ),
                active.cloned(),
            )?,
            Err(_) => runtime.mark_selection_unavailable(),
        }
        self.update_settings_theme_selection()
            .map_err(|_| ThemeRuntimeFailureClass::Settings)
    }

    pub(crate) fn take_settings_theme_completion(
        &self,
    ) -> Option<crate::theme_runtime::ThemeSettingsCompletion> {
        self.settings_themes.as_ref()?.take_completion()
    }

    pub(crate) fn update_settings_theme_selection(
        &self,
    ) -> Result<(), crate::theme_runtime::ThemeSettingsBridgeError> {
        if let Some(broker) = self.settings_themes.as_ref() {
            broker.set_active_selection(
                self.loaded_theme
                    .as_ref()
                    .and_then(|runtime| runtime.proven_selection()),
            )?;
        }
        Ok(())
    }
    pub(crate) fn settings(&self) -> Option<&Arc<crate::settings_service::SettingsService>> {
        self.settings.as_ref()
    }

    pub(crate) fn settings_theme_reader(
        &self,
    ) -> Option<crate::theme_runtime::SettingsThemeReader> {
        self.settings_themes.as_ref().map(|broker| broker.reader())
    }
    pub(crate) fn activity_reader(&self) -> Option<crate::activity_service::ActivityWindowReader> {
        if self.shutdown.is_some() {
            return None;
        }
        Some(self.activity.as_ref()?.window_reader(
            self.state.session(),
            self.cas().home_mutation_observer(),
            Arc::downgrade(self.restore_lifetime.as_ref()?),
        ))
    }
    pub(crate) fn catalog_query_reader(&self) -> crate::catalog_query::PublishedCatalogQueryReader {
        self.catalog_query
            .as_ref()
            .expect("published catalog query service")
            .reader()
    }
    pub(crate) fn catalog_source_reader(&self) -> crate::catalog_readiness::CatalogSourceReader {
        self.catalog_source
            .as_ref()
            .expect("published catalog source coordinator")
            .reader()
    }

    pub(crate) fn current_appearance(
        &self,
    ) -> Option<Arc<crate::theme_runtime::AppearanceGeneration>> {
        self.theme
            .as_ref()
            .map(|theme| theme.current())
            .or_else(|| self.loaded_theme.as_ref().and_then(ThemeRuntime::current))
    }

    pub(crate) fn restored_window_attempt(
        &self,
    ) -> Result<crate::main_window::RestoredWindowPreparationAttempt, String> {
        if self.shutdown.is_some() {
            return Err("service graph is shutting down".to_owned());
        }
        crate::main_window::RestoredWindowPreparationAttempt::new(
            Arc::new(self.home().service_reference()),
            self.state.session(),
            self.syndic.clone(),
            Arc::downgrade(
                self.restore_lifetime
                    .as_ref()
                    .ok_or_else(|| "restore service generation is retired".to_owned())?,
            ),
        )
    }

    pub(crate) fn home(&self) -> &HomeStore {
        self.home.as_ref().expect("published home")
    }
    pub(crate) fn state(&self) -> &BerylState {
        &self.state
    }
    pub(crate) fn syndic(&self) -> &SyndicStorage {
        &self.syndic
    }
    pub(crate) fn cas(&self) -> &ProjectionConnectionService {
        self.cas.as_ref().expect("published CAS")
    }
    pub(crate) fn sessions(&self) -> &ScheduledExecutionSessions {
        &self.sessions
    }
    pub(crate) fn attention(&self) -> &Arc<ProcessLifecycleAttentionPool> {
        &self.attention
    }
    pub(crate) fn marker(&self) -> DraftMarkerSealService {
        self.marker.as_ref().expect("published marker").clone()
    }
    pub(crate) fn activity(&self) -> &ActivityService {
        self.activity.as_ref().expect("published Activity")
    }
    pub(crate) fn theme(&mut self) -> Option<&mut ThemeRuntime> {
        self.loaded_theme.as_mut()
    }

    pub(crate) fn release_theme(&mut self) -> Result<(), AppThemeLoadError> {
        let settings = Arc::new(
            self.prepared_settings
                .take()
                .ok_or(AppThemeLoadError::AlreadyAttempted)?
                .release_published(self.home().service_reference())
                .map_err(AppThemeLoadError::Settings)?,
        );
        self.settings = Some(Arc::clone(&settings));
        self.cas()
            .bind_applied_settings(settings.applied_reader())
            .map_err(|_| AppThemeLoadError::SettingsBinding)?;
        let prepared = self
            .theme
            .take()
            .ok_or(AppThemeLoadError::AlreadyAttempted)?;
        self.loaded_theme = Some(
            prepared
                .release_published(self.home())
                .map_err(AppThemeLoadError::Start)?,
        );
        let runtime = self.loaded_theme.as_ref().expect("released theme runtime");
        let source = runtime
            .settings_source()
            .map_err(AppThemeLoadError::ThemeSource)?;
        settings
            .bind_theme_source(source.clone())
            .map_err(AppThemeLoadError::Settings)?;
        let broker = crate::theme_runtime::SettingsThemeBroker::start(
            self.home().service_reference(),
            source,
        )
        .map_err(AppThemeLoadError::ThemeBroker)?;
        broker
            .set_active_selection(runtime.proven_selection())
            .map_err(AppThemeLoadError::ThemeBroker)?;
        self.settings_themes = Some(broker);
        Ok(())
    }
}
