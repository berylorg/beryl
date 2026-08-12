use std::error::Error;

use fjall::{Keyspace, PersistMode};

use super::reopen::validate_registry;
use super::*;
use crate::{
    HomeStore,
    health::{ClassifiedFjallError, FailureSeverity},
    metadata::MAX_DOMAIN_METADATA_BYTES,
    store::StoreGeneration,
};

impl HomeStore {
    /// Registers or routinely reacquires one typed logical domain without an
    /// exhaustive scan of persisted application records.
    ///
    /// A second registration of the same stable name in one generation is an
    /// error. On reopen, persistent schema and family declarations must match
    /// exactly and every required physical keyspace must already exist.
    pub fn register_domain<D: StorageDomain>(
        &mut self,
    ) -> Result<DomainHandle<D>, DomainRegistrationError> {
        self.register_domain_inner::<D>(false)
    }

    /// Registers or reacquires one typed logical domain at an explicit schema-
    /// validation boundary, exhaustively validating persisted records and sidecars.
    pub fn register_domain_with_schema_validation<D: StorageDomain>(
        &mut self,
    ) -> Result<DomainHandle<D>, DomainRegistrationError> {
        self.register_domain_inner::<D>(true)
    }

    fn register_domain_inner<D: StorageDomain>(
        &mut self,
        validate_schema: bool,
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

        let result = register_definition::<D>(
            generation,
            &definition,
            validate_schema.then_some(&sidecars),
        );
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
        admission.confirm_database(&generation.database, |source| {
            DomainRegistrationError::Storage {
                domain: D::NAME,
                stage: DomainRegistrationStage::ConfirmHealth,
                source: Box::new(source),
            }
        })?;
        Ok(handle)
    }

    /// Runs or joins the bounded per-home whole-home scrub flight.
    pub fn scrub_whole_home(
        &self,
        trigger: crate::WholeHomeScrubTrigger,
    ) -> Result<(), WholeHomeScrubError> {
        self.scrub
            .run(trigger, || self.scrub_registered_domains_once())
            .map_err(WholeHomeScrubError::new)
    }

    fn scrub_registered_domains_once(&self) -> Result<(), DomainValidationError> {
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
        let result = validate_registry(generation, &crate::SidecarVerifier::new(self));
        if let Err(error) = &result {
            admission.fail(validation_failure_severity(error));
        } else {
            admission.confirm_database(&generation.database, |source| {
                DomainValidationError::Health {
                    source: Box::new(source),
                }
            })?;
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
        admission.confirm_database(&generation.database, |source| {
            DomainHandleError::StorageHealth {
                source: Box::new(source),
            }
        })?;
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
    sidecars: Option<&crate::SidecarVerifier<'_>>,
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

    let persisted = read_persisted_metadata(generation, definition.name)?;
    let (registered, existing) = match persisted {
        Some(persisted) => {
            validate_persisted(definition, &persisted)?;
            (open_existing_families::<D>(generation, definition)?, true)
        }
        None => (create_new_families::<D>(generation, definition)?, false),
    };

    if existing {
        if let Some(sidecars) = sidecars {
            let snapshot = generation.database.snapshot().map_err(|source| {
                registration_storage::<D>(DomainRegistrationStage::ReadRegistry, source)
            })?;
            registered
                .validate_schema(&snapshot, sidecars)
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
        }
    } else {
        persist_new_registration::<D>(generation, definition)?;
    }
    Ok(registered)
}

pub(super) fn read_persisted_metadata(
    generation: &StoreGeneration,
    domain: &'static str,
) -> Result<Option<DomainMetadata>, DomainRegistrationError> {
    let snapshot = generation.database.snapshot().map_err(|source| {
        registration_storage_for(domain, DomainRegistrationStage::ReadRegistry, source)
    })?;
    let Some(point) = snapshot
        .point(generation.domains_keyspace(), domain.as_bytes())
        .map_err(|source| {
            registration_storage_for(domain, DomainRegistrationStage::ReadRegistry, source)
        })?
    else {
        return Ok(None);
    };
    let actual = usize::try_from(point.stored_value_len())
        .expect("u32 stored-value length fits usize on supported targets");
    if actual > MAX_DOMAIN_METADATA_BYTES {
        return Err(invalid_metadata_for(
            domain,
            std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "domain registration exceeds its byte bound",
            ),
        ));
    }
    let pair = point.acquire().map_err(|source| {
        registration_storage_for(domain, DomainRegistrationStage::ReadRegistry, source)
    })?;
    DomainMetadata::decode(pair.value())
        .map(Some)
        .map_err(|source| invalid_metadata_for(domain, source))
}

fn open_existing_families<D: StorageDomain>(
    generation: &StoreGeneration,
    definition: &DomainBlueprint,
) -> Result<RegisteredDomain, DomainRegistrationError> {
    let mut families = Vec::with_capacity(definition.families.len());
    for family in &definition.families {
        if !generation
            .database
            .keyspace_exists(&family.physical_name)
            .map_err(|source| {
                registration_storage::<D>(DomainRegistrationStage::OpenKeyspace, source)
            })?
        {
            return Err(DomainRegistrationError::MissingKeyspace {
                domain: D::NAME,
                keyspace: family.physical_name.clone(),
            });
        }
        let keyspace = generation
            .database
            .open_keyspace(&family.physical_name)
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
    let mut keyspaces: Vec<Option<Keyspace>> = Vec::with_capacity(definition.families.len());
    for family in &definition.families {
        let exists = generation
            .database
            .keyspace_exists(&family.physical_name)
            .map_err(|source| {
                registration_storage::<D>(DomainRegistrationStage::OpenKeyspace, source)
            })?;
        let keyspace = if exists {
            Some(
                generation
                    .database
                    .open_keyspace(&family.physical_name)
                    .map_err(|source| {
                        registration_storage::<D>(DomainRegistrationStage::OpenKeyspace, source)
                    })?,
            )
        } else {
            None
        };
        keyspaces.push(keyspace);
    }

    let snapshot = generation.database.snapshot().map_err(|source| {
        registration_storage::<D>(DomainRegistrationStage::OpenKeyspace, source)
    })?;
    for (family, keyspace) in definition.families.iter().zip(&keyspaces) {
        let Some(keyspace) = keyspace else {
            continue;
        };
        let mut cursor = snapshot.exhaustive(keyspace).map_err(|source| {
            registration_storage::<D>(DomainRegistrationStage::OpenKeyspace, source)
        })?;
        if cursor
            .next()
            .map_err(|source| {
                registration_storage::<D>(DomainRegistrationStage::OpenKeyspace, source)
            })?
            .is_some()
        {
            return Err(DomainRegistrationError::UnexpectedKeyspace {
                keyspace: family.physical_name.clone(),
            });
        }
    }
    drop(snapshot);

    for (family, keyspace) in definition.families.iter().zip(&mut keyspaces) {
        if keyspace.is_some() {
            continue;
        }
        *keyspace = Some(
            generation
                .database
                .create_keyspace(&family.physical_name)
                .map_err(|source| {
                    registration_storage::<D>(DomainRegistrationStage::OpenKeyspace, source)
                })?,
        );
    }

    let families = definition
        .families
        .iter()
        .zip(keyspaces)
        .map(|(family, keyspace)| {
            registered_family(
                family,
                keyspace.expect("every declared family was opened or created"),
            )
        })
        .collect();
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
    let key = definition.name.as_bytes().to_vec().into_boxed_slice();
    let metadata = metadata.into_boxed_slice();
    let key_bytes = u64::try_from(key.len()).map_err(invalid_metadata::<D>)?;
    let value_bytes = u64::try_from(metadata.len()).map_err(invalid_metadata::<D>)?;
    let capacity = generation
        .database
        .storage_policy()
        .batch_capacity(1, key_bytes, value_bytes)
        .map_err(|source| {
            registration_storage::<D>(DomainRegistrationStage::CommitRegistry, source)
        })?;
    let mut batch = generation
        .database
        .batch(capacity, PersistMode::Buffer)
        .map_err(|source| {
            registration_storage::<D>(DomainRegistrationStage::CommitRegistry, source)
        })?;
    batch
        .insert(generation.domains_keyspace(), key, metadata)
        .map_err(|source| {
            registration_storage::<D>(DomainRegistrationStage::CommitRegistry, source)
        })?;
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
        reopen_validator: definition.reopen_validator,
        reconciler: definition.reconciler,
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
    invalid_metadata_for(D::NAME, source)
}

fn invalid_metadata_for(
    domain: &'static str,
    source: impl Error + Send + Sync + 'static,
) -> DomainRegistrationError {
    DomainRegistrationError::InvalidMetadata {
        domain,
        source: Box::new(source),
    }
}

fn registration_storage<D: StorageDomain>(
    stage: DomainRegistrationStage,
    source: fjall::Error,
) -> DomainRegistrationError {
    registration_storage_for(D::NAME, stage, source)
}

fn registration_storage_for(
    domain: &'static str,
    stage: DomainRegistrationStage,
    source: fjall::Error,
) -> DomainRegistrationError {
    DomainRegistrationError::Storage {
        domain,
        stage,
        source: Box::new(ClassifiedFjallError::direct(source)),
    }
}

fn registration_failure_severity(error: &DomainRegistrationError) -> Option<FailureSeverity> {
    match error {
        DomainRegistrationError::InvalidDefinition(_)
        | DomainRegistrationError::DuplicateDomain { .. }
        | DomainRegistrationError::OwnerTypeMismatch { .. }
        | DomainRegistrationError::HealthGate(_) => None,
        DomainRegistrationError::Storage { source, .. } => {
            source.downcast_ref::<ClassifiedFjallError>().map_or(
                Some(FailureSeverity::Structural),
                ClassifiedFjallError::severity,
            )
        }
        DomainRegistrationError::ValidationAccess { source, .. } => {
            callback::callback_failure_severity(source)
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
        DomainValidationError::Access { source, .. } => {
            callback::callback_failure_severity(source).unwrap_or(FailureSeverity::Structural)
        }
        DomainValidationError::Snapshot { source } | DomainValidationError::Health { source } => {
            source
                .downcast_ref::<ClassifiedFjallError>()
                .and_then(ClassifiedFjallError::severity)
                .unwrap_or(FailureSeverity::Structural)
        }
        DomainValidationError::HealthGate(_)
        | DomainValidationError::GenerationPoisoned
        | DomainValidationError::Rejected { .. }
        | DomainValidationError::WorkerPanicked => FailureSeverity::Structural,
    }
}
