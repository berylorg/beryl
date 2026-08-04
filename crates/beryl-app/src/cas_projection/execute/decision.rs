use beryl_home_store::HomeStore;
use syndic_storage::{
    NativeProjectionPlan, NativeProjectionRequest, RecoveryProjectionRequest, SyndicStorage,
};

use super::{cleanup::StaleObservation, point_limit};
use crate::cas_projection::connection::ThreadRetirement;
use crate::cas_projection::{
    AdmittedProjectionSession, CasProjectionCoordinator, LoadedCasProjection,
    NativeLineageRecoveryDecision, ProjectionCancellationToken, ProjectionCoordinatorError,
    ProjectionExecutionError, ProjectionPublicationFailure,
};
use crate::conversation_tools::ConversationToolRegistry;

impl CasProjectionCoordinator {
    /// Consumes one recovery decision and retries its exact retained native source.
    pub fn retry_native_lineage(
        &self,
        home: &HomeStore,
        storage: SyndicStorage,
        session: &mut AdmittedProjectionSession,
        decision: NativeLineageRecoveryDecision,
        cancellation: &ProjectionCancellationToken,
    ) -> Result<LoadedCasProjection, ProjectionExecutionError> {
        self.ensure_decision_current(home, session, &decision, cancellation)?;
        let _flight = self.begin_projection(decision.target_thread_id())?;
        self.ensure_home(home)?;
        let plan = self.prepare_decision_plan(home, storage, &decision)?;
        if !decision.matches_plan(&plan) {
            return Err(
                ProjectionExecutionError::NativeLineageRecoveryDecisionStale {
                    thread_id: decision.target_thread_id(),
                },
            );
        }
        if cancellation.is_cancelled() {
            return Err(ProjectionExecutionError::Cancelled);
        }
        self.execute_plan(
            home,
            storage,
            session,
            decision.request(),
            cancellation,
            plan,
        )
    }

    /// Verifies whether one exact recovery decision can currently project its complete history.
    ///
    /// This read-only preflight lets the GUI keep `Recover from Syndic history` visible but
    /// disabled with the exact bounded recovery error. The consuming recovery command repeats all
    /// checks and does not rely on this result as mutation authority.
    pub fn validate_native_lineage_recovery(
        &self,
        home: &HomeStore,
        storage: SyndicStorage,
        session: &AdmittedProjectionSession,
        decision: &NativeLineageRecoveryDecision,
        cancellation: &ProjectionCancellationToken,
    ) -> Result<(), ProjectionExecutionError> {
        self.ensure_decision_current(home, session, decision, cancellation)?;
        let _flight = self.begin_projection(decision.target_thread_id())?;
        let plan = self.prepare_decision_plan(home, storage, decision)?;
        if !decision.matches_plan(&plan) {
            return Err(
                ProjectionExecutionError::NativeLineageRecoveryDecisionStale {
                    thread_id: decision.target_thread_id(),
                },
            );
        }
        if decision.basis().represented_prefix().tail().is_none() {
            return Ok(());
        }
        storage.prepare_recovery_projection(
            home,
            RecoveryProjectionRequest::for_pending_selected_turn_parent(
                decision.target_thread_id(),
                decision.request().selected_path(),
                decision.request().model_context_window_tokens(),
            ),
        )?;
        Ok(())
    }

    /// Consumes one recovery decision and establishes a fresh target from Syndic history.
    pub fn recover_native_lineage_from_syndic(
        &self,
        home: &HomeStore,
        storage: SyndicStorage,
        session: &mut AdmittedProjectionSession,
        decision: NativeLineageRecoveryDecision,
        cancellation: &ProjectionCancellationToken,
    ) -> Result<LoadedCasProjection, ProjectionExecutionError> {
        self.ensure_decision_current(home, session, &decision, cancellation)?;
        let _flight = self.begin_projection(decision.target_thread_id())?;
        self.ensure_home(home)?;
        let plan = self.prepare_decision_plan(home, storage, &decision)?;
        if !decision.matches_plan(&plan) {
            return Err(
                ProjectionExecutionError::NativeLineageRecoveryDecisionStale {
                    thread_id: decision.target_thread_id(),
                },
            );
        }

        let basis = if decision.source_thread_id() == decision.target_thread_id() {
            let post_retirement_revision = decision
                .basis()
                .expected_binding_revision()
                .checked_next()
                .map_err(|_| ProjectionPublicationFailure::BindingRevisionExhausted)?;
            let retired_revision =
                self.retire_decision_target(home, storage, session, &decision)?;
            if retired_revision != post_retirement_revision {
                return Err(
                    ProjectionExecutionError::NativeLineageRecoveryDecisionStale {
                        thread_id: decision.target_thread_id(),
                    },
                );
            }
            self.ensure_home(home)?;
            let replanned_basis = self
                .prepare_decision_plan(home, storage, &decision)?
                .basis();
            if replanned_basis.expected_binding_revision() != post_retirement_revision {
                return Err(
                    ProjectionExecutionError::NativeLineageRecoveryDecisionStale {
                        thread_id: decision.target_thread_id(),
                    },
                );
            }
            replanned_basis
        } else {
            decision.basis()
        };
        if cancellation.is_cancelled() {
            return Err(ProjectionExecutionError::Cancelled);
        }
        if basis.represented_prefix().tail().is_none() {
            self.start_fresh_native(
                home,
                storage,
                session,
                decision.request(),
                cancellation,
                basis,
            )
        } else {
            self.recover_projection(
                home,
                storage,
                session,
                decision.request(),
                cancellation,
                basis,
            )
        }
    }

    fn ensure_decision_current(
        &self,
        home: &HomeStore,
        session: &AdmittedProjectionSession,
        decision: &NativeLineageRecoveryDecision,
        cancellation: &ProjectionCancellationToken,
    ) -> Result<(), ProjectionExecutionError> {
        if decision.home_id() != self.home_id()
            || decision.home_generation() != self.home_generation()
        {
            return Err(
                ProjectionExecutionError::NativeLineageRecoveryDecisionStale {
                    thread_id: decision.target_thread_id(),
                },
            );
        }
        self.ensure_home(home)?;
        if session.runtime_id() != decision.request().execution_binding().runtime_id() {
            return Err(ProjectionExecutionError::RuntimeMismatch {
                requested: decision.request().execution_binding().runtime_id(),
                admitted: session.runtime_id(),
            });
        }
        if decision.request().thread_options().is_ephemeral() {
            return Err(ProjectionExecutionError::EphemeralProjectionThread);
        }
        if decision.basis().tool_profile() != ConversationToolRegistry::canonical().profile() {
            return Err(
                ProjectionExecutionError::NativeLineageRecoveryDecisionStale {
                    thread_id: decision.target_thread_id(),
                },
            );
        }
        if cancellation.is_cancelled() {
            return Err(ProjectionExecutionError::Cancelled);
        }
        Ok(())
    }

    fn prepare_decision_plan(
        &self,
        home: &HomeStore,
        storage: SyndicStorage,
        decision: &NativeLineageRecoveryDecision,
    ) -> Result<NativeProjectionPlan, ProjectionExecutionError> {
        storage
            .prepare_native_projection(
                home,
                &NativeProjectionRequest::new(
                    decision.target_thread_id(),
                    decision.request().selected_path(),
                    decision.request().execution_binding().clone(),
                    decision.basis().tool_profile(),
                ),
                point_limit(),
            )
            .map_err(ProjectionExecutionError::from)
    }

    fn retire_decision_target(
        &self,
        home: &HomeStore,
        storage: SyndicStorage,
        session: &AdmittedProjectionSession,
        decision: &NativeLineageRecoveryDecision,
    ) -> Result<beryl_model::BindingRevision, ProjectionExecutionError> {
        let source = decision.source();
        let retirement = session.retire_loaded_thread(
            source.binding().cas_thread_id(),
            source.thread_id(),
            decision.request().timeout(),
        );
        let (release_error, retired_loaded_generation) = match retirement {
            Ok(ThreadRetirement::Absent) => (None, None),
            Ok(ThreadRetirement::Retired {
                generation,
                release_error,
            }) => (release_error, Some(generation)),
            Ok(ThreadRetirement::AnotherConnection) => {
                return Err(
                    ProjectionExecutionError::LoadedProjectionConnectionMismatch {
                        thread_id: source.binding().cas_thread_id().clone(),
                    },
                );
            }
            Ok(ThreadRetirement::AnotherOwner { existing_owner }) => {
                return Err(ProjectionCoordinatorError::CasThreadOwnerCollision {
                    runtime_id: session.runtime_id(),
                    process_generation: session.process_generation(),
                    cas_thread_id: source.binding().cas_thread_id().clone(),
                    existing_owner,
                    offered_owner: source.thread_id(),
                }
                .into());
            }
            Err(error) => (Some(error), None),
        };
        let retired_revision = self.publish_abandoned_target(
            home,
            storage,
            decision.request(),
            decision.basis(),
            source.binding().cas_thread_id().clone(),
            StaleObservation::exact(
                source.binding().execution().clone(),
                source.binding().represented_prefix(),
                source.binding().tool_profile(),
                source.binding().lineage(),
                source.binding().native_turn_count(),
                retired_loaded_generation,
            ),
            "operator selected fresh recovery from Syndic history",
        )?;
        if let Some(error) = release_error {
            return Err(ProjectionExecutionError::LeaseRelease(Box::new(error)));
        }
        Ok(retired_revision)
    }
}
