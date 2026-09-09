use std::sync::Arc;

use super::ProjectionConnectionService;
use crate::cas_projection::compaction_work::{
    CompactionWorkCursor, CompactionWorkError, CompactionWorkPage, CompactionWorkPageLimits,
    CompactionWorkRevision,
};

impl ProjectionConnectionService {
    fn check_compaction_work_open(&self) -> Result<(), CompactionWorkError> {
        if self.settled || !self.command_authorizer.is_open() {
            Err(CompactionWorkError::Closed)
        } else {
            Ok(())
        }
    }

    fn check_compaction_work_owner(
        &self,
        revision: &CompactionWorkRevision,
    ) -> Result<(), CompactionWorkError> {
        if !Arc::ptr_eq(&revision.owner, self.connections.work_owner())
            || revision.home_id != self.home_id
            || revision.home_generation != self.home_generation
            || revision.service_generation != self.service_generation
        {
            return Err(CompactionWorkError::ForeignRevision);
        }
        self.check_compaction_work_open()
    }

    pub fn compaction_work_revision(&self) -> Result<CompactionWorkRevision, CompactionWorkError> {
        self.check_compaction_work_open()?;
        let stamp = self
            .context_compaction
            .as_ref()
            .ok_or(CompactionWorkError::Closed)?
            .work_revision()?;
        self.check_compaction_work_open()?;
        Ok(CompactionWorkRevision {
            owner: Arc::clone(self.connections.work_owner()),
            home_id: self.home_id,
            home_generation: self.home_generation,
            service_generation: self.service_generation,
            stamp,
        })
    }

    pub fn validate_compaction_work_revision(
        &self,
        revision: &CompactionWorkRevision,
    ) -> Result<(), CompactionWorkError> {
        self.check_compaction_work_owner(revision)?;
        if self
            .context_compaction
            .as_ref()
            .ok_or(CompactionWorkError::Closed)?
            .work_revision()?
            != revision.stamp
        {
            return Err(CompactionWorkError::StaleRevision);
        }
        self.check_compaction_work_open()
    }

    pub fn compaction_work_page(
        &self,
        revision: &CompactionWorkRevision,
        cursor: Option<&CompactionWorkCursor>,
        limits: CompactionWorkPageLimits,
    ) -> Result<CompactionWorkPage, CompactionWorkError> {
        self.check_compaction_work_owner(revision)?;
        if cursor.is_some_and(|cursor| &cursor.revision != revision) {
            return Err(CompactionWorkError::ForeignRevision);
        }
        let page = self
            .context_compaction
            .as_ref()
            .ok_or(CompactionWorkError::Closed)?
            .work_page(revision.clone(), cursor.map(|cursor| cursor.after), limits)?;
        self.validate_compaction_work_revision(revision)?;
        Ok(page)
    }
}
