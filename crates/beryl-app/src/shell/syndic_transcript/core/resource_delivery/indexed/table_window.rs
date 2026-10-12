use super::*;

impl ResidentTranscriptCore {
    pub(crate) fn indexed_table_window(
        &self,
        resource: &ResourceId,
    ) -> Vec<Arc<IndexedResourceResponse>> {
        self.indexed_resources
            .entries
            .iter()
            .find(|entry| {
                entry.demand.resource_id == *resource && self.indexed_owner_current(&entry.demand)
            })
            .map(|entry| entry.table_window.clone())
            .unwrap_or_default()
    }

    pub(crate) fn acknowledge_table_window(
        &mut self,
        owner: &ResourceTranscriptDemand,
        pages: &[Arc<IndexedResourceResponse>],
    ) -> bool {
        if !self.indexed_owner_current(owner) {
            return false;
        }
        let Some(entry) = self.indexed_resources.entries.iter_mut().find(|entry| {
            entry.demand == *owner
                && !entry.table_retired_window.is_empty()
                && entry.table_window.len() == pages.len()
                && entry
                    .table_window
                    .iter()
                    .zip(pages)
                    .all(|(current, adopted)| Arc::ptr_eq(current, adopted))
        }) else {
            return false;
        };
        entry.table_retired_window.clear();
        self.refresh_budget_accounting();
        true
    }

    #[cfg(test)]
    pub(crate) fn indexed_table_state_for_test(&self, resource: &ResourceId) -> String {
        let Some(entry) = self
            .indexed_resources
            .entries
            .iter()
            .find(|entry| entry.demand.resource_id == *resource)
        else {
            return "absent".to_owned();
        };
        let summarize = |pages: &[Arc<IndexedResourceResponse>]| {
            pages
                .iter()
                .map(|page| match &page.value {
                    IndexedResourceValue::Range(range) => format!(
                        "{:?}/{:?}/complete{}/items{}",
                        range.cursor.request,
                        range.cursor.next,
                        range.request_complete,
                        range.items.len()
                    ),
                    IndexedResourceValue::Pending => "pending".to_owned(),
                    _ => "action".to_owned(),
                })
                .collect::<Vec<_>>()
        };
        format!(
            "inner={:?}, response={:?}, staged={:?}, window={:?}, retired={}",
            entry.inner.query,
            summarize(
                entry
                    .response
                    .as_ref()
                    .map(std::slice::from_ref)
                    .unwrap_or_default()
            ),
            summarize(&entry.table_staging),
            summarize(&entry.table_window),
            entry.table_retired_window.len()
        )
    }
}

pub(super) fn table_request_bounded(request: &index::Request) -> bool {
    matches!(request, index::Request::Table { role, rows, columns }
        if rows.start <= rows.end && columns.start < columns.end
            && columns.end - columns.start <= 8
            && match role { index::RowRole::Header => rows.start == 0 && rows.end == 1,
                index::RowRole::Body => rows.end - rows.start <= 15 })
}

pub(super) fn admit_table_page(
    entry: &mut IndexedEntry,
    inner: &IndexedResourceDemand,
    response: &Arc<IndexedResourceResponse>,
    range: &index::Response,
) -> bool {
    let IndexedResourceQuery::Range {
        request:
            index::Request::Table {
                role,
                rows,
                columns,
            },
        cursor,
        ..
    } = &inner.query
    else {
        return false;
    };
    let Some(root) = &response.root else {
        return false;
    };
    if columns.end > root.columns
        || (*role == index::RowRole::Body && rows.end > root.body_rows)
        || entry.table_staging.len() >= 128
        || range.cursor.fragment_offset != 0
    {
        return false;
    }
    let expected = cursor
        .map(|cursor| cursor.next)
        .unwrap_or(index::Position::Table {
            row: rows.start,
            column: columns.start,
        });
    let index::Position::Table {
        row: mut expected_row,
        column: mut expected_column,
    } = expected
    else {
        return false;
    };
    if let Some(previous) = entry.table_staging.last() {
        let IndexedResourceValue::Range(previous_range) = &previous.value else {
            return false;
        };
        if previous.root != response.root || previous_range.binding != range.binding {
            return false;
        }
        match cursor {
            Some(cursor)
                if *cursor == previous_range.cursor && !previous_range.request_complete => {}
            None if *role == index::RowRole::Body
                && previous_range.request_complete
                && matches!(previous_range.cursor.request, index::Request::Table { role:index::RowRole::Header, columns: prior, .. } if prior == *columns) =>
                {}
            _ => return false,
        }
    } else if cursor.is_some() {
        return false;
    }
    for item in &range.items {
        let valid = match item {
            index::Item::Descriptor(index::IndexedDescriptor {
                descriptor:
                    index::Descriptor::TableCell {
                        role: cell_role,
                        row,
                        column,
                        ..
                    },
                ..
            })
            | index::Item::Missing {
                role: cell_role,
                row,
                column,
            } => *cell_role == *role && *row == expected_row && *column == expected_column,
            index::Item::Descriptor(index::IndexedDescriptor {
                descriptor:
                    index::Descriptor::TableRow {
                        role: row_role,
                        ordinal,
                        disposition: index::RowDisposition::WholeRowFallback,
                        ..
                    },
                ..
            }) => {
                if *row_role != *role || *ordinal != expected_row {
                    return false;
                }
                expected_column = columns.end - 1;
                true
            }
            _ => false,
        };
        if !valid || expected_row >= rows.end {
            return false;
        }
        expected_column += 1;
        if expected_column == columns.end {
            expected_row += 1;
            expected_column = columns.start;
        }
    }
    if range.cursor.next
        != (index::Position::Table {
            row: expected_row,
            column: expected_column,
        })
        || range.request_complete != (expected_row == rows.end)
    {
        return false;
    }
    let items = entry
        .table_staging
        .iter()
        .map(|page| match &page.value {
            IndexedResourceValue::Range(value) => value.items.len(),
            _ => 0,
        })
        .sum::<usize>();
    if items + range.items.len() > 128 || (range.items.is_empty() && !range.request_complete) {
        return false;
    }
    entry.table_staging.push(response.clone());
    let has_header=entry.table_staging.first().is_some_and(|page|matches!(&page.value,IndexedResourceValue::Range(range) if matches!(range.cursor.request,index::Request::Table{role:index::RowRole::Header,..})));
    if has_header
        && range.request_complete
        && (*role == index::RowRole::Body || root.body_rows == 0)
    {
        if !entry.table_retired_window.is_empty() {
            return false;
        }
        entry.table_retired_window = std::mem::take(&mut entry.table_window);
        entry.table_window = std::mem::take(&mut entry.table_staging);
    }
    true
}
