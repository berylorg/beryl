use super::{DomainValidationError, RegisteredDomain, callback::ErasedCallbackError};
use crate::{DomainCallbackSource, ReadError, SidecarVerifier};

impl RegisteredDomain {
    pub(crate) fn validate(&self, snapshot: &fjall::Snapshot) -> Result<(), ErasedCallbackError> {
        self.validate_physical_families(snapshot)?;
        (self.validator)(snapshot, self)
    }

    pub(crate) fn validate_reopen(
        &self,
        snapshot: &fjall::Snapshot,
        sidecars: &SidecarVerifier<'_>,
    ) -> Result<(), ErasedCallbackError> {
        self.validate_physical_families(snapshot)?;
        (self.reopen_validator)(snapshot, self, sidecars)
    }

    fn validate_physical_families(
        &self,
        snapshot: &fjall::Snapshot,
    ) -> Result<(), ErasedCallbackError> {
        for family in &self.families {
            crate::read::validate_physical_family(snapshot, self.name, family).map_err(access)?;
        }
        Ok(())
    }
}

pub(super) fn public_validation_error(
    domain: &'static str,
    source: ErasedCallbackError,
) -> DomainValidationError {
    match source {
        ErasedCallbackError::Access(source) => DomainValidationError::Access { domain, source },
        ErasedCallbackError::Rejected(source) => DomainValidationError::Rejected { domain, source },
    }
}

fn access(source: ReadError) -> ErasedCallbackError {
    ErasedCallbackError::Access(DomainCallbackSource::Read(source))
}
