use std::{error::Error, fmt};

use beryl_home_store::{
    DomainCallbackError, DomainCallbackSource, DomainReader, DomainValidator, HomeStore,
    PointReadLimit, ReadError, ValidationContribution,
};
use beryl_model::{DomainRevision, RootId, RuntimeId};

use super::{
    codec::{RootIdIndexCodec, RootRecordCodec, RuntimeRecordCodec, RuntimeRootKey},
    RootRecord, RuntimeRecord, RuntimeRootDomain, RuntimeRootState, ROOT_RECORD_LIMIT,
    RUNTIME_RECORD_LIMIT,
};

/// Exact runtime and root records used to build one compact catalog row.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RuntimeRootCatalogSource {
    runtime: RuntimeRecord,
    root: RootRecord,
}

impl RuntimeRootCatalogSource {
    fn new(
        runtime: RuntimeRecord,
        root: RootRecord,
    ) -> Result<Self, RuntimeRootCatalogSourceError> {
        if root.runtime_id() != runtime.runtime_id() {
            return Err(RuntimeRootCatalogSourceError::RootRuntimeMismatch {
                root_id: root.root_id(),
                expected: runtime.runtime_id(),
                current: root.runtime_id(),
            });
        }
        Ok(Self { runtime, root })
    }

    #[must_use]
    pub const fn runtime(&self) -> &RuntimeRecord {
        &self.runtime
    }

    #[must_use]
    pub const fn root(&self) -> &RootRecord {
        &self.root
    }
}

impl RuntimeRootState {
    /// Reads the exact configured runtime/root pair used by a catalog join.
    pub fn catalog_source(
        &self,
        store: &HomeStore,
        runtime_id: RuntimeId,
        root_id: RootId,
    ) -> Result<RuntimeRootCatalogSource, RuntimeRootCatalogSourceError> {
        let runtime = self
            .runtime(store, runtime_id)?
            .ok_or(RuntimeRootCatalogSourceError::RuntimeMissing { runtime_id })?;
        let root = self
            .root(store, root_id)?
            .ok_or(RuntimeRootCatalogSourceError::RootMissing { root_id })?;
        RuntimeRootCatalogSource::new(runtime, root)
    }

    /// Seals an exact runtime/root source guard for a heterogeneous home command.
    #[must_use]
    pub fn validate_catalog_source(
        &self,
        expected_revision: DomainRevision,
        source: RuntimeRootCatalogSource,
    ) -> ValidationContribution {
        self.handle.validation(expected_revision, source)
    }
}

impl DomainValidator<RuntimeRootDomain> for RuntimeRootCatalogSource {
    type Error = RuntimeRootCatalogSourceError;

    fn validate(&self, reader: &DomainReader<'_, RuntimeRootDomain>) -> Result<(), Self::Error> {
        let runtime = reader.point::<RuntimeRecordCodec>(
            &self.runtime.runtime_id(),
            point_limit(RUNTIME_RECORD_LIMIT),
        )?;
        if runtime.as_ref() != Some(&self.runtime) {
            return Err(RuntimeRootCatalogSourceError::SourceChanged("runtime"));
        }

        let indexed_runtime =
            reader.point::<RootIdIndexCodec>(&self.root.root_id(), point_limit(32))?;
        if indexed_runtime != Some(self.runtime.runtime_id()) {
            return Err(RuntimeRootCatalogSourceError::SourceChanged(
                "root identity index",
            ));
        }

        let root = reader.point::<RootRecordCodec>(
            &RuntimeRootKey::new(self.runtime.runtime_id(), self.root.root_id()),
            point_limit(ROOT_RECORD_LIMIT),
        )?;
        if root.as_ref() != Some(&self.root) {
            return Err(RuntimeRootCatalogSourceError::SourceChanged("root"));
        }
        Ok(())
    }
}

/// Why catalog source preparation or exact writer-snapshot validation failed.
#[derive(Debug)]
pub enum RuntimeRootCatalogSourceError {
    Read(ReadError),
    RuntimeMissing {
        runtime_id: RuntimeId,
    },
    RootMissing {
        root_id: RootId,
    },
    RootRuntimeMismatch {
        root_id: RootId,
        expected: RuntimeId,
        current: RuntimeId,
    },
    SourceChanged(&'static str),
}

impl fmt::Display for RuntimeRootCatalogSourceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Read(source) => source.fmt(formatter),
            Self::RuntimeMissing { runtime_id } => {
                write!(formatter, "catalog runtime {runtime_id} is not registered")
            }
            Self::RootMissing { root_id } => {
                write!(formatter, "catalog root {root_id} is not registered")
            }
            Self::RootRuntimeMismatch {
                root_id,
                expected,
                current,
            } => write!(
                formatter,
                "catalog root {root_id} belongs to runtime {current}, not {expected}"
            ),
            Self::SourceChanged(kind) => {
                write!(
                    formatter,
                    "catalog {kind} source changed before publication"
                )
            }
        }
    }
}

impl Error for RuntimeRootCatalogSourceError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Read(source) => Some(source),
            _ => None,
        }
    }
}

impl DomainCallbackError for RuntimeRootCatalogSourceError {
    fn into_callback_source(self) -> Result<DomainCallbackSource, Self> {
        match self {
            Self::Read(source) => Ok(DomainCallbackSource::Read(source)),
            source => Err(source),
        }
    }
}

impl From<ReadError> for RuntimeRootCatalogSourceError {
    fn from(source: ReadError) -> Self {
        Self::Read(source)
    }
}

fn point_limit(maximum_payload: usize) -> PointReadLimit {
    PointReadLimit::new(maximum_payload + 4).expect("schema point limit is nonzero")
}
