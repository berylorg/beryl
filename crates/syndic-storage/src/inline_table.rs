use std::mem::size_of;

use crate::{
    MARKDOWN_TABLE_INLINE_MAX_BODY_ROWS, MARKDOWN_TABLE_INLINE_MAX_BYTES,
    MARKDOWN_TABLE_INLINE_MAX_COLUMNS, MarkdownBlockId, MarkdownBlockKind, ProjectionPayload,
    ProjectionSourceRange,
    indexed_resource::{
        Alignment, ByteRange, CellPresentation, Descriptor, RowDisposition, RowRole,
    },
    table_syntax,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InlineTableRow {
    pub role: RowRole,
    pub ordinal: u64,
    pub raw: ByteRange,
    pub authored_cells: u64,
    pub disposition: RowDisposition,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InlineTableCell {
    pub role: RowRole,
    pub row: u64,
    pub column: u64,
    pub raw: Option<ByteRange>,
    pub trimmed: Option<ByteRange>,
    pub display: Option<ByteRange>,
    pub presentation: CellPresentation,
}

impl InlineTableCell {
    pub const fn is_missing(&self) -> bool {
        self.raw.is_none()
    }

    pub fn is_empty(&self) -> bool {
        self.trimmed.is_some_and(|range| range.start == range.end)
    }
}

#[derive(Debug, Eq, PartialEq)]
pub struct PreparedInlineTable {
    block_id: MarkdownBlockId,
    source_range: ProjectionSourceRange,
    source: Box<str>,
    delimiter: ByteRange,
    rows: Vec<InlineTableRow>,
    cells: Vec<InlineTableCell>,
    alignments: Vec<Alignment>,
    retained_bytes: usize,
}

impl PreparedInlineTable {
    pub fn block_id(&self) -> MarkdownBlockId {
        self.block_id
    }

    pub fn source_range(&self) -> ProjectionSourceRange {
        self.source_range
    }

    pub fn source(&self) -> &str {
        &self.source
    }

    pub fn delimiter(&self) -> ByteRange {
        self.delimiter
    }

    pub fn rows(&self) -> &[InlineTableRow] {
        &self.rows
    }

    pub fn cells(&self) -> &[InlineTableCell] {
        &self.cells
    }

    pub fn alignments(&self) -> &[Alignment] {
        &self.alignments
    }

    pub fn body_rows(&self) -> u64 {
        (self.rows.len() - 1) as u64
    }

    pub fn columns(&self) -> u64 {
        self.alignments.len() as u64
    }

    pub fn retained_bytes(&self) -> usize {
        self.retained_bytes
    }

    pub fn cell(&self, role: RowRole, row: u64, column: u64) -> Option<&InlineTableCell> {
        self.cells
            .iter()
            .find(|cell| cell.role == role && cell.row == row && cell.column == column)
    }

    pub fn cell_display(&self, cell: &InlineTableCell) -> Option<&str> {
        let range = cell.display?;
        self.source.get(range.start as usize..range.end as usize)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum InlineTablePreparationErrorKind {
    #[error("inline Table structure or source binding is invalid")]
    InvalidStructure,
    #[error("inline Table preparation exceeds its allowance")]
    Capacity,
}

#[derive(Debug, thiserror::Error)]
#[error("{kind}")]
pub struct InlineTablePreparationError {
    kind: InlineTablePreparationErrorKind,
    payload: ProjectionPayload,
}

impl InlineTablePreparationError {
    pub fn kind(&self) -> InlineTablePreparationErrorKind {
        self.kind
    }

    pub fn payload(&self) -> &ProjectionPayload {
        &self.payload
    }

    pub fn into_payload(self) -> ProjectionPayload {
        self.payload
    }
}

pub fn prepare_inline_table(
    payload: ProjectionPayload,
    allowance: usize,
) -> Result<PreparedInlineTable, InlineTablePreparationError> {
    match prepare(&payload, allowance) {
        Ok(mut prepared) => {
            let ProjectionPayload::InlineMarkdown { source, .. } = payload else {
                unreachable!("validated inline Table payload")
            };
            prepared.source = source;
            Ok(prepared)
        }
        Err(kind) => Err(InlineTablePreparationError { kind, payload }),
    }
}

fn prepare(
    payload: &ProjectionPayload,
    allowance: usize,
) -> Result<PreparedInlineTable, InlineTablePreparationErrorKind> {
    use InlineTablePreparationErrorKind::{Capacity, InvalidStructure};
    let ProjectionPayload::InlineMarkdown {
        block_id,
        block_kind: MarkdownBlockKind::Table,
        span_ordinal: 1,
        source_range,
        source,
    } = payload
    else {
        return Err(InvalidStructure);
    };
    if source.is_empty()
        || source.len() > MARKDOWN_TABLE_INLINE_MAX_BYTES
        || source_range.len() != source.len() as u64
    {
        return Err(InvalidStructure);
    }
    let mut lines = source.split_inclusive('\n');
    let header = lines.next().ok_or(InvalidStructure)?;
    let delimiter_source = lines.next().ok_or(InvalidStructure)?;
    let columns = table_syntax::columns(header).ok_or(InvalidStructure)?;
    if columns > MARKDOWN_TABLE_INLINE_MAX_COLUMNS
        || table_syntax::delimiter_columns(delimiter_source) != Some(columns)
    {
        return Err(InvalidStructure);
    }
    let delimiter = ByteRange {
        start: header.len() as u64,
        end: (header.len() + delimiter_source.len()) as u64,
    };
    let mut row_plan = [None; 33];
    row_plan[0] = Some(row(header, 0, RowRole::Header, 0, columns)?);
    let mut row_count = 1;
    let mut cell_count = columns as usize;
    let mut offset = delimiter.end;
    for line in lines {
        if row_count > MARKDOWN_TABLE_INLINE_MAX_BODY_ROWS as usize {
            return Err(InvalidStructure);
        }
        let value = row(line, offset, RowRole::Body, (row_count - 1) as u64, columns)?;
        if value.disposition == RowDisposition::Ordinary {
            cell_count = cell_count.checked_add(columns as usize).ok_or(Capacity)?;
        }
        row_plan[row_count] = Some(value);
        row_count += 1;
        offset += line.len() as u64;
    }
    let minimum = charge(source.len(), row_count, cell_count, columns as usize).ok_or(Capacity)?;
    if minimum > allowance {
        return Err(Capacity);
    }
    let mut rows = Vec::new();
    let mut cells = Vec::new();
    let mut alignments = Vec::new();
    rows.try_reserve_exact(row_count).map_err(|_| Capacity)?;
    cells.try_reserve_exact(cell_count).map_err(|_| Capacity)?;
    alignments
        .try_reserve_exact(columns as usize)
        .map_err(|_| Capacity)?;
    let retained_bytes = charge(
        source.len(),
        rows.capacity(),
        cells.capacity(),
        alignments.capacity(),
    )
    .ok_or(Capacity)?;
    if retained_bytes > allowance {
        return Err(Capacity);
    }
    for delimiter_cell in table_syntax::delimiter_cells(delimiter_source) {
        let (left, right) =
            table_syntax::delimiter_alignment(delimiter_cell).ok_or(InvalidStructure)?;
        alignments.push(match (left, right) {
            (true, true) => Alignment::Center,
            (true, false) => Alignment::Left,
            (false, true) => Alignment::Right,
            (false, false) => Alignment::Unspecified,
        });
    }
    for value in row_plan[..row_count].iter().flatten().copied() {
        rows.push(value);
        if value.disposition == RowDisposition::WholeRowFallback {
            continue;
        }
        let line = &source[value.raw.start as usize..value.raw.end as usize];
        for (column, raw) in table_syntax::cells(line).enumerate() {
            let trimmed = table_syntax::trimmed_cell(line, raw.clone());
            let presentation = if table_syntax::literal_fallback(&line[trimmed.clone()]) {
                CellPresentation::LiteralFallback
            } else {
                CellPresentation::Literal
            };
            let relative = |range: std::ops::Range<usize>| ByteRange {
                start: value.raw.start + range.start as u64,
                end: value.raw.start + range.end as u64,
            };
            let trimmed = relative(trimmed);
            cells.push(InlineTableCell {
                role: value.role,
                row: value.ordinal,
                column: column as u64,
                raw: Some(relative(raw)),
                trimmed: Some(trimmed),
                display: Some(trimmed),
                presentation,
            });
        }
        for column in value.authored_cells..columns {
            cells.push(InlineTableCell {
                role: value.role,
                row: value.ordinal,
                column,
                raw: None,
                trimmed: None,
                display: None,
                presentation: CellPresentation::Literal,
            });
        }
    }
    Ok(PreparedInlineTable {
        block_id: *block_id,
        source_range: *source_range,
        source: Box::default(),
        delimiter,
        rows,
        cells,
        alignments,
        retained_bytes,
    })
}

fn row(
    source: &str,
    start: u64,
    role: RowRole,
    ordinal: u64,
    columns: u64,
) -> Result<InlineTableRow, InlineTablePreparationErrorKind> {
    let raw = ByteRange {
        start,
        end: start + source.len() as u64,
    };
    let Descriptor::TableRow {
        role,
        ordinal,
        raw,
        authored_cells,
        disposition,
    } = crate::resource_structure::table_row(source, raw, role, ordinal, columns)
        .map_err(|_| InlineTablePreparationErrorKind::InvalidStructure)?
    else {
        return Err(InlineTablePreparationErrorKind::InvalidStructure);
    };
    Ok(InlineTableRow {
        role,
        ordinal,
        raw,
        authored_cells,
        disposition,
    })
}

fn charge(source: usize, rows: usize, cells: usize, alignments: usize) -> Option<usize> {
    size_of::<PreparedInlineTable>()
        .checked_add(source)?
        .checked_add(rows.checked_mul(size_of::<InlineTableRow>())?)?
        .checked_add(cells.checked_mul(size_of::<InlineTableCell>())?)?
        .checked_add(alignments.checked_mul(size_of::<Alignment>())?)
}
