use std::{error::Error, fmt};

use super::*;

#[derive(Debug)]
pub enum CatalogQueryError {
    Foreign,
    Released,
    GenerationChanged,
    Retired,
    Cancelled,
    CollectionLimit,
    IdentityExhausted,
    RequestExhausted,
    CountExhausted,
    Limit,
    StaleRow { thread_id: SyndicThreadId },
    Structural(&'static str),
    Catalog(CatalogReadError),
    RuntimeRoot(RuntimeRootCatalogSourceError),
    Read(ReadError),
}

impl fmt::Display for CatalogQueryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Catalog(error) => error.fmt(f),
            Self::RuntimeRoot(error) => error.fmt(f),
            Self::Read(error) => error.fmt(f),
            Self::Structural(reason) => f.write_str(reason),
            Self::StaleRow { thread_id } => {
                write!(f, "frozen query contains stale thread {thread_id}")
            }
            other => write!(f, "frozen catalog query refused: {other:?}"),
        }
    }
}

impl Error for CatalogQueryError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Catalog(error) => Some(error),
            Self::RuntimeRoot(error) => Some(error),
            Self::Read(error) => Some(error),
            _ => None,
        }
    }
}

impl From<ReadError> for CatalogQueryError {
    fn from(error: ReadError) -> Self {
        Self::Read(error)
    }
}
impl From<CatalogReadError> for CatalogQueryError {
    fn from(error: CatalogReadError) -> Self {
        Self::Catalog(error)
    }
}
impl From<RuntimeRootCatalogSourceError> for CatalogQueryError {
    fn from(error: RuntimeRootCatalogSourceError) -> Self {
        Self::RuntimeRoot(error)
    }
}

#[derive(Debug)]
pub struct CatalogQueryOpenError {
    pub error: CatalogQueryError,
    retained: Option<FrozenHomeRead>,
}

impl CatalogQueryOpenError {
    pub fn into_parts(self) -> (CatalogQueryError, Option<FrozenHomeRead>) {
        (self.error, self.retained)
    }
    pub fn retained(&self) -> Option<&FrozenHomeRead> {
        self.retained.as_ref()
    }
    pub(super) fn new(error: CatalogQueryError, retained: Option<FrozenHomeRead>) -> Self {
        Self { error, retained }
    }
}

impl fmt::Display for CatalogQueryOpenError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.error.fmt(f)
    }
}
impl Error for CatalogQueryOpenError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(&self.error)
    }
}
