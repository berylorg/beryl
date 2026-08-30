use std::{error::Error, io};

use fjall::PersistMode;
use thiserror::Error;

use crate::{
    DomainHandle, DomainHandleError, DomainRegistrationError, HomeGeneration, HomeHealthState,
    HomeOpenError, HomeStore, StorageDomain,
    domain::{DomainBlueprint, DomainOwnerId, reopen::reacquire_registry},
    fault::FaultPoint,
    health::{ClassifiedFjallError, HealthMaintenance, HealthMaintenanceError},
    layout::{DatabaseDisposition, HomeLayout, LayoutAdmissionError, inspect_database},
    metadata::{
        DomainMetadata, HEADER_KEY, HOME_REVISION_BYTES, HOME_REVISION_KEY,
        MAX_DOMAIN_METADATA_BYTES, MAX_HOME_HEADER_BYTES, decode_home_revision,
    },
    store::{StoreGeneration, next_store_instance, open_existing_database},
};

/// Why exact same-home forced recovery could not construct a private candidate.
#[derive(Debug, Error)]
pub enum HomeRecoveryError {
    /// Another recovery attempt already owns maintenance.
    #[error("home maintenance is already in progress while the store is {state:?}")]
    InProgress {
        /// Current coherent health state.
        state: HomeHealthState,
    },
    /// Recovery was requested outside the `failed` state.
    #[error("same-home recovery cannot start while the store is {state:?}")]
    InvalidState {
        /// Current coherent health state.
        state: HomeHealthState,
    },
    /// A panic poisoned an internal store lock.
    #[error("an internal Beryl-home lock is poisoned")]
    LockPoisoned,
    /// The retained home no longer contains the exact existing database layout.
    #[error("same-home recovery rejected the physical database layout: {source}")]
    Layout {
        /// Layout inspection source.
        #[source]
        source: Box<dyn Error + Send + Sync>,
    },
    /// Fjall could not force-recover the same existing database.
    #[error(transparent)]
    Open(#[from] HomeOpenError),
    /// The reopened header does not identify the exact retained home.
    #[error("same-home recovery found a different durable home identity or schema")]
    HomeMismatch,
    /// A registered typed domain could not be structurally reacquired.
    #[error(transparent)]
    Domain(#[from] DomainRegistrationError),
    /// The reopened home header or registered domain metadata is invalid.
    #[error("reopened home rejected authoritative control state: {source}")]
    Validation {
        /// Bounded structural source.
        #[source]
        source: Box<dyn Error + Send + Sync>,
    },
    /// The reopened database failed its final persistence barrier.
    #[error("reopened home persistence barrier failed: {source}")]
    Persistence {
        /// Engine or deterministic fault source.
        #[source]
        source: Box<dyn Error + Send + Sync>,
    },
    /// Fjall reported a retained maintenance terminal before recovered publication.
    #[error("reopened home could not confirm storage health: {source}")]
    StorageHealth {
        /// Stable classified engine source.
        #[source]
        source: Box<dyn Error + Send + Sync>,
    },
    /// The monotonic in-process home generation is exhausted.
    #[error("Beryl-home generation counter is exhausted")]
    GenerationExhausted,
}

/// Successful exact same-home recovery identity.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RecoveryReceipt {
    generation: HomeGeneration,
}

impl RecoveryReceipt {
    /// Returns the private candidate generation.
    #[must_use]
    pub const fn generation(self) -> HomeGeneration {
        self.generation
    }
}

/// A fully reopened store generation that remains unavailable until the owning
/// recovery supervisor publishes its complete typed stack.
pub struct HomeRecoveryCandidate {
    store: Option<HomeStore>,
    maintenance: Option<HealthMaintenance>,
    receipt: RecoveryReceipt,
}

impl std::fmt::Debug for HomeRecoveryCandidate {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("HomeRecoveryCandidate")
            .field("receipt", &self.receipt)
            .finish_non_exhaustive()
    }
}

impl HomeRecoveryCandidate {
    /// Returns the durable identity of the retained same home.
    #[must_use]
    pub fn home_id(&self) -> beryl_model::BerylHomeId {
        self.store
            .as_ref()
            .expect("candidate store is present")
            .home_id()
    }

    /// Returns the candidate's new process-local generation.
    #[must_use]
    pub const fn receipt(&self) -> RecoveryReceipt {
        self.receipt
    }

    /// Returns the candidate's new process-local generation.
    #[must_use]
    pub const fn generation(&self) -> HomeGeneration {
        self.receipt.generation()
    }

    /// Returns the tier retained from the same successfully locked home.
    #[must_use]
    pub fn durability_tier(&self) -> crate::HomeDurabilityTier {
        self.store
            .as_ref()
            .expect("candidate store is present")
            .durability_tier()
    }

    /// Reacquires one exact typed handle without opening ordinary store admission.
    pub fn domain_handle<D: StorageDomain>(&self) -> Result<DomainHandle<D>, DomainHandleError> {
        let store = self.store.as_ref().expect("candidate store is present");
        let generation = store
            .generation
            .read()
            .map_err(|_| DomainHandleError::GenerationPoisoned)?;
        let generation = generation
            .as_ref()
            .ok_or(DomainHandleError::GenerationPoisoned)?;
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
        Ok(DomainHandle::new(generation.instance_id, slot))
    }

    pub fn with_domain_attachment<D: StorageDomain, R>(
        &self,
        capability: &crate::DomainAttachmentCapability<D>,
        callback: impl FnOnce(&D::RuntimeAttachment) -> R,
    ) -> Result<R, crate::DomainAttachmentAccessError> {
        let store = self.store.as_ref().expect("candidate store is present");
        let generation = store
            .generation
            .read()
            .map_err(|_| crate::DomainAttachmentAccessError::GenerationPoisoned)?;
        let generation = generation
            .as_ref()
            .ok_or(crate::DomainAttachmentAccessError::StaleOrForeign)?;
        generation.database.health().map_err(|source| {
            crate::DomainAttachmentAccessError::StorageHealth {
                source: Box::new(ClassifiedFjallError::direct(source)),
            }
        })?;
        generation.with_domain_attachment(capability, callback)
    }

    /// Publishes the candidate as the new healthy store generation.
    #[must_use]
    pub fn publish(mut self) -> HomeStore {
        let maintenance = self
            .maintenance
            .take()
            .expect("candidate maintenance authority is present");
        maintenance.finish_healthy(self.receipt.generation);
        self.store.take().expect("candidate store is present")
    }

    /// Aborts unpublished stack construction and returns failed authority for
    /// a later recovery attempt or orderly close.
    #[must_use]
    pub fn abort(mut self) -> HomeStore {
        let maintenance = self
            .maintenance
            .take()
            .expect("candidate maintenance authority is present");
        maintenance.finish_failed();
        let mut store = self.store.take().expect("candidate store is present");
        store.retire_generation();
        store
    }
}

impl Drop for HomeRecoveryCandidate {
    fn drop(&mut self) {
        let Some(mut store) = self.store.take() else {
            return;
        };
        // Dropping an unpublished candidate is a fail-closed abandonment, not
        // successful disposal. Retain only the lifetime custodian until process
        // exit so another service cannot open the home without an explicit abort.
        drop(self.maintenance.take());
        let retained_lifecycle = std::sync::Arc::clone(&store.lifecycle);
        store.recovery_transferred = true;
        drop(store);
        std::mem::forget(retained_lifecycle);
    }
}

/// A failed recovery attempt together with the still-owned failed authority.
#[derive(Debug)]
pub struct HomeRecoveryFailure {
    store: HomeStore,
    error: HomeRecoveryError,
}

impl HomeRecoveryFailure {
    /// Returns the typed recovery error.
    #[must_use]
    pub const fn error(&self) -> &HomeRecoveryError {
        &self.error
    }

    /// Recovers the failed store authority for a later retry or orderly close.
    #[must_use]
    pub fn into_store(self) -> HomeStore {
        self.store
    }

    /// Separates the retained failed authority and its typed error.
    #[must_use]
    pub fn into_parts(self) -> (HomeStore, HomeRecoveryError) {
        (self.store, self.error)
    }
}

impl std::fmt::Display for HomeRecoveryFailure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.error.fmt(formatter)
    }
}

impl Error for HomeRecoveryFailure {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(&self.error)
    }
}

impl HomeStore {
    /// Consumes failed authority and constructs an unpublished fresh service.
    ///
    /// The outer OS ownership lock remains held. Every Fjall and keyspace handle
    /// from the failed generation is dropped before `Database::recover`, and no
    /// create-or-recover fallback is used.
    pub fn recover_same_home(mut self) -> Result<HomeRecoveryCandidate, HomeRecoveryFailure> {
        let maintenance = match self.health.begin_recovery() {
            Ok(maintenance) => maintenance,
            Err(error) => {
                return Err(HomeRecoveryFailure {
                    store: self,
                    error: map_recovery_maintenance(error),
                });
            }
        };
        let result = self.recover_same_home_inner();
        match result {
            Ok((store, receipt)) => {
                self.recovery_transferred = true;
                drop(self);
                Ok(HomeRecoveryCandidate {
                    store: Some(store),
                    maintenance: Some(maintenance),
                    receipt,
                })
            }
            Err(error) => {
                maintenance.finish_failed();
                Err(HomeRecoveryFailure { store: self, error })
            }
        }
    }

    fn recover_same_home_inner(
        &mut self,
    ) -> Result<(HomeStore, RecoveryReceipt), HomeRecoveryError> {
        let current = self
            .health
            .snapshot()
            .generation()
            .ok_or(HomeRecoveryError::LockPoisoned)?;
        let next = current
            .checked_next()
            .ok_or(HomeRecoveryError::GenerationExhausted)?;
        let registrations = self
            .registrations
            .lock()
            .map_err(|_| HomeRecoveryError::LockPoisoned)?
            .clone();
        let old_writer = std::mem::replace(&mut self.writer, std::sync::Mutex::new(()));
        drop(old_writer);
        let mut generation_slot = self
            .generation
            .write()
            .map_err(|_| HomeRecoveryError::LockPoisoned)?;
        drop(generation_slot.take());

        self.faults
            .check(FaultPoint::BeforeReopen)
            .map_err(|source| HomeRecoveryError::Layout {
                source: Box::new(source),
            })?;
        require_existing_layout(self.database_path())?;
        let layout = HomeLayout::at(self.canonical_path());
        let opened = open_existing_database(self.configured_path(), &layout, self.storage_profile)?;
        if opened.header.home_id != self.home_id() || opened.header.schema != self.schema() {
            return Err(HomeRecoveryError::HomeMismatch);
        }
        let mut generation = StoreGeneration {
            database: opened.database,
            control: opened.control,
            registry: Default::default(),
            instance_id: next_store_instance(),
        };
        validate_generation_control(&generation, self, &registrations)
            .map_err(|source| HomeRecoveryError::Validation { source })?;
        reacquire_registry(&mut generation, &registrations)?;
        generation
            .database
            .persist(PersistMode::SyncAll)
            .map_err(|source| HomeRecoveryError::Persistence {
                source: Box::new(ClassifiedFjallError::direct(source)),
            })?;
        self.faults
            .check(FaultPoint::AfterReopen)
            .map_err(|source| HomeRecoveryError::Persistence {
                source: Box::new(source),
            })?;
        generation
            .database
            .health()
            .map_err(|source| HomeRecoveryError::StorageHealth {
                source: Box::new(ClassifiedFjallError::direct(source)),
            })?;
        let writer_id = crate::store::next_writer_instance();
        let recovered = HomeStore {
            generation: std::sync::RwLock::new(Some(generation)),
            registrations: std::sync::Mutex::new(registrations),
            writer: std::sync::Mutex::new(()),
            theme_mutation: std::sync::Mutex::new(()),
            theme_watcher: crate::theme::ThemeWatcherCoordinator::default(),
            writer_id,
            health: std::sync::Arc::clone(&self.health),
            faults: self.faults.clone(),
            reconciliation: self.reconciliation.clone(),
            scrub: std::sync::Arc::clone(&self.scrub),
            lifecycle: std::sync::Arc::clone(&self.lifecycle),
            storage_profile: self.storage_profile,
            database_path: self.database_path.clone(),
            home_id: self.home_id,
            schema: self.schema,
            recovery_transferred: false,
        };
        Ok((recovered, RecoveryReceipt { generation: next }))
    }
}

fn validate_generation_control(
    generation: &StoreGeneration,
    store: &HomeStore,
    registrations: &[DomainBlueprint],
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let snapshot = generation.database.snapshot().map_err(boxed_fjall)?;
    let header = {
        let encoded = bounded_control_point(
            &snapshot,
            generation.header_keyspace(),
            HEADER_KEY,
            MAX_HOME_HEADER_BYTES,
            "home header",
        )?
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "home header is missing"))?;
        crate::HomeHeader::decode(encoded.value())?
    };
    if header.home_id != store.home_id() || header.schema != store.schema() {
        return Err(Box::new(io::Error::new(
            io::ErrorKind::InvalidData,
            "home header identity or schema changed",
        )));
    }
    {
        let revision = bounded_control_point(
            &snapshot,
            generation.header_keyspace(),
            HOME_REVISION_KEY,
            HOME_REVISION_BYTES,
            "home revision",
        )?
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "home revision record is missing",
            )
        })?;
        decode_home_revision(revision.value())?;
    }

    for definition in registrations {
        for family in &definition.families {
            if !generation
                .database
                .keyspace_exists(&family.physical_name)
                .map_err(boxed_fjall)?
            {
                return Err(Box::new(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("required keyspace `{}` is missing", family.physical_name),
                )));
            }
        }
        let metadata = {
            let encoded = bounded_control_point(
                &snapshot,
                generation.domains_keyspace(),
                definition.name.as_bytes(),
                MAX_DOMAIN_METADATA_BYTES,
                "domain registry",
            )?
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("domain `{}` registration is missing", definition.name),
                )
            })?;
            DomainMetadata::decode(encoded.value())?
        };
        if metadata != definition.metadata(metadata.revision) {
            return Err(Box::new(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("domain `{}` registration changed", definition.name),
            )));
        }
    }
    Ok(())
}

fn bounded_control_point<'origin>(
    snapshot: &'origin fjall::Snapshot,
    keyspace: &'origin fjall::Keyspace,
    key: &[u8],
    maximum: usize,
    kind: &'static str,
) -> Result<Option<fjall::KvPair<'origin>>, Box<dyn Error + Send + Sync>> {
    let Some(point) = snapshot.point(keyspace, key).map_err(boxed_fjall)? else {
        return Ok(None);
    };
    let actual = usize::try_from(point.stored_value_len())
        .expect("u32 stored-value length fits usize on supported targets");
    if actual > maximum {
        return Err(Box::new(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("{kind} record has {actual} stored bytes, exceeding {maximum}"),
        )));
    }
    point.acquire().map(Some).map_err(boxed_fjall)
}

fn boxed_fjall(source: fjall::Error) -> Box<dyn Error + Send + Sync> {
    Box::new(ClassifiedFjallError::direct(source))
}

fn require_existing_layout(path: &std::path::Path) -> Result<(), HomeRecoveryError> {
    match inspect_database(path) {
        Ok(DatabaseDisposition::Existing) => Ok(()),
        Ok(DatabaseDisposition::Fresh) => Err(HomeRecoveryError::Layout {
            source: Box::new(io::Error::new(
                io::ErrorKind::NotFound,
                "the failed Beryl-home database is no longer present",
            )),
        }),
        Err(LayoutAdmissionError::Collision(source))
        | Err(LayoutAdmissionError::Unreadable { source, .. }) => Err(HomeRecoveryError::Layout {
            source: Box::new(source),
        }),
    }
}

fn map_recovery_maintenance(error: HealthMaintenanceError) -> HomeRecoveryError {
    match error {
        HealthMaintenanceError::InProgress { state } => HomeRecoveryError::InProgress { state },
        HealthMaintenanceError::InvalidState { state } => HomeRecoveryError::InvalidState { state },
    }
}
