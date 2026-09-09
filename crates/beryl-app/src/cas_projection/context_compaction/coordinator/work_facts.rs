use super::ContextCompactionCoordinator;
use crate::cas_projection::compaction_work::{
    CompactionWorkError, CompactionWorkPage, CompactionWorkPageLimits, CompactionWorkRevision,
};

impl ContextCompactionCoordinator {
    pub(in crate::cas_projection) fn work_revision(&self) -> Result<u64, CompactionWorkError> {
        if self.operations.is_poisoned() || self.stop.continuation_work_is_poisoned() {
            self.custody.source.invalidate();
            return Err(CompactionWorkError::SourceUnavailable);
        }
        self.custody.source.revision()
    }

    pub(in crate::cas_projection) fn work_page(
        &self,
        revision: CompactionWorkRevision,
        after: Option<u64>,
        limits: CompactionWorkPageLimits,
    ) -> Result<CompactionWorkPage, CompactionWorkError> {
        if self.work_revision()? != revision.stamp {
            return Err(CompactionWorkError::StaleRevision);
        }
        Ok(self
            .custody
            .source
            .page(revision.stamp, after, limits)?
            .finish(revision))
    }
}
