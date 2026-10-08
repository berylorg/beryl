use beryl_home_store::{
    CursorDirection, CursorRange, CursorReadLimits, HomeStore, RECORD_VERSION_BYTES,
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
        let last = SyndicThreadId::from_bytes([u8::MAX; 16]);
        let range = match after {
            Some(after) => CursorRange::after(after, last),
            None => CursorRange::closed(SyndicThreadId::from_bytes([0; 16]), last),
        };
        let limits = CursorReadLimits::new(
            limits.max_items().min(THREAD_DISCOVERY_PAGE_MAX_ITEMS),
            limits.max_bytes().min(THREAD_DISCOVERY_PAGE_MAX_BYTES),
        )
        .expect("clamped nonzero thread discovery limits remain nonzero");
        let page = store.read_cursor::<SyndicDomain, ExactCodec<ThreadsFamily>>(
            &self.handle,
            &range,
            CursorDirection::Forward,
            limits,
        )?;
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
}
