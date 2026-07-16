use std::error::Error;

use fjall::{Keyspace, KeyspaceCreateOptions, PersistMode, Readable};

use super::reopen::validate_registry;
use super::*;
use crate::{HomeStore, health::FailureSeverity, store::StoreGeneration};

impl HomeStore {
    /// Registers or reacquires one typed logical domain before process services start.
    ///
    /// A second registration of the same stable name in one generation is an
    /// error. On reopen, persistent schema and family declarations must match
    /// exactly and every required physical keyspace must already exist.
    pub fn register_domain<D: StorageDomain>(
        &mut self,
    ) -> Result<DomainHandle<D>, DomainRegistrationError> {
        let definition = DomainBlueprint::for_domain::<D>()?;
        let sidecars = crate::SidecarVerifier::new(self);
        let admission = self.health.admit()?;
        let mut registrations = match self.registrations.lock() {
            Ok(registrations) => registrations,
            Err(_) => {
                admission.fail(FailureSeverity::Structural);
                return Err(DomainRegistrationError::RegistryPoisoned);
            }
        };
        let mut generation_guard = match self.generation.write() {
            Ok(generation) => generation,
            Err(_) => {
                admission.fail(FailureSeverity::Structural);
                return Err(DomainRegistrationError::RegistryPoisoned);
            }
        };
        let generation = match generation_guard.as_mut() {
            Some(generation) => generation,
            None => {
                admission.fail(FailureSeverity::Structural);
                return Err(DomainRegistrationError::RegistryPoisoned);
            }
        };

        let result = register_definition::<D>(generation, &definition, &sidecars);
        let handle = match result {
            Ok(registered) => {
                let slot = generation.registry.insert(registered);
                registrations.push(definition);
                DomainHandle::new(generation.instance_id, slot)
            }
            Err(error) => {
                if let Some(severity) = registration_failure_severity(&error) {
                    admission.fail(severity);
                }
                return Err(error);
            }
        };
        drop(registrations);
        admission.confirm()?;
        Ok(handle)
    }

    /// Runs every registered authoritative-domain validator on one snapshot.
    pub fn validate_registered_domains(&self) -> Result<(), DomainValidationError> {
        let admission = self.health.admit()?;
        let generation = match self.generation.read() {
            Ok(generation) => generation,
            Err(_) => {
                admission.fail(FailureSeverity::Structural);
                return Err(DomainValidationError::GenerationPoisoned);
            }
        };
        let generation = match generation.as_ref() {
            Some(generation) => generation,
            None => {
                admission.fail(FailureSeverity::Structural);
                return Err(DomainValidationError::GenerationPoisoned);
            }
        };
        let result = validate_registry(generation);
        if let Err(error) = &result {
            admission.fail(validation_failure_severity(error));
        } else {
            admission.confirm()?;
        }
        result
    }

    /// Reacquires one typed domain handle for the current healthy generation.
    pub fn domain_handle<D: StorageDomain>(&self) -> Result<DomainHandle<D>, DomainHandleError> {
        let admission = self.health.admit()?;
        let generation = match self.generation.read() {
            Ok(generation) => generation,
            Err(_) => {
                admission.fail(FailureSeverity::Structural);
                return Err(DomainHandleError::GenerationPoisoned);
            }
        };
        let generation = match generation.as_ref() {
            Some(generation) => generation,
            None => {
                admission.fail(FailureSeverity::Structural);
                return Err(DomainHandleError::GenerationPoisoned);
            }
        };
        let slot = generation
            .registry
            .slot_for(D::NAME)
            .ok_or(DomainHandleError::NotRegistered { domain: D::NAME })?;
        let domain = generation
            .registry
            .get(slot)
            .ok_or(DomainHandleError::NotRegistered { domain: D::NAME })?;
        if domain.owner != DomainOwnerId::of::<D>() {
            return Err(DomainHandleError::OwnerTypeMismatch { domain: D::NAME });
        }
        if domain.schema != D::SCHEMA_VERSION {
            return Err(DomainHandleError::NotRegistered { domain: D::NAME });
        }
        let handle = DomainHandle::new(generation.instance_id, slot);
        admission.confirm()?;
        Ok(handle)
    }
}

impl StoreGeneration {
    pub(crate) fn resolve_domain<D: StorageDomain>(
        &self,
        handle: DomainHandle<D>,
    ) -> Option<&RegisteredDomain> {
        if handle.store != self.instance_id || handle.owner != DomainOwnerId::of::<D>() {
            return None;
        }
        let domain = self.registry.get(handle.slot)?;
        (domain.name == D::NAME
            && domain.schema == D::SCHEMA_VERSION
            && domain.owner == handle.owner)
            .then_some(domain)
    }
}

fn register_definition<D: StorageDomain>(
    generation: &StoreGeneration,
    definition: &DomainBlueprint,
    sidecars: &crate::SidecarVerifier<'_>,
) -> Result<RegisteredDomain, DomainRegistrationError> {
    if let Some(slot) = generation.registry.slot_for(definition.name) {
        let registered = generation
            .registry
            .get(slot)
            .expect("a registered domain name always resolves to its slot");
        if registered.owner != definition.owner {
            return Err(DomainRegistrationError::OwnerTypeMismatch {
                domain: definition.name,
            });
        }
        return Err(DomainRegistrationError::DuplicateDomain {
            domain: definition.name,
        });
    }
    for family in &definition.families {
        if generation
            .registry
            .contains_physical_name(&family.physical_name)
        {
            return Err(DomainRegistrationError::UnexpectedKeyspace {
                keyspace: family.physical_name.clone(),
            });
        }
    }

    let snapshot = generation.database.snapshot();
    let encoded = snapshot
        .get(generation.domains_keyspace(), definition.name.as_bytes())
        .map_err(|source| {
            registration_storage::<D>(DomainRegistrationStage::ReadRegistry, source)
        })?;
    let (registered, existing) = match encoded {
        Some(encoded) => {
            if encoded.len() > 8 * 1_024 {
                return Err(invalid_metadata::<D>(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "domain registration exceeds its byte bound",
                )));
            }
            let persisted = DomainMetadata::decode(&encoded).map_err(invalid_metadata::<D>)?;
            validate_persisted(definition, &persisted)?;
            (open_existing_families::<D>(generation, definition)?, true)
        }
        None => (create_new_families::<D>(generation, definition)?, false),
    };
    drop(snapshot);

    if existing {
        let snapshot = generation.database.snapshot();
        registered
            .validate_reopen(&snapshot, sidecars)
            .map_err(|source| match source {
                callback::ErasedCallbackError::Access(source) => {
                    DomainRegistrationError::ValidationAccess {
                        domain: D::NAME,
                        source,
                    }
                }
                callback::ErasedCallbackError::Rejected(source) => {
                    DomainRegistrationError::Validation {
                        domain: D::NAME,
                        source,
                    }
                }
            })?;
    } else {
        persist_new_registration::<D>(generation, definition)?;
    }
    Ok(registered)
}

fn open_existing_families<D: StorageDomain>(
    generation: &StoreGeneration,
    definition: &DomainBlueprint,
) -> Result<RegisteredDomain, DomainRegistrationError> {
    let mut families = Vec::with_capacity(definition.families.len());
    for family in &definition.families {
        if !generation.database.keyspace_exists(&family.physical_name) {
            return Err(DomainRegistrationError::MissingKeyspace {
                domain: D::NAME,
                keyspace: family.physical_name.clone(),
            });
        }
        let keyspace = generation
            .database
            .keyspace(&family.physical_name, KeyspaceCreateOptions::default)
            .map_err(|source| {
                registration_storage::<D>(DomainRegistrationStage::OpenKeyspace, source)
            })?;
        families.push(registered_family(family, keyspace));
    }
    Ok(registered_domain(definition, families))
}

fn create_new_families<D: StorageDomain>(
    generation: &StoreGeneration,
    definition: &DomainBlueprint,
) -> Result<RegisteredDomain, DomainRegistrationError> {
    for family in &definition.families {
        if generation.database.keyspace_exists(&family.physical_name) {
            return Err(DomainRegistrationError::UnexpectedKeyspace {
                keyspace: family.physical_name.clone(),
            });
        }
    }

    let mut families = Vec::with_capacity(definition.families.len());
    for family in &definition.families {
        let keyspace = generation
            .database
            .keyspace(&family.physical_name, KeyspaceCreateOptions::default)
            .map_err(|source| {
                registration_storage::<D>(DomainRegistrationStage::OpenKeyspace, source)
            })?;
        families.push(registered_family(family, keyspace));
    }
    Ok(registered_domain(definition, families))
}

fn persist_new_registration<D: StorageDomain>(
    generation: &StoreGeneration,
    definition: &DomainBlueprint,
) -> Result<(), DomainRegistrationError> {
    let metadata = definition
        .initial_metadata()
        .encode()
        .map_err(invalid_metadata::<D>)?;
    let mut batch = generation.database.batch();
    batch.insert(
        generation.domains_keyspace(),
        definition.name.as_bytes(),
        metadata,
    );
    batch.commit().map_err(|source| {
        registration_storage::<D>(DomainRegistrationStage::CommitRegistry, source)
    })?;
    generation
        .database
        .persist(PersistMode::SyncAll)
        .map_err(|source| {
            registration_storage::<D>(DomainRegistrationStage::PersistRegistry, source)
        })
}

pub(super) fn registered_family(
    definition: &FamilyBlueprint,
    keyspace: Keyspace,
) -> RegisteredFamily {
    RegisteredFamily {
        logical_name: definition.logical_name,
        physical_name: definition.physical_name.clone(),
        schema: definition.schema,
        codec_type: definition.codec_type,
        max_key_bytes: definition.max_key_bytes,
        max_stored_value_bytes: definition.max_stored_value_bytes,
        validate_envelope: definition.validate_envelope,
        keyspace,
    }
}

fn registered_domain(
    definition: &DomainBlueprint,
    families: Vec<RegisteredFamily>,
) -> RegisteredDomain {
    let family_slots = families
        .iter()
        .enumerate()
        .map(|(slot, family)| (family.logical_name, slot))
        .collect();
    RegisteredDomain {
        name: definition.name,
        schema: definition.schema,
        owner: definition.owner,
        families,
        family_slots,
        validator: definition.validator,
        reopen_validator: definition.reopen_validator,
    }
}

fn validate_persisted(
    definition: &DomainBlueprint,
    persisted: &DomainMetadata,
) -> Result<(), DomainRegistrationError> {
    validate_blueprint(definition, persisted)
}

pub(super) fn validate_blueprint(
    definition: &DomainBlueprint,
    persisted: &DomainMetadata,
) -> Result<(), DomainRegistrationError> {
    if persisted.schema != definition.schema {
        return Err(DomainRegistrationError::UnsupportedDomainSchema {
            domain: definition.name,
            supported: definition.schema,
            found: persisted.schema,
        });
    }
    if persisted.families.len() != definition.families.len() {
        return Err(DomainRegistrationError::IncompatibleKeyspaces {
            domain: definition.name,
        });
    }

    for (stored, declared) in persisted.families.iter().zip(&definition.families) {
        if stored.logical_name != declared.logical_name
            || stored.physical_name != declared.physical_name
        {
            return Err(DomainRegistrationError::IncompatibleKeyspaces {
                domain: definition.name,
            });
        }
        if stored.schema != declared.schema {
            return Err(DomainRegistrationError::UnsupportedKeyspaceSchema {
                domain: definition.name,
                family: stored.logical_name.clone(),
                supported: declared.schema,
                found: stored.schema,
            });
        }
    }
    Ok(())
}

fn invalid_metadata<D: StorageDomain>(
    source: impl Error + Send + Sync + 'static,
) -> DomainRegistrationError {
    DomainRegistrationError::InvalidMetadata {
        domain: D::NAME,
        source: Box::new(source),
    }
}

fn registration_storage<D: StorageDomain>(
    stage: DomainRegistrationStage,
    source: impl Error + Send + Sync + 'static,
) -> DomainRegistrationError {
    DomainRegistrationError::Storage {
        domain: D::NAME,
        stage,
        source: Box::new(source),
    }
}

fn registration_failure_severity(error: &DomainRegistrationError) -> Option<FailureSeverity> {
    match error {
        DomainRegistrationError::InvalidDefinition(_)
        | DomainRegistrationError::DuplicateDomain { .. }
        | DomainRegistrationError::OwnerTypeMismatch { .. }
        | DomainRegistrationError::HealthGate(_) => None,
        DomainRegistrationError::Storage { .. } => Some(FailureSeverity::Verify),
        DomainRegistrationError::ValidationAccess { source, .. } => {
            Some(callback::callback_failure_severity(source))
        }
        DomainRegistrationError::RegistryPoisoned => Some(FailureSeverity::Structural),
        DomainRegistrationError::UnexpectedKeyspace { .. }
        | DomainRegistrationError::MissingKeyspace { .. }
        | DomainRegistrationError::UnsupportedDomainSchema { .. }
        | DomainRegistrationError::UnsupportedKeyspaceSchema { .. }
        | DomainRegistrationError::IncompatibleKeyspaces { .. }
        | DomainRegistrationError::InvalidMetadata { .. }
        | DomainRegistrationError::Validation { .. } => Some(FailureSeverity::Structural),
    }
}

fn validation_failure_severity(error: &DomainValidationError) -> FailureSeverity {
    match error {
        DomainValidationError::Access { source, .. } => callback::callback_failure_severity(source),
        DomainValidationError::HealthGate(_)
        | DomainValidationError::GenerationPoisoned
        | DomainValidationError::Rejected { .. } => FailureSeverity::Structural,
    }
}
