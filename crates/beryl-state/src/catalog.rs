use std::{error::Error, fmt};

use beryl_home_store::{
    CursorDirection, CursorRange, CursorReadLimits, DomainHandle, DomainRegistrationError,
    DomainSchemaVersion, HomeStore, KeyspaceFamily, KeyspaceSchemaVersion, MutationBuildError,
    MutationContribution, ReadError, ReadLimitError, StorageDomain,
};
use beryl_model::{DomainRevision, SyndicThreadId};

#[path = "catalog/codec.rs"]
mod codec;
#[path = "catalog/error.rs"]
mod error;
#[path = "catalog/mutation.rs"]
mod mutation;
#[path = "catalog/row.rs"]
mod row;
#[cfg(test)]
#[path = "catalog/test_support.rs"]
#[allow(dead_code)]
mod test_support;
#[path = "catalog/validate.rs"]
mod validate;
#[path = "catalog/value.rs"]
mod value;

use codec::{CatalogRecencyCodec, CatalogRowCodec};
pub use error::CatalogValueError;
pub use mutation::{MarkCatalogRowStale, PublishCatalogRow};
pub use row::{CatalogFacts, CatalogRecencyCursor, CatalogRow};
pub use value::{
    CatalogArchiveSummary, CatalogAvailabilitySummary, CatalogClaimKind, CatalogClaimSummary,
    CatalogExecutionSummary, CatalogFreshness, CatalogLineageSummary, CatalogRevision,
    CatalogRowExpectation, CatalogSearchFields, CatalogSourceRevisions, CatalogTitleCandidate,
    CatalogTitleFacts, CatalogTitleSource,
};

const CATALOG_RECORD_LIMIT: usize = 256 * 1024;

/// Maximum stored bytes for one row-family point record, including key and version envelope.
pub const CATALOG_MAX_STORED_ROW_BYTES: usize = 16 + 4 + CATALOG_RECORD_LIMIT;

/// Maximum stored bytes for one recency-index record, including key and version envelope.
pub const CATALOG_MAX_STORED_RECENCY_BYTES: usize = 24 + 4 + CATALOG_RECORD_LIMIT;

const CATALOG_FAMILIES: &[KeyspaceFamily] = &[
    KeyspaceFamily::new("rows", KeyspaceSchemaVersion::new(1)),
    KeyspaceFamily::new("recency", KeyspaceSchemaVersion::new(1)),
];

pub(crate) struct CatalogDomain;

impl StorageDomain for CatalogDomain {
    const NAME: &'static str = "beryl-catalog";
    const SCHEMA_VERSION: DomainSchemaVersion = DomainSchemaVersion::new(1);
    const KEYSPACES: &'static [KeyspaceFamily] = CATALOG_FAMILIES;
    type ValidationError = CatalogValidationError;

    fn validate(
        reader: &beryl_home_store::DomainReader<'_, Self>,
    ) -> Result<(), Self::ValidationError> {
        validate::validate(reader)
    }
}

/// Nonzero total stored-byte bound for one catalog point read.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CatalogPointReadLimit {
    max_stored_bytes: usize,
}

impl CatalogPointReadLimit {
    pub fn new(max_stored_bytes: usize) -> Result<Self, ReadLimitError> {
        CursorReadLimits::new(1, max_stored_bytes)?;
        Ok(Self { max_stored_bytes })
    }

    #[must_use]
    pub const fn schema_maximum() -> Self {
        Self {
            max_stored_bytes: CATALOG_MAX_STORED_ROW_BYTES,
        }
    }

    #[must_use]
    pub const fn max_stored_bytes(self) -> usize {
        self.max_stored_bytes
    }
}

/// One point-read row together with its exact stored key-and-value byte cost.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CatalogStoredRow {
    row: CatalogRow,
    stored_bytes: usize,
}

impl CatalogStoredRow {
    #[must_use]
    pub const fn row(&self) -> &CatalogRow {
        &self.row
    }

    #[must_use]
    pub const fn stored_bytes(&self) -> usize {
        self.stored_bytes
    }
}

/// One bounded recent-first page with exact stored key-and-value byte accounting.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CatalogPage {
    rows: Vec<CatalogRow>,
    stored_bytes: usize,
    has_more: bool,
}

impl CatalogPage {
    #[must_use]
    pub fn rows(&self) -> &[CatalogRow] {
        &self.rows
    }

    #[must_use]
    pub const fn stored_bytes(&self) -> usize {
        self.stored_bytes
    }

    #[must_use]
    pub const fn has_more(&self) -> bool {
        self.has_more
    }

    #[must_use]
    pub fn next_after(&self) -> Option<CatalogRecencyCursor> {
        self.rows.last().map(CatalogRow::recency_cursor)
    }
}

/// Opaque typed access to the compact thread-catalog domain.
#[derive(Clone, Copy)]
pub struct CatalogState {
    handle: DomainHandle<CatalogDomain>,
}

impl CatalogState {
    pub(crate) fn register(store: &mut HomeStore) -> Result<Self, DomainRegistrationError> {
        store
            .register_domain::<CatalogDomain>()
            .map(|handle| Self { handle })
    }

    pub(crate) fn reacquire(
        store: &HomeStore,
    ) -> Result<Self, beryl_home_store::DomainHandleError> {
        store
            .domain_handle::<CatalogDomain>()
            .map(|handle| Self { handle })
    }

    pub fn revision(&self, store: &HomeStore) -> Result<DomainRevision, ReadError> {
        store.domain_revision(self.handle)
    }

    pub fn row(
        &self,
        store: &HomeStore,
        thread_id: SyndicThreadId,
        limit: CatalogPointReadLimit,
    ) -> Result<Option<CatalogStoredRow>, CatalogReadError> {
        let page = store.read_cursor::<CatalogDomain, CatalogRowCodec>(
            self.handle,
            &CursorRange::closed(thread_id, thread_id),
            CursorDirection::Forward,
            CursorReadLimits::new(1, limit.max_stored_bytes())
                .expect("catalog point limit is nonzero"),
        )?;
        if page.has_more() || page.records().len() > 1 {
            return Err(CatalogReadError::Invariant(
                "catalog point range returned more than one row",
            ));
        }
        let stored_bytes = page.stored_bytes();
        let Some(record) = page.into_records().into_iter().next() else {
            return Ok(None);
        };
        let (key, row) = record.into_parts();
        if key != row.thread_id() {
            return Err(CatalogReadError::Invariant(
                "catalog point key does not match its row identity",
            ));
        }
        Ok(Some(CatalogStoredRow { row, stored_bytes }))
    }

    pub fn recency_page(
        &self,
        store: &HomeStore,
        after: Option<CatalogRecencyCursor>,
        limits: CursorReadLimits,
    ) -> Result<CatalogPage, CatalogReadError> {
        let range = match after {
            Some(after) => CursorRange::after(after, CatalogRecencyCursor::last()),
            None => {
                CursorRange::closed(CatalogRecencyCursor::first(), CatalogRecencyCursor::last())
            }
        };
        let page = store.read_cursor::<CatalogDomain, CatalogRecencyCodec>(
            self.handle,
            &range,
            CursorDirection::Forward,
            limits,
        )?;
        for record in page.records() {
            if record.key() != &record.value().recency_cursor() {
                return Err(CatalogReadError::Invariant(
                    "catalog recency key does not match its row copy",
                ));
            }
        }
        let stored_bytes = page.stored_bytes();
        let has_more = page.has_more();
        Ok(CatalogPage {
            rows: page
                .into_records()
                .into_iter()
                .map(|record| record.into_parts().1)
                .collect(),
            stored_bytes,
            has_more,
        })
    }

    #[must_use]
    pub fn publish(
        &self,
        expected_revision: DomainRevision,
        command: PublishCatalogRow,
    ) -> MutationContribution {
        self.handle.contribution(expected_revision, command)
    }

    #[must_use]
    pub fn mark_stale(
        &self,
        expected_revision: DomainRevision,
        command: MarkCatalogRowStale,
    ) -> MutationContribution {
        self.handle.contribution(expected_revision, command)
    }

    #[cfg(test)]
    #[allow(dead_code)]
    pub(crate) fn corrupt_recency_copy_for_test(
        &self,
        expected_revision: DomainRevision,
        key: CatalogRecencyCursor,
        row: CatalogRow,
    ) -> MutationContribution {
        self.handle.contribution(
            expected_revision,
            test_support::CorruptRecencyCopy { key, row },
        )
    }
}

/// Why a bounded catalog read could not return a coherent typed row copy.
#[derive(Debug)]
pub enum CatalogReadError {
    Read(ReadError),
    Invariant(&'static str),
}

impl fmt::Display for CatalogReadError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Read(source) => source.fmt(formatter),
            Self::Invariant(message) => formatter.write_str(message),
        }
    }
}

impl Error for CatalogReadError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Read(source) => Some(source),
            Self::Invariant(_) => None,
        }
    }
}

impl From<ReadError> for CatalogReadError {
    fn from(source: ReadError) -> Self {
        Self::Read(source)
    }
}

/// Why publication or staleness admission rejected a compact catalog row.
#[derive(Debug)]
pub enum CatalogMutationError {
    Read(ReadError),
    Build(MutationBuildError),
    Value(CatalogValueError),
    RowExists {
        thread_id: SyndicThreadId,
    },
    RowMissing {
        thread_id: SyndicThreadId,
    },
    RevisionConflict {
        expected: CatalogRevision,
        current: CatalogRevision,
    },
    SourceRevisionRegressed {
        kind: &'static str,
    },
    AlreadyStale {
        thread_id: SyndicThreadId,
    },
    IndexMissing {
        thread_id: SyndicThreadId,
    },
    IndexMismatch {
        thread_id: SyndicThreadId,
    },
}

impl fmt::Display for CatalogMutationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Read(source) => source.fmt(formatter),
            Self::Build(source) => source.fmt(formatter),
            Self::Value(source) => source.fmt(formatter),
            Self::RowExists { thread_id } => {
                write!(formatter, "catalog row exists for {thread_id}")
            }
            Self::RowMissing { thread_id } => {
                write!(formatter, "catalog row is missing for {thread_id}")
            }
            Self::RevisionConflict { expected, current } => write!(
                formatter,
                "catalog row revision conflict: expected {}, current {}",
                expected.get(),
                current.get()
            ),
            Self::SourceRevisionRegressed { kind } => {
                write!(formatter, "catalog {kind} source revision cannot regress")
            }
            Self::AlreadyStale { thread_id } => {
                write!(formatter, "catalog row for {thread_id} is already stale")
            }
            Self::IndexMissing { thread_id } => {
                write!(
                    formatter,
                    "catalog recency index is missing for {thread_id}"
                )
            }
            Self::IndexMismatch { thread_id } => {
                write!(formatter, "catalog recency copy disagrees for {thread_id}")
            }
        }
    }
}

impl Error for CatalogMutationError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Read(source) => Some(source),
            Self::Build(source) => Some(source),
            Self::Value(source) => Some(source),
            _ => None,
        }
    }
}

impl From<ReadError> for CatalogMutationError {
    fn from(source: ReadError) -> Self {
        Self::Read(source)
    }
}

impl From<MutationBuildError> for CatalogMutationError {
    fn from(source: MutationBuildError) -> Self {
        Self::Build(source)
    }
}

impl From<CatalogValueError> for CatalogMutationError {
    fn from(source: CatalogValueError) -> Self {
        Self::Value(source)
    }
}

#[derive(Debug)]
pub(crate) enum CatalogValidationError {
    Read(ReadError),
    Invariant(&'static str),
}

impl fmt::Display for CatalogValidationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Read(source) => source.fmt(formatter),
            Self::Invariant(message) => formatter.write_str(message),
        }
    }
}

impl Error for CatalogValidationError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Read(source) => Some(source),
            Self::Invariant(_) => None,
        }
    }
}

impl From<ReadError> for CatalogValidationError {
    fn from(source: ReadError) -> Self {
        Self::Read(source)
    }
}
