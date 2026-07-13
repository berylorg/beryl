use std::{
    fmt, io,
    path::{Path, PathBuf},
    sync::{
        Mutex, RwLock,
        atomic::{AtomicU64, Ordering},
    },
};

use beryl_model::BerylHomeId;
use fjall::{Config, Database, Keyspace, KeyspaceCreateOptions, PersistMode, Readable};

use crate::{
    CanonicalHomeIdentity, HomeCloseError, HomeHeader, HomeOpenError, HomeOpenStage,
    HomeSchemaVersion, HomeUnreadableStage,
    domain::{DomainBlueprint, DomainRegistry, StoreInstanceId},
    fault::FaultController,
    health::HealthGate,
    layout::{
        DatabaseDisposition, HomeLayout, LayoutAdmissionError, inspect_database,
        reject_database_as_home,
    },
    metadata::{
        DOMAINS_KEYSPACE, HEADER_KEY, HEADER_KEYSPACE, HOME_REVISION_KEY, decode_home_revision,
        encode_home_revision,
    },
    ownership::{HomeOwnership, OpenedHomeDirectory},
};

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
        let directory = OpenedHomeDirectory::open(&configured_path)?;

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
            DatabaseDisposition::Fresh => {
                create_fresh_database(&configured_path, &layout, options.supported_schema)?
            }
            DatabaseDisposition::Existing => open_existing_database(&configured_path, &layout)?,
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

fn create_fresh_database(
    configured_path: &Path,
    layout: &HomeLayout,
    schema: HomeSchemaVersion,
) -> Result<OpenedDatabase, HomeOpenError> {
    let database = Database::open(Config::new(&layout.database_path)).map_err(|source| {
        HomeOpenError::open(configured_path, HomeOpenStage::CreateDatabase, source)
    })?;
    let header_keyspace = database
        .keyspace(HEADER_KEYSPACE, KeyspaceCreateOptions::default)
        .map_err(|source| {
            HomeOpenError::open(configured_path, HomeOpenStage::InitializeHeader, source)
        })?;
    let domains_keyspace = database
        .keyspace(DOMAINS_KEYSPACE, KeyspaceCreateOptions::default)
        .map_err(|source| {
            HomeOpenError::open(configured_path, HomeOpenStage::InitializeHeader, source)
        })?;

    let mut identity = [0; 16];
    getrandom::fill(&mut identity).map_err(|source| {
        HomeOpenError::open(configured_path, HomeOpenStage::GenerateHomeIdentity, source)
    })?;
    let header = HomeHeader {
        schema,
        home_id: BerylHomeId::from_bytes(identity),
    };

    let mut batch = database.batch();
    batch.insert(&header_keyspace, HEADER_KEY, header.encode());
    batch.insert(
        &header_keyspace,
        HOME_REVISION_KEY,
        encode_home_revision(beryl_model::HomeRevision::new(1).expect("one is nonzero")),
    );
    batch.commit().map_err(|source| {
        HomeOpenError::open(configured_path, HomeOpenStage::InitializeHeader, source)
    })?;
    database.persist(PersistMode::SyncAll).map_err(|source| {
        HomeOpenError::open(configured_path, HomeOpenStage::InitializeHeader, source)
    })?;

    let snapshot = database.snapshot();
    let persisted = snapshot
        .get(&header_keyspace, HEADER_KEY)
        .map_err(|source| {
            HomeOpenError::open(configured_path, HomeOpenStage::InitializeHeader, source)
        })?;
    let persisted = persisted.ok_or_else(|| {
        HomeOpenError::open(
            configured_path,
            HomeOpenStage::InitializeHeader,
            io::Error::new(
                io::ErrorKind::InvalidData,
                "fresh home header was not readable after persistence",
            ),
        )
    })?;
    let verified = HomeHeader::decode(&persisted).map_err(|source| {
        HomeOpenError::open(configured_path, HomeOpenStage::InitializeHeader, source)
    })?;
    let revision = snapshot
        .get(&header_keyspace, HOME_REVISION_KEY)
        .map_err(|source| {
            HomeOpenError::open(configured_path, HomeOpenStage::InitializeHeader, source)
        })?
        .ok_or_else(|| {
            HomeOpenError::open(
                configured_path,
                HomeOpenStage::InitializeHeader,
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    "fresh home revision was not readable after persistence",
                ),
            )
        })?;
    decode_home_revision(&revision).map_err(|source| {
        HomeOpenError::open(configured_path, HomeOpenStage::InitializeHeader, source)
    })?;
    drop(snapshot);

    Ok(OpenedDatabase {
        database,
        control: HomeControl {
            header: header_keyspace,
            domains: domains_keyspace,
        },
        header: verified,
    })
}

pub(crate) fn open_existing_database(
    configured_path: &Path,
    layout: &HomeLayout,
) -> Result<OpenedDatabase, HomeOpenError> {
    let database = Database::recover(Config::new(&layout.database_path)).map_err(|source| {
        HomeOpenError::unreadable(
            configured_path,
            HomeUnreadableStage::RecoverDatabase,
            source,
        )
    })?;
    if !database.keyspace_exists(HEADER_KEYSPACE) {
        return Err(HomeOpenError::unreadable(
            configured_path,
            HomeUnreadableStage::MissingHeaderKeyspace,
            io::Error::new(
                io::ErrorKind::InvalidData,
                "required Beryl-home header keyspace is missing",
            ),
        ));
    }
    if !database.keyspace_exists(DOMAINS_KEYSPACE) {
        return Err(HomeOpenError::unreadable(
            configured_path,
            HomeUnreadableStage::MissingDomainRegistryKeyspace,
            io::Error::new(
                io::ErrorKind::InvalidData,
                "required Beryl-home domain registry keyspace is missing",
            ),
        ));
    }

    let header_keyspace = database
        .keyspace(HEADER_KEYSPACE, KeyspaceCreateOptions::default)
        .map_err(|source| {
            HomeOpenError::unreadable(
                configured_path,
                HomeUnreadableStage::OpenHeaderKeyspace,
                source,
            )
        })?;
    let domains_keyspace = database
        .keyspace(DOMAINS_KEYSPACE, KeyspaceCreateOptions::default)
        .map_err(|source| {
            HomeOpenError::unreadable(
                configured_path,
                HomeUnreadableStage::OpenDomainRegistryKeyspace,
                source,
            )
        })?;
    let snapshot = database.snapshot();
    let encoded = snapshot
        .get(&header_keyspace, HEADER_KEY)
        .map_err(|source| {
            HomeOpenError::unreadable(
                configured_path,
                HomeUnreadableStage::OpenHeaderKeyspace,
                source,
            )
        })?;
    let encoded = encoded.ok_or_else(|| {
        HomeOpenError::unreadable(
            configured_path,
            HomeUnreadableStage::MissingHeaderRecord,
            io::Error::new(
                io::ErrorKind::InvalidData,
                "required Beryl-home header record is missing",
            ),
        )
    })?;
    let header = HomeHeader::decode(&encoded).map_err(|source| {
        HomeOpenError::unreadable(configured_path, HomeUnreadableStage::DecodeHeader, source)
    })?;
    let revision = snapshot
        .get(&header_keyspace, HOME_REVISION_KEY)
        .map_err(|source| {
            HomeOpenError::unreadable(
                configured_path,
                HomeUnreadableStage::DecodeHomeRevision,
                source,
            )
        })?
        .ok_or_else(|| {
            HomeOpenError::unreadable(
                configured_path,
                HomeUnreadableStage::MissingHomeRevision,
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    "required complete-home revision record is missing",
                ),
            )
        })?;
    decode_home_revision(&revision).map_err(|source| {
        HomeOpenError::unreadable(
            configured_path,
            HomeUnreadableStage::DecodeHomeRevision,
            source,
        )
    })?;
    drop(snapshot);

    Ok(OpenedDatabase {
        database,
        control: HomeControl {
            header: header_keyspace,
            domains: domains_keyspace,
        },
        header,
    })
}
