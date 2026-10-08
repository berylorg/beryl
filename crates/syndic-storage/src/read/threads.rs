use beryl_home_store::{
    CursorDirection, CursorPage, CursorRange, CursorReadLimits, FrozenHomeRead, HomeStore,
    RECORD_VERSION_BYTES,
};
use beryl_model::SyndicThreadId;

use crate::{
    SyndicStorage, ThreadRecord,
    codec::{ExactCodec, Family, ThreadsFamily},
    domain::SyndicDomain,
};

use super::{SyndicPage, SyndicReadError};

pub const THREAD_DISCOVERY_PAGE_MAX_ITEMS: usize = 16;
pub const THREAD_DISCOVERY_PAGE_MAX_BYTES: usize = THREAD_DISCOVERY_PAGE_MAX_ITEMS
    * (ThreadsFamily::MAX_KEY_BYTES + ThreadsFamily::MAX_VALUE_BYTES + RECORD_VERSION_BYTES);

impl SyndicStorage {
    pub fn threads_page(
        &self,
        store: &HomeStore,
        after: Option<SyndicThreadId>,
        limits: CursorReadLimits,
    ) -> Result<SyndicPage<ThreadRecord>, SyndicReadError> {
        let range = discovery_range(after);
        let limits = discovery_limits(limits);
        let page = store.read_cursor::<SyndicDomain, ExactCodec<ThreadsFamily>>(
            &self.handle,
            &range,
            CursorDirection::Forward,
            limits,
        )?;
        decode_page(page)
    }

    pub fn frozen_threads_page(
        &self,
        store: &HomeStore,
        frozen: &FrozenHomeRead,
        after: Option<SyndicThreadId>,
        limits: CursorReadLimits,
    ) -> Result<SyndicPage<ThreadRecord>, SyndicReadError> {
        let page = store.read_frozen_cursor::<SyndicDomain, ExactCodec<ThreadsFamily>>(
            frozen,
            &self.handle,
            &discovery_range(after),
            CursorDirection::Forward,
            discovery_limits(limits),
        )?;
        decode_page(page)
    }
}

fn discovery_range(after: Option<SyndicThreadId>) -> CursorRange<SyndicThreadId> {
    let last = SyndicThreadId::from_bytes([u8::MAX; 16]);
    match after {
        Some(after) => CursorRange::after(after, last),
        None => CursorRange::closed(SyndicThreadId::from_bytes([0; 16]), last),
    }
}

fn discovery_limits(limits: CursorReadLimits) -> CursorReadLimits {
    CursorReadLimits::new(
        limits.max_items().min(THREAD_DISCOVERY_PAGE_MAX_ITEMS),
        limits.max_bytes().min(THREAD_DISCOVERY_PAGE_MAX_BYTES),
    )
    .expect("clamped nonzero thread discovery limits remain nonzero")
}

fn decode_page(
    page: CursorPage<SyndicThreadId, ThreadRecord>,
) -> Result<SyndicPage<ThreadRecord>, SyndicReadError> {
    let stored_bytes = page.stored_bytes();
    let decoded_bytes = page.decoded_bytes();
    let has_more = page.has_more();
    let records = page
        .into_records()
        .into_iter()
        .map(|record| {
            let (key, value) = record.into_parts();
            if key != value.id() {
                return Err(SyndicReadError::Invariant(
                    "thread discovery key and identity disagree",
                ));
            }
            Ok(value)
        })
        .collect::<Result<_, _>>()?;
    Ok(SyndicPage {
        records,
        stored_bytes,
        decoded_bytes,
        has_more,
    })
}
