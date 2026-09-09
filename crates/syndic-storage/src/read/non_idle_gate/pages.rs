use beryl_home_store::{CursorDirection, CursorRange, CursorReadLimits, HomeGeneration};
use beryl_model::{BerylHomeId, DomainRevision};

use super::*;

pub const NON_IDLE_GATE_PAGE_MAX_RECORDS: usize = 256;
pub const NON_IDLE_GATE_PAGE_MAX_BYTES: usize = 65_536;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NonIdleGateSourceCursor {
    home_id: BerylHomeId,
    home_generation: HomeGeneration,
    source_revision: DomainRevision,
    after_thread_id: SyndicThreadId,
}

impl NonIdleGateSourceCursor {
    pub const fn source_revision(self) -> DomainRevision {
        self.source_revision
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NonIdleGateSourcePage {
    source_revision: DomainRevision,
    records: Vec<NonIdleGateSourceRecord>,
    stored_bytes: usize,
    decoded_bytes: usize,
    next_cursor: Option<NonIdleGateSourceCursor>,
}

impl NonIdleGateSourcePage {
    pub const fn source_revision(&self) -> DomainRevision {
        self.source_revision
    }
    pub fn records(&self) -> &[NonIdleGateSourceRecord] {
        &self.records
    }
    pub const fn stored_bytes(&self) -> usize {
        self.stored_bytes
    }
    pub const fn decoded_bytes(&self) -> usize {
        self.decoded_bytes
    }
    pub const fn next_cursor(&self) -> Option<NonIdleGateSourceCursor> {
        self.next_cursor
    }
}

impl SyndicStorage {
    pub fn non_idle_gate_source_page(
        &self,
        store: &HomeStore,
        expected_revision: DomainRevision,
        cursor: Option<NonIdleGateSourceCursor>,
        limits: CursorReadLimits,
    ) -> Result<NonIdleGateSourcePage, SyndicReadError> {
        if let Some(cursor) = cursor {
            self.validate_non_idle_cursor(store, cursor)?;
            if cursor.source_revision != expected_revision {
                return Err(SyndicReadError::InvalidNonIdleGateSourceCursor);
            }
        }
        self.check_non_idle_revision(store, expected_revision)?;
        let result = (|| {
            let last = SyndicThreadId::from_bytes([u8::MAX; 16]);
            let range = match cursor {
                Some(cursor) => CursorRange::after(cursor.after_thread_id, last),
                None => CursorRange::closed(SyndicThreadId::from_bytes([0; 16]), last),
            };
            let limits = CursorReadLimits::new(
                limits.max_items().min(NON_IDLE_GATE_PAGE_MAX_RECORDS),
                limits.max_bytes().min(NON_IDLE_GATE_PAGE_MAX_BYTES),
            )
            .expect("clamped nonzero source limits remain nonzero");
            let page = store.read_cursor::<crate::domain::SyndicDomain, NonIdleGateSourcesCodec>(
                &self.handle,
                &range,
                CursorDirection::Forward,
                limits,
            )?;
            let stored_bytes = page.stored_bytes();
            let decoded_bytes = page.decoded_bytes();
            let has_more = page.has_more();
            let mut records = Vec::with_capacity(page.records().len());
            for row in page.into_records() {
                let (key, source) = row.into_parts();
                if key != source.thread_id() {
                    return Err(SyndicReadError::Invariant(
                        "non-idle source key and identity disagree",
                    ));
                }
                records.push(source);
            }
            let next_cursor = if has_more {
                let last = records.last().ok_or(SyndicReadError::Invariant(
                    "non-idle source page reported more without a record",
                ))?;
                Some(NonIdleGateSourceCursor {
                    home_id: store.home_id(),
                    home_generation: self.home_generation,
                    source_revision: expected_revision,
                    after_thread_id: last.thread_id(),
                })
            } else {
                None
            };
            Ok(NonIdleGateSourcePage {
                source_revision: expected_revision,
                records,
                stored_bytes,
                decoded_bytes,
                next_cursor,
            })
        })();
        self.check_non_idle_revision(store, expected_revision)?;
        result
    }

    pub fn rebase_non_idle_gate_source_cursor(
        &self,
        store: &HomeStore,
        cursor: NonIdleGateSourceCursor,
    ) -> Result<NonIdleGateSourceCursor, SyndicReadError> {
        self.validate_non_idle_cursor(store, cursor)?;
        Ok(NonIdleGateSourceCursor {
            source_revision: self.revision(store)?,
            ..cursor
        })
    }

    pub fn resolve_non_idle_gate_source(
        &self,
        store: &HomeStore,
        expected_revision: DomainRevision,
        source: NonIdleGateSourceRecord,
        limit: SyndicPointReadLimit,
    ) -> Result<InputGateRecord, SyndicReadError> {
        self.check_non_idle_revision(store, expected_revision)?;
        let result = (|| {
            let current =
                self.point::<NonIdleGateSourcesFamily>(store, source.thread_id(), limit)?;
            let gate = self.point::<InputGatesFamily>(store, source.thread_id(), limit)?;
            if current != Some(source)
                || !non_idle_gate_source_matches(
                    source.thread_id(),
                    gate.as_ref(),
                    current.as_ref(),
                )
            {
                return Err(SyndicReadError::Invariant(
                    "selected non-idle source and current gate disagree",
                ));
            }
            gate.ok_or(SyndicReadError::Invariant(
                "selected non-idle source has no gate",
            ))
        })();
        self.check_non_idle_revision(store, expected_revision)?;
        result
    }

    fn validate_non_idle_cursor(
        &self,
        store: &HomeStore,
        cursor: NonIdleGateSourceCursor,
    ) -> Result<(), SyndicReadError> {
        if cursor.home_id != store.home_id() || cursor.home_generation != self.home_generation {
            return Err(SyndicReadError::InvalidNonIdleGateSourceCursor);
        }
        Ok(())
    }

    fn check_non_idle_revision(
        &self,
        store: &HomeStore,
        expected: DomainRevision,
    ) -> Result<(), SyndicReadError> {
        if self.revision(store)? != expected {
            return Err(SyndicReadError::StaleNonIdleGateSourceScan);
        }
        Ok(())
    }
}
