use std::{cmp::Reverse, sync::Arc};

use beryl_home_store::{CursorReadLimits, HomeServiceReference};
use beryl_model::{HomeRevision, RootId, RuntimeId};
use beryl_state::{BerylState, CatalogNormalizedQuery, RootRecord, RuntimeRecord};

use crate::cas_projection::ProjectionCancellationToken;

const SCAN_ROWS: usize = 32;
const PAGE_BYTES: usize = 16 * 1024 * 1024;

#[derive(Clone)]
pub(crate) struct RuntimeSetupCatalog {
    pub home: Arc<HomeServiceReference>,
    pub state: BerylState,
}

pub(crate) struct SetupPage<T> {
    pub revision: HomeRevision,
    pub total: usize,
    pub rows: Vec<T>,
}

#[derive(Clone)]
pub(crate) struct SetupRuntimeRow {
    pub runtime: RuntimeRecord,
    pub root_count: usize,
}

#[derive(Clone)]
pub(crate) struct SetupRootRow {
    pub runtime: RuntimeRecord,
    pub root: RootRecord,
    pub thread_count: usize,
}

type RootOrder = (Reverse<u64>, RootId);

impl RuntimeSetupCatalog {
    fn check(
        &self,
        revision: HomeRevision,
        cancellation: &ProjectionCancellationToken,
    ) -> Result<(), String> {
        if cancellation.is_cancelled() {
            return Err("Runtime/root collection read cancelled.".into());
        }
        if self.home.home_revision().map_err(error)? != revision {
            return Err("Runtime/root collection changed during read.".into());
        }
        Ok(())
    }

    fn runtimes(
        &self,
        revision: HomeRevision,
        cancellation: &ProjectionCancellationToken,
        mut visit: impl FnMut(&RuntimeRecord) -> Result<(), String>,
    ) -> Result<(), String> {
        let mut after = None;
        loop {
            self.check(revision, cancellation)?;
            let page = self
                .state
                .runtime_roots()
                .list_runtimes(&self.home, after, limits())
                .map_err(error)?;
            for runtime in page.records() {
                self.check(revision, cancellation)?;
                visit(runtime)?;
            }
            if !page.has_more() {
                break;
            }
            after = Some(
                page.records()
                    .last()
                    .ok_or("Runtime cursor did not progress.")?
                    .runtime_id(),
            );
        }
        self.check(revision, cancellation)
    }

    fn roots(
        &self,
        revision: HomeRevision,
        runtime: RuntimeId,
        cancellation: &ProjectionCancellationToken,
        mut visit: impl FnMut(&RootRecord) -> Result<(), String>,
    ) -> Result<(), String> {
        let mut after = None;
        loop {
            self.check(revision, cancellation)?;
            let page = self
                .state
                .runtime_roots()
                .list_roots(&self.home, runtime, after, limits())
                .map_err(error)?;
            for root in page.records() {
                self.check(revision, cancellation)?;
                visit(root)?;
            }
            if !page.has_more() {
                break;
            }
            after = Some(
                page.records()
                    .last()
                    .ok_or("Root cursor did not progress.")?
                    .root_id(),
            );
        }
        self.check(revision, cancellation)
    }

    pub fn runtime_page(
        &self,
        revision: HomeRevision,
        start: usize,
        count: usize,
        cancellation: &ProjectionCancellationToken,
    ) -> Result<SetupPage<SetupRuntimeRow>, String> {
        if count == 0 || count > SCAN_ROWS {
            return Err("Invalid runtime page bound.".into());
        }
        let end = start
            .checked_add(count)
            .ok_or("Runtime position overflow.")?;
        let mut total = 0_usize;
        let mut rows = Vec::new();
        self.runtimes(revision, cancellation, |runtime| {
            if (start..end).contains(&total) {
                let mut root_count = 0_usize;
                self.roots(revision, runtime.runtime_id(), cancellation, |_| {
                    root_count = root_count.checked_add(1).ok_or("Root count overflow.")?;
                    Ok(())
                })?;
                rows.push(SetupRuntimeRow {
                    runtime: runtime.clone(),
                    root_count,
                });
            }
            total = total.checked_add(1).ok_or("Runtime count overflow.")?;
            Ok(())
        })?;
        Ok(SetupPage {
            revision,
            total,
            rows,
        })
    }

    pub fn root_page(
        &self,
        revision: HomeRevision,
        scope: Option<RuntimeId>,
        query: &CatalogNormalizedQuery,
        start: usize,
        count: usize,
        cancellation: &ProjectionCancellationToken,
    ) -> Result<SetupPage<SetupRootRow>, String> {
        if count == 0 || count > SCAN_ROWS {
            return Err("Invalid root page bound.".into());
        }
        let mut after = None;
        let mut skipped = 0_usize;
        loop {
            let remaining = start - skipped;
            let batch = if remaining == 0 {
                count
            } else {
                remaining.min(SCAN_ROWS)
            };
            let mut selected: Vec<(RootOrder, RuntimeRecord, RootRecord)> = Vec::new();
            let mut total = 0_usize;
            self.runtimes(revision, cancellation, |runtime| {
                if scope.is_some_and(|id| id != runtime.runtime_id()) {
                    return Ok(());
                }
                self.roots(revision, runtime.runtime_id(), cancellation, |root| {
                    if !matches(query, runtime, root)? {
                        return Ok(());
                    }
                    total = total.checked_add(1).ok_or("Root count overflow.")?;
                    let key = root_order(root);
                    if after.is_some_and(|after| key <= after) {
                        return Ok(());
                    }
                    let index = selected.partition_point(|row| row.0 < key);
                    if index < batch {
                        selected.insert(index, (key, runtime.clone(), root.clone()));
                        if selected.len() > batch {
                            selected.pop();
                        }
                    }
                    Ok(())
                })
            })?;
            if remaining == 0 || selected.is_empty() {
                let mut rows = Vec::with_capacity(selected.len());
                for (_, runtime, root) in selected {
                    let thread_count = self.thread_count(revision, root.root_id(), cancellation)?;
                    rows.push(SetupRootRow {
                        runtime,
                        root,
                        thread_count,
                    });
                }
                self.check(revision, cancellation)?;
                return Ok(SetupPage {
                    revision,
                    total,
                    rows,
                });
            }
            skipped = skipped
                .checked_add(selected.len())
                .ok_or("Root position overflow.")?;
            after = selected.last().map(|row| row.0);
        }
    }

    fn thread_count(
        &self,
        revision: HomeRevision,
        root: RootId,
        cancellation: &ProjectionCancellationToken,
    ) -> Result<usize, String> {
        let mut after = None;
        let mut total = 0_usize;
        loop {
            self.check(revision, cancellation)?;
            let page = self
                .state
                .catalog()
                .recency_page(&self.home, after, limits())
                .map_err(error)?;
            for row in page.rows() {
                if row.facts().execution().root_id() == root {
                    total = total.checked_add(1).ok_or("Thread count overflow.")?;
                }
            }
            if !page.has_more() {
                break;
            }
            after = Some(
                page.next_after()
                    .ok_or("Catalog cursor did not progress.")?,
            );
        }
        self.check(revision, cancellation)?;
        Ok(total)
    }
}

fn limits() -> CursorReadLimits {
    CursorReadLimits::new(SCAN_ROWS, PAGE_BYTES).expect("fixed nonzero setup page limits")
}

fn error(value: impl std::fmt::Display) -> String {
    value.to_string()
}

fn root_order(root: &RootRecord) -> RootOrder {
    (
        Reverse(root.last_activity_at().map_or(0, |time| time.get())),
        root.root_id(),
    )
}

fn matches(
    query: &CatalogNormalizedQuery,
    runtime: &RuntimeRecord,
    root: &RootRecord,
) -> Result<bool, String> {
    if query.is_empty() {
        return Ok(true);
    }
    for field in [
        runtime.environment_label(),
        runtime.canonical_executable().as_str(),
        root.display_path().as_str(),
    ] {
        if CatalogNormalizedQuery::new(field)
            .map_err(error)?
            .as_str()
            .contains(query.as_str())
        {
            return Ok(true);
        }
    }
    Ok(false)
}
