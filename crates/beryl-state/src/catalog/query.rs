use std::sync::atomic::{AtomicU64, Ordering};

use beryl_home_store::{
    CommandCancellation, CursorReadLimits, FrozenHomeRead, FrozenReadAccessError,
    HomeGenerationIdentity, HomeStore, ReadError,
};
use beryl_model::{DomainRevision, HomeRevision, RootId, RuntimeId, SyndicThreadId};

use super::{
    CATALOG_MAX_STORED_RECENCY_BYTES, CatalogFreshness, CatalogNormalizedQuery, CatalogReadError,
    CatalogRecencyCursor, CatalogRow, CatalogState,
};
use crate::{
    RootRecord, RuntimeRecord, RuntimeRootCatalogSource, RuntimeRootCatalogSourceError,
    RuntimeRootState,
};

#[path = "query/error.rs"]
mod error;
#[path = "query/options.rs"]
mod options;
#[path = "query/owner.rs"]
mod owner;
#[path = "query/scan.rs"]
mod scan;
#[path = "query/types.rs"]
mod types;

pub use error::{CatalogQueryError, CatalogQueryOpenError};
pub use types::*;

pub const CATALOG_QUERY_COLLECTION_LIMIT: usize = 32;
pub const CATALOG_QUERY_PAGE_MAX_ITEMS: usize = 16;
pub const CATALOG_QUERY_PAGE_MAX_BYTES: usize = 8 * 1024 * 1024;
const SCAN_ITEMS: usize = 16;
const SCAN_BYTES: usize = SCAN_ITEMS * CATALOG_MAX_STORED_RECENCY_BYTES;
const PRESENTATION_ROW_BYTES: usize = CATALOG_MAX_STORED_RECENCY_BYTES
    + 2 * (132 * 1024 + 4 + 32)
    + std::mem::size_of::<CatalogQueryRow>()
    + std::mem::size_of::<CatalogQueryCursor>();
static NEXT_OWNER: AtomicU64 = AtomicU64::new(1);

pub struct CatalogQueryOwner {
    id: u64,
    generation: HomeGenerationIdentity,
    catalog: CatalogState,
    runtime_roots: RuntimeRootState,
    next_query: u64,
    requests: u64,
    entries: Vec<Entry>,
    retired: bool,
}

struct Entry {
    token: CatalogQueryToken,
    frozen: FrozenHomeRead,
    criteria: CatalogQueryCriteria,
    revision: Option<DomainRevision>,
    count: u64,
    retiring: bool,
}

fn check_cancel(cancel: &CommandCancellation) -> Result<(), CatalogQueryError> {
    if cancel.is_cancelled() {
        Err(CatalogQueryError::Cancelled)
    } else {
        Ok(())
    }
}

fn scan_limits() -> CursorReadLimits {
    CursorReadLimits::new(SCAN_ITEMS, SCAN_BYTES).expect("fixed nonzero catalog scanner limits")
}
