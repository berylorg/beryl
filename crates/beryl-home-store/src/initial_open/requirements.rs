use std::fmt;

use crate::{
    StorageDomain,
    domain::{DomainBlueprint, DomainRegistry},
};

use super::{HomeCandidateError, HomeDomainRequirementsError};

#[derive(Clone, Default)]
pub struct HomeDomainRequirements {
    declarations: Vec<DomainBlueprint>,
}

impl HomeDomainRequirements {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_domain<D: StorageDomain>(mut self) -> Result<Self, HomeDomainRequirementsError> {
        self.insert(DomainBlueprint::for_domain::<D>()?)?;
        Ok(self)
    }

    pub fn merge(mut self, other: Self) -> Result<Self, HomeDomainRequirementsError> {
        for declaration in other.declarations {
            self.insert(declaration)?;
        }
        Ok(self)
    }

    fn insert(&mut self, declaration: DomainBlueprint) -> Result<(), HomeDomainRequirementsError> {
        match self
            .declarations
            .binary_search_by_key(&declaration.name, |entry| entry.name)
        {
            Ok(_) => Err(HomeDomainRequirementsError::DuplicateDomain {
                domain: declaration.name,
            }),
            Err(index) => {
                self.declarations.insert(index, declaration);
                Ok(())
            }
        }
    }

    pub(super) fn validate(&self, registry: &DomainRegistry) -> Result<(), HomeCandidateError> {
        for required in &self.declarations {
            let registered = registry
                .slot_for(required.name)
                .and_then(|slot| registry.get(slot))
                .ok_or(HomeCandidateError::MissingDomain {
                    domain: required.name,
                })?;
            if registered.owner != required.owner
                || registered.schema != required.schema
                || registered.attachment.attachment_type() != required.attachment_type
                || registered.families.len() != required.families.len()
                || !registered
                    .families
                    .iter()
                    .zip(&required.families)
                    .all(|(actual, expected)| {
                        actual.logical_name == expected.logical_name
                            && actual.physical_name == expected.physical_name
                            && actual.schema == expected.schema
                            && actual.codec_type == expected.codec_type
                            && actual.max_key_bytes == expected.max_key_bytes
                            && actual.max_stored_value_bytes == expected.max_stored_value_bytes
                    })
            {
                return Err(HomeCandidateError::DomainMismatch {
                    domain: required.name,
                });
            }
            if !registered.attachment.is_active() {
                return Err(HomeCandidateError::AttachmentUnavailable {
                    domain: required.name,
                });
            }
        }
        for registered in registry.iter() {
            if self
                .declarations
                .binary_search_by_key(&registered.name, |entry| entry.name)
                .is_err()
            {
                return Err(HomeCandidateError::UnexpectedDomain {
                    domain: registered.name,
                });
            }
        }
        Ok(())
    }
}

impl fmt::Debug for HomeDomainRequirements {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_list()
            .entries(self.declarations.iter().map(|entry| entry.name))
            .finish()
    }
}
