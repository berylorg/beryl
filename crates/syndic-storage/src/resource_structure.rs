use beryl_model::{ProjectionRevision, SyndicResourceId};

use crate::{ProjectionTextSource, ResourceMetadataRecord};

pub mod access;
mod access_anchor;
mod access_construction;
mod advance;
mod code;
pub(crate) mod codec;
mod construction;
mod display;
mod frozen;
mod ledger;
mod query;
mod query_anchor;
mod query_display;
mod query_presentation;
mod query_response;
mod rows;
mod source;
mod table;
mod table_advance;
mod table_publication;
mod transition;

#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub(crate) enum DerivationError {
    #[error("resource structure checkpoint or source frontier is invalid")]
    InvalidFrontier,
    #[error("resource structure source is malformed UTF-8")]
    InvalidUtf8,
    #[error("resource structure source disagrees with the recognized table grammar")]
    InvalidSyntax,
    #[error("resource structure step allowance is invalid")]
    InvalidAllowance,
    #[error("resource structure coordinate exceeds its supported range")]
    Overflow,
}

pub(crate) struct DerivationStep {
    pub(crate) checkpoint: Checkpoint,
    pub(crate) descriptors: Vec<Descriptor>,
    pub(crate) consumed: usize,
}

pub(crate) use code::advance_code;
pub(crate) use rows::{advance_row, advance_rows};
pub(crate) use table::{advance_cells, advance_columns, drain_cells, drain_columns, table_row};

#[cfg(test)]
#[path = "../tests/unit/resource_structure_codec.rs"]
mod tests;

#[cfg(test)]
#[path = "../tests/unit/resource_structure_lines.rs"]
mod line_tests;

#[cfg(test)]
#[path = "../tests/unit/resource_structure_tables.rs"]
mod table_tests;

#[cfg(test)]
#[path = "../tests/unit/resource_structure_rows.rs"]
mod row_tests;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ResourceKey {
    pub id: SyndicResourceId,
    pub revision: ProjectionRevision,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BuildKey {
    pub resource: ResourceKey,
    pub operation: [u8; 16],
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SequenceKey {
    pub build: BuildKey,
    pub ordinal: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Binding {
    pub resource: ResourceKey,
    pub metadata: ResourceMetadataRecord,
    pub source: ProjectionTextSource,
    pub range: ByteRange,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ByteRange {
    pub start: u64,
    pub end: u64,
}

impl ByteRange {
    pub(crate) const fn is_valid(self) -> bool {
        self.start <= self.end
    }

    pub(crate) const fn contains(self, inner: Self) -> bool {
        inner.is_valid() && self.start <= inner.start && inner.end <= self.end
    }
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum RowRole {
    Header,
    Body,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RowDisposition {
    Ordinary,
    WholeRowFallback,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CellPresentation {
    Literal,
    LiteralFallback,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Alignment {
    Unspecified,
    Left,
    Center,
    Right,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Descriptor {
    CodeLine {
        ordinal: u64,
        raw: ByteRange,
        content: ByteRange,
        terminator: ByteRange,
    },
    TableRow {
        role: RowRole,
        ordinal: u64,
        raw: ByteRange,
        authored_cells: u64,
        disposition: RowDisposition,
    },
    TableCell {
        role: RowRole,
        row: u64,
        column: u64,
        raw: ByteRange,
        trimmed: ByteRange,
        presentation: CellPresentation,
    },
    TableColumn {
        ordinal: u64,
        alignment: Alignment,
    },
}

impl Descriptor {
    pub(crate) fn coordinate(&self) -> (u8, RowRole, u64, u64) {
        match *self {
            Self::CodeLine { ordinal, .. } => (0, RowRole::Header, ordinal, 0),
            Self::TableRow { role, ordinal, .. } => (1, role, ordinal, 0),
            Self::TableCell {
                role, row, column, ..
            } => (2, role, row, column),
            Self::TableColumn { ordinal, .. } => (3, RowRole::Header, ordinal, 0),
        }
    }

    pub(crate) fn raw(&self) -> Option<ByteRange> {
        match *self {
            Self::CodeLine { raw, .. }
            | Self::TableRow { raw, .. }
            | Self::TableCell { raw, .. } => Some(raw),
            Self::TableColumn { .. } => None,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Page {
    pub(crate) key: SequenceKey,
    pub(crate) binding_digest: [u8; 32],
    pub(crate) prior_chain: [u8; 32],
    pub(crate) descriptors: Vec<Descriptor>,
    pub(crate) digest: [u8; 32],
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum CoordinateKey {
    CodeLine(BuildKey, u64),
    TableRow(BuildKey, RowRole, u64),
    TableCell(BuildKey, RowRole, u64, u64),
    TableColumn(BuildKey, u64),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct Locator {
    pub(crate) key: CoordinateKey,
    pub(crate) page: u64,
    pub(crate) slot: u64,
    pub(crate) digest: [u8; 32],
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct AnchorKey {
    pub(crate) build: BuildKey,
    pub(crate) start: u64,
    pub(crate) tag: u8,
    pub(crate) role: RowRole,
    pub(crate) row: u64,
    pub(crate) column: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct AnchorLocator {
    pub(crate) key: AnchorKey,
    pub(crate) end: u64,
    pub(crate) page: u64,
    pub(crate) slot: u64,
    pub(crate) digest: [u8; 32],
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Lifecycle {
    Building,
    Completed,
    Cancelled,
    Superseded,
    Refused,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Pass {
    CodeLines,
    TableRows,
    TableCells,
    Columns,
    Sealed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ParsePhase {
    Consuming,
    DrainingSyntax,
    Eof,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Checkpoint {
    pub(crate) pass: Pass,
    pub(crate) next_byte: u64,
    pub(crate) line_start: u64,
    pub(crate) role: RowRole,
    pub(crate) next_row: u64,
    pub(crate) next_column: u64,
    pub(crate) phase: ParsePhase,
    pub(crate) pending_cr: bool,
    pub(crate) utf8_carry: Vec<u8>,
    pub(crate) syntax_start: u64,
    pub(crate) syntax: Vec<u8>,
    pub(crate) output_position: u64,
    pub(crate) escaped: bool,
    pub(crate) in_code: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct Reference {
    pub(crate) key: SequenceKey,
    pub(crate) digest: [u8; 32],
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Build {
    pub(crate) key: BuildKey,
    pub(crate) binding: Binding,
    pub(crate) binding_digest: [u8; 32],
    pub(crate) revision: u64,
    pub(crate) lifecycle: Lifecycle,
    pub(crate) transition: u64,
    pub(crate) progress: Option<Reference>,
    pub(crate) next_page: u64,
    pub(crate) chain: [u8; 32],
    pub(crate) counts: [u64; 4],
    pub(crate) checkpoint: Checkpoint,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Command {
    Advance,
    PassChange,
    Complete,
    Cancel,
    Supersede,
    Refuse,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Progress {
    pub(crate) key: SequenceKey,
    pub(crate) command: Command,
    pub(crate) prior: Option<Reference>,
    pub(crate) before_revision: u64,
    pub(crate) after_revision: u64,
    pub(crate) before_digest: [u8; 32],
    pub(crate) after_digest: [u8; 32],
    pub(crate) page: Option<Reference>,
    pub(crate) root: Option<[u8; 32]>,
    pub(crate) digest: [u8; 32],
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Root {
    pub key: BuildKey,
    pub binding_digest: [u8; 32],
    pub pages: u64,
    pub chain: [u8; 32],
    pub counts: [u64; 4],
    pub code_lines: u64,
    pub body_rows: u64,
    pub columns: u64,
    pub header: Option<ByteRange>,
    pub language: Option<Box<str>>,
    pub progress: SequenceKey,
    pub digest: [u8; 32],
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Head {
    pub(crate) key: ResourceKey,
    pub(crate) binding_digest: [u8; 32],
    pub(crate) build: BuildKey,
    pub(crate) root: [u8; 32],
    pub(crate) digest: [u8; 32],
}
