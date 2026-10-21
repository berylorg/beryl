use std::{
    sync::{Arc, OnceLock},
    task::{Wake, Waker},
};

use super::ProjectionConnectionService;

pub(super) struct HomeMutationWake {
    scheduler: Waker,
    catalog: OnceLock<Waker>,
    generated: OnceLock<Waker>,
}

struct GeneratedMutationWake(Arc<HomeMutationWake>);

impl Wake for GeneratedMutationWake {
    fn wake(self: Arc<Self>) { self.0.notify_generated(); }
    fn wake_by_ref(self: &Arc<Self>) { self.0.notify_generated(); }
}

impl HomeMutationWake {
    pub(in crate::cas_projection) fn generated_waker(self: &Arc<Self>) -> Waker {
        Waker::from(Arc::new(GeneratedMutationWake(Arc::clone(self))))
    }
    pub(super) fn new(scheduler: Waker) -> Self {
        Self {
            scheduler,
            catalog: OnceLock::new(),
            generated: OnceLock::new(),
        }
    }

    fn notify(&self) {
        self.scheduler.wake_by_ref();
        if let Some(catalog) = self.catalog.get() {
            catalog.wake_by_ref();
        }
        self.notify_generated();
    }

    pub(in crate::cas_projection) fn notify_generated(&self) {
        if let Some(generated) = self.generated.get() { generated.wake_by_ref(); }
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
    pub(crate) fn install_generated_media_waker(&self, waker: Waker) {
        assert!(self.mutation_wake.generated.set(waker).is_ok(), "one generated observer per service graph");
    }

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
