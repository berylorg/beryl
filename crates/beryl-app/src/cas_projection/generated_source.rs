use std::sync::Arc;

use beryl_backend::{GeneratedOutputError, GeneratedOutputPermit, GeneratedOutputTarget};
use syndic_storage::GeneratedMediaSourceProof;

use super::{
    ProjectionCoordinatorError, connection::LoadedThreadKey,
    persistent_failure::LiveCommandAuthorizer,
    runtime_interest::{GeneratedRuntimeOwner, RuntimeInterestOwner},
    service_registry::ProjectionServiceConnectionRegistry,
};

pub(crate) use super::runtime_interest::{
    GeneratedRuntimeSource, GeneratedSourceCloseFailure, GeneratedSourceOpenFailure,
};

#[derive(Debug)]
pub(crate) enum GeneratedSourceEligibility {
    Busy,
    LostSourceAuthority,
    Backend(GeneratedOutputError),
    Internal(ProjectionCoordinatorError),
}

impl From<GeneratedOutputError> for GeneratedSourceEligibility {
    fn from(error: GeneratedOutputError) -> Self {
        match error {
            GeneratedOutputError::Busy => Self::Busy,
            GeneratedOutputError::LostSourceAuthority => Self::LostSourceAuthority,
            error => Self::Backend(error),
        }
    }
}

impl From<super::LiveCommandAdmissionError> for GeneratedSourceEligibility {
    fn from(error: super::LiveCommandAdmissionError) -> Self {
        match error {
            super::LiveCommandAdmissionError::Closed => Self::LostSourceAuthority,
            error @ super::LiveCommandAdmissionError::Unavailable => Self::Internal(
                ProjectionCoordinatorError::AcquisitionServiceUnavailable(error),
            ),
        }
    }
}

impl GeneratedSourceEligibility {
    pub(crate) fn error(&self) -> GeneratedOutputError {
        match self {
            Self::Busy => GeneratedOutputError::Busy,
            Self::LostSourceAuthority => GeneratedOutputError::LostSourceAuthority,
            Self::Backend(error) => *error,
            Self::Internal(_) => GeneratedOutputError::Protocol,
        }
    }
}

#[derive(Clone)]
pub(crate) struct GeneratedSourceAuthority {
    commands: LiveCommandAuthorizer,
    connections: Arc<ProjectionServiceConnectionRegistry>,
    runtime: Option<Arc<RuntimeInterestOwner>>,
}

pub(crate) struct AuthorizedGeneratedSource {
    runtime: GeneratedRuntimeOwner,
    permit: GeneratedOutputPermit,
}

impl GeneratedSourceAuthority {
    pub(super) fn new(
        commands: LiveCommandAuthorizer,
        connections: Arc<ProjectionServiceConnectionRegistry>,
        runtime: Option<Arc<RuntimeInterestOwner>>,
    ) -> Self {
        Self { commands, connections, runtime }
    }

    pub(crate) fn authorize(
        &self,
        proof: &GeneratedMediaSourceProof,
    ) -> Result<AuthorizedGeneratedSource, GeneratedSourceEligibility> {
        self.authorize_incarnation(proof, proof.source_incarnation())
    }

    pub(super) fn authorize_incarnation(
        &self,
        proof: &GeneratedMediaSourceProof,
        incarnation: Option<beryl_model::GeneratedSourceIncarnation>,
    ) -> Result<AuthorizedGeneratedSource, GeneratedSourceEligibility> {
        let command = self.commands.authorize().map_err(GeneratedSourceEligibility::from)?;
        let incarnation = incarnation.ok_or(GeneratedSourceEligibility::LostSourceAuthority)?;
        let snapshot = proof.execution_snapshot();
        let generation = snapshot.loaded_generation();
        let runtime_id = snapshot.execution().runtime_id();
        let runtime = self.runtime.as_ref().ok_or(GeneratedSourceEligibility::LostSourceAuthority)?
            .generated_source_owner(runtime_id, generation.process())?;
        let key = LoadedThreadKey {
            runtime_id,
            process_generation: generation.process(),
            cas_thread_id: snapshot.cas_thread_id().clone(),
        };
        let connection = {
            let connections = self.connections.lock().map_err(|_| GeneratedSourceEligibility::Backend(GeneratedOutputError::Protocol))?;
            let mut matched = None;
            for connection in connections.iter() {
                if connection.runtime_id() == runtime_id
                    && connection.process_generation() == generation.process()
                    && connection.generated_source_belongs_to(self.commands.service_generation())
                    && super::connection::registry::generated_owner_is_current(
                        &key, connection.generated_source_connection_generation(), proof.thread_id(), generation,
                    )?
                {
                    if matched.replace(Arc::clone(connection)).is_some() {
                        return Err(GeneratedSourceEligibility::Backend(GeneratedOutputError::Protocol));
                    }
                }
            }
            matched.ok_or(GeneratedSourceEligibility::LostSourceAuthority)?
        };
        let item = proof.item().cas_source().ok_or(GeneratedSourceEligibility::Backend(GeneratedOutputError::Protocol))?;
        let target = GeneratedOutputTarget::new(*proof.resource_id().as_bytes(), proof.active_cas_turn().cas_turn_id().clone(), item.item_id().clone());
        let permit = connection.authorize_generated_source(key, proof.thread_id(), generation, incarnation, target)?;
        if !command.is_current() {
            return Err(GeneratedSourceEligibility::LostSourceAuthority);
        }
        Ok(AuthorizedGeneratedSource { runtime, permit })
    }
}

impl AuthorizedGeneratedSource {
    pub(crate) fn open(
        self,
        path: beryl_model::RuntimeNativePath,
        allowance: beryl_backend::GeneratedReaderAllowance,
        cancel: Arc<std::sync::atomic::AtomicBool>,
    ) -> Result<GeneratedRuntimeSource, GeneratedSourceOpenFailure> {
        self.runtime.open(self.permit, path, allowance, cancel)
    }
}
