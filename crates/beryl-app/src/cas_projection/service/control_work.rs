use super::ProjectionConnectionService;
use super::work_sources::ProcessWorkRead;

impl ProjectionConnectionService {
    pub fn control_work_revision(&self) -> Result<ControlWorkRevision, ControlWorkError> {
        self.work_read().control_work_revision()
    }

    pub fn validate_control_work_revision(
        &self,
        revision: &ControlWorkRevision,
    ) -> Result<(), ControlWorkError> {
        self.work_read().validate_control_work_revision(revision)
    }

    pub fn control_work_page(
        &self,
        revision: &ControlWorkRevision,
        cursor: Option<&ControlWorkCursor>,
        limits: ControlWorkPageLimits,
    ) -> Result<ControlWorkPage, ControlWorkError> {
        self.work_read().control_work_page(revision, cursor, limits)
    }
}

use crate::cas_projection::{
    CompactionWorkError, CompactionWorkPageLimits, ControlWorkCursor, ControlWorkError,
    ControlWorkPage, ControlWorkPageLimits, ControlWorkRevision, StopWorkPageLimits,
    control_work::ControlWorkPosition,
};

impl ProcessWorkRead {
    pub fn control_work_revision(&self) -> Result<ControlWorkRevision, ControlWorkError> {
        let revision = ControlWorkRevision {
            stop: self.stop_work_revision()?,
            compaction: self.compaction_work_revision()?,
        };
        self.validate_control_work_revision(&revision)?;
        Ok(revision)
    }

    pub fn validate_control_work_revision(
        &self,
        revision: &ControlWorkRevision,
    ) -> Result<(), ControlWorkError> {
        self.validate_stop_work_revision(&revision.stop)?;
        self.validate_compaction_work_revision(&revision.compaction)?;
        Ok(())
    }

    pub fn control_work_page(
        &self,
        revision: &ControlWorkRevision,
        cursor: Option<&ControlWorkCursor>,
        limits: ControlWorkPageLimits,
    ) -> Result<ControlWorkPage, ControlWorkError> {
        self.collect_control_work_page(
            revision,
            cursor,
            limits,
            #[cfg(test)]
            || {},
        )
    }

    fn collect_control_work_page(
        &self,
        revision: &ControlWorkRevision,
        cursor: Option<&ControlWorkCursor>,
        limits: ControlWorkPageLimits,
        #[cfg(test)] after_stop: impl FnOnce(),
    ) -> Result<ControlWorkPage, ControlWorkError> {
        self.validate_control_work_revision(revision)?;
        if cursor.is_some_and(|cursor| &cursor.revision != revision) {
            return Err(ControlWorkError::ForeignCursor);
        }
        let mut page = ControlWorkPage {
            revision: revision.clone(),
            stop_records: Vec::new(),
            compaction_records: Vec::new(),
            bytes: 0,
            next_cursor: None,
        };
        let position = cursor.map(|cursor| &cursor.position);
        if !matches!(position, Some(ControlWorkPosition::Compaction(_))) {
            let stop_cursor = match position {
                Some(ControlWorkPosition::Stop(cursor)) => Some(cursor),
                _ => None,
            };
            let stop = self.stop_work_page(
                &revision.stop,
                stop_cursor,
                StopWorkPageLimits::new(limits.max_records, limits.max_bytes)?,
            )?;
            page.stop_records = stop.records;
            page.bytes = stop.bytes;
            page.next_cursor = stop.next_cursor.map(|cursor| ControlWorkCursor {
                revision: revision.clone(),
                position: ControlWorkPosition::Stop(cursor),
            });
        }
        #[cfg(test)]
        after_stop();
        if page.next_cursor.is_none() {
            let compaction_cursor = match position {
                Some(ControlWorkPosition::Compaction(cursor)) => cursor.as_ref(),
                _ => None,
            };
            let remaining_records = limits.max_records - page.len();
            let remaining_bytes = limits.max_bytes - page.bytes;
            let probe = remaining_records == 0 || remaining_bytes == 0;
            let compaction_limits = if probe {
                CompactionWorkPageLimits::new(1, 1)?
            } else {
                CompactionWorkPageLimits::new(remaining_records, remaining_bytes)?
            };
            let next = match self.compaction_work_page(
                &revision.compaction,
                compaction_cursor,
                compaction_limits,
            ) {
                Ok(compaction) => {
                    page.compaction_records = compaction.records;
                    page.bytes += compaction.bytes;
                    compaction
                        .next_cursor
                        .map(|cursor| ControlWorkPosition::Compaction(Some(cursor)))
                }
                Err(CompactionWorkError::ByteLimit) if !page.is_empty() => {
                    Some(ControlWorkPosition::Compaction(compaction_cursor.cloned()))
                }
                Err(error) => return Err(error.into()),
            };
            page.next_cursor = next.map(|position| ControlWorkCursor {
                revision: revision.clone(),
                position,
            });
        }
        self.validate_control_work_revision(revision)?;
        Ok(page)
    }
}

#[cfg(all(test, feature = "test-faults"))]
mod tests {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/unit/control_work_pages.rs"
    ));
}
