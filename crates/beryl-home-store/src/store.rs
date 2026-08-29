use std::{
    fmt,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex, RwLock,
        atomic::{AtomicU64, Ordering},
    },
};

use beryl_model::BerylHomeId;
use fjall::{Database, Keyspace};

use crate::{
    HomeCloseError, HomeHeader, HomeOpenError, HomeOpenStage, HomeSchemaVersion,
    domain::{DomainBlueprint, DomainRegistry, StoreInstanceId},
    fault::FaultController,
    health::{ClassifiedFjallError, HealthGate},
    layout::{
        DatabaseDisposition, HomeLayout, LayoutAdmissionError, inspect_database,
        reject_database_as_home,
    },
    ownership::{CanonicalHomePath, HomeLifecycleCustodian},
    reconciliation::ReconciliationRegistry,
};

mod opening;
mod profile;

use opening::create_fresh_database;
pub(crate) use opening::open_existing_database;
use profile::StorageProfile;

static NEXT_STORE_INSTANCE: AtomicU64 = AtomicU64::new(1);
static NEXT_WRITER_ID: AtomicU64 = AtomicU64::new(1);

/// The crash-durability contract established for an opened Beryl home.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HomeDurabilityTier {
    /// Native local NTFS with the package's full crash-durability contract.
    Full,
    /// An accessible reliably locked filesystem without the full NTFS contract.
    BestEffort,
}

/// Concrete ownership boundary conditions injected only by package tests.
#[cfg(feature = "test-faults")]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HomeOwnershipTestSeam {
    /// Simulates an accessible UNC location after ordinary local test access.
    UncPath,
    /// Simulates an accessible mapped-remote location after ordinary local test access.
    MappedRemotePath,
    /// Makes the reliable exclusive lifetime lock unsupported.
    UnsupportedExclusiveLock,
}

pub(crate) struct HomeControl {
    header: Keyspace,
    domains: Keyspace,
}

pub(crate) struct OpenedDatabase {
    pub(crate) database: Database,
    pub(crate) control: HomeControl,
    pub(crate) header: HomeHeader,
}

pub(crate) struct StoreGeneration {
    pub(crate) database: Database,
    pub(crate) control: HomeControl,
    pub(crate) registry: DomainRegistry,
    pub(crate) instance_id: StoreInstanceId,
}

impl StoreGeneration {
    pub(crate) fn header_keyspace(&self) -> &Keyspace {
        &self.control.header
    }

    pub(crate) fn domains_keyspace(&self) -> &Keyspace {
        &self.control.domains
    }
}

impl Drop for StoreGeneration {
    fn drop(&mut self) {
        self.registry.retire_attachments();
    }
}

/// Immutable input for opening exactly one configured Beryl home.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HomeOpenOptions {
    configured_path: PathBuf,
    supported_schema: HomeSchemaVersion,
    #[cfg(feature = "test-faults")]
    ownership_test_seam: Option<HomeOwnershipTestSeam>,
    #[cfg(feature = "test-faults")]
    durability_tier_override: Option<HomeDurabilityTier>,
}

impl HomeOpenOptions {
    /// Constructs an opener for one absolute host path and exact schema.
    #[must_use]
    pub fn new(path: impl Into<PathBuf>, supported_schema: HomeSchemaVersion) -> Self {
        Self {
            configured_path: path.into(),
            supported_schema,
            #[cfg(feature = "test-faults")]
            ownership_test_seam: None,
            #[cfg(feature = "test-faults")]
            durability_tier_override: None,
        }
    }

    /// Returns the user-configured home path spelling.
    #[must_use]
    pub fn configured_path(&self) -> &Path {
        &self.configured_path
    }

    /// Returns the exact schema accepted or created by this opener.
    #[must_use]
    pub const fn supported_schema(&self) -> HomeSchemaVersion {
        self.supported_schema
    }

    /// Injects one concrete ownership boundary condition for package tests.
    #[cfg(feature = "test-faults")]
    #[must_use]
    pub fn with_ownership_test_seam(mut self, seam: HomeOwnershipTestSeam) -> Self {
        self.ownership_test_seam = Some(seam);
        self
    }

    /// Overrides only the exposed tier for existing deterministic package tests.
    #[cfg(feature = "test-faults")]
    #[must_use]
    pub fn with_durability_tier_for_tests(mut self, tier: HomeDurabilityTier) -> Self {
        self.durability_tier_override = Some(tier);
        self
    }
}

/// Healthy, exclusively owned physical Beryl-home store.
///
/// Dropping this value releases disposable Fjall state. It releases process
/// ownership only when no retained reconciliation scope remains.
pub struct HomeStore {
    pub(crate) generation: RwLock<Option<StoreGeneration>>,
    pub(crate) registrations: Mutex<Vec<DomainBlueprint>>,
    pub(crate) writer: Mutex<()>,
    pub(crate) theme_mutation: Mutex<()>,
    pub(crate) theme_watcher: crate::theme::ThemeWatcherCoordinator,
    pub(crate) writer_id: StoreInstanceId,
    pub(crate) health: Arc<HealthGate>,
    pub(crate) faults: FaultController,
    pub(crate) reconciliation: ReconciliationRegistry,
    pub(crate) scrub: Arc<crate::scrub::ScrubCoordinator>,
    pub(crate) lifecycle: Arc<HomeLifecycleCustodian>,
    pub(crate) storage_profile: StorageProfile,
    pub(crate) database_path: PathBuf,
    pub(crate) home_id: BerylHomeId,
    pub(crate) schema: HomeSchemaVersion,
    pub(crate) recovery_transferred: bool,
}

impl HomeStore {
    /// Opens, exclusively owns, and validates one Beryl home.
    ///
    /// Existing state is force-recovered. It is never passed through Fjall's
    /// create-or-recover dispatch when the physical database is nonempty.
    pub fn open(options: HomeOpenOptions) -> Result<Self, HomeOpenError> {
        Self::open_inner(options, FaultController::new())
    }

    /// Opens one home with store-local deterministic fault controls.
    #[cfg(feature = "test-faults")]
    pub fn open_with_faults(
        options: HomeOpenOptions,
        faults: FaultController,
    ) -> Result<Self, HomeOpenError> {
        Self::open_inner(options, faults)
    }

    fn open_inner(
        options: HomeOpenOptions,
        faults: FaultController,
    ) -> Result<Self, HomeOpenError> {
        let configured_path = options.configured_path.clone();
        let storage_profile = StorageProfile::production().map_err(|source| {
            HomeOpenError::open(
                &configured_path,
                HomeOpenStage::ConfigureStoragePolicy,
                ClassifiedFjallError::direct(source),
            )
        })?;
        let directory = CanonicalHomePath::open(&configured_path)?;
        #[cfg(feature = "test-faults")]
        let directory = match options.ownership_test_seam {
            Some(seam) => directory.with_test_seam(seam),
            None => directory,
        };
        #[cfg(feature = "test-faults")]
        let directory = match options.durability_tier_override {
            Some(tier) => directory.with_durability_tier(tier),
            None => directory,
        };

        reject_database_as_home(directory.canonical_path()).map_err(|source| {
            HomeOpenError::open(&configured_path, HomeOpenStage::AdmitPhysicalLayout, source)
        })?;
        let layout = HomeLayout::at(directory.canonical_path());
        let ownership = directory.acquire_lock(&layout.lock_path)?;
        let disposition =
            inspect_database(&layout.database_path).map_err(|failure| match failure {
                LayoutAdmissionError::Collision(source) => HomeOpenError::open(
                    &configured_path,
                    HomeOpenStage::AdmitPhysicalLayout,
                    source,
                ),
                LayoutAdmissionError::Unreadable { stage, source } => {
                    HomeOpenError::unreadable(&configured_path, stage, source)
                }
            })?;
        let opened = match disposition {
            DatabaseDisposition::Fresh => create_fresh_database(
                &configured_path,
                &layout,
                options.supported_schema,
                storage_profile,
            )?,
            DatabaseDisposition::Existing => {
                open_existing_database(&configured_path, &layout, storage_profile)?
            }
        };

        if opened.header.schema != options.supported_schema {
            return Err(HomeOpenError::UnsupportedSchema {
                path: configured_path,
                supported: options.supported_schema,
                found: opened.header.schema,
            });
        }

        let instance_id = next_store_instance();
        let writer_id = next_writer_instance();

        let health = Arc::new(HealthGate::healthy());
        let lifecycle = Arc::new(HomeLifecycleCustodian::new(ownership));
        let reconciliation = ReconciliationRegistry::new(
            storage_profile.reconciliation_descriptor_bytes(),
            storage_profile.reconciliation_reserved_bytes(),
            Arc::clone(&health),
            Arc::clone(&lifecycle),
        );
        Ok(Self {
            generation: RwLock::new(Some(StoreGeneration {
                database: opened.database,
                control: opened.control,
                registry: DomainRegistry::default(),
                instance_id,
            })),
            registrations: Mutex::new(Vec::new()),
            writer: Mutex::new(()),
            theme_mutation: Mutex::new(()),
            theme_watcher: crate::theme::ThemeWatcherCoordinator::default(),
            writer_id,
            health,
            faults,
            reconciliation,
            scrub: Arc::new(crate::scrub::ScrubCoordinator::default()),
            lifecycle,
            storage_profile,
            database_path: layout.database_path,
            home_id: opened.header.home_id,
            schema: opened.header.schema,
            recovery_transferred: false,
        })
    }

    /// Returns the coherent process-wide state-store health observation.
    #[must_use]
    pub fn health(&self) -> crate::HomeHealthSnapshot {
        self.health.snapshot()
    }

    /// Returns the durable opaque identity stored in the home header.
    #[must_use]
    pub const fn home_id(&self) -> BerylHomeId {
        self.home_id
    }

    /// Returns the exact validated durable schema.
    #[must_use]
    pub const fn schema(&self) -> HomeSchemaVersion {
        self.schema
    }

    /// Returns the configured path spelling supplied by the caller.
    #[must_use]
    pub fn configured_path(&self) -> &Path {
        self.lifecycle.configured_path()
    }

    /// Returns the canonical configured-home path used for process-local identity.
    #[must_use]
    pub fn canonical_path(&self) -> &Path {
        self.lifecycle.canonical_path()
    }

    /// Performs one synchronous free-space observation against this home's canonical path.
    ///
    /// The input is the opaque validated turn-start requirement; arbitrary byte totals cannot be
    /// queried. The result is uncached and does not reserve capacity. Callers decide
    /// whether the outcome admits their operation; a sufficient result does
    /// not change ordinary later filesystem error classification.
    #[must_use]
    pub fn query_free_space(
        &self,
        requirement: crate::TurnStartAdmissionRequirement,
    ) -> crate::FreeSpaceOutcome {
        crate::free_space::query(
            self.canonical_path(),
            requirement.total_bytes(),
            &self.faults,
        )
    }

    /// Returns the crash-durability tier of this opened home.
    #[must_use]
    pub fn durability_tier(&self) -> HomeDurabilityTier {
        self.lifecycle.durability_tier()
    }

    /// Returns the admitted physical Fjall directory for diagnostics.
    #[must_use]
    pub fn database_path(&self) -> &Path {
        &self.database_path
    }

    /// Stops new reconciliation reservations, then releases disposable Fjall state and ownership
    /// when no scope retains the home lifecycle.
    ///
    /// If descriptor custody remains reserved or installed, the returned error owns the still-open
    /// store. Use [`HomeCloseError::into_open_store`] to recover it after joining the pending Phase
    /// 102 classification work.
    pub fn close(mut self) -> Result<(), HomeCloseError> {
        let pending_scopes = self.reconciliation.begin_close();
        if pending_scopes != 0 {
            return Err(HomeCloseError::pending_reconciliation(self, pending_scopes));
        }
        self.release_disposable();
        self.lifecycle.release()
    }

    fn release_disposable(&mut self) {
        self.theme_watcher.shutdown();
        self.registrations
            .get_mut()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clear();
        self.retire_generation();
    }

    pub(crate) fn retire_generation(&mut self) {
        let generation = self
            .generation
            .get_mut()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .take();
        drop(generation);
    }
}

pub(crate) fn next_store_instance() -> StoreInstanceId {
    let instance = StoreInstanceId(NEXT_STORE_INSTANCE.fetch_add(1, Ordering::Relaxed));
    assert!(instance.0 != 0, "process store-instance counter exhausted");
    instance
}

pub(crate) fn next_writer_instance() -> StoreInstanceId {
    let writer_id = StoreInstanceId(NEXT_WRITER_ID.fetch_add(1, Ordering::Relaxed));
    assert!(
        writer_id.0 != 0,
        "process writer-identity counter exhausted"
    );
    writer_id
}

impl fmt::Debug for HomeStore {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("HomeStore")
            .field("configured_path", &self.configured_path())
            .field("canonical_path", &self.canonical_path())
            .field("durability_tier", &self.durability_tier())
            .field("database_path", &self.database_path)
            .field("home_id", &self.home_id)
            .field("schema", &self.schema)
            .finish_non_exhaustive()
    }
}

impl Drop for HomeStore {
    fn drop(&mut self) {
        if self.recovery_transferred {
            self.release_disposable();
            return;
        }
        let pending_scopes = self.reconciliation.begin_drop();
        self.release_disposable();
        if pending_scopes == 0 {
            let _ = self.lifecycle.release();
        }
    }
}
