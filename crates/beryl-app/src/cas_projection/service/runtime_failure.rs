use super::*;
use std::sync::Weak;

use crate::cas_projection::{RuntimeFailureSnapshot, runtime_interest::RuntimeInterestOwner};

#[derive(Clone)]
pub struct RuntimeFailureReader {
    home: Weak<HomeServiceReference>,
    owner: Weak<RuntimeInterestOwner>,
    home_id: BerylHomeId,
    home_generation: HomeGeneration,
    service_generation: ProjectionServiceGeneration,
    storage: SyndicStorage,
    commands: LiveCommandAuthorizer,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SelectedRuntimeFailureObservation {
    Unknown,
    Unavailable {
        execution: ExecutionBinding,
        failure: RuntimeFailureSnapshot,
    },
}

impl ProjectionConnectionService {
    pub fn runtime_failure_reader(&self) -> RuntimeFailureReader {
        RuntimeFailureReader {
            home: self.home.as_ref().map_or_else(Weak::new, Arc::downgrade),
            owner: self
                .runtime_interest
                .as_ref()
                .map_or_else(Weak::new, Arc::downgrade),
            home_id: self.home_id,
            home_generation: self.home_generation,
            service_generation: self.service_generation,
            storage: self.storage.clone(),
            commands: self.command_authorizer.clone(),
        }
    }
}

impl RuntimeFailureReader {
    pub fn observe(
        &self,
        session: &beryl_state::SessionState,
        window: beryl_model::WindowId,
        claim: beryl_state::WindowClaimSelection,
    ) -> SelectedRuntimeFailureObservation {
        self.read(session, window, claim)
            .unwrap_or(SelectedRuntimeFailureObservation::Unknown)
    }

    fn read(
        &self,
        session: &beryl_state::SessionState,
        window: beryl_model::WindowId,
        claim: beryl_state::WindowClaimSelection,
    ) -> Option<SelectedRuntimeFailureObservation> {
        let _permit = self.commands.authorize().ok()?;
        let home = self.home.upgrade()?;
        let owner = self.owner.upgrade()?;
        admission::ensure_current_home(
            Some(&home),
            self.home_id,
            self.home_generation,
            &self.storage,
        )
        .ok()?;
        let limit = SyndicPointReadLimit::new(1_000_000).ok()?;
        let current = || {
            session
                .window_claim_catalog_source(&home, window)
                .ok()?
                .claim()
                .filter(|current| {
                    current.window_id() == window
                        && current.thread_id() == claim.thread_id()
                        && current.generation() == claim.generation()
                        && current.revision() == claim.revision()
                })
        };
        current()?;
        let execution = self
            .storage
            .thread_execution(&home, claim.thread_id(), limit)
            .ok()??;
        let failure = owner.observe_failure(execution.execution().runtime_id())?;
        if !failure.belongs_to_service(self.service_generation) {
            return None;
        }
        current()?;
        if self
            .storage
            .thread_execution(&home, claim.thread_id(), limit)
            .ok()?
            .as_ref()
            != Some(&execution)
            || !self.commands.is_open()
            || admission::ensure_current_home(
                Some(&home),
                self.home_id,
                self.home_generation,
                &self.storage,
            )
            .is_err()
        {
            return None;
        }
        let latest = owner.observe_failure(failure.runtime_id())?;
        if !failure.same_attempt(latest) {
            return None;
        }
        Some(SelectedRuntimeFailureObservation::Unavailable {
            execution: execution.execution().clone(),
            failure: latest,
        })
    }

    #[cfg(feature = "test-faults")]
    pub fn test_resource_strong_counts(&self) -> [usize; 2] {
        [self.home.strong_count(), self.owner.strong_count()]
    }
}
