use std::sync::Arc;

use super::ProjectionConnectionService;
use crate::cas_projection::stop_work::{
    StopWorkCursor, StopWorkError, StopWorkPage, StopWorkPageBuilder, StopWorkPageLimits,
    StopWorkRevision,
};

impl ProjectionConnectionService {
    fn check_stop_work_open(&self) -> Result<(), StopWorkError> {
        if self.settled || !self.command_authorizer.is_open() {
            Err(StopWorkError::Closed)
        } else {
            Ok(())
        }
    }

    fn check_stop_work_owner(&self, revision: &StopWorkRevision) -> Result<(), StopWorkError> {
        if !Arc::ptr_eq(&revision.owner, self.connections.work_owner())
            || revision.home_id != self.home_id
            || revision.home_generation != self.home_generation
            || revision.service_generation != self.service_generation
        {
            return Err(StopWorkError::ForeignRevision);
        }
        self.check_stop_work_open()
    }

    pub fn stop_work_revision(&self) -> Result<StopWorkRevision, StopWorkError> {
        self.check_stop_work_open()?;
        let stamp = self.stop_coordinator.work_revision()?;
        self.check_stop_work_open()?;
        Ok(StopWorkRevision {
            owner: Arc::clone(self.connections.work_owner()),
            home_id: self.home_id,
            home_generation: self.home_generation,
            service_generation: self.service_generation,
            stamp,
        })
    }

    pub fn validate_stop_work_revision(
        &self,
        revision: &StopWorkRevision,
    ) -> Result<(), StopWorkError> {
        self.check_stop_work_owner(revision)?;
        if self.stop_coordinator.work_revision()? != revision.stamp {
            return Err(StopWorkError::StaleRevision);
        }
        self.check_stop_work_open()
    }

    pub fn stop_work_page(
        &self,
        revision: &StopWorkRevision,
        cursor: Option<&StopWorkCursor>,
        limits: StopWorkPageLimits,
    ) -> Result<StopWorkPage, StopWorkError> {
        self.check_stop_work_owner(revision)?;
        if cursor.is_some_and(|cursor| &cursor.revision != revision) {
            return Err(StopWorkError::ForeignRevision);
        }
        let mut page = StopWorkPageBuilder::new(cursor.map(|cursor| &cursor.after), limits);
        self.stop_coordinator
            .collect_work_records(revision.stamp, &mut page)?;
        self.validate_stop_work_revision(revision)?;
        Ok(page.finish(revision.clone()))
    }
}
