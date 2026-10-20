use super::*;

impl LoadedCasProjection {
    pub(in crate::cas_projection) fn shutdown_release_identity(
        &self,
    ) -> Option<crate::cas_projection::connection::ShutdownProjectionReleaseIdentity> {
        self.lease.as_ref()?.shutdown_release_identity()
    }

    pub(in crate::cas_projection) fn belongs_to_session(
        &self,
        session: &crate::cas_projection::AdmittedProjectionSession,
    ) -> bool {
        self.lease
            .as_ref()
            .is_some_and(|lease| lease.belongs_to_connection(session.connection()))
    }

    pub(in crate::cas_projection) fn elect_idle_release(
        &self,
        owner: &crate::cas_projection::runtime_interest::RuntimeInterestOwner,
        observation: &beryl_home_store::HomeMutationObservation,
    ) -> Result<bool, crate::cas_projection::ProjectionCoordinatorError> {
        match &self.lease {
            Some(lease) => lease.elect_idle_release(owner, &self.execution_binding, observation),
            None => Ok(false),
        }
    }
}
