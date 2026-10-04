use super::*;
use crate::catalog_projection::{
    CatalogProjectionBuildError, project_facts, validate_execution_binding,
};
use beryl_state::{
    BerylState, CatalogClaimKind, CatalogClaimSummary, CatalogNormalizedQuery, ThreadClaimState,
};
use std::sync::{Arc, Weak};

#[derive(Clone)]
pub struct ProcessWorkReader {
    sources: super::super::work_sources::ProcessWorkSources,
    sessions: crate::cas_projection::process_sessions::WeakScheduledExecutionSessions,
    attention: Weak<ProcessLifecycleAttentionPool>,
}

impl ProjectionConnectionService {
    pub fn process_work_reader(
        &self,
        sessions: &ScheduledExecutionSessions,
        attention: &Arc<ProcessLifecycleAttentionPool>,
    ) -> ProcessWorkReader {
        ProcessWorkReader {
            sources: self.work_sources(),
            sessions: sessions.downgrade(),
            attention: Arc::downgrade(attention),
        }
    }
}

impl ProcessWorkReader {
    fn read<T>(
        &self,
        read: impl FnOnce(ProcessWorkInventory<'_>) -> Result<T, ProcessWorkError>,
    ) -> Result<T, ProcessWorkError> {
        let sessions = self.sessions.upgrade().ok_or(ProcessWorkError::Closed)?;
        let attention = self.attention.upgrade().ok_or(ProcessWorkError::Closed)?;
        read(ProcessWorkInventory {
            service: self.sources.read()?,
            sessions: &sessions,
            attention: &attention,
        })
    }

    pub fn query_revision(
        &self,
        state: &BerylState,
    ) -> Result<ProcessWorkQueryRevision, ProcessWorkError> {
        self.read(|inventory| inventory.query_revision(state))
    }

    pub fn query_page(
        &self,
        state: &BerylState,
        revision: &ProcessWorkQueryRevision,
        query: &CatalogNormalizedQuery,
        logical_start: u64,
        limits: ProcessWorkPageLimits,
        cancellation: &ProjectionCancellationToken,
    ) -> Result<ProcessWorkQueryPage, ProcessWorkError> {
        self.read(|inventory| {
            inventory.query_page(state, revision, query, logical_start, limits, cancellation)
        })
    }

    pub fn query_position(
        &self,
        state: &BerylState,
        revision: &ProcessWorkQueryRevision,
        query: &CatalogNormalizedQuery,
        thread_id: SyndicThreadId,
        cancellation: &ProjectionCancellationToken,
    ) -> Result<Option<u64>, ProcessWorkError> {
        self.read(|inventory| {
            inventory.query_position(state, revision, query, thread_id, cancellation)
        })
    }
}

impl ProcessWorkInventory<'_> {
    pub fn query_revision(
        &self,
        state: &BerylState,
    ) -> Result<ProcessWorkQueryRevision, ProcessWorkError> {
        let home = self.home()?;
        let revision = ProcessWorkQueryRevision {
            home: home.home_revision()?,
            work: self.revision()?,
        };
        state.runtime_roots().revision(home)?;
        state.session().revision(home)?;
        self.validate_query_revision(&revision)?;
        Ok(revision)
    }

    fn validate_query_revision(
        &self,
        revision: &ProcessWorkQueryRevision,
    ) -> Result<(), ProcessWorkError> {
        self.validate_revision(&revision.work)?;
        if self.home()?.home_revision()? != revision.home {
            return Err(ProcessWorkError::StaleRevision);
        }
        Ok(())
    }

    fn scan_query(
        &self,
        state: &BerylState,
        revision: &ProcessWorkQueryRevision,
        query: &CatalogNormalizedQuery,
        cancellation: &ProjectionCancellationToken,
        mut visit: impl FnMut(ProcessWorkQueryRecord) -> Result<(), ProcessWorkError>,
    ) -> Result<(u64, u64, u64), ProcessWorkError> {
        check_cancelled(cancellation)?;
        self.validate_query_revision(revision)?;
        let home = self.home()?;
        state.runtime_roots().revision(home)?;
        state.session().revision(home)?;
        let live = self.live_facts(&revision.work, cancellation)?;
        let mut total = 0_u64;
        let mut matched = 0_u64;
        let mut attention_threads = 0_u64;
        self.scan_threads(&revision.work, live, cancellation, |thread_id, facts| {
            check_cancelled(cancellation)?;
            total = total
                .checked_add(1)
                .ok_or(ProcessWorkError::CountOverflow)?;
            if !facts.attention.is_empty() {
                attention_threads = attention_threads
                    .checked_add(1)
                    .ok_or(ProcessWorkError::CountOverflow)?;
            }
            let prepared = self
                .service
                .storage
                .prepare_thread_catalog_summary(home, thread_id)?
                .ok_or(ProcessWorkError::MissingMetadata)?;
            let summary = match &prepared {
                ThreadCatalogSummaryPreparation::ExactCurrent(current) => current.summary(),
                ThreadCatalogSummaryPreparation::PreparedReplacement(replacement) => {
                    replacement.replacement()
                }
            };
            let runtime = state
                .runtime_roots()
                .catalog_source(
                    home,
                    summary.execution().runtime_id(),
                    summary.execution().root_id(),
                )
                .map_err(CatalogProjectionBuildError::from)?;
            validate_execution_binding(summary, &runtime)?;
            let source = state
                .session()
                .thread_claim_catalog_source(home, thread_id)
                .map_err(CatalogProjectionBuildError::from)?;
            let claim = source.claim();
            let claim_summary = claim.map_or(CatalogClaimSummary::Unclaimed, |claim| {
                CatalogClaimSummary::claimed(
                    claim.window_id(),
                    match claim.state() {
                        ThreadClaimState::Active => CatalogClaimKind::Active,
                        ThreadClaimState::Restoring => CatalogClaimKind::Restoring,
                    },
                )
            });
            let catalog = project_facts(summary, &runtime, claim_summary)?;
            if catalog.search().matches(query) {
                matched = matched
                    .checked_add(1)
                    .ok_or(ProcessWorkError::CountOverflow)?;
                visit(ProcessWorkQueryRecord {
                    thread_id,
                    catalog,
                    claim,
                    facts: facts.work,
                    attention: facts.attention,
                })?;
            }
            Ok(())
        })?;
        self.validate_query_revision(revision)?;
        check_cancelled(cancellation)?;
        Ok((total, matched, attention_threads))
    }

    pub fn query_page(
        &self,
        state: &BerylState,
        revision: &ProcessWorkQueryRevision,
        query: &CatalogNormalizedQuery,
        logical_start: u64,
        limits: ProcessWorkPageLimits,
        cancellation: &ProjectionCancellationToken,
    ) -> Result<ProcessWorkQueryPage, ProcessWorkError> {
        let mut after = None;
        let mut skipped = 0_u64;
        loop {
            check_cancelled(cancellation)?;
            let remaining = logical_start - skipped;
            let page_limits = if remaining == 0 {
                limits
            } else {
                ProcessWorkPageLimits {
                    max_records: limits
                        .max_records
                        .min(usize::try_from(remaining).unwrap_or(usize::MAX)),
                    max_bytes: limits.max_bytes,
                }
            };
            let mut selected = QuerySelection::new(page_limits, after);
            let (total_threads, matched_threads, attention_threads) =
                self.scan_query(state, revision, query, cancellation, |row| {
                    selected.insert(row);
                    Ok(())
                })?;
            let (records, bytes, has_more) = selected.finish()?;
            if remaining == 0 || records.is_empty() {
                return Ok(ProcessWorkQueryPage {
                    revision: revision.clone(),
                    query: query.clone(),
                    logical_start,
                    total_threads,
                    matched_threads,
                    attention_threads,
                    records,
                    bytes,
                });
            }
            skipped = skipped
                .checked_add(records.len() as u64)
                .ok_or(ProcessWorkError::CountOverflow)?;
            if !has_more && skipped < logical_start {
                return Ok(ProcessWorkQueryPage {
                    revision: revision.clone(),
                    query: query.clone(),
                    logical_start,
                    total_threads,
                    matched_threads,
                    attention_threads,
                    records: Vec::new(),
                    bytes: 0,
                });
            }
            after = records.last().map(ProcessWorkQueryRecord::key);
        }
    }

    pub fn query_position(
        &self,
        state: &BerylState,
        revision: &ProcessWorkQueryRevision,
        query: &CatalogNormalizedQuery,
        thread_id: SyndicThreadId,
        cancellation: &ProjectionCancellationToken,
    ) -> Result<Option<u64>, ProcessWorkError> {
        let mut key = None;
        self.scan_query(state, revision, query, cancellation, |row| {
            if row.thread_id == thread_id {
                key = Some(row.key());
            }
            Ok(())
        })?;
        let Some(key) = key else {
            return Ok(None);
        };
        let mut position = 0_u64;
        self.scan_query(state, revision, query, cancellation, |row| {
            if row.key() < key {
                position = position
                    .checked_add(1)
                    .ok_or(ProcessWorkError::CountOverflow)?;
            }
            Ok(())
        })?;
        Ok(Some(position))
    }
}

struct QuerySelection {
    limits: ProcessWorkPageLimits,
    after: Option<types::SortKey>,
    records: Vec<ProcessWorkQueryRecord>,
    bytes: usize,
    first_omitted: Option<types::SortKey>,
}

impl QuerySelection {
    fn new(limits: ProcessWorkPageLimits, after: Option<types::SortKey>) -> Self {
        Self {
            limits,
            after,
            records: Vec::new(),
            bytes: 0,
            first_omitted: None,
        }
    }

    fn insert(&mut self, row: ProcessWorkQueryRecord) {
        let key = row.key();
        if self.after.is_some_and(|after| key <= after)
            || self.first_omitted.is_some_and(|omitted| key >= omitted)
        {
            return;
        }
        let index = self.records.partition_point(|row| row.key() < key);
        self.bytes += row.bytes();
        self.records.insert(index, row);
        while self.records.len() > self.limits.max_records || self.bytes > self.limits.max_bytes {
            let row = self
                .records
                .pop()
                .expect("an over-budget prefix contains a row");
            self.bytes -= row.bytes();
            self.first_omitted = Some(row.key());
        }
    }

    fn finish(self) -> Result<(Vec<ProcessWorkQueryRecord>, usize, bool), ProcessWorkError> {
        if self.records.is_empty() && self.first_omitted.is_some() {
            return Err(ProcessWorkError::ByteLimit);
        }
        Ok((self.records, self.bytes, self.first_omitted.is_some()))
    }
}
