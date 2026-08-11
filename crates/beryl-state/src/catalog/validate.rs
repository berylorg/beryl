use beryl_home_store::{
    CursorDirection, CursorRange, CursorReadLimits, DomainReader, PointReadLimit,
};
use beryl_model::SyndicThreadId;

use super::{
    codec::{CatalogRecencyCodec, CatalogRowCodec},
    CatalogDomain, CatalogRecencyCursor, CatalogValidationError, CATALOG_RECORD_LIMIT,
};

const VALIDATION_PAGE_ITEMS: usize = 64;
const VALIDATION_PAGE_BYTES: usize = 8 * 1024 * 1024;

pub(super) fn validate(
    reader: &DomainReader<'_, CatalogDomain>,
) -> Result<(), CatalogValidationError> {
    validate_rows(reader)?;
    validate_recency(reader)
}

fn validate_rows(reader: &DomainReader<'_, CatalogDomain>) -> Result<(), CatalogValidationError> {
    let mut after = None;
    loop {
        let end = SyndicThreadId::from_bytes([u8::MAX; 16]);
        let range = match after {
            Some(after) => CursorRange::after(after, end),
            None => CursorRange::closed(SyndicThreadId::from_bytes([0; 16]), end),
        };
        let page = reader.cursor::<CatalogRowCodec>(&range, CursorDirection::Forward, limits())?;
        for record in page.records() {
            let row = record.value();
            if record.key() != &row.thread_id() {
                return Err(CatalogValidationError::Invariant(
                    "catalog row key does not match its thread identity",
                ));
            }
            let copy = reader
                .point::<CatalogRecencyCodec>(&row.recency_cursor(), point_limit())?
                .ok_or(CatalogValidationError::Invariant(
                    "catalog row has no recency-index copy",
                ))?;
            if &copy != row {
                return Err(CatalogValidationError::Invariant(
                    "catalog row and recency-index copies disagree",
                ));
            }
        }
        if !page.has_more() {
            return Ok(());
        }
        after = page.records().last().map(|record| *record.key());
        if after.is_none() {
            return Err(CatalogValidationError::Invariant(
                "bounded catalog-row cursor reported more without a row",
            ));
        }
    }
}

fn validate_recency(
    reader: &DomainReader<'_, CatalogDomain>,
) -> Result<(), CatalogValidationError> {
    let mut after = None;
    loop {
        let range = match after {
            Some(after) => CursorRange::after(after, CatalogRecencyCursor::last()),
            None => {
                CursorRange::closed(CatalogRecencyCursor::first(), CatalogRecencyCursor::last())
            }
        };
        let page =
            reader.cursor::<CatalogRecencyCodec>(&range, CursorDirection::Forward, limits())?;
        for record in page.records() {
            let row = record.value();
            if record.key() != &row.recency_cursor() {
                return Err(CatalogValidationError::Invariant(
                    "catalog recency key does not match its row copy",
                ));
            }
            let authoritative = reader
                .point::<CatalogRowCodec>(&row.thread_id(), point_limit())?
                .ok_or(CatalogValidationError::Invariant(
                    "catalog recency copy has no authoritative row",
                ))?;
            if authoritative != *row {
                return Err(CatalogValidationError::Invariant(
                    "catalog recency copy and authoritative row disagree",
                ));
            }
        }
        if !page.has_more() {
            return Ok(());
        }
        after = page.records().last().map(|record| *record.key());
        if after.is_none() {
            return Err(CatalogValidationError::Invariant(
                "bounded catalog-recency cursor reported more without a row",
            ));
        }
    }
}

fn limits() -> CursorReadLimits {
    CursorReadLimits::new(VALIDATION_PAGE_ITEMS, VALIDATION_PAGE_BYTES)
        .expect("catalog validation limits are nonzero")
}

fn point_limit() -> PointReadLimit {
    PointReadLimit::new(CATALOG_RECORD_LIMIT + 4).expect("catalog point limit is nonzero")
}
