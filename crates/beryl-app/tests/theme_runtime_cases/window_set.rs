use super::*;
use beryl_app::theme_runtime::{
    AdapterRegistrationError, AppearancePublicationFailure, AppearancePublicationTarget,
    AppearanceWindowSetSnapshot, StalePublicationReason, ThemeRuntime, WindowEpochExhausted,
    WindowSetEpoch,
};
use std::ops::{Deref, DerefMut};

struct TestSetState {
    snapshot: AppearanceWindowSetSnapshot,
    adapters: Vec<Arc<TestAdapter>>,
}

struct TestWindowSet(Mutex<TestSetState>);

impl TestWindowSet {
    fn new(current: Arc<AppearanceGeneration>, capacity: usize) -> Arc<Self> {
        Arc::new(Self(Mutex::new(TestSetState {
            snapshot: AppearanceWindowSetSnapshot {
                epoch: WindowSetEpoch::initial(),
                count: 0,
                capacity,
                current,
                active: true,
            },
            adapters: Vec::new(),
        })))
    }

    fn register(&self, adapter: Arc<TestAdapter>) -> Result<(), AdapterRegistrationError> {
        let mut state = self.0.lock().unwrap();
        if state.adapters.len() == state.snapshot.capacity {
            return Err(AdapterRegistrationError::CapacityReached);
        }
        if state
            .adapters
            .iter()
            .any(|entry| entry.id() == adapter.id())
        {
            return Err(AdapterRegistrationError::DuplicateIdentity(adapter.id()));
        }
        let next = state
            .snapshot
            .epoch
            .checked_next()
            .map_err(|_| AdapterRegistrationError::WindowEpochExhausted)?;
        adapter
            .prepare(state.snapshot.current.clone())
            .map_err(|class| AdapterRegistrationError::Preparation {
                adapter: adapter.id(),
                class,
            })?
            .commit();
        state.adapters.push(adapter);
        state.snapshot.count = state.adapters.len();
        state.snapshot.epoch = next;
        Ok(())
    }

    fn unregister(&self, id: WindowAdapterId) -> Result<bool, WindowEpochExhausted> {
        let mut state = self.0.lock().unwrap();
        let Some(index) = state.adapters.iter().position(|entry| entry.id() == id) else {
            return Ok(false);
        };
        state.snapshot.epoch = state.snapshot.epoch.checked_next()?;
        state.adapters.remove(index);
        state.snapshot.count = state.adapters.len();
        Ok(true)
    }
}

impl AppearancePublicationTarget for TestWindowSet {
    fn snapshot(&self) -> AppearanceWindowSetSnapshot {
        self.0.lock().unwrap().snapshot.clone()
    }

    fn publish(
        &self,
        epoch: WindowSetEpoch,
        previous: Arc<AppearanceGeneration>,
        generation: Arc<AppearanceGeneration>,
    ) -> Result<(), AppearancePublicationFailure> {
        let mut state = self.0.lock().unwrap();
        if !state.snapshot.active {
            return Err(AppearancePublicationFailure::Unavailable);
        }
        if epoch != state.snapshot.epoch {
            return Err(AppearancePublicationFailure::Stale(
                StalePublicationReason::WindowSetEpoch,
            ));
        }
        assert!(Arc::ptr_eq(&previous, &state.snapshot.current));
        let prepared = state
            .adapters
            .iter()
            .map(|adapter| {
                adapter.prepare(generation.clone()).map_err(|class| {
                    AppearancePublicationFailure::Adapter {
                        adapter: adapter.id(),
                        class,
                    }
                })
            })
            .collect::<Result<Vec<_>, _>>()?;
        for publication in prepared {
            publication.commit();
        }
        state.snapshot.current = generation;
        Ok(())
    }

    fn is_publication_thread(&self) -> bool {
        false
    }

    fn retire(&self) {
        let mut state = self.0.lock().unwrap();
        state.snapshot.active = false;
        state.adapters.clear();
        state.snapshot.count = 0;
    }
}

pub struct TestCoordinator {
    coordinator: AppearanceCoordinator,
    windows: Arc<TestWindowSet>,
}

impl TestCoordinator {
    pub fn new(mut coordinator: AppearanceCoordinator) -> Self {
        let windows = TestWindowSet::new(
            coordinator.current(),
            coordinator.diagnostics().adapter_capacity().get(),
        );
        coordinator
            .attach_publication_target(windows.clone())
            .unwrap();
        Self {
            coordinator,
            windows,
        }
    }

    pub fn register_adapter(
        &mut self,
        adapter: Arc<TestAdapter>,
    ) -> Result<(), AdapterRegistrationError> {
        self.windows.register(adapter)
    }
    pub fn unregister_adapter(
        &mut self,
        id: WindowAdapterId,
    ) -> Result<bool, WindowEpochExhausted> {
        self.windows.unregister(id)
    }
    pub fn retire(self) {
        self.coordinator.retire();
    }
}

impl Deref for TestCoordinator {
    type Target = AppearanceCoordinator;
    fn deref(&self) -> &Self::Target {
        &self.coordinator
    }
}

impl DerefMut for TestCoordinator {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.coordinator
    }
}

pub fn attach_test_adapter(runtime: &mut ThemeRuntime, adapter: Arc<TestAdapter>) {
    let capacity = runtime
        .diagnostics()
        .appearance()
        .unwrap()
        .adapter_capacity()
        .get();
    let windows = TestWindowSet::new(runtime.current().unwrap(), capacity);
    runtime.attach_publication_target(windows.clone()).unwrap();
    windows.register(adapter).unwrap();
}
