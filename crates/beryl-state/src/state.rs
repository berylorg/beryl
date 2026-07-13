use std::{error::Error, fmt};

use beryl_home_store::{DomainHandleError, DomainRegistrationError, HomeStore};
use beryl_model::BerylHomeId;

use crate::{
    AssetState, CatalogState, DurableJobState, RuntimeRootState, SessionState, SettingsState,
    ThreadMetadataState,
};

/// One explicitly bounded page of typed state records.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StatePage<T> {
    pub(crate) records: Vec<T>,
    pub(crate) stored_bytes: usize,
    pub(crate) has_more: bool,
}

impl<T> StatePage<T> {
    #[must_use]
    pub fn records(&self) -> &[T] {
        &self.records
    }

    #[must_use]
    pub const fn stored_bytes(&self) -> usize {
        self.stored_bytes
    }

    #[must_use]
    pub const fn has_more(&self) -> bool {
        self.has_more
    }
}

/// Session-only registration token used by the minimal pre-window startup path.
pub struct BerylStateBootstrap {
    home_id: BerylHomeId,
    session: SessionState,
}

impl BerylStateBootstrap {
    /// Registers and validates only the bounded session/window/claim domain.
    pub fn register(store: &mut HomeStore) -> Result<Self, BerylStateRegistrationError> {
        let session = SessionState::register(store).map_err(|source| {
            BerylStateRegistrationError::Domain {
                domain: "beryl-session",
                source,
            }
        })?;
        Ok(Self {
            home_id: store.home_id(),
            session,
        })
    }

    /// Returns the session boundary used for the minimal bootstrap read.
    #[must_use]
    pub const fn session(&self) -> SessionState {
        self.session
    }

    /// Registers the remaining Beryl domains after ordinary shells may open.
    pub fn complete(
        self,
        store: &mut HomeStore,
    ) -> Result<BerylState, BerylStateRegistrationError> {
        let current = store.home_id();
        if current != self.home_id {
            return Err(BerylStateRegistrationError::BootstrapHomeMismatch {
                expected: self.home_id,
                current,
            });
        }
        let session = SessionState::reacquire(store)
            .map_err(|source| BerylStateRegistrationError::BootstrapHandle { source })?;
        BerylState::register_after_session(store, session)
    }
}

/// Complete set of registered Beryl-owned typed state domains.
#[derive(Clone, Copy)]
pub struct BerylState {
    session: SessionState,
    runtime_roots: RuntimeRootState,
    thread_metadata: ThreadMetadataState,
    settings: SettingsState,
    durable_jobs: DurableJobState,
    catalog: CatalogState,
    assets: AssetState,
}

impl BerylState {
    /// Registers or validates every exact Beryl-owned state schema.
    pub fn register(store: &mut HomeStore) -> Result<Self, BerylStateRegistrationError> {
        BerylStateBootstrap::register(store)?.complete(store)
    }

    fn register_after_session(
        store: &mut HomeStore,
        session: SessionState,
    ) -> Result<Self, BerylStateRegistrationError> {
        let runtime_roots = RuntimeRootState::register(store).map_err(|source| {
            BerylStateRegistrationError::Domain {
                domain: "beryl-runtime-root",
                source,
            }
        })?;
        let thread_metadata = ThreadMetadataState::register(store).map_err(|source| {
            BerylStateRegistrationError::Domain {
                domain: "beryl-thread-metadata",
                source,
            }
        })?;
        let settings = SettingsState::register(store).map_err(|source| {
            BerylStateRegistrationError::Domain {
                domain: "beryl-settings",
                source,
            }
        })?;
        let durable_jobs = DurableJobState::register(store).map_err(|source| {
            BerylStateRegistrationError::Domain {
                domain: "beryl-durable-job",
                source,
            }
        })?;
        let catalog = CatalogState::register(store).map_err(|source| {
            BerylStateRegistrationError::Domain {
                domain: "beryl-catalog",
                source,
            }
        })?;
        let assets =
            AssetState::register(store).map_err(|source| BerylStateRegistrationError::Domain {
                domain: "beryl-assets",
                source,
            })?;
        Ok(Self {
            session,
            runtime_roots,
            thread_metadata,
            settings,
            durable_jobs,
            catalog,
            assets,
        })
    }

    /// Reacquires current-generation handles after successful same-home recovery.
    pub fn reacquire(store: &HomeStore) -> Result<Self, BerylStateReacquireError> {
        let session =
            SessionState::reacquire(store).map_err(|source| BerylStateReacquireError::Domain {
                domain: "beryl-session",
                source,
            })?;
        let runtime_roots = RuntimeRootState::reacquire(store).map_err(|source| {
            BerylStateReacquireError::Domain {
                domain: "beryl-runtime-root",
                source,
            }
        })?;
        let thread_metadata = ThreadMetadataState::reacquire(store).map_err(|source| {
            BerylStateReacquireError::Domain {
                domain: "beryl-thread-metadata",
                source,
            }
        })?;
        let settings =
            SettingsState::reacquire(store).map_err(|source| BerylStateReacquireError::Domain {
                domain: "beryl-settings",
                source,
            })?;
        let durable_jobs = DurableJobState::reacquire(store).map_err(|source| {
            BerylStateReacquireError::Domain {
                domain: "beryl-durable-job",
                source,
            }
        })?;
        let catalog =
            CatalogState::reacquire(store).map_err(|source| BerylStateReacquireError::Domain {
                domain: "beryl-catalog",
                source,
            })?;
        let assets =
            AssetState::reacquire(store).map_err(|source| BerylStateReacquireError::Domain {
                domain: "beryl-assets",
                source,
            })?;
        Ok(Self {
            session,
            runtime_roots,
            thread_metadata,
            settings,
            durable_jobs,
            catalog,
            assets,
        })
    }

    #[must_use]
    pub const fn session(&self) -> SessionState {
        self.session
    }

    #[must_use]
    pub const fn runtime_roots(&self) -> RuntimeRootState {
        self.runtime_roots
    }

    #[must_use]
    pub const fn thread_metadata(&self) -> ThreadMetadataState {
        self.thread_metadata
    }

    #[must_use]
    pub const fn settings(&self) -> SettingsState {
        self.settings
    }

    #[must_use]
    pub const fn durable_jobs(&self) -> DurableJobState {
        self.durable_jobs
    }

    #[must_use]
    pub const fn catalog(&self) -> CatalogState {
        self.catalog
    }

    #[must_use]
    pub const fn assets(&self) -> AssetState {
        self.assets
    }
}

/// Failure while registering or validating one exact Beryl-state domain.
#[derive(Debug)]
pub enum BerylStateRegistrationError {
    Domain {
        domain: &'static str,
        source: DomainRegistrationError,
    },
    BootstrapHandle {
        source: DomainHandleError,
    },
    BootstrapHomeMismatch {
        expected: BerylHomeId,
        current: BerylHomeId,
    },
}

impl fmt::Display for BerylStateRegistrationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Domain { domain, source } => {
                write!(
                    formatter,
                    "could not register Beryl-state domain `{domain}`: {source}"
                )
            }
            Self::BootstrapHandle { source } => write!(
                formatter,
                "could not reacquire the current session bootstrap handle: {source}"
            ),
            Self::BootstrapHomeMismatch { expected, current } => write!(
                formatter,
                "session bootstrap belongs to Beryl home {expected}, not {current}"
            ),
        }
    }
}

impl Error for BerylStateRegistrationError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Domain { source, .. } => Some(source),
            Self::BootstrapHandle { source } => Some(source),
            Self::BootstrapHomeMismatch { .. } => None,
        }
    }
}

/// Failure while reacquiring typed handles for a recovered home generation.
#[derive(Debug)]
pub enum BerylStateReacquireError {
    Domain {
        domain: &'static str,
        source: DomainHandleError,
    },
}

impl fmt::Display for BerylStateReacquireError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Domain { domain, source } => write!(
                formatter,
                "could not reacquire Beryl-state domain `{domain}`: {source}"
            ),
        }
    }
}

impl Error for BerylStateReacquireError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Domain { source, .. } => Some(source),
        }
    }
}
