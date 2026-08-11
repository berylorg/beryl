use std::{convert::Infallible, error::Error};

use thiserror::Error;

use crate::{health::FailureSeverity, ReadError, SidecarError};

/// Storage-owned failure provenance returned by a logical-domain callback.
#[derive(Debug, Error)]
pub enum DomainCallbackSource {
    /// A typed home-store read failed.
    #[error(transparent)]
    Read(#[from] ReadError),
    /// A typed sidecar verification failed.
    #[error(transparent)]
    Sidecar(#[from] SidecarError),
}

/// Explicit provenance contract for logical-domain callback errors.
///
/// Domain errors must extract direct typed home-store access failures. All
/// other values remain domain-owned semantic rejections. The home store never
/// guesses provenance by walking an arbitrary [`Error::source`] chain.
pub trait DomainCallbackError: Error + Send + Sync + 'static {
    /// Extracts a direct storage-owned source or returns the semantic error.
    fn into_callback_source(self) -> Result<DomainCallbackSource, Self>
    where
        Self: Sized;
}

impl DomainCallbackError for Infallible {
    fn into_callback_source(self) -> Result<DomainCallbackSource, Self> {
        match self {}
    }
}

pub(crate) enum ErasedCallbackError {
    Access(DomainCallbackSource),
    Rejected(Box<dyn Error + Send + Sync>),
}

impl ErasedCallbackError {
    pub(crate) fn from_typed<E: DomainCallbackError>(source: E) -> Self {
        match source.into_callback_source() {
            Ok(source) => Self::Access(source),
            Err(source) => Self::Rejected(Box::new(source)),
        }
    }
}

pub(crate) const fn callback_failure_severity(source: &DomainCallbackSource) -> FailureSeverity {
    match source {
        DomainCallbackSource::Read(ReadError::Storage { .. })
        | DomainCallbackSource::Read(ReadError::HealthGate(_))
        | DomainCallbackSource::Sidecar(SidecarError::Storage { .. })
        | DomainCallbackSource::Sidecar(SidecarError::HealthGate(_)) => FailureSeverity::Verify,
        DomainCallbackSource::Read(_)
        | DomainCallbackSource::Sidecar(SidecarError::GenerationPoisoned)
        | DomainCallbackSource::Sidecar(SidecarError::BoundExceeded { .. })
        | DomainCallbackSource::Sidecar(SidecarError::Missing)
        | DomainCallbackSource::Sidecar(SidecarError::ContentMismatch)
        | DomainCallbackSource::Sidecar(SidecarError::InvalidLayout) => FailureSeverity::Structural,
    }
}
