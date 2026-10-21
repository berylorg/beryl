use super::{SourceValue, codec::*};
use crate::{SyndicReadError, SyndicStorage, domain::SyndicDomain};
use beryl_home_store::{CursorDirection, CursorRange, CursorReadLimits, HomeStore};
use beryl_model::{BerylHomeId, SyndicResourceId};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GeneratedMediaSweep {
    home: BerylHomeId,
    high_water: u64,
    after: u64,
}
impl GeneratedMediaSweep {
    pub fn high_water(&self) -> u64 {
        self.high_water
    }
    pub fn after_sequence(&self) -> u64 {
        self.after
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GeneratedMediaPendingEntry {
    sequence: u64,
    resource: SyndicResourceId,
}
impl GeneratedMediaPendingEntry {
    pub fn sequence(self) -> u64 {
        self.sequence
    }
    pub fn resource_id(self) -> SyndicResourceId {
        self.resource
    }
}
#[derive(Clone, Debug)]
pub struct GeneratedMediaPendingPage {
    entries: Vec<GeneratedMediaPendingEntry>,
    continuation: Option<GeneratedMediaSweep>,
    stored_bytes: usize,
}
impl GeneratedMediaPendingPage {
    pub fn entries(&self) -> &[GeneratedMediaPendingEntry] {
        &self.entries
    }
    pub fn continuation(&self) -> Option<&GeneratedMediaSweep> {
        self.continuation.as_ref()
    }
    pub fn into_continuation(self) -> Option<GeneratedMediaSweep> {
        self.continuation
    }
    pub fn stored_bytes(&self) -> usize {
        self.stored_bytes
    }
}
impl SyndicStorage {
    pub fn begin_generated_media_sweep(
        &self,
        store: &HomeStore,
    ) -> Result<GeneratedMediaSweep, SyndicReadError> {
        let high_water = match self.generated_point::<SourcesFamily>(store, SourceKey::Allocator)? {
            None => 0,
            Some(SourceValue::Allocator(high)) => high,
            _ => return Err(super::source::invalid()),
        };
        Ok(GeneratedMediaSweep {
            home: store.home_id(),
            high_water,
            after: 0,
        })
    }
    pub fn pending_generated_media_page(
        &self,
        store: &HomeStore,
        sweep: &GeneratedMediaSweep,
        max_records: usize,
        max_bytes: usize,
    ) -> Result<GeneratedMediaPendingPage, SyndicReadError> {
        if sweep.home != store.home_id()
            || max_records == 0
            || max_records > 256
            || max_bytes == 0
            || max_bytes > 65_536
        {
            return Err(super::source::invalid());
        }
        if sweep.after >= sweep.high_water {
            return Ok(GeneratedMediaPendingPage {
                entries: Vec::new(),
                continuation: None,
                stored_bytes: 0,
            });
        }
        let page = store.read_cursor::<SyndicDomain, PendingCodec>(
            &self.handle,
            &CursorRange::closed(
                PendingKey(
                    sweep
                        .after
                        .checked_add(1)
                        .ok_or_else(super::source::invalid)?,
                ),
                PendingKey(sweep.high_water),
            ),
            CursorDirection::Forward,
            CursorReadLimits::new(max_records, max_bytes).expect("checked pending page bounds"),
        )?;
        let entries: Vec<_> = page
            .records()
            .iter()
            .map(|record| GeneratedMediaPendingEntry {
                sequence: record.key().0,
                resource: *record.value(),
            })
            .collect();
        let continuation = if page.has_more() {
            Some(GeneratedMediaSweep {
                after: entries.last().ok_or_else(super::source::invalid)?.sequence,
                ..sweep.clone()
            })
        } else {
            None
        };
        Ok(GeneratedMediaPendingPage {
            entries,
            continuation,
            stored_bytes: page.stored_bytes(),
        })
    }
}
