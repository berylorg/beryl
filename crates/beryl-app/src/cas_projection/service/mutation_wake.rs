use std::{
    sync::{Arc, OnceLock},
    task::{Wake, Waker},
};

use super::ProjectionConnectionService;

pub(super) struct HomeMutationWake {
    scheduler: Waker,
    catalog: OnceLock<Waker>,
}

impl HomeMutationWake {
    pub(super) fn new(scheduler: Waker) -> Self {
        Self {
            scheduler,
            catalog: OnceLock::new(),
        }
    }

    fn notify(&self) {
        self.scheduler.wake_by_ref();
        if let Some(catalog) = self.catalog.get() {
            catalog.wake_by_ref();
        }
    }
}

impl Wake for HomeMutationWake {
    fn wake(self: Arc<Self>) {
        self.notify();
    }

    fn wake_by_ref(self: &Arc<Self>) {
        self.notify();
    }
}

impl ProjectionConnectionService {
    pub(crate) fn catalog_source_start_gate(&self) -> Arc<super::InitialStartGate> {
        Arc::clone(&self.initial_start)
    }

    pub(crate) fn install_catalog_source_waker(&self, waker: Waker) {
        assert!(
            self.mutation_wake.catalog.set(waker).is_ok(),
            "a private service graph installs its catalog observer once"
        );
    }
}

#[cfg(test)]
mod tests {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/unit/home_mutation_wake.rs"
    ));
}
