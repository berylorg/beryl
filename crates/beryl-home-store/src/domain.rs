use std::{
    collections::{HashMap, HashSet},
    error::Error,
    fmt,
    marker::PhantomData,
};

use beryl_model::DomainRevision;
use fjall::Keyspace;
use thiserror::Error;

use crate::{
    DomainReader, DomainSchemaVersion, KeyspaceSchemaVersion, SidecarVerifier,
    metadata::{DomainMetadata, PersistedFamily},
};

mod registration;
pub(crate) mod reopen;

const MAX_COMPONENT_BYTES: usize = 64;

/// Stable declaration of one keyspace family owned by a logical domain.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct KeyspaceFamily {
    name: &'static str,
    schema: KeyspaceSchemaVersion,
}

impl KeyspaceFamily {
    /// Declares one logical family and its exact persisted schema.
    #[must_use]
    pub const fn new(name: &'static str, schema: KeyspaceSchemaVersion) -> Self {
        Self { name, schema }
    }

    /// Returns the domain-local family name.
    #[must_use]
    pub const fn name(self) -> &'static str {
        self.name
    }

    /// Returns the exact family schema.
    #[must_use]
    pub const fn schema(self) -> KeyspaceSchemaVersion {
        self.schema
    }
}

/// Typed logical owner of private record families inside one Beryl home.
pub trait StorageDomain: Send + Sync + Sized + 'static {
    /// Stable home-wide domain name.
    const NAME: &'static str;
    /// Exact persisted domain schema.
    const SCHEMA_VERSION: DomainSchemaVersion;
    /// Complete stable family declaration for this domain schema.
    const KEYSPACES: &'static [KeyspaceFamily];
    /// Domain-owned invariant-validation failure.
    type ValidationError: Error + Send + Sync + 'static;

    /// Validates authoritative records through bounded typed reads.
    ///
    /// Validation must be deterministic, bounded, and free of external I/O or
    /// durable side effects. It runs while store admission is controlled and
    /// must not attempt to enter the home writer recursively.
    fn validate(reader: &DomainReader<'_, Self>) -> Result<(), Self::ValidationError>;

    /// Validates authoritative records plus their referenced physical sidecars.
    ///
    /// The default preserves domains that own no sidecars. Sidecar-owning
    /// domains override this hook and use only bounded verifier calls.
    fn validate_reopen(
        reader: &DomainReader<'_, Self>,
        _sidecars: &SidecarVerifier<'_>,
    ) -> Result<(), Self::ValidationError> {
        Self::validate(reader)
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) struct StoreInstanceId(pub(crate) u64);

/// Opaque registration token for one typed logical domain.
pub struct DomainHandle<D: StorageDomain> {
    pub(crate) store: StoreInstanceId,
    pub(crate) slot: usize,
    _domain: PhantomData<fn() -> D>,
}

impl<D: StorageDomain> Copy for DomainHandle<D> {}

impl<D: StorageDomain> Clone for DomainHandle<D> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<D: StorageDomain> fmt::Debug for DomainHandle<D> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("DomainHandle")
            .field("domain", &D::NAME)
            .finish_non_exhaustive()
    }
}

impl<D: StorageDomain> DomainHandle<D> {
    pub(crate) const fn new(store: StoreInstanceId, slot: usize) -> Self {
        Self {
            store,
            slot,
            _domain: PhantomData,
        }
    }
}

/// Invalid static domain declaration detected before touching Fjall.
#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum DomainDefinitionError {
    /// Domain or family components use an invalid bounded identifier.
    #[error(
        "invalid {kind} name `{name}`; use 1-{MAX_COMPONENT_BYTES} lowercase ASCII letters, digits, `_`, or `-`"
    )]
    InvalidName {
        /// Kind of component being validated.
        kind: &'static str,
        /// Rejected identifier.
        name: String,
    },
    /// A domain must own at least one keyspace family.
    #[error("domain `{domain}` declares no keyspace families")]
    NoKeyspaces {
        /// Domain whose declaration is empty.
        domain: &'static str,
    },
    /// One logical family appears more than once.
    #[error("domain `{domain}` declares keyspace family `{family}` more than once")]
    DuplicateKeyspace {
        /// Owning domain.
        domain: &'static str,
        /// Duplicate family.
        family: &'static str,
    },
}

/// Stage at which physical domain registration touched the store.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DomainRegistrationStage {
    /// Read the persistent domain registry.
    ReadRegistry,
    /// Create or reacquire a required physical keyspace.
    OpenKeyspace,
    /// Commit a new persistent domain registration.
    CommitRegistry,
    /// Complete the registration durability barrier.
    PersistRegistry,
}

/// Why a typed logical domain could not join this home generation.
#[derive(Debug, Error)]
pub enum DomainRegistrationError {
    /// The process-wide health gate is not accepting state-dependent work.
    #[error(transparent)]
    HealthGate(#[from] crate::HealthGateError),

    /// A panic poisoned an internal generation or registration lock.
    #[error("the Beryl-home domain registry lock is poisoned")]
    RegistryPoisoned,

    /// The domain's static definition is invalid.
    #[error(transparent)]
    InvalidDefinition(#[from] DomainDefinitionError),

    /// The same stable domain has already registered in this generation.
    #[error("domain `{domain}` is already registered in this home generation")]
    DuplicateDomain {
        /// Stable duplicate domain name.
        domain: &'static str,
    },

    /// A new registration found a physical family without registry authority.
    #[error("unregistered physical keyspace `{keyspace}` already exists")]
    UnexpectedKeyspace {
        /// Existing physical keyspace name.
        keyspace: String,
    },

    /// An existing registration references a missing required family.
    #[error("registered domain `{domain}` is missing physical keyspace `{keyspace}`")]
    MissingKeyspace {
        /// Stable domain name.
        domain: &'static str,
        /// Missing physical keyspace name.
        keyspace: String,
    },

    /// The stored domain schema is not the exact supported schema.
    #[error("domain `{domain}` uses schema {found}, but this code supports {supported}")]
    UnsupportedDomainSchema {
        /// Stable domain name.
        domain: &'static str,
        /// Exact supported schema.
        supported: DomainSchemaVersion,
        /// Exact stored schema.
        found: DomainSchemaVersion,
    },

    /// One stored keyspace family is not the exact supported schema.
    #[error(
        "domain `{domain}` family `{family}` uses schema {found}, but this code supports {supported}"
    )]
    UnsupportedKeyspaceSchema {
        /// Stable domain name.
        domain: &'static str,
        /// Logical family name.
        family: String,
        /// Exact supported schema.
        supported: KeyspaceSchemaVersion,
        /// Exact stored schema.
        found: KeyspaceSchemaVersion,
    },

    /// Stored and declared family sets differ.
    #[error("domain `{domain}` has an incompatible persisted keyspace-family declaration")]
    IncompatibleKeyspaces {
        /// Stable domain name.
        domain: &'static str,
    },

    /// Persistent registration metadata is malformed.
    #[error("domain `{domain}` has invalid registration metadata: {source}")]
    InvalidMetadata {
        /// Stable domain name.
        domain: &'static str,
        /// Bounded metadata decoder failure.
        #[source]
        source: Box<dyn Error + Send + Sync>,
    },

    /// The domain rejected its existing authoritative records.
    #[error("domain `{domain}` failed invariant validation: {source}")]
    Validation {
        /// Stable domain name.
        domain: &'static str,
        /// Domain-owned validator failure.
        #[source]
        source: Box<dyn Error + Send + Sync>,
    },

    /// Fjall failed during registration.
    #[error("domain `{domain}` registration failed during {stage:?}: {source}")]
    Storage {
        /// Stable domain name.
        domain: &'static str,
        /// Registration stage.
        stage: DomainRegistrationStage,
        /// Engine source hidden behind the package boundary.
        #[source]
        source: Box<dyn Error + Send + Sync>,
    },
}

/// Failure from re-running all registered domain validators.
#[derive(Debug, Error)]
#[error("domain `{domain}` failed invariant validation: {source}")]
pub struct DomainValidationError {
    /// Stable domain name.
    pub domain: &'static str,
    /// Domain-owned validation failure or typed read failure.
    #[source]
    pub source: Box<dyn Error + Send + Sync>,
}

/// Why a caller could not reacquire a typed handle after home recovery.
#[derive(Debug, Error)]
pub enum DomainHandleError {
    /// The process-wide health gate is not accepting state-dependent work.
    #[error(transparent)]
    HealthGate(#[from] crate::HealthGateError),
    /// A panic poisoned the in-process generation lock.
    #[error("the Beryl-home generation lock is poisoned")]
    GenerationPoisoned,
    /// The requested typed domain was never registered in this process.
    #[error("domain `{domain}` is not registered in this home generation")]
    NotRegistered {
        /// Stable requested domain name.
        domain: &'static str,
    },
}

pub(crate) struct RegisteredFamily {
    pub(crate) logical_name: &'static str,
    pub(crate) physical_name: String,
    pub(crate) schema: KeyspaceSchemaVersion,
    pub(crate) keyspace: Keyspace,
}

pub(crate) type ErasedValidator =
    fn(&fjall::Snapshot, &RegisteredDomain) -> Result<(), Box<dyn Error + Send + Sync>>;
pub(crate) type ErasedReopenValidator = fn(
    &fjall::Snapshot,
    &RegisteredDomain,
    &SidecarVerifier<'_>,
) -> Result<(), Box<dyn Error + Send + Sync>>;

#[derive(Clone)]
pub(crate) struct DomainBlueprint {
    pub(crate) name: &'static str,
    pub(crate) schema: DomainSchemaVersion,
    pub(crate) families: Vec<FamilyBlueprint>,
    pub(crate) validator: ErasedValidator,
    pub(crate) reopen_validator: ErasedReopenValidator,
}

#[derive(Clone)]
pub(crate) struct FamilyBlueprint {
    pub(crate) logical_name: &'static str,
    pub(crate) physical_name: String,
    pub(crate) schema: KeyspaceSchemaVersion,
}

impl DomainBlueprint {
    pub(crate) fn metadata(&self, revision: DomainRevision) -> DomainMetadata {
        DomainMetadata {
            schema: self.schema,
            revision,
            families: self
                .families
                .iter()
                .map(|family| PersistedFamily {
                    logical_name: family.logical_name.to_owned(),
                    physical_name: family.physical_name.clone(),
                    schema: family.schema,
                })
                .collect(),
        }
    }
}

pub(crate) struct RegisteredDomain {
    pub(crate) name: &'static str,
    pub(crate) schema: DomainSchemaVersion,
    pub(crate) families: Vec<RegisteredFamily>,
    family_slots: HashMap<&'static str, usize>,
    validator: ErasedValidator,
    reopen_validator: ErasedReopenValidator,
}

impl RegisteredDomain {
    pub(crate) fn family(&self, logical_name: &str) -> Option<&RegisteredFamily> {
        self.family_slots
            .get(logical_name)
            .and_then(|slot| self.families.get(*slot))
    }

    pub(crate) fn family_slot(&self, logical_name: &str) -> Option<usize> {
        self.family_slots.get(logical_name).copied()
    }

    pub(crate) fn metadata(&self, revision: DomainRevision) -> DomainMetadata {
        DomainMetadata {
            schema: self.schema,
            revision,
            families: self
                .families
                .iter()
                .map(|family| PersistedFamily {
                    logical_name: family.logical_name.to_owned(),
                    physical_name: family.physical_name.clone(),
                    schema: family.schema,
                })
                .collect(),
        }
    }

    pub(crate) fn validate(
        &self,
        snapshot: &fjall::Snapshot,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        (self.validator)(snapshot, self)
    }

    pub(crate) fn validate_reopen(
        &self,
        snapshot: &fjall::Snapshot,
        sidecars: &SidecarVerifier<'_>,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        (self.reopen_validator)(snapshot, self, sidecars)
    }
}

#[derive(Default)]
pub(crate) struct DomainRegistry {
    entries: Vec<RegisteredDomain>,
    names: HashMap<&'static str, usize>,
    physical_names: HashSet<String>,
}

impl DomainRegistry {
    pub(crate) fn contains_name(&self, name: &str) -> bool {
        self.names.contains_key(name)
    }

    pub(crate) fn contains_physical_name(&self, name: &str) -> bool {
        self.physical_names.contains(name)
    }

    pub(crate) fn insert(&mut self, domain: RegisteredDomain) -> usize {
        let slot = self.entries.len();
        for family in &domain.families {
            self.physical_names.insert(family.physical_name.clone());
        }
        self.names.insert(domain.name, slot);
        self.entries.push(domain);
        slot
    }

    pub(crate) fn get(&self, slot: usize) -> Option<&RegisteredDomain> {
        self.entries.get(slot)
    }

    pub(crate) fn iter(&self) -> impl Iterator<Item = &RegisteredDomain> {
        self.entries.iter()
    }

    pub(crate) fn slot_for(&self, name: &str) -> Option<usize> {
        self.names.get(name).copied()
    }
}
