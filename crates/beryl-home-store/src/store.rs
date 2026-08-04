use std::{
    fmt, io,
    path::{Path, PathBuf},
    sync::{
        Mutex, RwLock,
        atomic::{AtomicU64, Ordering},
    },
};

use beryl_model::BerylHomeId;
use fjall::{Database, Keyspace};

use crate::{
    CanonicalHomeIdentity, HomeCloseError, HomeHeader, HomeOpenError, HomeOpenStage,
    HomeSchemaVersion,
    domain::{DomainBlueprint, DomainRegistry, StoreInstanceId},
    fault::FaultController,
    health::{ClassifiedFjallError, HealthGate},
    layout::{
        DatabaseDisposition, HomeLayout, LayoutAdmissionError, inspect_database,
        reject_database_as_home,
    },
    ownership::{HomeOwnership, OpenedHomeDirectory},
};

mod opening;
mod profile;

use opening::create_fresh_database;
pub(crate) use opening::open_existing_database;
use profile::StorageProfile;

static NEXT_STORE_INSTANCE: AtomicU64 = AtomicU64::new(1);
static NEXT_WRITER_ID: AtomicU64 = AtomicU64::new(1);

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

/// Immutable input for opening exactly one configured Beryl home.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HomeOpenOptions {
    configured_path: PathBuf,
    supported_schema: HomeSchemaVersion,
}

impl HomeOpenOptions {
    /// Constructs an opener for one absolute host path and exact schema.
    #[must_use]
    pub fn new(path: impl Into<PathBuf>, supported_schema: HomeSchemaVersion) -> Self {
        Self {
            configured_path: path.into(),
            supported_schema,
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
}

/// Healthy, exclusively owned physical Beryl-home store.
///
/// Dropping this value releases process ownership after all Fjall handles owned
/// by this generation are dropped. [`Self::close`] performs the same ordering
/// explicitly and reports an unlock failure.
pub struct HomeStore {
    pub(crate) generation: RwLock<Option<StoreGeneration>>,
    pub(crate) registrations: Mutex<Vec<DomainBlueprint>>,
    pub(crate) writer: Mutex<()>,
    pub(crate) writer_id: StoreInstanceId,
    pub(crate) health: HealthGate,
    pub(crate) faults: FaultController,
    ownership: Option<HomeOwnership>,
    pub(crate) storage_profile: StorageProfile,
    database_path: PathBuf,
    home_id: BerylHomeId,
    schema: HomeSchemaVersion,
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
        let directory = OpenedHomeDirectory::open(&configured_path)?;

        reject_database_as_home(directory.canonical_path()).map_err(|source| {
            HomeOpenError::open(&configured_path, HomeOpenStage::AdmitPhysicalLayout, source)
        })?;
        let layout = HomeLayout::at(directory.canonical_path());
        let mut ownership = directory.acquire_lock(&layout.lock_path)?;
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
        ownership
            .retain_state_directory(&layout.database_path)
            .map_err(|source| {
                HomeOpenError::open(&configured_path, HomeOpenStage::AdmitPhysicalLayout, source)
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
        let writer_id = StoreInstanceId(NEXT_WRITER_ID.fetch_add(1, Ordering::Relaxed));
        assert!(
            writer_id.0 != 0,
            "process writer-identity counter exhausted"
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
            writer_id,
            health: HealthGate::healthy(),
            faults,
            ownership: Some(ownership),
            storage_profile,
            database_path: layout.database_path,
            home_id: opened.header.home_id,
            schema: opened.header.schema,
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
        self.ownership().configured_path()
    }

    /// Returns the canonical path resolved from the retained directory handle.
    #[must_use]
    pub fn canonical_path(&self) -> &Path {
        self.ownership().canonical_path()
    }

    /// Returns the live opened-object identity used to collapse path aliases.
    #[must_use]
    pub fn canonical_identity(&self) -> CanonicalHomeIdentity {
        self.ownership().canonical_identity()
    }

    /// Returns the admitted physical Fjall directory for diagnostics.
    #[must_use]
    pub fn database_path(&self) -> &Path {
        &self.database_path
    }

    /// Releases database handles and then the retained OS ownership lock.
    pub fn close(mut self) -> Result<(), HomeCloseError> {
        self.release()
    }

    fn ownership(&self) -> &HomeOwnership {
        self.ownership
            .as_ref()
            .expect("live HomeStore always retains ownership")
    }

    pub(crate) fn require_same_state_directory(&self) -> io::Result<()> {
        self.ownership()
            .require_same_state_directory(&self.database_path)
    }

    fn release(&mut self) -> Result<(), HomeCloseError> {
        self.registrations
            .get_mut()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clear();
        let generation = self
            .generation
            .get_mut()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .take();
        drop(generation);
        match self.ownership.as_mut() {
            Some(ownership) => ownership.release(),
            None => Ok(()),
        }
    }
}

pub(crate) fn next_store_instance() -> StoreInstanceId {
    let instance = StoreInstanceId(NEXT_STORE_INSTANCE.fetch_add(1, Ordering::Relaxed));
    assert!(instance.0 != 0, "process store-instance counter exhausted");
    instance
}

impl fmt::Debug for HomeStore {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("HomeStore")
            .field("configured_path", &self.configured_path())
            .field("canonical_path", &self.canonical_path())
            .field("canonical_identity", &self.canonical_identity())
            .field("database_path", &self.database_path)
            .field("home_id", &self.home_id)
            .field("schema", &self.schema)
            .finish_non_exhaustive()
    }
}

impl Drop for HomeStore {
    fn drop(&mut self) {
        let _ = self.release();
    }
}
