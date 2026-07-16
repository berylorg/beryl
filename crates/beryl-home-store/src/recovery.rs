use std::{error::Error, io};

use fjall::{PersistMode, Readable};
use thiserror::Error;

use crate::{
    DomainRegistrationError, DomainValidationError, HomeGeneration, HomeHealthSnapshot,
    HomeHealthState, HomeOpenError, HomeStore,
    domain::reopen::{reacquire_registry, validate_reopen_registry},
    fault::FaultPoint,
    health::HealthMaintenanceError,
    layout::{DatabaseDisposition, HomeLayout, LayoutAdmissionError, inspect_database},
    metadata::HEADER_KEY,
    read::{read_domain_metadata, read_home_revision},
    store::{StoreGeneration, next_store_instance, open_existing_database},
};

/// Why the bounded in-place verification attempt could not restore admission.
#[derive(Debug, Error)]
pub enum HealthVerificationError {
    /// Another verification or recovery attempt already owns maintenance.
    #[error("home maintenance is already in progress while the store is {state:?}")]
    InProgress {
        /// Current coherent health state.
        state: HomeHealthState,
    },
    /// Verification was requested outside the `verifying` state.
    #[error("home verification cannot start while the store is {state:?}")]
    InvalidState {
        /// Current coherent health state.
        state: HomeHealthState,
    },
    /// A panic poisoned an internal store lock.
    #[error("an internal Beryl-home lock is poisoned")]
    LockPoisoned,
    /// The current engine generation failed its explicit persistence barrier.
    #[error("home verification persistence barrier failed: {source}")]
    Persistence {
        /// Engine or deterministic fault source.
        #[source]
        source: Box<dyn Error + Send + Sync>,
    },
    /// The current home header or registered domain authority is invalid.
    #[error("home verification rejected authoritative state: {source}")]
    Validation {
        /// Bounded structural or domain source.
        #[source]
        source: Box<dyn Error + Send + Sync>,
    },
    /// One registered domain rejected exhaustive authoritative validation.
    #[error(transparent)]
    DomainValidation(#[from] DomainValidationError),
}

/// Why exact same-home forced recovery did not publish a new generation.
#[derive(Debug, Error)]
pub enum HomeRecoveryError {
    /// Another verification or recovery attempt already owns maintenance.
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
    /// A registered typed domain could not be reacquired or validated.
    #[error(transparent)]
    Domain(#[from] DomainRegistrationError),
    /// The reopened home header or registered domain metadata is invalid.
    #[error("reopened home rejected authoritative control state: {source}")]
    Validation {
        /// Bounded structural source.
        #[source]
        source: Box<dyn Error + Send + Sync>,
    },
    /// One reopened domain rejected exhaustive authoritative validation.
    #[error(transparent)]
    DomainValidation(#[from] DomainValidationError),
    /// The reopened database failed its final persistence barrier.
    #[error("reopened home persistence barrier failed: {source}")]
    Persistence {
        /// Engine or deterministic fault source.
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
    /// Returns the newly published healthy generation.
    #[must_use]
    pub const fn generation(self) -> HomeGeneration {
        self.generation
    }
}

impl HomeStore {
    /// Runs the one bounded verification attempt for the current generation.
    ///
    /// Admission remains closed until the persistence barrier, control records,
    /// registrations, and every registered domain validator agree.
    pub fn verify_health(&self) -> Result<HomeHealthSnapshot, HealthVerificationError> {
        let maintenance = self
            .health
            .begin_verification()
            .map_err(map_verification_maintenance)?;
        let result = self.verify_current_generation();
        match result {
            Ok(generation) => {
                maintenance.finish_healthy(generation);
                Ok(self.health.snapshot())
            }
            Err(error) => {
                maintenance.finish_failed();
                Err(error)
            }
        }
    }

    /// Force-recovers, validates, and republishes only the same retained home.
    ///
    /// The outer OS ownership lock remains held. Every Fjall and keyspace handle
    /// from the failed generation is dropped before `Database::recover`, and no
    /// create-or-recover fallback is used.
    pub fn recover_same_home(&self) -> Result<RecoveryReceipt, HomeRecoveryError> {
        let maintenance = self
            .health
            .begin_recovery()
            .map_err(map_recovery_maintenance)?;
        let result = self.recover_same_home_inner();
        match result {
            Ok(receipt) => {
                maintenance.finish_healthy(receipt.generation);
                Ok(receipt)
            }
            Err(error) => {
                maintenance.finish_failed();
                Err(error)
            }
        }
    }

    fn verify_current_generation(&self) -> Result<HomeGeneration, HealthVerificationError> {
        drop(
            self.writer
                .lock()
                .map_err(|_| HealthVerificationError::LockPoisoned)?,
        );
        self.faults
            .check(FaultPoint::BeforeVerification)
            .map_err(|source| HealthVerificationError::Persistence {
                source: Box::new(source),
            })?;
        let generation = self
            .generation
            .read()
            .map_err(|_| HealthVerificationError::LockPoisoned)?;
        let generation = generation
            .as_ref()
            .ok_or(HealthVerificationError::LockPoisoned)?;
        generation
            .database
            .persist(PersistMode::SyncAll)
            .map_err(|source| HealthVerificationError::Persistence {
                source: Box::new(source),
            })?;
        validate_generation_control(generation, self)
            .map_err(|source| HealthVerificationError::Validation { source })?;
        validate_reopen_registry(generation, &crate::SidecarVerifier::new(self))?;
        self.health
            .snapshot()
            .generation()
            .ok_or(HealthVerificationError::LockPoisoned)
    }

    fn recover_same_home_inner(&self) -> Result<RecoveryReceipt, HomeRecoveryError> {
        drop(
            self.writer
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner()),
        );
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
        self.require_same_state_directory()
            .map_err(|source| HomeRecoveryError::Layout {
                source: Box::new(source),
            })?;
        require_existing_layout(self.database_path())?;
        let layout = HomeLayout::at(self.canonical_path());
        let opened = open_existing_database(self.configured_path(), &layout)?;
        if opened.header.home_id != self.home_id() || opened.header.schema != self.schema() {
            return Err(HomeRecoveryError::HomeMismatch);
        }
        let mut generation = StoreGeneration {
            database: opened.database,
            control: opened.control,
            registry: Default::default(),
            instance_id: next_store_instance(),
        };
        reacquire_registry(&mut generation, &registrations)?;
        validate_generation_control(&generation, self)
            .map_err(|source| HomeRecoveryError::Validation { source })?;
        validate_reopen_registry(&generation, &crate::SidecarVerifier::new(self))?;
        generation
            .database
            .persist(PersistMode::SyncAll)
            .map_err(|source| HomeRecoveryError::Persistence {
                source: Box::new(source),
            })?;
        self.faults
            .check(FaultPoint::AfterReopen)
            .map_err(|source| HomeRecoveryError::Persistence {
                source: Box::new(source),
            })?;
        *generation_slot = Some(generation);
        // Recovery may unpoison only this unit writer lock, and only after the
        // validated replacement generation has been published.
        self.writer.clear_poison();
        Ok(RecoveryReceipt { generation: next })
    }
}

fn validate_generation_control(
    generation: &StoreGeneration,
    store: &HomeStore,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let snapshot = generation.database.snapshot();
    let encoded_header = snapshot
        .get(generation.header_keyspace(), HEADER_KEY)?
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "home header is missing"))?;
    let header = crate::HomeHeader::decode(&encoded_header)?;
    if header.home_id != store.home_id() || header.schema != store.schema() {
        return Err(Box::new(io::Error::new(
            io::ErrorKind::InvalidData,
            "home header identity or schema changed",
        )));
    }
    read_home_revision(&snapshot, generation.header_keyspace())?;

    for domain in generation.registry.iter() {
        for family in &domain.families {
            if !generation.database.keyspace_exists(&family.physical_name) {
                return Err(Box::new(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("required keyspace `{}` is missing", family.physical_name),
                )));
            }
        }
        let metadata = read_domain_metadata(&snapshot, generation.domains_keyspace(), domain.name)?;
        if metadata != domain.metadata(metadata.revision) {
            return Err(Box::new(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("domain `{}` registration changed", domain.name),
            )));
        }
    }
    Ok(())
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

fn map_verification_maintenance(error: HealthMaintenanceError) -> HealthVerificationError {
    match error {
        HealthMaintenanceError::InProgress { state } => {
            HealthVerificationError::InProgress { state }
        }
        HealthMaintenanceError::InvalidState { state } => {
            HealthVerificationError::InvalidState { state }
        }
    }
}

fn map_recovery_maintenance(error: HealthMaintenanceError) -> HomeRecoveryError {
    match error {
        HealthMaintenanceError::InProgress { state } => HomeRecoveryError::InProgress { state },
        HealthMaintenanceError::InvalidState { state } => HomeRecoveryError::InvalidState { state },
    }
}
