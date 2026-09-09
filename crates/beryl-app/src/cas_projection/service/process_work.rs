use std::collections::BTreeMap;

use super::ProjectionConnectionService;
use crate::{
    cas_projection::{ProjectionCancellationToken, ScheduledExecutionSessions},
    lifecycle_attention::{LifecycleAttentionRecord, ProcessLifecycleAttentionPool},
};
use beryl_model::SyndicThreadId;
use syndic_storage::ThreadCatalogSummaryPreparation;

mod durable;
mod live;
mod required;
pub(in crate::cas_projection) use required::RequiredSessionWork;
mod selection;
mod types;
pub use types::*;

#[derive(Default)]
struct LiveFacts {
    work: ProcessWorkFacts,
    attention: Vec<LifecycleAttentionRecord>,
}

pub struct ProcessWorkInventory<'a> {
    service: super::work_sources::ProcessWorkRead,
    sessions: &'a ScheduledExecutionSessions,
    attention: &'a ProcessLifecycleAttentionPool,
}

impl ProjectionConnectionService {
    pub fn process_work_inventory<'a>(
        &'a self,
        sessions: &'a ScheduledExecutionSessions,
        attention: &'a ProcessLifecycleAttentionPool,
    ) -> ProcessWorkInventory<'a> {
        ProcessWorkInventory {
            service: self.work_read(),
            sessions,
            attention,
        }
    }
}

impl ProcessWorkInventory<'_> {
    fn home(&self) -> Result<&beryl_home_store::HomeStore, ProcessWorkError> {
        self.service.home.as_deref().ok_or(ProcessWorkError::Closed)
    }

    pub fn revision(&self) -> Result<ProcessWorkRevision, ProcessWorkError> {
        let revision = ProcessWorkRevision {
            work: self.service.required_work_revision(self.sessions)?,
            attention: self.attention.work_revision()?,
        };
        self.validate_revision(&revision)?;
        Ok(revision)
    }

    pub fn validate_revision(
        &self,
        revision: &ProcessWorkRevision,
    ) -> Result<(), ProcessWorkError> {
        self.service
            .validate_required_work_revision(self.sessions, &revision.work)?;
        self.attention.validate_work_revision(&revision.attention)?;
        Ok(())
    }

    pub fn page(
        &self,
        revision: &ProcessWorkRevision,
        cursor: Option<&ProcessWorkCursor>,
        limits: ProcessWorkPageLimits,
        cancellation: &ProjectionCancellationToken,
    ) -> Result<ProcessWorkPage, ProcessWorkError> {
        self.collect_page(
            revision,
            cursor,
            limits,
            cancellation,
            #[cfg(test)]
            || {},
        )
    }

    fn collect_page(
        &self,
        revision: &ProcessWorkRevision,
        cursor: Option<&ProcessWorkCursor>,
        limits: ProcessWorkPageLimits,
        cancellation: &ProjectionCancellationToken,
        #[cfg(test)] after_scan: impl FnOnce(),
    ) -> Result<ProcessWorkPage, ProcessWorkError> {
        check_cancelled(cancellation)?;
        self.validate_revision(revision)?;
        if cursor.is_some_and(|cursor| &cursor.revision != revision) {
            return Err(ProcessWorkError::ForeignCursor);
        }
        let live = self.live_facts(revision, cancellation)?;
        let mut selected = selection::Selection::new(limits, cursor.map(|cursor| cursor.after));
        let mut total_threads = 0_u64;
        self.scan_threads(revision, live, cancellation, |thread_id, facts| {
            check_cancelled(cancellation)?;
            total_threads = total_threads
                .checked_add(1)
                .ok_or(ProcessWorkError::CountOverflow)?;
            let prepared = self
                .service
                .storage
                .prepare_thread_catalog_summary(self.home()?, thread_id)?
                .ok_or(ProcessWorkError::MissingMetadata)?;
            let summary = match &prepared {
                ThreadCatalogSummaryPreparation::ExactCurrent(current) => current.summary(),
                ThreadCatalogSummaryPreparation::PreparedReplacement(replacement) => {
                    replacement.replacement()
                }
            };
            selected.insert(ProcessWorkRecord {
                thread_id,
                title: summary.title().cloned(),
                execution: summary.execution().clone(),
                last_activity_at: summary.last_activity_at(),
                facts: facts.work,
                attention: facts.attention,
            });
            Ok(())
        })?;
        #[cfg(test)]
        after_scan();
        self.validate_revision(revision)?;
        check_cancelled(cancellation)?;
        selected.finish(revision.clone(), total_threads)
    }
}

fn check_cancelled(cancellation: &ProjectionCancellationToken) -> Result<(), ProcessWorkError> {
    if cancellation.is_cancelled() {
        Err(ProcessWorkError::Cancelled)
    } else {
        Ok(())
    }
}

type LiveMap = BTreeMap<SyndicThreadId, LiveFacts>;

#[cfg(all(test, feature = "test-faults"))]
mod tests {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/unit/process_work_inventory.rs"
    ));
}
