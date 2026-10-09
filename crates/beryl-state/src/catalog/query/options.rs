use std::cmp::Reverse;

use super::*;

const OPTION_ROW_BYTES: usize = 2 * (132 * 1024 + 4 + 32)
    + std::mem::size_of::<CatalogRootRow>()
    + std::mem::size_of::<RootOrder>();
type RootOrder = (Reverse<u64>, RootId);

impl CatalogQueryOwner {
    pub fn runtime_page(
        &mut self,
        store: &HomeStore,
        token: &CatalogQueryToken,
        search: &CatalogNormalizedQuery,
        start: u64,
        limit: CatalogQueryPageLimit,
        cancel: &CommandCancellation,
    ) -> Result<CatalogOptionPage<CatalogRuntimeRow>, CatalogQueryError> {
        self.request(store)?;
        let entry = &self.entries[self.entry_index(token)?];
        let bound = option_bound(limit)?;
        let end = start
            .checked_add(bound as u64)
            .ok_or(CatalogQueryError::CountExhausted)?;
        let mut count = 0u64;
        let mut rows = Vec::new();
        self.walk_runtimes(store, entry, cancel, |runtime| {
            if !option_matches(search, runtime, None)? {
                return Ok(());
            }
            if (start..end).contains(&count) {
                let mut root_count = 0u64;
                self.walk_roots(store, entry, runtime.runtime_id(), cancel, |_| {
                    increment(&mut root_count)
                })?;
                rows.push(CatalogRuntimeRow {
                    runtime: runtime.clone(),
                    root_count,
                });
            }
            increment(&mut count)
        })?;
        check_cancel(cancel)?;
        Ok(CatalogOptionPage {
            revision: entry.frozen.home_revision(),
            count,
            offset: start,
            bytes: rows.len() * OPTION_ROW_BYTES,
            rows,
        })
    }

    pub fn root_page(
        &mut self,
        store: &HomeStore,
        token: &CatalogQueryToken,
        runtime_id: RuntimeId,
        search: &CatalogNormalizedQuery,
        start: u64,
        limit: CatalogQueryPageLimit,
        cancel: &CommandCancellation,
    ) -> Result<CatalogOptionPage<CatalogRootRow>, CatalogQueryError> {
        self.request(store)?;
        let entry = &self.entries[self.entry_index(token)?];
        let runtime = self.option_runtime(store, entry, runtime_id)?;
        let bound = option_bound(limit)?;
        let mut after = None;
        let mut skipped = 0u64;
        loop {
            let remaining = start - skipped;
            let batch = if remaining == 0 {
                bound
            } else {
                remaining.min(bound as u64) as usize
            };
            let mut selected: Vec<(RootOrder, RootRecord)> = Vec::with_capacity(batch);
            let mut count = 0u64;
            self.walk_roots(store, entry, runtime_id, cancel, |root| {
                if !option_matches(search, &runtime, Some(root))? {
                    return Ok(());
                }
                increment(&mut count)?;
                let key = root_order(root);
                if after.is_some_and(|after| key <= after) {
                    return Ok(());
                }
                let position = selected.partition_point(|row| row.0 < key);
                if position < batch {
                    selected.insert(position, (key, root.clone()));
                    if selected.len() > batch {
                        selected.pop();
                    }
                }
                Ok(())
            })?;
            if remaining == 0 || selected.is_empty() {
                let mut rows = Vec::with_capacity(selected.len());
                for (_, root) in selected {
                    let mut thread_count = 0u64;
                    self.walk_recency(store, entry, None, cancel, |row| {
                        let execution = row.facts().execution();
                        if execution.runtime_id() == runtime_id
                            && execution.root_id() == root.root_id()
                        {
                            increment(&mut thread_count)?;
                        }
                        Ok(true)
                    })?;
                    rows.push(CatalogRootRow {
                        runtime: runtime.clone(),
                        root,
                        thread_count,
                    });
                }
                check_cancel(cancel)?;
                return Ok(CatalogOptionPage {
                    revision: entry.frozen.home_revision(),
                    count,
                    offset: start,
                    bytes: rows.len() * OPTION_ROW_BYTES,
                    rows,
                });
            }
            skipped = skipped
                .checked_add(selected.len() as u64)
                .ok_or(CatalogQueryError::CountExhausted)?;
            after = selected.last().map(|row| row.0);
        }
    }

    pub fn runtime_position(
        &mut self,
        store: &HomeStore,
        token: &CatalogQueryToken,
        search: &CatalogNormalizedQuery,
        runtime_id: RuntimeId,
        cancel: &CommandCancellation,
    ) -> Result<Option<u64>, CatalogQueryError> {
        self.request(store)?;
        let entry = &self.entries[self.entry_index(token)?];
        let mut position = 0u64;
        let mut found = None;
        self.walk_runtimes(store, entry, cancel, |runtime| {
            if option_matches(search, runtime, None)? {
                if runtime.runtime_id() == runtime_id {
                    found = Some(position);
                }
                increment(&mut position)?;
            }
            Ok(())
        })?;
        check_cancel(cancel)?;
        Ok(found)
    }

    pub fn root_position(
        &mut self,
        store: &HomeStore,
        token: &CatalogQueryToken,
        runtime_id: RuntimeId,
        search: &CatalogNormalizedQuery,
        root_id: RootId,
        cancel: &CommandCancellation,
    ) -> Result<Option<u64>, CatalogQueryError> {
        self.request(store)?;
        let entry = &self.entries[self.entry_index(token)?];
        let runtime = self.option_runtime(store, entry, runtime_id)?;
        check_cancel(cancel)?;
        let Some(root) = self
            .runtime_roots
            .frozen_root(store, &entry.frozen, root_id)?
        else {
            return Ok(None);
        };
        if root.runtime_id() != runtime_id || !option_matches(search, &runtime, Some(&root))? {
            return Ok(None);
        }
        let key = root_order(&root);
        let mut position = 0u64;
        self.walk_roots(store, entry, runtime_id, cancel, |row| {
            if root_order(row) < key && option_matches(search, &runtime, Some(row))? {
                increment(&mut position)?;
            }
            Ok(())
        })?;
        check_cancel(cancel)?;
        Ok(Some(position))
    }

    fn option_runtime(
        &self,
        store: &HomeStore,
        entry: &Entry,
        runtime_id: RuntimeId,
    ) -> Result<RuntimeRecord, CatalogQueryError> {
        Ok(self
            .runtime_roots
            .frozen_runtime(store, &entry.frozen, runtime_id)?
            .ok_or(RuntimeRootCatalogSourceError::RuntimeMissing { runtime_id })?)
    }

    pub(super) fn runtime_environment_count(
        &self,
        store: &HomeStore,
        entry: &Entry,
        environment: &str,
        cancel: &CommandCancellation,
    ) -> Result<u64, CatalogQueryError> {
        let mut count = 0u64;
        self.walk_runtimes(store, entry, cancel, |runtime| {
            if runtime.environment_label() == environment {
                increment(&mut count)?;
            }
            Ok(())
        })?;
        Ok(count)
    }

    fn walk_runtimes(
        &self,
        store: &HomeStore,
        entry: &Entry,
        cancel: &CommandCancellation,
        mut visit: impl FnMut(&RuntimeRecord) -> Result<(), CatalogQueryError>,
    ) -> Result<(), CatalogQueryError> {
        let mut after = None;
        loop {
            check_cancel(cancel)?;
            let page = self.runtime_roots.frozen_runtimes_page(
                store,
                &entry.frozen,
                after,
                scan_limits(),
            )?;
            for row in page.records() {
                check_cancel(cancel)?;
                visit(row)?;
            }
            if !page.has_more() {
                return Ok(());
            }
            let next = page
                .records()
                .last()
                .ok_or(CatalogQueryError::Structural(
                    "runtime scanner made no progress",
                ))?
                .runtime_id();
            if after == Some(next) {
                return Err(CatalogQueryError::Structural(
                    "runtime scanner repeated cursor",
                ));
            }
            after = Some(next);
        }
    }

    fn walk_roots(
        &self,
        store: &HomeStore,
        entry: &Entry,
        runtime: RuntimeId,
        cancel: &CommandCancellation,
        mut visit: impl FnMut(&RootRecord) -> Result<(), CatalogQueryError>,
    ) -> Result<(), CatalogQueryError> {
        let mut after = None;
        loop {
            check_cancel(cancel)?;
            let page = self.runtime_roots.frozen_roots_page(
                store,
                &entry.frozen,
                runtime,
                after,
                scan_limits(),
            )?;
            for row in page.records() {
                check_cancel(cancel)?;
                visit(row)?;
            }
            if !page.has_more() {
                return Ok(());
            }
            let next = page
                .records()
                .last()
                .ok_or(CatalogQueryError::Structural(
                    "root scanner made no progress",
                ))?
                .root_id();
            if after == Some(next) {
                return Err(CatalogQueryError::Structural(
                    "root scanner repeated cursor",
                ));
            }
            after = Some(next);
        }
    }
}

fn increment(count: &mut u64) -> Result<(), CatalogQueryError> {
    *count = count
        .checked_add(1)
        .ok_or(CatalogQueryError::CountExhausted)?;
    Ok(())
}
fn option_bound(limit: CatalogQueryPageLimit) -> Result<usize, CatalogQueryError> {
    let bound = limit.max_items().min(limit.max_bytes() / OPTION_ROW_BYTES);
    if bound == 0 {
        return Err(CatalogQueryError::Limit);
    }
    Ok(bound)
}
fn root_order(root: &RootRecord) -> RootOrder {
    (
        Reverse(root.last_activity_at().map_or(0, |time| time.get())),
        root.root_id(),
    )
}
fn option_matches(
    query: &CatalogNormalizedQuery,
    runtime: &RuntimeRecord,
    root: Option<&RootRecord>,
) -> Result<bool, CatalogQueryError> {
    if query.is_empty() {
        return Ok(true);
    }
    for field in [
        Some(runtime.environment_label()),
        Some(runtime.canonical_executable().as_str()),
        root.map(|root| root.display_path().as_str()),
    ]
    .into_iter()
    .flatten()
    {
        let normalized = super::super::normalization::normalize(
            "option search field",
            field,
            field.len().saturating_mul(18),
        )
        .map_err(|_| CatalogQueryError::Structural("option search normalization bound"))?;
        if normalized.contains(query.as_str()) {
            return Ok(true);
        }
    }
    Ok(false)
}
