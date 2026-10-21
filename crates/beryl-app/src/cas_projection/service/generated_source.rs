use super::ProjectionConnectionService;

impl ProjectionConnectionService {
    #[cfg(feature = "test-faults")]
    pub fn generated_source_eligibility_for_test(
        &self,
        proof: &syndic_storage::GeneratedMediaSourceProof,
    ) -> Result<(), beryl_backend::GeneratedOutputError> {
        self.generated_source_authority().authorize(proof).map(|_| ()).map_err(|error| error.error())
    }

    #[cfg(feature = "test-faults")]
    pub fn generated_source_incarnation_eligibility_for_test(
        &self,
        proof: &syndic_storage::GeneratedMediaSourceProof,
        historical: Option<beryl_model::GeneratedSourceIncarnation>,
    ) -> Result<(), beryl_backend::GeneratedOutputError> {
        self.generated_source_authority().authorize_incarnation(proof, historical)
            .map(|_| ()).map_err(|error| error.error())
    }

    pub(crate) fn generated_source_authority(&self) -> crate::cas_projection::GeneratedSourceAuthority {
        crate::cas_projection::GeneratedSourceAuthority::new(
            self.command_authorizer.clone(),
            std::sync::Arc::clone(&self.connections),
            self.runtime_interest.clone(),
        )
    }
}
