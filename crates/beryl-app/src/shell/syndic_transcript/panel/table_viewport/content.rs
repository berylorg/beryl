use super::*;

impl TableViewport {
    pub(in super::super) fn paint_cells(&self) -> Vec<TablePaintCell> {
        let (rows, columns) = self.realized_ranges();
        if let Some(owner) = self.owner.inline() {
            let mut paint: Vec<_> = owner
                .prepared
                .cells()
                .iter()
                .filter(|cell| {
                    cell.column >= columns.start
                        && cell.column < columns.end
                        && (cell.role == index::RowRole::Header
                            || cell.row >= rows.start && cell.row < rows.end)
                })
                .map(|cell| TablePaintCell {
                    id: TableCellId {
                        role: cell.role,
                        row: cell.row,
                        column: cell.column,
                    },
                    fallback: false,
                    text: gpui::SharedString::from(
                        owner.prepared.cell_display(cell).unwrap_or("").to_owned(),
                    ),
                    alignment: owner
                        .prepared
                        .alignments()
                        .get(cell.column as usize)
                        .copied(),
                    tooltip: cell.display.is_some(),
                    remaining: false,
                })
                .collect();
            for row in owner.prepared.rows().iter().filter(|row| {
                row.disposition == index::RowDisposition::WholeRowFallback
                    && (row.role == index::RowRole::Header
                        || row.ordinal >= rows.start && row.ordinal < rows.end)
            }) {
                paint.push(TablePaintCell {
                    id: TableCellId {
                        role: row.role,
                        row: row.ordinal,
                        column: columns.start,
                    },
                    fallback: true,
                    text: gpui::SharedString::from(
                        owner.prepared.source()[row.raw.start as usize..row.raw.end as usize]
                            .to_owned(),
                    ),
                    alignment: None,
                    tooltip: false,
                    remaining: false,
                });
            }
            return paint;
        }
        self.pages
            .iter()
            .filter_map(|page| match &page.value {
                IndexedResourceValue::Range(range) => Some(range),
                _ => None,
            })
            .flat_map(|range| {
                range
                    .items
                    .iter()
                    .enumerate()
                    .filter_map(move |(slot, item)| {
                        let (id, fallback) = match item {
                            index::Item::Descriptor(index::IndexedDescriptor {
                                descriptor:
                                    index::Descriptor::TableCell {
                                        role, row, column, ..
                                    },
                                ..
                            })
                            | index::Item::Missing { role, row, column } => (
                                TableCellId {
                                    role: *role,
                                    row: *row,
                                    column: *column,
                                },
                                false,
                            ),
                            index::Item::Descriptor(index::IndexedDescriptor {
                                descriptor:
                                    index::Descriptor::TableRow {
                                        role,
                                        ordinal,
                                        disposition: index::RowDisposition::WholeRowFallback,
                                        ..
                                    },
                                ..
                            }) => (
                                TableCellId {
                                    role: *role,
                                    row: *ordinal,
                                    column: columns.start,
                                },
                                true,
                            ),
                            _ => return None,
                        };
                        let presentation = range
                            .presentations
                            .as_ref()
                            .and_then(|values| values.get(slot));
                        let fragment = presentation.and_then(|value| value.fragment.as_ref());
                        Some(TablePaintCell {
                            id,
                            fallback,
                            text: fragment
                                .map(|fragment| gpui::SharedString::from(fragment.text.clone()))
                                .unwrap_or_default(),
                            alignment: presentation.and_then(|value| value.alignment),
                            tooltip: fragment.is_some(),
                            remaining: fragment.is_some_and(|fragment| {
                                fragment.remaining.end > fragment.remaining.start
                            }),
                        })
                    })
            })
            .collect()
    }

    pub(in super::super) fn inline(owner: super::super::super::core::InlineTableOwner) -> Self {
        let revision = beryl_model::ProjectionRevision::new(
            owner
                .provenance
                .projection_revision
                .expect("admitted inline revision")
                .0,
        )
        .expect("checked projection revision");
        let rows = owner.prepared.body_rows();
        let columns = owner.prepared.columns();
        let mut viewport = Self {
            owner: super::super::table_owner::TableOwner::Inline(owner),
            revision,
            root: None,
            binding: None,
            selection: None,
            focus: None,
            scroll: Rc::new(RefCell::new(TableScroll {
                bounds: gpui::Bounds::default(),
                header_bounds: gpui::Bounds::default(),
                horizontal: 0.0,
                vertical: 0.0,
                row_height: 28.0,
                column_width: 173.0,
                rows,
                columns,
            })),
            feedback: "",
            pending_focus: None,
            dragging: false,
            anchor: None,
            remapping: false,
            source_continuity: "initial",
            anchor_horizontal: 0.0,
            pages: Vec::new(),
        };
        viewport.focus = viewport.cells().first().map(|(id, _)| *id);
        viewport
    }

    pub(in super::super) fn counts(&self) -> Option<(u64, u64)> {
        self.owner
            .inline()
            .map(|owner| (owner.prepared.body_rows(), owner.prepared.columns()))
            .or_else(|| {
                self.root
                    .as_ref()
                    .map(|root| (root.body_rows, root.columns))
            })
    }

    pub(in super::super) fn charge(&self) -> usize {
        65536
            + self
                .pages
                .iter()
                .filter_map(|page| match &page.value {
                    IndexedResourceValue::Range(range) => range.presentations.as_ref(),
                    _ => None,
                })
                .flatten()
                .filter_map(|presentation| presentation.fragment.as_ref())
                .map(|fragment| fragment.text.len())
                .sum::<usize>()
    }

    pub(in super::super) fn realized_ranges(&self) -> (index::OrdinalRange, index::OrdinalRange) {
        if self.owner.inline().is_some() {
            return self.scroll.borrow().ranges();
        }
        let mut rows = index::OrdinalRange { start: 0, end: 0 };
        let mut columns = rows;
        for page in &self.pages {
            if let IndexedResourceValue::Range(range) = &page.value
                && let index::Request::Table {
                    role,
                    rows: page_rows,
                    columns: page_columns,
                } = range.cursor.request
            {
                columns = page_columns;
                if role == index::RowRole::Body {
                    rows = page_rows;
                }
            }
        }
        (rows, columns)
    }

    pub(in super::super) fn cells(&self) -> Vec<(TableCellId, TableCellSource)> {
        if let Some(owner) = self.owner.inline() {
            return owner
                .prepared
                .cells()
                .iter()
                .map(|cell| {
                    (
                        TableCellId {
                            role: cell.role,
                            row: cell.row,
                            column: cell.column,
                        },
                        TableCellSource { raw: cell.raw },
                    )
                })
                .collect();
        }
        self.pages
            .iter()
            .filter_map(|page| match &page.value {
                IndexedResourceValue::Range(range) => Some(range),
                _ => None,
            })
            .flat_map(|range| range.items.iter())
            .filter_map(|item| {
                let id = match item {
                    index::Item::Descriptor(index::IndexedDescriptor {
                        descriptor:
                            index::Descriptor::TableCell {
                                role, row, column, ..
                            },
                        ..
                    })
                    | index::Item::Missing { role, row, column } => TableCellId {
                        role: *role,
                        row: *row,
                        column: *column,
                    },
                    _ => return None,
                };
                TableCellSource::indexed(id, item)
                    .ok()
                    .map(|source| (id, source))
            })
            .collect()
    }

    pub(in super::super) fn fallback_rows(&self) -> Vec<(TableCellId, RawTableRow)> {
        if let Some(owner) = self.owner.inline() {
            return owner
                .prepared
                .rows()
                .iter()
                .filter(|row| row.disposition == index::RowDisposition::WholeRowFallback)
                .map(|row| {
                    (
                        TableCellId {
                            role: row.role,
                            row: row.ordinal,
                            column: 0,
                        },
                        RawTableRow { raw: Some(row.raw) },
                    )
                })
                .collect();
        }
        self.pages
            .iter()
            .filter_map(|page| match &page.value {
                IndexedResourceValue::Range(range) => Some(range),
                _ => None,
            })
            .flat_map(|range| range.items.iter())
            .filter_map(|item| match item {
                index::Item::Descriptor(index::IndexedDescriptor {
                    descriptor:
                        index::Descriptor::TableRow {
                            role,
                            ordinal,
                            disposition: index::RowDisposition::WholeRowFallback,
                            ..
                        },
                    absolute_raw,
                }) => Some((
                    TableCellId {
                        role: *role,
                        row: *ordinal,
                        column: 0,
                    },
                    RawTableRow { raw: *absolute_raw },
                )),
                _ => None,
            })
            .collect()
    }
}
