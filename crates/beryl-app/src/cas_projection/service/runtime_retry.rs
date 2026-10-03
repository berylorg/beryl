use super::*;
use std::sync::Weak;

use crate::cas_projection::{
    ProjectionCancellationToken, RuntimeFailureSnapshot, ScheduledExecutionSessions,
    process_sessions::{
        RetainedSelectedProjection, SelectedProjectionRecoveryError, WeakScheduledExecutionSessions,
    },
};

#[derive(Clone)]
pub struct SelectedRuntimeRetryWorker {
    home: Weak<HomeServiceReference>,
    home_id: BerylHomeId,
    home_generation: HomeGeneration,
    service_generation: ProjectionServiceGeneration,
    storage: SyndicStorage,
    commands: LiveCommandAuthorizer,
    sessions: WeakScheduledExecutionSessions,
    mutation_observer: beryl_home_store::HomeMutationObserver,
}

pub struct SelectedRuntimeUsability {
    window: beryl_model::WindowId,
    claim: beryl_state::WindowClaimSelection,
    execution: ExecutionBinding,
    failure: RuntimeFailureSnapshot,
    projection: RetainedSelectedProjection,
    observation: beryl_home_store::HomeMutationObservation,
}

impl SelectedRuntimeUsability {
    pub(crate) fn matches_selection(
        &self,
        window: beryl_model::WindowId,
        claim: beryl_state::WindowClaimSelection,
    ) -> bool {
        self.window == window && self.claim == claim
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Error)]
pub enum SelectedRuntimeRetryError {
    #[error("the selected runtime recovery authority is no longer current")]
    Revoked,
    #[error("the selected runtime recovery is unavailable or at capacity")]
    Unavailable,
    #[error("the selected runtime recovery was cancelled")]
    Cancelled,
    #[error("the selected runtime projection could not be established")]
    Failed,
}

impl ProjectionConnectionService {
    pub fn selected_runtime_retry_worker(
        &self,
        sessions: &ScheduledExecutionSessions,
    ) -> SelectedRuntimeRetryWorker {
        SelectedRuntimeRetryWorker {
            home: self.home.as_ref().map_or_else(Weak::new, Arc::downgrade),
            home_id: self.home_id,
            home_generation: self.home_generation,
            service_generation: self.service_generation,
            storage: self.storage.clone(),
            commands: self.command_authorizer.clone(),
            sessions: sessions.downgrade(),
            mutation_observer: self.mutation_observer.clone(),
        }
    }
}

impl SelectedRuntimeRetryWorker {
    fn selected_current(
        &self,
        home: &HomeServiceReference,
        session: &beryl_state::SessionState,
        window: beryl_model::WindowId,
        claim: beryl_state::WindowClaimSelection,
        execution: &ExecutionBinding,
    ) -> bool {
        admission::ensure_current_home(
            Some(home),
            self.home_id,
            self.home_generation,
            &self.storage,
        )
        .is_ok()
            && session
                .window_claim_catalog_source(home, window)
                .ok()
                .and_then(|source| source.claim())
                .is_some_and(|current| {
                    current.window_id() == window
                        && current.thread_id() == claim.thread_id()
                        && current.generation() == claim.generation()
                        && current.revision() == claim.revision()
                })
            && self
                .storage
                .thread_execution(
                    home,
                    claim.thread_id(),
                    SyndicPointReadLimit::new(1_000_000).expect("bounded point read"),
                )
                .ok()
                .flatten()
                .is_some_and(|current| current.execution() == execution)
            && self.commands.is_open()
    }

    pub fn eligible(
        &self,
        session: &beryl_state::SessionState,
        window: beryl_model::WindowId,
        claim: beryl_state::WindowClaimSelection,
        execution: &ExecutionBinding,
        failure: RuntimeFailureSnapshot,
    ) -> bool {
        if self.commands.execution_candidate().is_err() {
            return false;
        }
        let Some(home) = self.home.upgrade() else {
            return false;
        };
        let Some(sessions) = self.sessions.upgrade() else {
            return false;
        };
        failure.belongs_to_service(self.service_generation)
            && failure.runtime_id() == execution.runtime_id()
            && self.selected_current(&home, session, window, claim, execution)
            && sessions.selected_projection_recovery_eligible(failure, claim.thread_id(), execution)
            && self.selected_current(&home, session, window, claim, execution)
    }

    pub fn recover(
        &self,
        session: &beryl_state::SessionState,
        window: beryl_model::WindowId,
        claim: beryl_state::WindowClaimSelection,
        execution: &ExecutionBinding,
        failure: RuntimeFailureSnapshot,
        cancellation: &ProjectionCancellationToken,
    ) -> Result<SelectedRuntimeUsability, SelectedRuntimeRetryError> {
        if cancellation.is_cancelled() {
            return Err(SelectedRuntimeRetryError::Cancelled);
        }
        if !self.eligible(session, window, claim, execution, failure) {
            return Err(SelectedRuntimeRetryError::Unavailable);
        }
        let _command = self
            .commands
            .authorize()
            .map_err(|_| SelectedRuntimeRetryError::Revoked)?;
        let home = self
            .home
            .upgrade()
            .ok_or(SelectedRuntimeRetryError::Revoked)?;
        let sessions = self
            .sessions
            .upgrade()
            .ok_or(SelectedRuntimeRetryError::Revoked)?;
        if !self.selected_current(&home, session, window, claim, execution) {
            return Err(SelectedRuntimeRetryError::Revoked);
        }
        let projection = sessions
            .recover_selected_projection(failure, claim.thread_id(), execution, cancellation)
            .map_err(|error| match error {
                SelectedProjectionRecoveryError::Unavailable => {
                    SelectedRuntimeRetryError::Unavailable
                }
                SelectedProjectionRecoveryError::Cancelled => SelectedRuntimeRetryError::Cancelled,
                SelectedProjectionRecoveryError::Failed => SelectedRuntimeRetryError::Failed,
            })?;
        let observation = self
            .mutation_observer
            .observe()
            .map_err(|_| SelectedRuntimeRetryError::Revoked)?;
        let proof = SelectedRuntimeUsability {
            window,
            claim,
            execution: execution.clone(),
            failure,
            projection,
            observation,
        };
        if cancellation.is_cancelled() {
            return Err(SelectedRuntimeRetryError::Cancelled);
        }
        if !self.usability_current(session, &proof) {
            return Err(SelectedRuntimeRetryError::Revoked);
        }
        Ok(proof)
    }

    pub fn usability_current(
        &self,
        session: &beryl_state::SessionState,
        proof: &SelectedRuntimeUsability,
    ) -> bool {
        let Some(home) = self.home.upgrade() else {
            return false;
        };
        let Some(sessions) = self.sessions.upgrade() else {
            return false;
        };
        proof.failure.belongs_to_service(self.service_generation)
            && proof.failure.runtime_id() == proof.execution.runtime_id()
            && self.selected_current(&home, session, proof.window, proof.claim, &proof.execution)
            && sessions.retained_selected_projection_matches(&proof.projection)
            && self.with_usability_publication(proof, || ()).is_some()
    }

    pub(crate) fn with_usability_publication<T>(
        &self,
        proof: &SelectedRuntimeUsability,
        publish: impl FnOnce() -> T,
    ) -> Option<T> {
        if !proof.failure.belongs_to_service(self.service_generation) {
            return None;
        }
        let home = self.home.upgrade()?;
        let sessions = self.sessions.upgrade()?;
        sessions.with_retained_selected_projection(&proof.projection, || {
            self.commands.try_with_work_open(|| {
                home.try_elect_observed_coherent(&proof.observation, self.home_generation, publish)
                    .ok()
            })?
        })?
    }
}
