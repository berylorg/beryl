use beryl_home_store::{
    CursorDirection, CursorRange, CursorReadLimits, DomainReader, PointReadLimit,
};

use crate::{
    codec::{ExactCodec, Family, ScanKey},
    domain::SyndicDomain,
    error::SyndicValidationError,
};

pub(crate) const PAGE_ITEMS: usize = 64;
pub(crate) const PAGE_BYTES: usize = 32 * 1024 * 1024;
const POINT_BYTES: usize = 512 * 1024;

pub(super) fn scan<F: Family>(
    reader: &DomainReader<'_, SyndicDomain>,
    inspect: impl FnMut(&F::Key, &F::Value) -> Result<(), SyndicValidationError>,
) -> Result<(), SyndicValidationError>
where
    F::Key: ScanKey,
{
    scan_range::<F>(reader, F::Key::first(), F::Key::last(), inspect)
}

pub(super) fn scan_range<F: Family>(
    reader: &DomainReader<'_, SyndicDomain>,
    first: F::Key,
    last: F::Key,
    mut inspect: impl FnMut(&F::Key, &F::Value) -> Result<(), SyndicValidationError>,
) -> Result<(), SyndicValidationError>
where
    F::Key: ScanKey,
{
    let mut after: Option<F::Key> = None;
    loop {
        let range = match after.as_ref() {
            Some(after) => CursorRange::after(after.clone(), last.clone()),
            None => CursorRange::closed(first.clone(), last.clone()),
        };
        let page = reader.cursor::<ExactCodec<F>>(
            &range,
            CursorDirection::Forward,
            CursorReadLimits::new(PAGE_ITEMS, PAGE_BYTES).expect("validation bounds are nonzero"),
        )?;
        #[cfg(feature = "test-faults")]
        crate::test_faults::metrics::record_validation_page(
            F::NAME,
            page.records().len(),
            page.stored_bytes(),
        );
        for record in page.records() {
            inspect(record.key(), record.value())?;
        }
        if !page.has_more() {
            return Ok(());
        }
        after = page.records().last().map(|record| record.key().clone());
        if after.is_none() {
            return Err(SyndicValidationError::Invariant(
                "bounded validation cursor reported more without a record",
            ));
        }
    }
}

pub(super) fn point<F: Family>(
    reader: &DomainReader<'_, SyndicDomain>,
    key: &F::Key,
) -> Result<Option<F::Value>, SyndicValidationError> {
    reader
        .point::<ExactCodec<F>>(
            key,
            PointReadLimit::new(POINT_BYTES).expect("validation point bound is nonzero"),
        )
        .map_err(Into::into)
}

pub(super) fn require<F: Family>(
    reader: &DomainReader<'_, SyndicDomain>,
    key: &F::Key,
    missing: &'static str,
) -> Result<F::Value, SyndicValidationError> {
    point::<F>(reader, key)?.ok_or(SyndicValidationError::Invariant(missing))
}
