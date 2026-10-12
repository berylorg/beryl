use super::*;

impl TableViewport {
    pub(in super::super) fn source_pages(&self) -> &[Arc<IndexedResourceResponse>] {
        &self.pages
    }

    pub(in super::super) fn capture_anchor(&mut self) {
        let Some(binding) = &self.binding else {
            return;
        };
        let scroll = self.scroll.borrow();
        let row = (scroll.vertical / scroll.row_height).floor() as u64;
        let column = (scroll.horizontal / scroll.column_width).floor() as u64;
        let target = self
            .cells()
            .into_iter()
            .filter(|(cell, _)| {
                cell.role == index::RowRole::Body && cell.row >= row && cell.column >= column
            })
            .min_by_key(|(cell, _)| (cell.row, cell.column))
            .and_then(|(cell, source)| source.raw.map(|raw| (cell, raw)))
            .or_else(|| {
                self.fallback_rows()
                    .into_iter()
                    .filter(|(cell, _)| cell.row >= row)
                    .min_by_key(|(cell, _)| cell.row)
                    .and_then(|(cell, source)| source.raw.map(|raw| (cell, raw)))
            });
        if let Some((cell, raw)) = target {
            self.anchor = Some(index::SourceAnchor {
                source: binding.source,
                block_start: binding.range.start,
                kind: syndic_storage::ResourceKind::Table,
                position: raw.start,
                visual_offset: (cell.row as f32 * scroll.row_height - scroll.vertical).round()
                    as i64,
            });
            self.anchor_horizontal = cell.column as f32 * scroll.column_width - scroll.horizontal;
        } else {
            self.anchor = None;
        }
    }

    pub(in super::super) fn rebind(
        &mut self,
        owner: ResourceTranscriptDemand,
        revision: beryl_model::ProjectionRevision,
    ) {
        self.capture_anchor();
        self.owner = super::super::table_owner::TableOwner::Indexed(owner);
        self.revision = revision;
        self.root = None;
        self.binding = None;
        self.pages.clear();
        self.selection = None;
        self.focus = None;
        self.pending_focus = None;
        self.dragging = false;
        self.remapping = self.anchor.is_some();
        self.source_continuity = if self.remapping {
            "pending"
        } else {
            "invalidated"
        };
    }

    pub(in super::super) fn adopt(
        &mut self,
        response: Option<&IndexedResourceResponse>,
        pages: Vec<Arc<IndexedResourceResponse>>,
    ) {
        if let Some(response) = response
            && let Some(root) = &response.root
        {
            if self
                .root
                .as_ref()
                .is_some_and(|old| old.digest != root.digest)
            {
                self.selection = None;
                self.focus = None;
                self.pages.clear();
            }
            self.root = Some(root.clone());
            let mut scroll = self.scroll.borrow_mut();
            scroll.rows = root.body_rows;
            scroll.columns = root.columns;
            if let IndexedResourceValue::Range(range) = &response.value {
                self.binding = Some(range.binding.clone());
            }
            if let IndexedResourceValue::Anchor(value) = &response.value {
                self.binding = Some(value.binding.clone());
                if self.remapping
                    && let Some(old) = self.anchor
                {
                    if let Some(resolved) = &value.resolved
                        && old.source.can_extend(value.binding.source)
                        && resolved.anchor.source == value.binding.source
                        && resolved.anchor.block_start == old.block_start
                    {
                        let coordinate = match &resolved.descriptor.descriptor {
                            index::Descriptor::TableCell {
                                role, row, column, ..
                            } => Some((*role, *row, *column)),
                            index::Descriptor::TableRow { role, ordinal, .. } => {
                                Some((*role, *ordinal, 0))
                            }
                            _ => None,
                        };
                        if let Some((role, row, column)) = coordinate {
                            let vertical = if role == index::RowRole::Header {
                                0.0
                            } else {
                                row as f32 * scroll.row_height - old.visual_offset as f32
                            };
                            let horizontal =
                                column as f32 * scroll.column_width - self.anchor_horizontal;
                            scroll.set_offset(gpui_scrollbar::Axis::Vertical, vertical);
                            scroll.set_offset(gpui_scrollbar::Axis::Horizontal, horizontal);
                            self.anchor = Some(resolved.anchor);
                            self.source_continuity = "preserved";
                        } else {
                            self.anchor = None;
                            self.source_continuity = "invalidated";
                        }
                    } else {
                        self.anchor = None;
                        self.source_continuity = "invalidated";
                        scroll.vertical = 0.0;
                        scroll.horizontal = 0.0;
                        self.feedback = "Table source anchor invalidated";
                    }
                    self.remapping = false;
                }
            }
        }
        if !pages.is_empty() {
            self.pages = pages;
        }
        if self.selection.is_some() && self.selection_cells().is_none() {
            self.selection = None;
            self.feedback = "Table selection invalidated";
        }
        let cells = self.cells();
        if self
            .focus
            .is_none_or(|focus| !cells.iter().any(|(cell, _)| *cell == focus))
        {
            let old = self.focus;
            self.focus = cells
                .iter()
                .min_by_key(|(cell, _)| {
                    old.map_or(0, |old| {
                        old.ordinal()
                            .abs_diff(cell.ordinal())
                            .saturating_add(old.column.abs_diff(cell.column))
                    })
                })
                .map(|(cell, _)| *cell);
        }
        if let Some((cell, _)) = self.pending_focus
            && self.fallback_row(cell)
        {
            self.pending_focus = None;
            self.dragging = false;
            self.feedback = "Table row requires source actions";
        }
        if let Some((cell, extend)) = self.pending_focus
            && cells.iter().any(|(resident, _)| *resident == cell)
        {
            self.pending_focus = None;
            if extend && self.selection.is_none() {
                self.focus = Some(cell);
            } else {
                self.select(cell, extend);
            }
        }
    }

    pub(in super::super) fn query(
        &self,
        response: Option<&IndexedResourceResponse>,
    ) -> IndexedResourceDemand {
        if self.remapping
            && let Some(anchor) = self.anchor
        {
            return IndexedResourceDemand {
                revision: self.revision,
                root: self.root.as_ref().map(|root| root.digest),
                query: IndexedResourceQuery::Anchor {
                    anchor,
                    bytes: 8192,
                },
            };
        }
        let (rows, columns) = self.scroll.borrow().ranges();
        let header = index::Request::Table {
            role: index::RowRole::Header,
            rows: index::OrdinalRange { start: 0, end: 1 },
            columns,
        };
        let body = index::Request::Table {
            role: index::RowRole::Body,
            rows,
            columns,
        };
        if response.is_none()
            && let Some(current) = self
                .owner
                .indexed()
                .and_then(|owner| owner.indexed.as_ref())
            && matches!(&current.query, IndexedResourceQuery::Range { request, .. }
                if *request == header || *request == body)
        {
            return current.clone();
        }
        let mut request = header;
        let mut cursor = None;
        if let Some(response) = response
            && let IndexedResourceValue::Range(range) = &response.value
        {
            if range.cursor.request == header || range.cursor.request == body {
                if !range.request_complete {
                    request = range.cursor.request;
                    cursor = Some(range.cursor);
                } else if range.cursor.request == header
                    && self.root.as_ref().is_some_and(|root| root.body_rows > 0)
                {
                    request = body;
                } else {
                    return self
                        .owner
                        .indexed()
                        .and_then(|owner| owner.indexed.clone())
                        .unwrap_or(IndexedResourceDemand {
                            revision: self.revision,
                            root: self.root.as_ref().map(|root| root.digest),
                            query: IndexedResourceQuery::Range {
                                request: range.cursor.request,
                                cursor: None,
                                descriptors: 128,
                                bytes: WINDOW_RESPONSE_BYTES,
                            },
                        });
                }
            }
        }
        IndexedResourceDemand {
            revision: self.revision,
            root: self.root.as_ref().map(|root| root.digest),
            query: IndexedResourceQuery::Range {
                request,
                cursor,
                descriptors: 128,
                bytes: WINDOW_RESPONSE_BYTES,
            },
        }
    }
}
