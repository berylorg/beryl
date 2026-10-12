#[path = "table_viewport/content.rs"]
mod content;
#[path = "table_viewport/indexed.rs"]
mod indexed;
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
    pub owner: super::table_owner::TableOwner,
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
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct TableCellSource {
    pub raw: Option<index::ByteRange>,
}

impl TableCellSource {
    pub(super) fn indexed(id: TableCellId, item: &index::Item) -> Result<Self, &'static str> {
        let raw = match item {
            index::Item::Descriptor(index::IndexedDescriptor {
                descriptor:
                    index::Descriptor::TableCell {
                        role, row, column, ..
                    },
                absolute_raw: Some(raw),
            }) if *role == id.role && *row == id.row && *column == id.column => Some(*raw),
            index::Item::Missing { role, row, column }
                if *role == id.role && *row == id.row && *column == id.column =>
            {
                None
            }
            _ => return Err("Copy unavailable: non-cell selection"),
        };
        Ok(Self { raw })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct RawTableRow {
    pub raw: Option<index::ByteRange>,
}
pub(super) struct TablePaintCell {
    pub id: TableCellId,
    pub fallback: bool,
    pub text: gpui::SharedString,
    pub alignment: Option<index::Alignment>,
    pub tooltip: bool,
    pub remaining: bool,
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
        if self.owner.inline().is_none() {
            self.selection = None;
        }
        true
    }
    pub(super) fn new(
        owner: ResourceTranscriptDemand,
        revision: beryl_model::ProjectionRevision,
    ) -> Self {
        Self {
            owner: super::table_owner::TableOwner::Indexed(owner),
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
    pub(super) fn fallback_row(&self, cell: TableCellId) -> bool {
        self.fallback_rows()
            .iter()
            .any(|(row, _)| row.role == cell.role && row.row == cell.row)
    }
    pub(super) fn hit(&self, position: gpui::Point<gpui::Pixels>) -> Option<TableCellId> {
        let (body_rows, columns) = self.counts()?;
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
        (column < columns && (role == index::RowRole::Header || row < body_rows))
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
            if self.owner.inline().is_some() {
                return false;
            }
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
        let last = self.counts()?.0;
        for _ in 0..=if self.owner.inline().is_some() {
            32
        } else {
            15
        } {
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
    pub(super) fn selection_cells(&self) -> Option<Vec<(TableCellId, TableCellSource)>> {
        let selection = self.selection.as_ref()?;
        let rows = selection
            .anchor
            .ordinal()
            .abs_diff(selection.head.ordinal())
            + 1;
        let columns = selection.anchor.column.abs_diff(selection.head.column) + 1;
        if rows.checked_mul(columns)?
            > if self.owner.inline().is_some() {
                396
            } else {
                128
            }
        {
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
        let Some((body_rows, column_count)) = self.counts() else {
            return false;
        };
        let Some(selection) = &self.selection else {
            return false;
        };
        selection.anchor.ordinal().min(selection.head.ordinal()) == 0
            && selection.anchor.ordinal().max(selection.head.ordinal()) == body_rows
            && selection.anchor.column.min(selection.head.column) == 0
            && selection.anchor.column.max(selection.head.column) + 1 == column_count
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
}
