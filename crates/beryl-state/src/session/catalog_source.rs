use std::{error::Error, fmt};

use beryl_home_store::{
    DomainCallbackError, DomainCallbackSource, DomainReader, DomainValidator, HomeStore,
    PointReadLimit, ReadError, ValidationContribution,
};
use beryl_model::{DomainRevision, SyndicThreadId};

use super::{
    codec::{ClaimByThreadCodec, ClaimByWindowCodec},
    SessionDomain, SessionState, ThreadClaimRecord, CLAIM_V1_BYTES,
};

/// Coherent current claim facts for one thread, including proven absence.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ThreadClaimCatalogSource {
    thread_id: SyndicThreadId,
    claim: Option<ThreadClaimRecord>,
}

impl ThreadClaimCatalogSource {
    #[must_use]
    pub const fn thread_id(self) -> SyndicThreadId {
        self.thread_id
    }

    #[must_use]
    pub const fn claim(self) -> Option<ThreadClaimRecord> {
        self.claim
    }
}

impl SessionState {
    /// Reads one claim through both reverse copies or proves it is unclaimed.
    pub fn thread_claim_catalog_source(
        &self,
        store: &HomeStore,
        thread_id: SyndicThreadId,
    ) -> Result<ThreadClaimCatalogSource, ThreadClaimCatalogSourceError> {
        let claim = store.read_point::<SessionDomain, ClaimByThreadCodec>(
            self.handle,
            &thread_id,
            point_limit(),
        )?;
        let Some(claim) = claim else {
            return Ok(ThreadClaimCatalogSource {
                thread_id,
                claim: None,
            });
        };
        if claim.thread_id() != thread_id {
            return Err(ThreadClaimCatalogSourceError::ThreadCopyMismatch { thread_id });
        }
        let by_window = store.read_point::<SessionDomain, ClaimByWindowCodec>(
            self.handle,
            &claim.window_id(),
            point_limit(),
        )?;
        if by_window != Some(claim) {
            return Err(ThreadClaimCatalogSourceError::ReverseCopiesDisagree { thread_id });
        }
        Ok(ThreadClaimCatalogSource {
            thread_id,
            claim: Some(claim),
        })
    }

    /// Seals an exact claim-present or claim-absent guard for a home command.
    #[must_use]
    pub fn validate_thread_claim_catalog_source(
        &self,
        expected_revision: DomainRevision,
        source: ThreadClaimCatalogSource,
    ) -> ValidationContribution {
        self.handle.validation(expected_revision, source)
    }
}

impl DomainValidator<SessionDomain> for ThreadClaimCatalogSource {
    type Error = ThreadClaimCatalogSourceError;

    fn validate(&self, reader: &DomainReader<'_, SessionDomain>) -> Result<(), Self::Error> {
        let by_thread = reader.point::<ClaimByThreadCodec>(&self.thread_id, point_limit())?;
        if by_thread != self.claim {
            return Err(ThreadClaimCatalogSourceError::SourceChanged {
                thread_id: self.thread_id,
            });
        }
        if let Some(claim) = self.claim {
            let by_window =
                reader.point::<ClaimByWindowCodec>(&claim.window_id(), point_limit())?;
            if by_window != Some(claim) {
                return Err(ThreadClaimCatalogSourceError::ReverseCopiesDisagree {
                    thread_id: self.thread_id,
                });
            }
        }
        Ok(())
    }
}

/// Why one coherent claim source read or writer-snapshot guard failed.
#[derive(Debug)]
pub enum ThreadClaimCatalogSourceError {
    Read(ReadError),
    ThreadCopyMismatch { thread_id: SyndicThreadId },
    ReverseCopiesDisagree { thread_id: SyndicThreadId },
    SourceChanged { thread_id: SyndicThreadId },
}

impl fmt::Display for ThreadClaimCatalogSourceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Read(source) => source.fmt(formatter),
            Self::ThreadCopyMismatch { thread_id } => write!(
                formatter,
                "claim-by-thread key and record identity disagree for {thread_id}"
            ),
            Self::ReverseCopiesDisagree { thread_id } => {
                write!(formatter, "reverse claim copies disagree for {thread_id}")
            }
            Self::SourceChanged { thread_id } => {
                write!(
                    formatter,
                    "claim source changed for {thread_id} before publication"
                )
            }
        }
    }
}

impl Error for ThreadClaimCatalogSourceError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Read(source) => Some(source),
            _ => None,
        }
    }
}

impl DomainCallbackError for ThreadClaimCatalogSourceError {
    fn into_callback_source(self) -> Result<DomainCallbackSource, Self> {
        match self {
            Self::Read(source) => Ok(DomainCallbackSource::Read(source)),
            source => Err(source),
        }
    }
}

impl From<ReadError> for ThreadClaimCatalogSourceError {
    fn from(source: ReadError) -> Self {
        Self::Read(source)
    }
}

fn point_limit() -> PointReadLimit {
    PointReadLimit::new(CLAIM_V1_BYTES + 4).expect("fixed claim point limit is nonzero")
}
