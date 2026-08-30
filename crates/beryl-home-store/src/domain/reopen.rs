use super::*;
use crate::{health::ClassifiedFjallError, store::StoreGeneration};

use super::registration::{read_persisted_metadata, registered_family, validate_blueprint};

pub(crate) fn reacquire_registry(
    generation: &mut StoreGeneration,
    blueprints: &[DomainBlueprint],
) -> Result<(), DomainRegistrationError> {
    let mut registry = DomainRegistry::default();
    for blueprint in blueprints {
        let persisted = read_persisted_metadata(generation, blueprint.name)?.ok_or(
            DomainRegistrationError::InvalidMetadata {
                domain: blueprint.name,
                source: Box::new(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "registered domain metadata is missing during reopen",
                )),
            },
        )?;
        validate_blueprint(blueprint, &persisted)?;
        let families = reacquire_families(generation, blueprint)?;
        let snapshot =
            generation
                .database
                .snapshot()
                .map_err(|source| DomainRegistrationError::Storage {
                    domain: blueprint.name,
                    stage: DomainRegistrationStage::ReadRegistry,
                    source: Box::new(ClassifiedFjallError::direct(source)),
                })?;
        let domain = super::registration::registered_domain(blueprint, families, &snapshot)?;
        registry.insert(domain);
    }
    generation.registry = registry;
    Ok(())
}

pub(crate) fn validate_registry(
    generation: &StoreGeneration,
    sidecars: &crate::SidecarVerifier<'_>,
) -> Result<(), DomainValidationError> {
    let snapshot =
        generation
            .database
            .snapshot()
            .map_err(|source| DomainValidationError::Snapshot {
                source: Box::new(ClassifiedFjallError::direct(source)),
            })?;
    for domain in generation.registry.iter() {
        domain
            .validate_schema(&snapshot, sidecars)
            .map_err(|source| super::validation::public_validation_error(domain.name, source))?;
    }
    Ok(())
}

fn reacquire_families(
    generation: &StoreGeneration,
    blueprint: &DomainBlueprint,
) -> Result<Vec<RegisteredFamily>, DomainRegistrationError> {
    let mut families = Vec::with_capacity(blueprint.families.len());
    for family in &blueprint.families {
        if !generation
            .database
            .keyspace_exists(&family.physical_name)
            .map_err(|source| DomainRegistrationError::Storage {
                domain: blueprint.name,
                stage: DomainRegistrationStage::OpenKeyspace,
                source: Box::new(ClassifiedFjallError::direct(source)),
            })?
        {
            return Err(DomainRegistrationError::MissingKeyspace {
                domain: blueprint.name,
                keyspace: family.physical_name.clone(),
            });
        }
        let keyspace = generation
            .database
            .open_keyspace(&family.physical_name)
            .map_err(|source| DomainRegistrationError::Storage {
                domain: blueprint.name,
                stage: DomainRegistrationStage::OpenKeyspace,
                source: Box::new(ClassifiedFjallError::direct(source)),
            })?;
        families.push(registered_family(family, keyspace));
    }
    Ok(families)
}
