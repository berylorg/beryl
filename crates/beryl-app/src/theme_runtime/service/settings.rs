use super::*;

mod catalog;

#[derive(Clone)]
pub struct ThemeSettingsSource {
    service: ThemeService,
    config: ThemeRuntimeConfig,
    admission: Arc<AtomicBool>,
    selection_available: Arc<AtomicBool>,
}

impl ThemeSettingsSource {
    pub(crate) fn verify_current_selection(
        &self,
        store: &HomeStore,
        expected: ThemeSettingsIdentity,
        active: Option<&InstalledThemeId>,
    ) -> Result<(), ThemeRuntimeFailureClass> {
        self.check(store)?;
        let state = beryl_state::BerylState::reacquire(store)
            .map_err(|_| ThemeRuntimeFailureClass::Settings)?;
        let settings = state.settings();
        let revision = settings
            .revision(store)
            .map_err(|_| ThemeRuntimeFailureClass::Settings)?;
        let record = settings
            .setting(store, beryl_state::SettingKey::ActiveThemeId)
            .map_err(|_| ThemeRuntimeFailureClass::Settings)?;
        let selected = ThemeService::active_theme_from_setting(record.as_ref())
            .map_err(|_| ThemeRuntimeFailureClass::Settings)?;
        if self.service.settings_identity(revision, record.as_ref()) != expected
            || selected.as_ref() != active
            || settings
                .revision(store)
                .map_err(|_| ThemeRuntimeFailureClass::Settings)?
                != revision
        {
            return Err(ThemeRuntimeFailureClass::Settings);
        }
        Ok(())
    }

    pub(crate) fn prepare_current_bound(
        &self,
        store: &HomeStore,
        settings: ThemeSettingsIdentity,
        active: Option<&InstalledThemeId>,
        previous: Option<&ThemeDocumentIdentity>,
    ) -> Result<PreparedThemeAppearance, ThemeRuntimeFailureClass> {
        self.verify_current_selection(store, settings, active)?;
        let prepared = match active {
            None => PreparedThemeAppearance::fallback(settings),
            Some(active) => {
                let previous = previous.filter(|previous| {
                    previous.manifest().home() == self.home() && previous.theme_id() == active
                });
                let repository = load::ThemeLoadAccess::from(store)
                    .observe(&self.service, self.config)
                    .map_err(|error| map_repository_load_failure(&error))?;
                let mut pages = 0;
                load_prepared(
                    &self.service,
                    store,
                    &repository,
                    active,
                    settings,
                    previous,
                    self.config,
                    &mut pages,
                )
                .map_err(|error| map_startup_failure(&error))?
                .0
            }
        };
        self.verify_current_selection(store, settings, active)?;
        Ok(prepared)
    }

    pub(crate) fn revalidate_prepared(
        &self,
        store: &HomeStore,
        prepared: &PreparedThemeAppearance,
        active: Option<&InstalledThemeId>,
    ) -> Result<(), ThemeRuntimeFailureClass> {
        let current = self.prepare_current_bound(
            store,
            prepared.settings(),
            active,
            installed_identity(prepared),
        )?;
        if &current != prepared {
            return Err(ThemeRuntimeFailureClass::Identity);
        }
        Ok(())
    }

    pub fn home(&self) -> beryl_state::ThemeHomeIdentity {
        self.service.home()
    }

    pub(crate) fn service(&self) -> &ThemeService {
        &self.service
    }

    pub(crate) fn max_manifest_bytes(&self) -> NonZeroU64 {
        self.config.max_manifest_bytes
    }

    pub(crate) fn check(&self, store: &HomeStore) -> Result<(), ThemeRuntimeFailureClass> {
        if !self.admission.load(Ordering::Acquire) {
            return Err(ThemeRuntimeFailureClass::Retired);
        }
        if store.home_id() != self.home().home_id()
            || store.health().generation() != Some(self.home().home_generation())
        {
            return Err(ThemeRuntimeFailureClass::Identity);
        }
        store
            .home_revision()
            .map_err(|_| ThemeRuntimeFailureClass::Settings)?;
        Ok(())
    }

    pub fn prepare_selection(
        &self,
        store: &HomeStore,
        draft: ThemeDraftIdentity,
        revision: ThemeDraftRevision,
        intended: ThemeSettingsIdentity,
        active: Option<InstalledThemeId>,
    ) -> Result<ConfirmedSettingsTheme, ThemeRuntimeFailureClass> {
        if !self.admission.load(Ordering::Acquire) {
            return Err(ThemeRuntimeFailureClass::Retired);
        }
        if !self.selection_available.load(Ordering::Acquire) {
            return Err(ThemeRuntimeFailureClass::Reconciliation);
        }
        self.check(store)?;
        if intended.home() != self.home() {
            return Err(ThemeRuntimeFailureClass::Identity);
        }
        let prepared = match active.as_ref() {
            None => PreparedThemeAppearance::fallback(intended),
            Some(active) => {
                let repository = load::ThemeLoadAccess::from(store)
                    .observe(&self.service, self.config)
                    .map_err(|error| map_repository_load_failure(&error))?;
                let mut pages = 0;
                load_prepared(
                    &self.service,
                    store,
                    &repository,
                    active,
                    intended,
                    None,
                    self.config,
                    &mut pages,
                )
                .map_err(|failure| map_startup_failure(&failure))?
                .0
            }
        };
        self.check(store)?;
        if !self.selection_available.load(Ordering::Acquire) {
            return Err(ThemeRuntimeFailureClass::Reconciliation);
        }
        Ok(ConfirmedSettingsTheme::new(
            draft, revision, intended, active, prepared,
        ))
    }
}

impl ThemeRuntime {
    pub fn settings_source(&self) -> Result<ThemeSettingsSource, ThemeRuntimeFailureClass> {
        self.ensure_running()?;
        Ok(ThemeSettingsSource {
            service: self.service.as_ref().expect("running service").clone(),
            config: self.config,
            admission: Arc::clone(&self.preparation_admission),
            selection_available: Arc::clone(&self.selection_available),
        })
    }
}
