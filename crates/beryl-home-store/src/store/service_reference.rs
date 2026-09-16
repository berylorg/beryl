use std::{ops::Deref, sync::Arc};

use super::HomeStore;

#[derive(Debug)]
pub struct HomeServiceReference {
    store: HomeStore,
}

impl Clone for HomeServiceReference {
    fn clone(&self) -> Self {
        self.store.service_reference()
    }
}

impl Deref for HomeServiceReference {
    type Target = HomeStore;

    fn deref(&self) -> &Self::Target {
        &self.store
    }
}

impl HomeStore {
    pub fn service_reference(&self) -> HomeServiceReference {
        HomeServiceReference {
            store: Self {
                generation: Arc::clone(&self.generation),
                registrations: Arc::clone(&self.registrations),
                writer: Arc::clone(&self.writer),
                mutation_boundary: Arc::clone(&self.mutation_boundary),
                theme_mutation: Arc::clone(&self.theme_mutation),
                theme_watcher: Arc::clone(&self.theme_watcher),
                writer_id: self.writer_id,
                health: Arc::clone(&self.health),
                faults: self.faults.clone(),
                reconciliation: self.reconciliation.clone(),
                scrub: Arc::clone(&self.scrub),
                lifecycle: Arc::clone(&self.lifecycle),
                storage_profile: self.storage_profile,
                database_path: self.database_path.clone(),
                home_id: self.home_id,
                schema: self.schema,
                recovery_transferred: false,
                owns_lifecycle: false,
                admitted_generation: self.admitted_generation,
            },
        }
    }
}
