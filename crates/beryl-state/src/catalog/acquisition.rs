use std::{error::Error, fmt};

use beryl_home_store::{
    CursorReadLimits, DomainCallbackError, DomainCallbackSource, DomainReader, DomainValidator,
    HomeStore, PointReadLimit, ReadError,
};
use beryl_model::{ClaimRevision, DomainRevision, SyndicThreadId, WindowId};

use super::{
    CATALOG_POINT_READ_MAX_BYTES, CatalogClaimKind, CatalogClaimSummary, CatalogDomain,
    CatalogFreshness, CatalogPointReadLimit, CatalogReadError, CatalogRecencyCodec,
    CatalogRecencyCursor, CatalogRow, CatalogRowCodec, CatalogState,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CatalogCurrentScan {
    revision: DomainRevision,
}

impl CatalogCurrentScan {
    #[must_use]
    pub const fn revision(self) -> DomainRevision {
        self.revision
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CatalogCurrentRow {
    row: CatalogRow,
}

impl CatalogCurrentRow {
    #[must_use]
    pub const fn row(&self) -> &CatalogRow {
        &self.row
    }

    pub(super) fn into_row(self) -> CatalogRow {
        self.row
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CatalogCurrentPage {
    rows: Vec<CatalogCurrentRow>,
    stored_bytes: usize,
    decoded_bytes: usize,
    has_more: bool,
    next_after: Option<CatalogRecencyCursor>,
}

impl CatalogCurrentPage {
    #[must_use]
    pub fn rows(&self) -> &[CatalogCurrentRow] {
        &self.rows
    }

    #[must_use]
    pub const fn stored_bytes(&self) -> usize {
        self.stored_bytes
    }

    #[must_use]
    pub const fn decoded_bytes(&self) -> usize {
        self.decoded_bytes
    }

    #[must_use]
    pub const fn has_more(&self) -> bool {
        self.has_more
    }

    #[must_use]
    pub const fn next_after(&self) -> Option<CatalogRecencyCursor> {
        self.next_after
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CatalogWindowClaim {
    thread_id: SyndicThreadId,
    window_id: WindowId,
    revision: ClaimRevision,
}

impl CatalogWindowClaim {
    pub(crate) const fn active(
        thread_id: SyndicThreadId,
        window_id: WindowId,
        revision: ClaimRevision,
    ) -> Self {
        Self {
            thread_id,
            window_id,
            revision,
        }
    }

    #[must_use]
    pub const fn thread_id(self) -> SyndicThreadId {
        self.thread_id
    }

    #[must_use]
    pub const fn window_id(self) -> WindowId {
        self.window_id
    }

    #[must_use]
    pub const fn revision(self) -> ClaimRevision {
        self.revision
    }

    pub(super) const fn summary(self) -> CatalogClaimSummary {
        CatalogClaimSummary::claimed(self.window_id, CatalogClaimKind::Active)
    }
}

pub(super) fn begin_current_scan(
    state: &CatalogState,
    store: &HomeStore,
) -> Result<CatalogCurrentScan, CatalogReadError> {
    Ok(CatalogCurrentScan {
        revision: state.revision(store)?,
    })
}

pub(super) fn current_page(
    state: &CatalogState,
    store: &HomeStore,
    scan: CatalogCurrentScan,
    after: Option<CatalogRecencyCursor>,
    limits: CursorReadLimits,
) -> Result<CatalogCurrentPage, CatalogReadError> {
    ensure_revision(state, store, scan.revision)?;
    let page = state.recency_page(store, after, limits)?;
    let point_limit = CatalogPointReadLimit::schema_maximum();
    let mut rows = Vec::with_capacity(page.rows.len());
    for row in &page.rows {
        if state.row(store, row.thread_id(), point_limit)? != Some(row.clone()) {
            return Err(CatalogReadError::Invariant(
                "catalog point and recency copies disagree during current scan",
            ));
        }
        if row.freshness() != CatalogFreshness::Current {
            return Err(CatalogReadError::StaleRow {
                thread_id: row.thread_id(),
            });
        }
        rows.push(CatalogCurrentRow { row: row.clone() });
    }
    ensure_revision(state, store, scan.revision)?;
    let validation_bytes = page
        .rows
        .len()
        .checked_mul(CATALOG_POINT_READ_MAX_BYTES)
        .ok_or(CatalogReadError::Invariant(
            "catalog current-page byte accounting overflowed",
        ))?;
    let stored_bytes =
        page.stored_bytes
            .checked_add(validation_bytes)
            .ok_or(CatalogReadError::Invariant(
                "catalog current-page stored-byte accounting overflowed",
            ))?;
    let decoded_bytes =
        page.decoded_bytes
            .checked_add(validation_bytes)
            .ok_or(CatalogReadError::Invariant(
                "catalog current-page decoded-byte accounting overflowed",
            ))?;
    Ok(CatalogCurrentPage {
        rows,
        stored_bytes,
        decoded_bytes,
        has_more: page.has_more,
        next_after: page.next_after(),
    })
}

pub(super) fn current_row_source(
    state: &CatalogState,
    store: &HomeStore,
    thread_id: SyndicThreadId,
    limit: CatalogPointReadLimit,
) -> Result<Option<CatalogCurrentRow>, CatalogCurrentRowError> {
    let Some(row) = store.read_point::<CatalogDomain, CatalogRowCodec>(
        &state.handle,
        &thread_id,
        PointReadLimit::new(limit.max_bytes()).expect("catalog point limit is nonzero"),
    )?
    else {
        return Ok(None);
    };
    if row.thread_id() != thread_id {
        return Err(CatalogCurrentRowError::IdentityMismatch { thread_id });
    }
    if row.freshness() != CatalogFreshness::Current {
        return Err(CatalogCurrentRowError::NotCurrent { thread_id });
    }
    let index = store.read_point::<CatalogDomain, CatalogRecencyCodec>(
        &state.handle,
        &row.recency_cursor(),
        PointReadLimit::new(CATALOG_POINT_READ_MAX_BYTES).expect("catalog point limit is nonzero"),
    )?;
    if index != Some(row.clone()) {
        return Err(CatalogCurrentRowError::ReverseCopyChanged { thread_id });
    }
    Ok(Some(CatalogCurrentRow { row }))
}

fn ensure_revision(
    state: &CatalogState,
    store: &HomeStore,
    expected: DomainRevision,
) -> Result<(), CatalogReadError> {
    let current = state.revision(store)?;
    if current != expected {
        return Err(CatalogReadError::RevisionChanged { expected, current });
    }
    Ok(())
}

impl DomainValidator<CatalogDomain> for CatalogCurrentRow {
    type Error = CatalogCurrentRowError;

    fn validate(&self, reader: &DomainReader<'_, CatalogDomain>) -> Result<(), Self::Error> {
        let thread_id = self.row.thread_id();
        let point = reader.point::<CatalogRowCodec>(
            &thread_id,
            PointReadLimit::new(CATALOG_POINT_READ_MAX_BYTES)
                .expect("catalog point limit is nonzero"),
        )?;
        if point.as_ref() != Some(&self.row) {
            return Err(CatalogCurrentRowError::SourceChanged { thread_id });
        }
        if self.row.freshness() != CatalogFreshness::Current {
            return Err(CatalogCurrentRowError::NotCurrent { thread_id });
        }
        let index = reader.point::<CatalogRecencyCodec>(
            &self.row.recency_cursor(),
            PointReadLimit::new(CATALOG_POINT_READ_MAX_BYTES)
                .expect("catalog point limit is nonzero"),
        )?;
        if index.as_ref() != Some(&self.row) {
            return Err(CatalogCurrentRowError::ReverseCopyChanged { thread_id });
        }
        Ok(())
    }
}

#[derive(Debug)]
pub enum CatalogCurrentRowError {
    Read(ReadError),
    IdentityMismatch { thread_id: SyndicThreadId },
    NotCurrent { thread_id: SyndicThreadId },
    SourceChanged { thread_id: SyndicThreadId },
    ReverseCopyChanged { thread_id: SyndicThreadId },
}

impl fmt::Display for CatalogCurrentRowError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Read(source) => source.fmt(formatter),
            Self::IdentityMismatch { thread_id } => {
                write!(
                    formatter,
                    "catalog point identity disagrees for {thread_id}"
                )
            }
            Self::NotCurrent { thread_id } => {
                write!(formatter, "catalog row {thread_id} is not current")
            }
            Self::SourceChanged { thread_id } => {
                write!(formatter, "catalog current row changed for {thread_id}")
            }
            Self::ReverseCopyChanged { thread_id } => {
                write!(formatter, "catalog reverse copy changed for {thread_id}")
            }
        }
    }
}

impl Error for CatalogCurrentRowError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Read(source) => Some(source),
            _ => None,
        }
    }
}

impl DomainCallbackError for CatalogCurrentRowError {
    fn into_callback_source(self) -> Result<DomainCallbackSource, Self> {
        match self {
            Self::Read(source) => Ok(DomainCallbackSource::Read(source)),
            source => Err(source),
        }
    }
}

impl From<ReadError> for CatalogCurrentRowError {
    fn from(source: ReadError) -> Self {
        Self::Read(source)
    }
}
