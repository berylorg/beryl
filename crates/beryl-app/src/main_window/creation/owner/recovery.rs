use super::*;

pub(crate) struct PreparedCreationRebind {
    owner: Entity<MainWindowCreationOwner>,
    services: Arc<MainWindowCreationServices>,
    appearance: Entity<GpuiAppearanceWindowSet>,
    generation: beryl_home_store::HomeGeneration,
}

impl PreparedCreationRebind {
    pub(crate) fn require_live_source(&self) -> Result<(), String> {
        self.services.validate_source()
    }

    pub(crate) fn apply(self, app: &mut App) {
        self.owner.update(app, |owner, cx| {
            owner.services = self.services;
            owner.appearance = self.appearance;
            owner.generation = self.generation;
            owner.fenced = false;
            cx.notify();
        });
    }
}

impl MainWindowCreationOwner {
    pub(crate) fn validate_recovered_process(
        home: &beryl_home_store::HomeServiceReference,
        app: &App,
    ) -> Result<(), String> {
        let Some(global) = app.try_global::<CreationProcessOwner>() else {
            return Ok(());
        };
        let owner = global._owner.read(app);
        if !owner.entries.is_empty()
            || owner.services.store.home_id() != home.home_id()
            || home.health().generation().is_none()
            || home.health().generation() == Some(owner.generation)
        {
            return Err(
                "creation replacement requires settled entries and a fresh same-home graph".into(),
            );
        }
        Ok(())
    }

    pub(crate) fn prepare_recovered_process(
        services: &crate::app_services::ProcessServiceOwner,
        appearance: &Entity<GpuiAppearanceWindowSet>,
        app: &App,
    ) -> Result<Option<PreparedCreationRebind>, String> {
        let Some(global) = app.try_global::<CreationProcessOwner>() else {
            return Ok(None);
        };
        let owner = global._owner.clone();
        let retained = owner.read(app);
        if !retained.entries.is_empty() {
            return Err("creation replacement retains unfinished entries".into());
        }
        let replacement =
            services.recovered_creation_services(&retained.services, retained.generation)?;
        let generation = replacement
            .store
            .health()
            .generation()
            .ok_or("recovered creation generation is unavailable")?;
        if generation == retained.generation {
            return Err("creation replacement retained the old generation".into());
        }
        Ok(Some(PreparedCreationRebind {
            owner,
            services: replacement,
            appearance: appearance.clone(),
            generation,
        }))
    }
}
