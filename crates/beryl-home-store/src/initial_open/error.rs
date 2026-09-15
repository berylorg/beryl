use std::{error::Error, fmt};

use thiserror::Error;

#[derive(Debug, Error)]
pub enum HomeDomainRequirementsError {
    #[error(transparent)]
    Definition(#[from] crate::DomainDefinitionError),
    #[error("domain `{domain}` is declared more than once")]
    DuplicateDomain { domain: &'static str },
}

#[derive(Debug, Error)]
pub enum HomeCandidateError {
    #[error(transparent)]
    HealthGate(#[from] crate::HealthGateError),
    #[error("the opening home generation is unavailable")]
    GenerationUnavailable,
    #[error("the opening home generation no longer matches its candidate")]
    GenerationMismatch,
    #[error("required domain `{domain}` is not registered")]
    MissingDomain { domain: &'static str },
    #[error("domain `{domain}` is not part of the required declaration")]
    UnexpectedDomain { domain: &'static str },
    #[error("domain `{domain}` does not match its required typed declaration")]
    DomainMismatch { domain: &'static str },
    #[error("domain `{domain}` has no active runtime attachment")]
    AttachmentUnavailable { domain: &'static str },
    #[error("candidate publication is blocked by {count} unsettled reconciliation scopes")]
    PendingReconciliation { count: usize },
    #[error("opening candidate could not confirm storage health: {source}")]
    StorageHealth {
        #[source]
        source: Box<dyn Error + Send + Sync>,
    },
}

#[derive(Debug)]
pub struct HomeCandidateFailure<C> {
    error: HomeCandidateError,
    candidate: C,
}

impl<C> HomeCandidateFailure<C> {
    pub(crate) fn new(error: HomeCandidateError, candidate: C) -> Self {
        Self { error, candidate }
    }

    pub fn error(&self) -> &HomeCandidateError {
        &self.error
    }

    pub fn candidate(&self) -> &C {
        &self.candidate
    }

    pub fn into_parts(self) -> (HomeCandidateError, C) {
        (self.error, self.candidate)
    }
}

impl<C> fmt::Display for HomeCandidateFailure<C> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.error.fmt(formatter)
    }
}

impl<C: fmt::Debug> Error for HomeCandidateFailure<C> {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(&self.error)
    }
}
