use beryl_home_store::{CursorDirection, CursorRange, CursorReadLimits, HomeStore};
use beryl_model::SyndicThreadId;

use crate::{
    ActivityQueryEntryRecord, ActivityQueryHeadRecord, ActivityQueryOrder, ActivityQueryRevision,
    ProjectionLifecycle, SyndicReadError, codec::*, domain::SyndicStorage,
};

use super::{QUERY_PAGE_MAX_RECORDS, QUERY_PAGE_MAX_STORED_BYTES};
use crate::read::SyndicPointReadLimit;

/// Revision-bound continuation for one durable activity query.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ActivityQueryCursor {
    thread_id: SyndicThreadId,
    work_period: crate::ActivityWorkPeriod,
    revision: ActivityQueryRevision,
    after: ActivityQueryOrder,
}

/// Revision- and work-period-bound continuation for exact activity sources.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ActivityQuerySourceCursor {
    thread_id: SyndicThreadId,
    work_period: crate::ActivityWorkPeriod,
    revision: ActivityQueryRevision,
    after: crate::ActivityQuerySource,
}

/// One fixed-cap activity page and its optional exact continuation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ActivityQueryPage {
    records: Vec<ActivityQueryEntryRecord>,
    stored_bytes: usize,
    decoded_bytes: usize,
    next_cursor: Option<ActivityQueryCursor>,
}

impl ActivityQueryPage {
    #[must_use]
    pub fn records(&self) -> &[ActivityQueryEntryRecord] {
        &self.records
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
    pub const fn next_cursor(&self) -> Option<ActivityQueryCursor> {
        self.next_cursor
    }
}

/// One fixed-cap page of exact source-turn memberships.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ActivityQuerySourcePage {
    records: Vec<crate::ActivityQuerySourceRecord>,
    stored_bytes: usize,
    decoded_bytes: usize,
    next_cursor: Option<ActivityQuerySourceCursor>,
}

impl ActivityQuerySourcePage {
    #[must_use]
    pub fn records(&self) -> &[crate::ActivityQuerySourceRecord] {
        &self.records
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
    pub const fn next_cursor(&self) -> Option<ActivityQuerySourceCursor> {
        self.next_cursor
    }
}

impl SyndicStorage {
    pub fn activity_query_head(
        &self,
        store: &HomeStore,
        thread_id: SyndicThreadId,
        limit: SyndicPointReadLimit,
    ) -> Result<Option<ActivityQueryHeadRecord>, SyndicReadError> {
        self.point::<ActivityQueryHeadsFamily>(store, thread_id, limit)
    }

    pub fn activity_query_page(
        &self,
        store: &HomeStore,
        expected: &ActivityQueryHeadRecord,
        cursor: Option<ActivityQueryCursor>,
        limits: CursorReadLimits,
    ) -> Result<ActivityQueryPage, SyndicReadError> {
        if expected.lifecycle() != ProjectionLifecycle::Current {
            return Err(SyndicReadError::ActivityQueryIsStale);
        }
        let limit = query_point_limit();
        let before = self
            .activity_query_head(store, expected.thread_id(), limit)?
            .ok_or(SyndicReadError::StaleActivityQuery)?;
        if &before != expected {
            return Err(SyndicReadError::StaleActivityQuery);
        }
        let after = match cursor {
            Some(cursor)
                if cursor.thread_id == expected.thread_id()
                    && cursor.work_period == expected.work_period()
                    && cursor.revision == expected.revision() =>
            {
                Some(cursor.after)
            }
            Some(_) => return Err(SyndicReadError::InvalidActivityQueryCursor),
            None => None,
        };
        let last =
            ActivityQueryEntryKey::last_for_period(expected.thread_id(), expected.work_period());
        let range = match after {
            Some(after) => CursorRange::after(
                ActivityQueryEntryKey {
                    thread: expected.thread_id(),
                    work_period: expected.work_period(),
                    order: after,
                },
                last,
            ),
            None => CursorRange::closed(
                if expected.source_active() {
                    ActivityQueryEntryKey::first_for_period(
                        expected.thread_id(),
                        expected.work_period(),
                    )
                } else {
                    ActivityQueryEntryKey::first_completed_for_period(
                        expected.thread_id(),
                        expected.work_period(),
                    )
                },
                last,
            ),
        };
        let limits = CursorReadLimits::new(
            limits.max_items().min(QUERY_PAGE_MAX_RECORDS),
            limits.max_bytes().min(QUERY_PAGE_MAX_STORED_BYTES),
        )
        .expect("clamped nonzero query limits remain nonzero");
        let page = store.read_cursor::<crate::domain::SyndicDomain, ActivityQueryEntriesCodec>(
            self.handle,
            &range,
            CursorDirection::Forward,
            limits,
        )?;
        let stored_bytes = page.stored_bytes();
        let decoded_bytes = page.decoded_bytes();
        let has_more = page.has_more();
        let records: Vec<_> = page
            .into_records()
            .into_iter()
            .map(|record| record.into_parts().1)
            .collect();
        let confirmed = self
            .activity_query_head(store, expected.thread_id(), limit)?
            .ok_or(SyndicReadError::StaleActivityQuery)?;
        if &confirmed != expected {
            return Err(SyndicReadError::StaleActivityQuery);
        }
        let next_cursor = has_more.then(|| ActivityQueryCursor {
            thread_id: expected.thread_id(),
            work_period: expected.work_period(),
            revision: expected.revision(),
            after: records
                .last()
                .expect("more requires one activity entry")
                .order(),
        });
        Ok(ActivityQueryPage {
            records,
            stored_bytes,
            decoded_bytes,
            next_cursor,
        })
    }

    pub fn activity_query_source_page(
        &self,
        store: &HomeStore,
        expected: &ActivityQueryHeadRecord,
        cursor: Option<ActivityQuerySourceCursor>,
        limits: CursorReadLimits,
    ) -> Result<ActivityQuerySourcePage, SyndicReadError> {
        if expected.lifecycle() != ProjectionLifecycle::Current {
            return Err(SyndicReadError::ActivityQueryIsStale);
        }
        let limit = query_point_limit();
        let before = self
            .activity_query_head(store, expected.thread_id(), limit)?
            .ok_or(SyndicReadError::StaleActivityQuery)?;
        if &before != expected {
            return Err(SyndicReadError::StaleActivityQuery);
        }
        let last =
            ActivityQuerySourceKey::last_for_period(expected.thread_id(), expected.work_period());
        let range = match cursor {
            Some(cursor)
                if cursor.thread_id == expected.thread_id()
                    && cursor.work_period == expected.work_period()
                    && cursor.revision == expected.revision() =>
            {
                CursorRange::after(
                    ActivityQuerySourceKey {
                        thread: expected.thread_id(),
                        work_period: expected.work_period(),
                        source_thread: cursor.after.thread_id(),
                        source_turn: cursor.after.turn_id(),
                    },
                    last,
                )
            }
            Some(_) => return Err(SyndicReadError::InvalidActivityQueryCursor),
            None => CursorRange::closed(
                ActivityQuerySourceKey::first_for_period(
                    expected.thread_id(),
                    expected.work_period(),
                ),
                last,
            ),
        };
        let limits = CursorReadLimits::new(
            limits.max_items().min(QUERY_PAGE_MAX_RECORDS),
            limits.max_bytes().min(QUERY_PAGE_MAX_STORED_BYTES),
        )
        .expect("clamped nonzero query limits remain nonzero");
        let page = store.read_cursor::<crate::domain::SyndicDomain, ActivityQuerySourcesCodec>(
            self.handle,
            &range,
            CursorDirection::Forward,
            limits,
        )?;
        let stored_bytes = page.stored_bytes();
        let decoded_bytes = page.decoded_bytes();
        let has_more = page.has_more();
        let records: Vec<_> = page
            .into_records()
            .into_iter()
            .map(|record| record.into_parts().1)
            .collect();
        let confirmed = self
            .activity_query_head(store, expected.thread_id(), limit)?
            .ok_or(SyndicReadError::StaleActivityQuery)?;
        if &confirmed != expected {
            return Err(SyndicReadError::StaleActivityQuery);
        }
        let next_cursor = has_more.then(|| ActivityQuerySourceCursor {
            thread_id: expected.thread_id(),
            work_period: expected.work_period(),
            revision: expected.revision(),
            after: records
                .last()
                .expect("more requires one activity source")
                .source(),
        });
        Ok(ActivityQuerySourcePage {
            records,
            stored_bytes,
            decoded_bytes,
            next_cursor,
        })
    }
}

fn query_point_limit() -> SyndicPointReadLimit {
    SyndicPointReadLimit::new(crate::codec::SMALL_MAX + 128).expect("query point bound is nonzero")
}
