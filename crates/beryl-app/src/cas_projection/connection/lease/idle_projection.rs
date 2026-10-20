use super::*;

pub(in crate::cas_projection) struct ShutdownProjectionReleaseIdentity {
    connection: Arc<ProjectionConnection>,
    key: LoadedThreadKey,
    owner: SyndicThreadId,
    generation: CasLoadedSessionGeneration,
    token: LeaseToken,
}

impl ShutdownProjectionReleaseIdentity {
    pub(in crate::cas_projection) fn elect<T>(
        self,
        command: &crate::cas_projection::LiveCommandPermit,
        publish: impl FnOnce() -> T,
    ) -> Result<Option<T>, ProjectionCoordinatorError> {
        self.connection.elect_shutdown_projection_release(
            command,
            || {
                registry::try_owns_only_loaded_lease(
                    &self.key,
                    self.connection.authority.generation,
                    self.owner,
                    self.generation,
                    self.token,
                )
            },
            publish,
        )
    }
}

impl LoadedProjectionLease {
    pub(in crate::cas_projection) fn shutdown_release_identity(
        &self,
    ) -> Option<ShutdownProjectionReleaseIdentity> {
        self.active.then(|| ShutdownProjectionReleaseIdentity {
            connection: Arc::clone(&self.connection),
            key: self.key.clone(),
            owner: self.owner,
            generation: self.generation,
            token: self.token,
        })
    }

    pub(in crate::cas_projection) fn belongs_to_connection(
        &self,
        connection: &Arc<ProjectionConnection>,
    ) -> bool {
        Arc::ptr_eq(&self.connection, connection)
    }

    pub(in crate::cas_projection) fn elect_idle_release(
        &self,
        owner: &crate::cas_projection::runtime_interest::RuntimeInterestOwner,
        binding: &beryl_model::ExecutionBinding,
        observation: &beryl_home_store::HomeMutationObservation,
    ) -> Result<bool, ProjectionCoordinatorError> {
        if !self.active {
            return Ok(false);
        }
        self.connection
            .elect_idle_projection_release(owner, binding, observation, || {
                registry::owns_only_loaded_lease(
                    &self.key,
                    self.connection.authority.generation,
                    self.owner,
                    self.generation,
                    self.token,
                )
            })
    }
}
