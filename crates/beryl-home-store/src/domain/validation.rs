use fjall::Readable;

use super::{DomainValidationError, RegisteredDomain, callback::ErasedCallbackError};
use crate::{DomainCallbackSource, ReadError, ReadStage, SidecarVerifier};

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
            for guard in snapshot.iter(&family.keyspace) {
                let key = guard.key().map_err(|source| {
                    access(ReadError::Storage {
                        stage: ReadStage::PhysicalKey,
                        source: Box::new(source),
                    })
                })?;
                crate::read::validate_stored_key_size(
                    self.name,
                    family.logical_name,
                    family.max_key_bytes,
                    &key,
                )
                .map_err(access)?;

                let value_size = snapshot
                    .size_of(&family.keyspace, &key)
                    .map_err(|source| {
                        access(ReadError::Storage {
                            stage: ReadStage::PhysicalValueSize,
                            source: Box::new(source),
                        })
                    })?
                    .ok_or_else(|| {
                        access(ReadError::MalformedRecord {
                            domain: self.name,
                            family: family.logical_name,
                        })
                    })?;
                let value_size = usize::try_from(value_size)
                    .expect("u32 always fits usize on supported targets");
                if value_size > family.max_stored_value_bytes {
                    return Err(access(ReadError::InvalidStoredValueSize {
                        domain: self.name,
                        family: family.logical_name,
                        maximum: family.max_stored_value_bytes,
                        actual: value_size,
                    }));
                }

                let value = snapshot
                    .get(&family.keyspace, &key)
                    .map_err(|source| {
                        access(ReadError::Storage {
                            stage: ReadStage::PhysicalValue,
                            source: Box::new(source),
                        })
                    })?
                    .ok_or_else(|| {
                        access(ReadError::MalformedRecord {
                            domain: self.name,
                            family: family.logical_name,
                        })
                    })?;
                (family.validate_envelope)(&key, &value).map_err(access)?;
            }
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
