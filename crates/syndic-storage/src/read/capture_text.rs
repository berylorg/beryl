use beryl_home_store::HomeStore;

use crate::{ProjectionTextSource, SyndicReadError, domain::SyndicStorage};

use super::{
    ReadByteTotals, SyndicCaptureItem, SyndicPointReadLimit,
    content_text::{CONTENT_TEXT_MAX_PAYLOAD_BYTES, invalid_offset},
    range::read_projection_text_source_range,
};

/// One bounded UTF-8 page from an exact live-capture projection text source.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SyndicCaptureTextRangeRead {
    source: ProjectionTextSource,
    start: u64,
    text: Box<str>,
    next_offset: Option<u64>,
    stored_bytes: usize,
    decoded_bytes: usize,
}

impl SyndicCaptureTextRangeRead {
    /// Returns the exact composer-content or provider-narrative source read by this page.
    #[must_use]
    pub const fn source(&self) -> ProjectionTextSource {
        self.source
    }

    #[must_use]
    pub const fn start(&self) -> u64 {
        self.start
    }

    #[must_use]
    pub const fn text(&self) -> &str {
        &self.text
    }

    #[must_use]
    pub const fn next_offset(&self) -> Option<u64> {
        self.next_offset
    }

    #[must_use]
    pub const fn stored_bytes(&self) -> usize {
        self.stored_bytes
    }

    #[must_use]
    pub const fn decoded_bytes(&self) -> usize {
        self.decoded_bytes
    }
}

impl SyndicStorage {
    /// Reads one bounded logical UTF-8 page from an exact live-capture item snapshot.
    ///
    /// The CAS index, canonical item, and owned live or finalized manifest are stabilized before
    /// and after the page. Mutations of unrelated items do not invalidate this read.
    pub fn capture_item_text_range(
        &self,
        store: &HomeStore,
        item: &SyndicCaptureItem,
        start: u64,
        max_payload_bytes: usize,
        limit: SyndicPointReadLimit,
    ) -> Result<SyndicCaptureTextRangeRead, SyndicReadError> {
        if max_payload_bytes == 0 || max_payload_bytes > CONTENT_TEXT_MAX_PAYLOAD_BYTES {
            return Err(SyndicReadError::InvalidContentTextReadLimit {
                maximum: CONTENT_TEXT_MAX_PAYLOAD_BYTES,
                actual: max_payload_bytes,
            });
        }
        let cas_source = item.item().cas_source().ok_or(SyndicReadError::Invariant(
            "live-capture text item has no CAS source",
        ))?;
        let before = self
            .capture_item(store, cas_source, limit)?
            .ok_or_else(concurrent)?;
        if &before != item {
            return Err(concurrent());
        }
        let source = before
            .item()
            .projection_source()
            .ok_or(SyndicReadError::CaptureItemHasNoTextContent)?;
        let source_bytes = source.logical_utf8_bytes();
        if start > source_bytes {
            return Err(invalid_offset(source_bytes, start));
        }
        let (bytes, end, totals) = if start == source_bytes {
            (Vec::new(), start, ReadByteTotals::default())
        } else {
            let payload_bytes_u64 = u64::try_from(max_payload_bytes)
                .expect("the fixed content text payload bound fits u64");
            let desired_end = start.saturating_add(payload_bytes_u64).min(source_bytes);
            let (bytes, totals) =
                read_projection_text_source_range(self, store, source, start, desired_end)?;
            let (bytes, end) =
                finish_page(bytes, start, desired_end, source_bytes, max_payload_bytes)?;
            (bytes, end, totals)
        };
        let after = self
            .capture_item(store, cas_source, limit)?
            .ok_or_else(concurrent)?;
        if after != before {
            return Err(concurrent());
        }
        let text = String::from_utf8(bytes)
            .map_err(|_| SyndicReadError::Invariant("live-capture text page is not valid UTF-8"))?
            .into_boxed_str();
        Ok(SyndicCaptureTextRangeRead {
            source,
            start,
            text,
            next_offset: (end < source_bytes).then_some(end),
            stored_bytes: totals.stored,
            decoded_bytes: totals.decoded,
        })
    }
}

fn finish_page(
    mut bytes: Vec<u8>,
    start: u64,
    desired_end: u64,
    source_bytes: u64,
    max_payload_bytes: usize,
) -> Result<(Vec<u8>, u64), SyndicReadError> {
    match std::str::from_utf8(&bytes) {
        Ok(_) => {}
        Err(error) if error.error_len().is_none() && desired_end < source_bytes => {
            bytes.truncate(error.valid_up_to());
            if bytes.is_empty() {
                return Err(SyndicReadError::ContentTextReadLimitTooSmall {
                    offset: start,
                    actual: max_payload_bytes,
                });
            }
        }
        Err(error) if error.valid_up_to() == 0 => {
            return Err(invalid_offset(source_bytes, start));
        }
        Err(_) => {
            return Err(SyndicReadError::Invariant(
                "live-capture projection text is not valid UTF-8",
            ));
        }
    }
    let returned = u64::try_from(bytes.len())
        .map_err(|_| SyndicReadError::Invariant("live-capture text page length overflowed"))?;
    let end = start
        .checked_add(returned)
        .ok_or(SyndicReadError::Invariant(
            "live-capture text page end overflowed",
        ))?;
    Ok((bytes, end))
}

fn concurrent() -> SyndicReadError {
    SyndicReadError::ConcurrentChange {
        operation: "live-capture item text-range read",
    }
}
