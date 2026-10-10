use std::{
    sync::{Mutex, Weak},
    time::Duration,
};

use beryl_backend::{BackendConfigDefaults, ModelListOptions, ModelPage};
use beryl_home_store::{HomeMutationObservation, HomeMutationObserver};
use beryl_model::WindowId;
use beryl_state::{SessionState, WindowClaimSelection};

use super::*;
use crate::cas_projection::runtime_interest::{
    RuntimeInterestOwner, RuntimeModelRead, RuntimeModelReadError,
};

#[derive(Debug, thiserror::Error)]
pub(crate) enum ModelSourceError {
    #[error("selected model source authority is unavailable")]
    Unavailable,
    #[error("selected model source authority changed")]
    Selection,
    #[error("model changes require an idle selected thread")]
    Active,
    #[error("model source publication is busy or changed")]
    Publication,
    #[error(transparent)]
    Runtime(#[from] RuntimeModelReadError),
}

#[derive(Clone)]
pub(crate) struct ModelReadSource {
    home: Weak<HomeServiceReference>,
    owner: Weak<RuntimeInterestOwner>,
    home_id: BerylHomeId,
    home_generation: HomeGeneration,
    service_generation: ProjectionServiceGeneration,
    storage: SyndicStorage,
    commands: LiveCommandAuthorizer,
    mutations: HomeMutationObserver,
}

pub(crate) struct ModelReadObservation {
    source: ModelReadSource,
    session: SessionState,
    window: WindowId,
    claim: WindowClaimSelection,
    execution: ExecutionBinding,
    runtime: RuntimeModelRead,
    idle_revision: Mutex<Option<beryl_model::InputGateRevision>>,
}

impl ProjectionConnectionService {
    #[cfg(all(test, feature = "test-faults"))]
    pub(crate) fn attach_model_connector_for_test(
        &self,
        binding: &ExecutionBinding,
        generation: CasProcessGeneration,
        connector: beryl_backend::ManagedBackendClientConnector,
    ) {
        self.runtime_interest
            .as_ref()
            .expect("configured runtime")
            .attach_model_connector_for_test(binding, generation, connector);
    }

    #[cfg(all(test, feature = "test-faults"))]
    pub(crate) fn retire_model_readiness_for_test(&self, runtime: RuntimeId) {
        self.runtime_interest
            .as_ref()
            .expect("configured runtime")
            .retire_model_readiness_for_test(runtime);
    }

    #[cfg(all(test, feature = "test-faults"))]
    pub(crate) fn retry_model_runtime_for_test(
        &self,
        binding: ExecutionBinding,
        connector: &beryl_backend::ManagedBackendClientConnector,
        generation: CasProcessGeneration,
        timeout: Duration,
    ) -> Result<super::super::AdmittedProjectionSession, super::super::RuntimeSessionAdmissionError>
    {
        use super::super::{RuntimeInterestStatus, RuntimeSessionAdmissionError};
        let owner = self
            .runtime_interest
            .as_ref()
            .ok_or(RuntimeSessionAdmissionError::ServiceUnavailable)?;
        let interest = owner
            .retry_model_interest_for_test(binding.clone(), generation, timeout)
            .map_err(|_| RuntimeSessionAdmissionError::RuntimeUnavailable)?;
        let ready = match interest.wait_for_change(RuntimeInterestStatus::Starting, timeout) {
            RuntimeInterestStatus::Ready(ready) if ready.process_generation() == generation => {
                ready
            }
            _ => return Err(RuntimeSessionAdmissionError::RuntimeUnavailable),
        };
        let session = self.admit_lifecycle_test_candidate(
            connector,
            binding.runtime_id(),
            generation,
            std::path::Path::new(binding.root_path().as_str()),
            timeout,
        )?;
        let admitted = interest.publish_session(ready, session)?;
        self.attach_model_connector_for_test(&binding, generation, connector.clone());
        Ok(admitted)
    }

    pub(crate) fn model_read_source(&self) -> ModelReadSource {
        ModelReadSource {
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
            mutations: self.mutation_observer.clone(),
        }
    }
}

#[cfg(all(test, feature = "test-faults"))]
impl CasProjectionCoordinator {
    pub(crate) fn obtain_completed_projection_for_test(
        &self,
        home: &HomeStore,
        storage: &SyndicStorage,
        session: &mut super::super::AdmittedProjectionSession,
        request: &super::super::CasProjectionRequest,
        cancellation: &super::super::ProjectionCancellationToken,
    ) -> Result<super::super::LoadedCasProjection, super::super::ProjectionExecutionError> {
        use super::super::ProjectionExecutionError;
        use syndic_storage::{NativeProjectionRecoveryPlan, NativeProjectionRequest};

        self.ensure_home(home)?;
        if cancellation.is_cancelled() {
            return Err(ProjectionExecutionError::Cancelled);
        }
        let acquisition = session.connection().admit_projection_acquisition()?;
        let flight = self
            .begin_projection(request.thread_id())?
            .with_acquisition(acquisition);
        let NativeProjectionRecoveryPlan::Ready { plan, basis } = storage
            .prepare_native_projection_recovery(
                home,
                &NativeProjectionRequest::new(
                    request.thread_id(),
                    request.selected_path(),
                    request.execution_binding().clone(),
                    crate::conversation_tools::ConversationToolRegistry::canonical().profile(),
                ),
                crate::cas_projection::input_replay::point_limit(),
            )?
        else {
            return Err(ProjectionExecutionError::ProjectionBasisChanged {
                thread_id: request.thread_id(),
            });
        };
        if !storage.validate_native_projection_recovery_basis(
            home,
            &basis,
            crate::cas_projection::input_replay::point_limit(),
        )? {
            return Err(ProjectionExecutionError::ProjectionBasisChanged {
                thread_id: request.thread_id(),
            });
        }
        self.obtain_selected_recovery_projection(
            home,
            storage,
            session,
            request,
            cancellation,
            &flight,
            plan,
        )
    }
}

impl ModelReadSource {
    pub(crate) fn selection_facts(
        &self,
        session: &SessionState,
        window: WindowId,
        expected: WindowClaimSelection,
    ) -> Result<(ExecutionBinding, bool, HomeMutationObservation), ModelSourceError> {
        let boundary = self
            .mutations
            .observe()
            .map_err(|_| ModelSourceError::Publication)?;
        let _permit = self
            .commands
            .authorize()
            .map_err(|_| ModelSourceError::Unavailable)?;
        let home = self.home.upgrade().ok_or(ModelSourceError::Unavailable)?;
        admission::ensure_current_home(
            Some(&home),
            self.home_id,
            self.home_generation,
            &self.storage,
        )
        .map_err(|_| ModelSourceError::Unavailable)?;
        let claim = session
            .window_claim_catalog_source(&home, window)
            .map_err(|_| ModelSourceError::Selection)?
            .claim()
            .ok_or(ModelSourceError::Selection)?;
        if claim.window_id() != window
            || claim.thread_id() != expected.thread_id()
            || claim.generation() != expected.generation()
            || claim.revision() != expected.revision()
        {
            return Err(ModelSourceError::Selection);
        }
        let limit =
            SyndicPointReadLimit::new(1_000_000).map_err(|_| ModelSourceError::Unavailable)?;
        let execution = self
            .storage
            .thread_execution(&home, claim.thread_id(), limit)
            .map_err(|_| ModelSourceError::Selection)?
            .ok_or(ModelSourceError::Selection)?
            .execution()
            .clone();
        let thread = self
            .storage
            .thread(&home, claim.thread_id(), limit)
            .map_err(|_| ModelSourceError::Selection)?
            .ok_or(ModelSourceError::Selection)?;
        let gate = self
            .storage
            .input_gate(&home, claim.thread_id(), limit)
            .map_err(|_| ModelSourceError::Selection)?
            .ok_or(ModelSourceError::Selection)?;
        let draft_only = thread.committed_tail().is_none()
            && gate.accepted_high_water() == 0
            && gate.state() == &syndic_storage::InputGateState::Idle;
        self.commands
            .try_with_work_open(|| {
                home.try_elect_observed_coherent(&boundary, self.home_generation, || ())
            })
            .ok_or(ModelSourceError::Unavailable)?
            .map_err(|_| ModelSourceError::Publication)?;
        Ok((execution, draft_only, boundary))
    }

    pub(crate) fn identity(&self) -> (BerylHomeId, HomeGeneration, ProjectionServiceGeneration) {
        (self.home_id, self.home_generation, self.service_generation)
    }

    pub(crate) fn prepare(
        &self,
        session: SessionState,
        window: WindowId,
        claim: WindowClaimSelection,
        execution: ExecutionBinding,
    ) -> Result<ModelReadObservation, ModelSourceError> {
        let owner = self.owner.upgrade().ok_or(ModelSourceError::Unavailable)?;
        let runtime = owner.model_read(execution.clone())?;
        let observation = ModelReadObservation {
            source: self.clone(),
            session,
            window,
            claim,
            execution,
            runtime,
            idle_revision: Mutex::new(None),
        };
        let boundary = observation.validate()?;
        observation.with_current(&boundary, || ())?;
        Ok(observation)
    }
}

impl ModelReadObservation {
    pub(crate) fn validate_idle(&self) -> Result<HomeMutationObservation, ModelSourceError> {
        let boundary = self.validate()?;
        let home = self
            .source
            .home
            .upgrade()
            .ok_or(ModelSourceError::Unavailable)?;
        let limit =
            SyndicPointReadLimit::new(1_000_000).map_err(|_| ModelSourceError::Unavailable)?;
        let gate = self
            .source
            .storage
            .input_gate(&home, self.claim.thread_id(), limit)
            .map_err(|_| ModelSourceError::Selection)?
            .ok_or(ModelSourceError::Selection)?;
        if gate.state() != &syndic_storage::InputGateState::Idle {
            return Err(ModelSourceError::Active);
        }
        let mut original = self
            .idle_revision
            .lock()
            .map_err(|_| ModelSourceError::Unavailable)?;
        if original.is_some_and(|revision| revision != gate.revision()) {
            return Err(ModelSourceError::Selection);
        }
        self.with_current(&boundary, || ())?;
        *original = Some(gate.revision());
        Ok(boundary)
    }

    pub(crate) fn validate(&self) -> Result<HomeMutationObservation, ModelSourceError> {
        let boundary = self
            .source
            .mutations
            .observe()
            .map_err(|_| ModelSourceError::Publication)?;
        let _permit = self
            .source
            .commands
            .authorize()
            .map_err(|_| ModelSourceError::Unavailable)?;
        let home = self
            .source
            .home
            .upgrade()
            .ok_or(ModelSourceError::Unavailable)?;
        admission::ensure_current_home(
            Some(&home),
            self.source.home_id,
            self.source.home_generation,
            &self.source.storage,
        )
        .map_err(|_| ModelSourceError::Unavailable)?;
        let current = self
            .session
            .window_claim_catalog_source(&home, self.window)
            .map_err(|_| ModelSourceError::Selection)?
            .claim()
            .ok_or(ModelSourceError::Selection)?;
        if current.window_id() != self.window
            || current.thread_id() != self.claim.thread_id()
            || current.generation() != self.claim.generation()
            || current.revision() != self.claim.revision()
        {
            return Err(ModelSourceError::Selection);
        }
        let limit =
            SyndicPointReadLimit::new(1_000_000).map_err(|_| ModelSourceError::Unavailable)?;
        let execution = self
            .source
            .storage
            .thread_execution(&home, self.claim.thread_id(), limit)
            .map_err(|_| ModelSourceError::Selection)?
            .ok_or(ModelSourceError::Selection)?;
        if execution.execution() != &self.execution {
            return Err(ModelSourceError::Selection);
        }
        self.with_current(&boundary, || ())?;
        Ok(boundary)
    }

    pub(crate) fn with_current<T>(
        &self,
        boundary: &HomeMutationObservation,
        publish: impl FnOnce() -> T,
    ) -> Result<T, ModelSourceError> {
        let home = self
            .source
            .home
            .upgrade()
            .ok_or(ModelSourceError::Unavailable)?;
        self.runtime
            .with_current(|| {
                home.try_elect_observed_coherent(boundary, self.source.home_generation, publish)
            })?
            .map_err(|_| ModelSourceError::Publication)
    }

    pub(crate) fn read_defaults(
        &self,
        timeout: Duration,
    ) -> Result<(BackendConfigDefaults, HomeMutationObservation), ModelSourceError> {
        self.validate()?;
        let defaults = self.runtime.read_defaults(timeout)?;
        let boundary = self.validate()?;
        Ok((defaults, boundary))
    }

    pub(crate) fn read_page(
        &self,
        options: &ModelListOptions,
        timeout: Duration,
    ) -> Result<(Box<ModelPage>, HomeMutationObservation), ModelSourceError> {
        self.validate()?;
        let page = self.runtime.read_page(options, timeout)?;
        let boundary = self.validate()?;
        Ok((page, boundary))
    }
}
