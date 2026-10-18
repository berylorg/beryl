use super::super::AdapterFailureClass;
use super::*;

impl GpuiAppearanceWindowSet {
    #[cfg(feature = "test-faults")]
    pub fn test_begin_hidden_registration(
        owner: Entity<Self>,
        adapter: Box<dyn AppearanceWindowAdapter>,
        app: &mut App,
    ) -> Task<Result<(), AdapterRegistrationError>> {
        Self::begin_registration(owner, adapter, app)
    }

    pub(crate) fn begin_registration(
        owner: Entity<Self>,
        adapter: Box<dyn AppearanceWindowAdapter>,
        app: &mut App,
    ) -> Task<Result<(), AdapterRegistrationError>> {
        Self::begin_registration_with(
            owner,
            adapter,
            |adapter, generation, app| adapter.prepare(generation, app),
            app,
        )
    }

    pub(crate) fn begin_registration_with(
        owner: Entity<Self>,
        adapter: Box<dyn AppearanceWindowAdapter>,
        prepare: impl Fn(
            &dyn AppearanceWindowAdapter,
            Arc<AppearanceGeneration>,
            &mut App,
        ) -> Result<Box<dyn PreparedWindowAppearance>, AdapterFailureClass>
        + 'static,
        app: &mut App,
    ) -> Task<Result<(), AdapterRegistrationError>> {
        app.spawn(async move |cx| {
            let id = adapter.id();
            loop {
                let (snapshot, mut prepared) = owner
                    .update(cx, |owner, cx| {
                        owner.prepare_registration(adapter.as_ref(), &prepare, cx)
                    })
                    .map_err(|_| AdapterRegistrationError::Preparation {
                        adapter: id,
                        class: AdapterFailureClass::Unavailable,
                    })??;
                let native = cx
                    .update(|cx| prepared.prepare_native(cx))
                    .map_err(|_| AdapterRegistrationError::Preparation {
                        adapter: id,
                        class: AdapterFailureClass::Unavailable,
                    })?
                    .map_err(|class| AdapterRegistrationError::Preparation {
                        adapter: id,
                        class,
                    })?;
                let readiness = match native {
                    Some(task) => task.await,
                    None => Ok(()),
                };
                let current = owner
                    .read_with(cx, |owner, _| owner.target.snapshot())
                    .map_err(|_| AdapterRegistrationError::Preparation {
                        adapter: id,
                        class: AdapterFailureClass::Unavailable,
                    })?;
                if !current.active {
                    return Err(AdapterRegistrationError::Preparation {
                        adapter: id,
                        class: AdapterFailureClass::Unavailable,
                    });
                }
                if current.epoch != snapshot.epoch
                    || !Arc::ptr_eq(&current.current, &snapshot.current)
                {
                    continue;
                }
                readiness.map_err(|class| AdapterRegistrationError::Preparation {
                    adapter: id,
                    class,
                })?;
                return owner
                    .update(cx, |owner, cx| {
                        owner.finish_registration(adapter, snapshot, prepared, cx)
                    })
                    .map_err(|_| AdapterRegistrationError::Preparation {
                        adapter: id,
                        class: AdapterFailureClass::Unavailable,
                    })?;
            }
        })
    }

    pub(super) fn prepare_registration(
        &self,
        adapter: &dyn AppearanceWindowAdapter,
        prepare: impl FnOnce(
            &dyn AppearanceWindowAdapter,
            Arc<AppearanceGeneration>,
            &mut App,
        ) -> Result<Box<dyn PreparedWindowAppearance>, AdapterFailureClass>,
        app: &mut App,
    ) -> Result<
        (
            AppearanceWindowSetSnapshot,
            Box<dyn PreparedWindowAppearance>,
        ),
        AdapterRegistrationError,
    > {
        let snapshot = self.target.snapshot();
        let id = adapter.id();
        if !snapshot.active {
            return Err(AdapterRegistrationError::Preparation {
                adapter: id,
                class: AdapterFailureClass::Unavailable,
            });
        }
        if self.adapters.len() == snapshot.capacity {
            return Err(AdapterRegistrationError::CapacityReached);
        }
        if self.adapters.iter().any(|existing| existing.id() == id) {
            return Err(AdapterRegistrationError::DuplicateIdentity(id));
        }
        snapshot
            .epoch
            .checked_next()
            .map_err(|_| AdapterRegistrationError::WindowEpochExhausted)?;
        let prepared = prepare(adapter, snapshot.current.clone(), app)
            .map_err(|class| AdapterRegistrationError::Preparation { adapter: id, class })?;
        Ok((snapshot, prepared))
    }

    pub(super) fn finish_registration(
        &mut self,
        adapter: Box<dyn AppearanceWindowAdapter>,
        snapshot: AppearanceWindowSetSnapshot,
        prepared: Box<dyn PreparedWindowAppearance>,
        app: &mut App,
    ) -> Result<(), AdapterRegistrationError> {
        let id = adapter.id();
        prepared
            .validate(app)
            .map_err(|class| AdapterRegistrationError::Preparation { adapter: id, class })?;
        let current = self.target.snapshot();
        if !current.active
            || current.epoch != snapshot.epoch
            || !Arc::ptr_eq(&current.current, &snapshot.current)
            || self.adapters.len() == current.capacity
            || self.adapters.iter().any(|existing| existing.id() == id)
        {
            return Err(AdapterRegistrationError::Preparation {
                adapter: id,
                class: AdapterFailureClass::Unavailable,
            });
        }
        let epoch = current
            .epoch
            .checked_next()
            .map_err(|_| AdapterRegistrationError::WindowEpochExhausted)?;
        prepared.commit(app);
        self.adapters.push(adapter);
        let mut state = self.target.state.lock().expect("appearance mailbox lock");
        let retired = !state.snapshot.active;
        state.snapshot.count = if retired { 0 } else { self.adapters.len() };
        state.snapshot.epoch = epoch;
        drop(state);
        if retired {
            self.adapters.clear();
        }
        app.refresh_windows();
        Ok(())
    }
}
