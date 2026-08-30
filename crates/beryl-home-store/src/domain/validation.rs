use super::{
    DomainBlueprint, DomainValidationError, RegisteredDomain, RegisteredFamily,
    callback::ErasedCallbackError,
};
use crate::{DomainCallbackSource, ReadError, SidecarVerifier};

impl RegisteredDomain {
    pub(crate) fn validate_schema(
        &self,
        snapshot: &fjall::Snapshot,
        sidecars: &SidecarVerifier<'_>,
    ) -> Result<(), ErasedCallbackError> {
        validate_provisional_schema(
            snapshot,
            self.name,
            &self.families,
            self.reopen_validator,
            sidecars,
        )
    }
}

pub(crate) fn validate_blueprint_schema(
    snapshot: &fjall::Snapshot,
    definition: &DomainBlueprint,
    families: &[RegisteredFamily],
    sidecars: &SidecarVerifier<'_>,
) -> Result<(), ErasedCallbackError> {
    validate_provisional_schema(
        snapshot,
        definition.name,
        families,
        definition.reopen_validator,
        sidecars,
    )
}

fn validate_provisional_schema(
    snapshot: &fjall::Snapshot,
    name: &'static str,
    families: &[RegisteredFamily],
    reopen_validator: super::ErasedReopenValidator,
    sidecars: &SidecarVerifier<'_>,
) -> Result<(), ErasedCallbackError> {
    for family in families {
        crate::read::validate_physical_family(snapshot, name, family).map_err(access)?;
    }
    reopen_validator(snapshot, families, sidecars)
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
