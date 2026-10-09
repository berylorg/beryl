use super::*;

impl CatalogQueryOwner {
    pub(super) fn prepare(
        &mut self,
        store: &HomeStore,
        index: usize,
        limit: CatalogQueryPageLimit,
        cancel: &CommandCancellation,
    ) -> Result<CatalogQueryOpened, CatalogQueryError> {
        check_cancel(cancel)?;
        let entry = &self.entries[index];
        let revision = self.catalog.frozen_revision(store, &entry.frozen)?;
        self.runtime_roots.frozen_revision(store, &entry.frozen)?;
        let scope = self.scope_presentation(store, entry)?;
        let mut after = None;
        loop {
            check_cancel(cancel)?;
            let page =
                self.catalog
                    .frozen_primary_page(store, &entry.frozen, after, scan_limits())?;
            for row in page.records() {
                Self::require_current(row)?;
                self.presentation_source(store, entry, row)?;
            }
            if !page.has_more() {
                break;
            }
            let next = page
                .records()
                .last()
                .ok_or(CatalogQueryError::Structural(
                    "catalog primary scanner made no progress",
                ))?
                .thread_id();
            if after == Some(next) {
                return Err(CatalogQueryError::Structural(
                    "catalog primary scanner repeated its cursor",
                ));
            }
            after = Some(next);
        }
        let mut count = 0u64;
        self.walk_recency(store, entry, None, cancel, |row| {
            if entry.criteria.matches(&row) {
                count = count
                    .checked_add(1)
                    .ok_or(CatalogQueryError::CountExhausted)?;
            }
            Ok(true)
        })?;
        self.entries[index].revision = Some(revision);
        self.entries[index].count = count;
        let entry = &self.entries[index];
        let first = self.collect_page(store, entry, None, limit, cancel)?;
        check_cancel(cancel)?;
        Ok(CatalogQueryOpened {
            token: entry.token.clone(),
            count,
            revision,
            home_revision: entry.frozen.home_revision(),
            criteria: entry.criteria.clone(),
            scope,
            first,
        })
    }

    pub(super) fn collect_page(
        &self,
        store: &HomeStore,
        entry: &Entry,
        cursor: Option<&CatalogQueryCursor>,
        limit: CatalogQueryPageLimit,
        cancel: &CommandCancellation,
    ) -> Result<CatalogQueryPage, CatalogQueryError> {
        check_cancel(cancel)?;
        let offset = cursor.map_or(0, |cursor| cursor.offset);
        let mut rows = Vec::new();
        let mut bytes = 0usize;
        let mut last = None;
        self.walk_recency(
            store,
            entry,
            cursor.map(|cursor| cursor.after),
            cancel,
            |row| {
                if !entry.criteria.matches(&row) {
                    return Ok(true);
                }
                let charged = bytes
                    .checked_add(PRESENTATION_ROW_BYTES)
                    .ok_or(CatalogQueryError::Limit)?;
                if rows.len() >= limit.max_items() || charged > limit.max_bytes() {
                    if rows.is_empty() {
                        return Err(CatalogQueryError::Limit);
                    }
                    return Ok(false);
                }
                let runtime_root = self.presentation_source(store, entry, &row)?;
                let runtime_environment_count = self.runtime_environment_count(
                    store,
                    entry,
                    runtime_root.runtime().environment_label(),
                    cancel,
                )?;
                last = Some(row.recency_cursor());
                rows.push(CatalogQueryRow {
                    catalog: row,
                    runtime_root,
                    runtime_environment_count,
                });
                bytes = charged;
                Ok(true)
            },
        )?;
        let consumed = offset
            .checked_add(u64::try_from(rows.len()).map_err(|_| CatalogQueryError::CountExhausted)?)
            .ok_or(CatalogQueryError::CountExhausted)?;
        if consumed > entry.count {
            return Err(CatalogQueryError::Structural(
                "query page exceeds exact count",
            ));
        }
        let next = if consumed < entry.count {
            Some(CatalogQueryCursor {
                token: entry.token.clone(),
                after: last.ok_or(CatalogQueryError::Structural(
                    "query continuation has no matching row",
                ))?,
                offset: consumed,
            })
        } else {
            None
        };
        Ok(CatalogQueryPage {
            rows,
            offset,
            bytes,
            next,
        })
    }

    pub(super) fn walk_recency(
        &self,
        store: &HomeStore,
        entry: &Entry,
        mut after: Option<CatalogRecencyCursor>,
        cancel: &CommandCancellation,
        mut visit: impl FnMut(CatalogRow) -> Result<bool, CatalogQueryError>,
    ) -> Result<(), CatalogQueryError> {
        loop {
            check_cancel(cancel)?;
            let page =
                self.catalog
                    .frozen_recency_page(store, &entry.frozen, after, scan_limits())?;
            let next = page.next_after();
            let more = page.has_more();
            for row in page.rows() {
                check_cancel(cancel)?;
                Self::require_current(row)?;
                if !visit(row.clone())? {
                    return Ok(());
                }
            }
            if !more {
                return Ok(());
            }
            let next = next.ok_or(CatalogQueryError::Structural(
                "catalog recency scanner made no progress",
            ))?;
            if after == Some(next) {
                return Err(CatalogQueryError::Structural(
                    "catalog recency scanner repeated its cursor",
                ));
            }
            after = Some(next);
        }
    }

    fn require_current(row: &CatalogRow) -> Result<(), CatalogQueryError> {
        if row.freshness() != CatalogFreshness::Current {
            return Err(CatalogQueryError::StaleRow {
                thread_id: row.thread_id(),
            });
        }
        Ok(())
    }

    fn scope_presentation(
        &self,
        store: &HomeStore,
        entry: &Entry,
    ) -> Result<CatalogQueryScopePresentation, CatalogQueryError> {
        Ok(match entry.criteria.scope() {
            CatalogQueryScope::All => CatalogQueryScopePresentation::All,
            CatalogQueryScope::Runtime(runtime_id) => CatalogQueryScopePresentation::Runtime(
                self.runtime_roots
                    .frozen_runtime(store, &entry.frozen, runtime_id)?
                    .ok_or(RuntimeRootCatalogSourceError::RuntimeMissing { runtime_id })?,
            ),
            CatalogQueryScope::Root {
                runtime_id,
                root_id,
            } => CatalogQueryScopePresentation::Root(self.runtime_roots.frozen_catalog_source(
                store,
                &entry.frozen,
                runtime_id,
                root_id,
            )?),
        })
    }

    fn presentation_source(
        &self,
        store: &HomeStore,
        entry: &Entry,
        row: &CatalogRow,
    ) -> Result<RuntimeRootCatalogSource, CatalogQueryError> {
        let execution = row.facts().execution();
        let source = self.runtime_roots.frozen_catalog_source(
            store,
            &entry.frozen,
            execution.runtime_id(),
            execution.root_id(),
        )?;
        let runtime = source.runtime();
        let root = source.root();
        if runtime.revision() != row.sources().runtime()
            || root.revision() != row.sources().root()
            || runtime.environment_label() != execution.environment_label()
            || runtime.canonical_executable() != execution.configured_executable_path()
            || root.display_path() != execution.full_root_path()
            || runtime.availability().availability() != execution.availability().runtime()
            || root.availability().availability() != execution.availability().root()
        {
            return Err(CatalogQueryError::Structural(
                "frozen catalog and runtime/root presentation disagree",
            ));
        }
        Ok(source)
    }
}
