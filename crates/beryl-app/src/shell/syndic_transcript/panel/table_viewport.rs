use super::super::provider::resource_index as index;
use super::super::{
    IndexedResourceDemand, IndexedResourceQuery, IndexedResourceResponse, IndexedResourceValue,
    ResourceTranscriptDemand,
};
use std::{cell::RefCell, rc::Rc, sync::Arc};

pub(super) const ROW_OVERSCAN: u64 = 2;
pub(super) const COLUMN_OVERSCAN: u64 = 1;
const WINDOW_RESPONSE_BYTES: usize = 16_384;

fn bounded_axis(first: u64, end: u64, total: u64, cap: u64, overscan: u64) -> index::OrdinalRange {
    let first = first.min(total);
    let end = end.max(first).min(total);
    let visible = end - first;
    let leading = overscan.min(cap.saturating_sub(visible)).min(first);
    let start = first - leading;
    let trailing = overscan.min(cap.saturating_sub(end - start));
    index::OrdinalRange {
        start,
        end: end.saturating_add(trailing).min(total),
    }
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(super) struct TableCellId {
    pub role: index::RowRole,
    pub row: u64,
    pub column: u64,
}
impl TableCellId {
    pub(super) fn ordinal(self) -> u64 {
        match self.role {
            index::RowRole::Header => 0,
            index::RowRole::Body => self.row + 1,
        }
    }
    pub(super) fn at(row: u64, column: u64) -> Self {
        if row == 0 {
            Self {
                role: index::RowRole::Header,
                row: 0,
                column,
            }
        } else {
            Self {
                role: index::RowRole::Body,
                row: row - 1,
                column,
            }
        }
    }
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct TableSelection {
    pub anchor: TableCellId,
    pub head: TableCellId,
}
impl TableSelection {
    pub(super) fn contains(&self, cell: TableCellId) -> bool {
        (self.anchor.ordinal().min(self.head.ordinal())
            ..=self.anchor.ordinal().max(self.head.ordinal()))
            .contains(&cell.ordinal())
            && (self.anchor.column.min(self.head.column)..=self.anchor.column.max(self.head.column))
                .contains(&cell.column)
    }
}

pub(super) struct TableScroll {
    pub bounds: gpui::Bounds<gpui::Pixels>,
    pub horizontal: f32,
    pub vertical: f32,
    pub header_bounds: gpui::Bounds<gpui::Pixels>,
    pub row_height: f32,
    pub column_width: f32,
    pub rows: u64,
    pub columns: u64,
}
impl TableScroll {
    pub fn extent(&self, axis: gpui_scrollbar::Axis) -> f32 {
        match axis {
            gpui_scrollbar::Axis::Vertical => {
                (self.rows as f32 * self.row_height - f32::from(self.bounds.size.height)).max(0.0)
            }
            gpui_scrollbar::Axis::Horizontal => (self.columns as f32 * self.column_width
                - f32::from(self.bounds.size.width))
            .max(0.0),
        }
    }
    pub fn set_offset(&mut self, axis: gpui_scrollbar::Axis, value: f32) {
        let value = value.clamp(0.0, self.extent(axis));
        match axis {
            gpui_scrollbar::Axis::Vertical => self.vertical = value,
            gpui_scrollbar::Axis::Horizontal => self.horizontal = value,
        }
    }
    pub fn scroll(&mut self, x: f32, y: f32) {
        self.set_offset(gpui_scrollbar::Axis::Horizontal, self.horizontal + x);
        self.set_offset(gpui_scrollbar::Axis::Vertical, self.vertical + y);
    }
    pub fn ranges(&self) -> (index::OrdinalRange, index::OrdinalRange) {
        let first_row = (self.vertical / self.row_height).floor().max(0.0) as u64;
        let first_column = (self.horizontal / self.column_width).floor().max(0.0) as u64;
        let visible_row_end = ((self.vertical + f32::from(self.bounds.size.height))
            / self.row_height)
            .ceil()
            .max(0.0) as u64;
        let visible_column_end = ((self.horizontal + f32::from(self.bounds.size.width))
            / self.column_width)
            .ceil()
            .max(1.0) as u64;
        (
            bounded_axis(first_row, visible_row_end, self.rows, 15, ROW_OVERSCAN),
            bounded_axis(
                first_column,
                visible_column_end,
                self.columns.max(1),
                8,
                COLUMN_OVERSCAN,
            ),
        )
    }
}

pub(super) struct TableViewport {
    pub owner: ResourceTranscriptDemand,
    pub revision: beryl_model::ProjectionRevision,
    pub root: Option<index::Root>,
    pub binding: Option<index::Binding>,
    pub selection: Option<TableSelection>,
    pub focus: Option<TableCellId>,
    pub scroll: Rc<RefCell<TableScroll>>,
    pub feedback: &'static str,
    pub pending_focus: Option<(TableCellId, bool)>,
    pub dragging: bool,
    pub anchor: Option<index::SourceAnchor>,
    pub remapping: bool,
    pub source_continuity: &'static str,
    anchor_horizontal: f32,
    pages: Vec<Arc<IndexedResourceResponse>>,
}
impl TableViewport {
    pub(super) fn set_tracks(&mut self, height: f32, width: f32) -> bool {
        let mut scroll = self.scroll.borrow_mut();
        if scroll.row_height == height && scroll.column_width == width {
            return false;
        }
        let row = (scroll.vertical / scroll.row_height).floor();
        let row_residual = scroll.vertical - row * scroll.row_height;
        let column = (scroll.horizontal / scroll.column_width).floor();
        let column_residual = scroll.horizontal - column * scroll.column_width;
        scroll.row_height = height;
        scroll.column_width = width;
        scroll.set_offset(gpui_scrollbar::Axis::Vertical, row * height + row_residual);
        scroll.set_offset(
            gpui_scrollbar::Axis::Horizontal,
            column * width + column_residual,
        );
        self.selection = None;
        true
    }
    pub(super) fn charge(&self) -> usize {
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
    pub(super) fn new(
        owner: ResourceTranscriptDemand,
        revision: beryl_model::ProjectionRevision,
    ) -> Self {
        Self {
            owner,
            revision,
            root: None,
            binding: None,
            selection: None,
            focus: None,
            pending_focus: None,
            dragging: false,
            anchor: None,
            remapping: false,
            source_continuity: "initial",
            anchor_horizontal: 0.0,
            feedback: "",
            pages: Vec::new(),
            scroll: Rc::new(RefCell::new(TableScroll {
                bounds: gpui::Bounds::default(),
                header_bounds: gpui::Bounds::default(),
                horizontal: 0.0,
                vertical: 0.0,
                row_height: 28.0,
                column_width: 173.0,
                rows: 0,
                columns: 1,
            })),
        }
    }
    pub(super) fn source_pages(&self) -> &[Arc<IndexedResourceResponse>] {
        &self.pages
    }
    pub(super) fn realized_ranges(&self) -> (index::OrdinalRange, index::OrdinalRange) {
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
    pub(super) fn capture_anchor(&mut self) {
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
            .and_then(|(cell, item)| match item {
                index::Item::Descriptor(descriptor) => Some((cell, descriptor)),
                _ => None,
            })
            .or_else(|| {
                self.fallback_rows()
                    .into_iter()
                    .filter(|(cell, _)| cell.row >= row)
                    .min_by_key(|(cell, _)| cell.row)
            });
        if let Some((cell, descriptor)) = target
            && let Some(raw) = descriptor.absolute_raw
        {
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
    pub(super) fn rebind(
        &mut self,
        owner: ResourceTranscriptDemand,
        revision: beryl_model::ProjectionRevision,
    ) {
        self.capture_anchor();
        self.owner = owner;
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
    pub(super) fn cells(&self) -> Vec<(TableCellId, index::Item)> {
        self.pages
            .iter()
            .filter_map(|page| match &page.value {
                IndexedResourceValue::Range(range) => Some(range),
                _ => None,
            })
            .flat_map(|range| range.items.iter())
            .filter_map(|item| {
                let cell = match item {
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
                Some((cell, item.clone()))
            })
            .collect()
    }
    pub(super) fn fallback_rows(&self) -> Vec<(TableCellId, index::IndexedDescriptor)> {
        self.pages
            .iter()
            .filter_map(|page| match &page.value {
                IndexedResourceValue::Range(range) => Some(range),
                _ => None,
            })
            .flat_map(|range| range.items.iter())
            .filter_map(|item| match item {
                index::Item::Descriptor(
                    descriptor @ index::IndexedDescriptor {
                        descriptor:
                            index::Descriptor::TableRow {
                                role,
                                ordinal,
                                disposition: index::RowDisposition::WholeRowFallback,
                                ..
                            },
                        ..
                    },
                ) => Some((
                    TableCellId {
                        role: *role,
                        row: *ordinal,
                        column: 0,
                    },
                    descriptor.clone(),
                )),
                _ => None,
            })
            .collect()
    }
    pub(super) fn fallback_row(&self, cell: TableCellId) -> bool {
        self.fallback_rows()
            .iter()
            .any(|(row, _)| row.role == cell.role && row.row == cell.row)
    }
    pub(super) fn hit(&self, position: gpui::Point<gpui::Pixels>) -> Option<TableCellId> {
        let root = self.root.as_ref()?;
        let scroll = self.scroll.borrow();
        let (role, bounds) = if scroll.header_bounds.contains(&position) {
            (index::RowRole::Header, scroll.header_bounds)
        } else if scroll.bounds.contains(&position) {
            (index::RowRole::Body, scroll.bounds)
        } else {
            return None;
        };
        let column = ((f32::from(position.x - bounds.origin.x) + scroll.horizontal)
            / scroll.column_width)
            .floor()
            .max(0.0) as u64;
        let row = if role == index::RowRole::Header {
            0
        } else {
            ((f32::from(position.y - bounds.origin.y) + scroll.vertical) / scroll.row_height)
                .floor()
                .max(0.0) as u64
        };
        (column < root.columns && (role == index::RowRole::Header || row < root.body_rows))
            .then_some(TableCellId { role, row, column })
    }
    pub(super) fn request_cell(&mut self, cell: TableCellId, extend: bool) -> bool {
        if self.fallback_row(cell) {
            self.feedback = "Source row fallback: use Copy row or Save row";
            return false;
        }
        if self.select(cell, extend) {
            self.pending_focus = None;
            true
        } else {
            self.pending_focus = Some((cell, extend));
            self.reveal(cell);
            false
        }
    }
    pub(super) fn skip_fallback(
        &self,
        mut cell: TableCellId,
        forward: bool,
    ) -> Option<TableCellId> {
        let last = self.root.as_ref()?.body_rows;
        for _ in 0..=15 {
            if !self.fallback_row(cell) {
                return Some(cell);
            }
            let ordinal = cell.ordinal();
            let next = if forward {
                ordinal.checked_add(1).filter(|next| *next <= last)?
            } else {
                ordinal.checked_sub(1)?
            };
            cell = TableCellId::at(next, cell.column);
        }
        None
    }
    pub(super) fn selection_cells(&self) -> Option<Vec<(TableCellId, index::Item)>> {
        let selection = self.selection.as_ref()?;
        let rows = selection
            .anchor
            .ordinal()
            .abs_diff(selection.head.ordinal())
            + 1;
        let columns = selection.anchor.column.abs_diff(selection.head.column) + 1;
        if rows.checked_mul(columns)? > 128 {
            return None;
        }
        let cells: Vec<_> = self
            .cells()
            .into_iter()
            .filter(|(cell, _)| selection.contains(*cell))
            .collect();
        (cells.len() as u64 == rows * columns).then_some(cells)
    }
    pub(super) fn whole_table_selection(&self) -> bool {
        let Some(root) = &self.root else {
            return false;
        };
        let Some(selection) = &self.selection else {
            return false;
        };
        selection.anchor.ordinal().min(selection.head.ordinal()) == 0
            && selection.anchor.ordinal().max(selection.head.ordinal()) == root.body_rows
            && selection.anchor.column.min(selection.head.column) == 0
            && selection.anchor.column.max(selection.head.column) + 1 == root.columns
            && self.selection_cells().is_some()
    }
    pub(super) fn select(&mut self, cell: TableCellId, extend: bool) -> bool {
        if !self.cells().iter().any(|(resident, _)| *resident == cell) {
            self.feedback = "Table range loading";
            return false;
        }
        let previous = self.selection.clone();
        let anchor = if extend {
            previous.as_ref().map_or(cell, |selection| selection.anchor)
        } else {
            cell
        };
        self.selection = Some(TableSelection { anchor, head: cell });
        if self.selection_cells().is_none() {
            self.selection = previous;
            self.feedback = "Table selection cannot cross an unavailable range";
            return false;
        }
        self.focus = Some(cell);
        self.feedback = "";
        true
    }
    pub(super) fn reveal(&mut self, cell: TableCellId) {
        let mut scroll = self.scroll.borrow_mut();
        let x = cell.column as f32 * scroll.column_width;
        let width = f32::from(scroll.bounds.size.width);
        let horizontal = if x < scroll.horizontal {
            x
        } else if x + scroll.column_width > scroll.horizontal + width {
            x + scroll.column_width - width
        } else {
            scroll.horizontal
        };
        scroll.set_offset(gpui_scrollbar::Axis::Horizontal, horizontal);
        if cell.role == index::RowRole::Body {
            let y = cell.row as f32 * scroll.row_height;
            let height = f32::from(scroll.bounds.size.height);
            let vertical = if y < scroll.vertical {
                y
            } else if y + scroll.row_height > scroll.vertical + height {
                y + scroll.row_height - height
            } else {
                scroll.vertical
            };
            scroll.set_offset(gpui_scrollbar::Axis::Vertical, vertical);
        }
    }
    pub(super) fn adopt(
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
    pub(super) fn query(
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
            && let Some(current) = &self.owner.indexed
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
                    return self.owner.indexed.clone().unwrap_or(IndexedResourceDemand {
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
