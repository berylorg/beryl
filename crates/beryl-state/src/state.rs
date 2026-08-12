use std::{error::Error, fmt};

use beryl_home_store::{DomainHandleError, DomainRegistrationError, HomeStore};
use beryl_model::BerylHomeId;

use crate::{
    AssetState, CatalogState, DurableJobState, RuntimeRootState, SessionState, SettingsState,
    ThemeService, ThemeServiceError,
};

/// One explicitly bounded page of typed state records.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StatePage<T> {
    pub(crate) records: Vec<T>,
    pub(crate) stored_bytes: usize,
    pub(crate) decoded_bytes: usize,
    pub(crate) has_more: bool,
}

impl<T> StatePage<T> {
    /// Returns the decoded records in cursor order.
    #[must_use]
    pub fn records(&self) -> &[T] {
        &self.records
    }

    /// Returns the cumulative encoded key and stored-value bytes for this page.
    #[must_use]
    pub const fn stored_bytes(&self) -> usize {
        self.stored_bytes
    }

    /// Returns the cumulative practical decoded-byte estimate for this page.
    #[must_use]
    pub const fn decoded_bytes(&self) -> usize {
        self.decoded_bytes
    }

    /// Returns whether another matching record exists beyond this page.
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
    /// Routinely registers only the bounded session/window/claim domain.
    ///
    /// This admits the exact persistent declaration, codec family, and current
    /// generation without scanning application records. Composition roots that
    /// deliberately need an exhaustive schema boundary must instead use
    /// [`BerylState::register_with_schema_validation`].
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

    /// Routinely registers the remaining Beryl domains after ordinary shells may open.
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
#[derive(Clone)]
pub struct BerylState {
    session: SessionState,
    runtime_roots: RuntimeRootState,
    settings: SettingsState,
    durable_jobs: DurableJobState,
    catalog: CatalogState,
    assets: AssetState,
    themes: ThemeService,
}

impl BerylState {
    /// Routinely registers every exact Beryl-owned state domain.
    ///
    /// Registration verifies durable declarations, exact owner and codec types,
    /// required families, and the current generation, but does not scan persisted
    /// application records. Use [`Self::register_with_schema_validation`] only
    /// when the composition root deliberately requests exhaustive validation.
    pub fn register(store: &mut HomeStore) -> Result<Self, BerylStateRegistrationError> {
        BerylStateBootstrap::register(store)?.complete(store)
    }

    /// Registers the complete Beryl-state handle set at an explicit exhaustive
    /// schema-validation boundary.
    ///
    /// This is intentionally separate from routine bootstrap and recovery
    /// reacquisition. Every Beryl domain keeps its exact exhaustive validator,
    /// which the home store invokes here before this method returns its handle
    /// set. A composition root should call this only for a deliberate scrub or
    /// schema-validation request.
    pub fn register_with_schema_validation(
        store: &mut HomeStore,
    ) -> Result<Self, BerylStateRegistrationError> {
        let session = SessionState::register_with_schema_validation(store).map_err(|source| {
            BerylStateRegistrationError::Domain {
                domain: "beryl-session",
                source,
            }
        })?;
        let runtime_roots =
            RuntimeRootState::register_with_schema_validation(store).map_err(|source| {
                BerylStateRegistrationError::Domain {
                    domain: "beryl-runtime-root",
                    source,
                }
            })?;
        let settings = SettingsState::register_with_schema_validation(store).map_err(|source| {
            BerylStateRegistrationError::Domain {
                domain: "beryl-settings",
                source,
            }
        })?;
        let durable_jobs =
            DurableJobState::register_with_schema_validation(store).map_err(|source| {
                BerylStateRegistrationError::Domain {
                    domain: "beryl-durable-job",
                    source,
                }
            })?;
        let catalog = CatalogState::register_with_schema_validation(store).map_err(|source| {
            BerylStateRegistrationError::Domain {
                domain: "beryl-catalog",
                source,
            }
        })?;
        let assets = AssetState::register_with_schema_validation(store).map_err(|source| {
            BerylStateRegistrationError::Domain {
                domain: "beryl-assets",
                source,
            }
        })?;
        let themes = ThemeService::acquire(store)
            .map_err(|source| BerylStateRegistrationError::Theme { source })?;
        Ok(Self {
            session,
            runtime_roots,
            settings,
            durable_jobs,
            catalog,
            assets,
            themes,
        })
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
        let themes = ThemeService::acquire(store)
            .map_err(|source| BerylStateRegistrationError::Theme { source })?;
        Ok(Self {
            session,
            runtime_roots,
            settings,
            durable_jobs,
            catalog,
            assets,
            themes,
        })
    }

    /// Routinely reacquires current-generation handles after successful same-home recovery.
    ///
    /// This checks exact registered domain identity and generation only; it never
    /// performs exhaustive application-record validation.
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
        let themes = ThemeService::acquire(store)
            .map_err(|source| BerylStateReacquireError::Theme { source })?;
        Ok(Self {
            session,
            runtime_roots,
            settings,
            durable_jobs,
            catalog,
            assets,
            themes,
        })
    }

    /// Reacquires the complete Beryl-state handle set from an unpublished
    /// same-home recovery candidate.
    ///
    /// The returned handles are bound to the candidate's fresh generation and
    /// remain unusable until its composition owner publishes that candidate. This
    /// routine checks only registered owner/type/declaration/generation facts and
    /// never invokes exhaustive domain validation.
    pub fn reacquire_candidate(
        candidate: &beryl_home_store::HomeRecoveryCandidate,
    ) -> Result<Self, BerylStateReacquireError> {
        let session = SessionState::reacquire_candidate(candidate).map_err(|source| {
            BerylStateReacquireError::Domain {
                domain: "beryl-session",
                source,
            }
        })?;
        let runtime_roots = RuntimeRootState::reacquire_candidate(candidate).map_err(|source| {
            BerylStateReacquireError::Domain {
                domain: "beryl-runtime-root",
                source,
            }
        })?;
        let settings = SettingsState::reacquire_candidate(candidate).map_err(|source| {
            BerylStateReacquireError::Domain {
                domain: "beryl-settings",
                source,
            }
        })?;
        let durable_jobs = DurableJobState::reacquire_candidate(candidate).map_err(|source| {
            BerylStateReacquireError::Domain {
                domain: "beryl-durable-job",
                source,
            }
        })?;
        let catalog = CatalogState::reacquire_candidate(candidate).map_err(|source| {
            BerylStateReacquireError::Domain {
                domain: "beryl-catalog",
                source,
            }
        })?;
        let assets = AssetState::reacquire_candidate(candidate).map_err(|source| {
            BerylStateReacquireError::Domain {
                domain: "beryl-assets",
                source,
            }
        })?;
        let themes = ThemeService::reacquire_candidate(candidate)
            .map_err(|source| BerylStateReacquireError::Theme { source })?;
        Ok(Self {
            session,
            runtime_roots,
            settings,
            durable_jobs,
            catalog,
            assets,
            themes,
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

    /// Returns the fresh typed theme service bound to this state generation.
    #[must_use]
    pub fn themes(&self) -> ThemeService {
        self.themes.clone()
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
    Theme {
        source: ThemeServiceError,
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
            Self::Theme { source } => {
                write!(formatter, "could not acquire Beryl theme service: {source}")
            }
        }
    }
}

impl Error for BerylStateRegistrationError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Domain { source, .. } => Some(source),
            Self::BootstrapHandle { source } => Some(source),
            Self::BootstrapHomeMismatch { .. } => None,
            Self::Theme { source } => Some(source),
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
    Theme {
        source: ThemeServiceError,
    },
}

impl fmt::Display for BerylStateReacquireError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Domain { domain, source } => write!(
                formatter,
                "could not reacquire Beryl-state domain `{domain}`: {source}"
            ),
            Self::Theme { source } => {
                write!(
                    formatter,
                    "could not reacquire Beryl theme service: {source}"
                )
            }
        }
    }
}

impl Error for BerylStateReacquireError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Domain { source, .. } => Some(source),
            Self::Theme { source } => Some(source),
        }
    }
}
