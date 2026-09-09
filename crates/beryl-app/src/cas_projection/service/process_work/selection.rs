use super::{
    ProcessWorkCursor, ProcessWorkError, ProcessWorkPage, ProcessWorkPageLimits, ProcessWorkRecord,
    ProcessWorkRevision, types::SortKey,
};

pub(super) struct Selection {
    limits: ProcessWorkPageLimits,
    after: Option<SortKey>,
    records: Vec<ProcessWorkRecord>,
    bytes: usize,
    first_omitted: Option<SortKey>,
}

#[cfg(all(test, feature = "test-faults"))]
mod tests {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/unit/process_work_selection.rs"
    ));
}

impl Selection {
    pub(super) fn new(limits: ProcessWorkPageLimits, after: Option<SortKey>) -> Self {
        Self {
            limits,
            after,
            records: Vec::new(),
            bytes: 0,
            first_omitted: None,
        }
    }

    pub(super) fn insert(&mut self, record: ProcessWorkRecord) {
        let key = record.key();
        if self.after.is_some_and(|after| key <= after)
            || self.first_omitted.is_some_and(|omitted| key >= omitted)
        {
            return;
        }
        let index = self.records.partition_point(|row| row.key() < key);
        self.bytes += record.bytes();
        self.records.insert(index, record);
        while self.records.len() > self.limits.max_records || self.bytes > self.limits.max_bytes {
            let removed = self
                .records
                .pop()
                .expect("an over-budget prefix contains a row");
            self.bytes -= removed.bytes();
            self.first_omitted = Some(removed.key());
        }
    }

    pub(super) fn finish(
        self,
        revision: ProcessWorkRevision,
        total_threads: u64,
    ) -> Result<ProcessWorkPage, ProcessWorkError> {
        if self.records.is_empty() && self.first_omitted.is_some() {
            return Err(ProcessWorkError::ByteLimit);
        }
        let next_cursor = self.first_omitted.map(|_| ProcessWorkCursor {
            revision: revision.clone(),
            after: self
                .records
                .last()
                .expect("a nonfinal page contains a row")
                .key(),
        });
        Ok(ProcessWorkPage {
            revision,
            records: self.records,
            total_threads,
            bytes: self.bytes,
            next_cursor,
        })
    }
}
