use fjall::{KeyspaceCreateOptions, Readable};

use super::*;
use crate::{metadata::DomainMetadata, store::StoreGeneration};

use super::registration::{registered_family, validate_blueprint};

pub(crate) fn reacquire_registry(
    generation: &mut StoreGeneration,
    blueprints: &[DomainBlueprint],
) -> Result<(), DomainRegistrationError> {
    let mut registry = DomainRegistry::default();
    for blueprint in blueprints {
        let encoded = generation
            .database
            .snapshot()
            .get(generation.domains_keyspace(), blueprint.name.as_bytes())
            .map_err(|source| DomainRegistrationError::Storage {
                domain: blueprint.name,
                stage: DomainRegistrationStage::ReadRegistry,
                source: Box::new(source),
            })?
            .ok_or(DomainRegistrationError::InvalidMetadata {
                domain: blueprint.name,
                source: Box::new(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "registered domain metadata is missing during reopen",
                )),
            })?;
        if encoded.len() > 8 * 1_024 {
            return Err(DomainRegistrationError::InvalidMetadata {
                domain: blueprint.name,
                source: Box::new(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "domain registration exceeds its byte bound",
                )),
            });
        }
        let persisted = DomainMetadata::decode(&encoded).map_err(|source| {
            DomainRegistrationError::InvalidMetadata {
                domain: blueprint.name,
                source: Box::new(source),
            }
        })?;
        validate_blueprint(blueprint, &persisted)?;
        let domain = reacquire_families(generation, blueprint)?;
        registry.insert(domain);
    }
    generation.registry = registry;
    Ok(())
}

pub(crate) fn validate_registry(generation: &StoreGeneration) -> Result<(), DomainValidationError> {
    let snapshot = generation.database.snapshot();
    for domain in generation.registry.iter() {
        domain
            .validate(&snapshot)
            .map_err(|source| DomainValidationError {
                domain: domain.name,
                source,
            })?;
    }
    Ok(())
}

pub(crate) fn validate_reopen_registry(
    generation: &StoreGeneration,
    sidecars: &crate::SidecarVerifier<'_>,
) -> Result<(), DomainValidationError> {
    let snapshot = generation.database.snapshot();
    for domain in generation.registry.iter() {
        domain
            .validate_reopen(&snapshot, sidecars)
            .map_err(|source| DomainValidationError {
                domain: domain.name,
                source,
            })?;
    }
    Ok(())
}

fn reacquire_families(
    generation: &StoreGeneration,
    blueprint: &DomainBlueprint,
) -> Result<RegisteredDomain, DomainRegistrationError> {
    let mut families = Vec::with_capacity(blueprint.families.len());
    for family in &blueprint.families {
        if !generation.database.keyspace_exists(&family.physical_name) {
            return Err(DomainRegistrationError::MissingKeyspace {
                domain: blueprint.name,
                keyspace: family.physical_name.clone(),
            });
        }
        let keyspace = generation
            .database
            .keyspace(&family.physical_name, KeyspaceCreateOptions::default)
            .map_err(|source| DomainRegistrationError::Storage {
                domain: blueprint.name,
                stage: DomainRegistrationStage::OpenKeyspace,
                source: Box::new(source),
            })?;
        families.push(registered_family(family, keyspace));
    }
    let family_slots = families
        .iter()
        .enumerate()
        .map(|(slot, family)| (family.logical_name, slot))
        .collect();
    Ok(RegisteredDomain {
        name: blueprint.name,
        schema: blueprint.schema,
        families,
        family_slots,
        validator: blueprint.validator,
        reopen_validator: blueprint.reopen_validator,
    })
}
