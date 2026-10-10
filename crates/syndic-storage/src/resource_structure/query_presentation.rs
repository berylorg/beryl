use super::{
    construction::ConstructionError,
    display::{Fragment, read_fragment},
    query::{IndexedQuery, QueryRecords},
    query_response::{Item, Position, Presentation, encode_presentation},
    source::SourceRecords,
    *,
};
use crate::codec::parts::Encoder;

impl IndexedQuery {
    pub(super) fn range_presentation(
        &mut self,
        records: &(impl QueryRecords + SourceRecords),
        frontier: u64,
        item: &Item,
        position: Position,
        offset: u64,
        bytes: usize,
    ) -> Result<Presentation, ConstructionError> {
        let (content, literal_fallback) = match item {
            Item::Descriptor(value) => match value.descriptor {
                Descriptor::CodeLine { content, .. } => (Some(content), false),
                Descriptor::TableCell {
                    trimmed,
                    presentation,
                    ..
                } => (
                    Some(trimmed),
                    presentation == CellPresentation::LiteralFallback,
                ),
                Descriptor::TableRow {
                    raw,
                    disposition: RowDisposition::WholeRowFallback,
                    ..
                } => (Some(raw), true),
                _ => return Err(ConstructionError::Corrupt),
            },
            Item::Missing { .. } => (None, false),
        };
        if content.is_some_and(|content| offset > content.end - content.start) {
            return Err(ConstructionError::Unavailable);
        }
        let alignment = match position {
            Position::Code(_) => None,
            Position::Table { column, .. } => match self.column(records, column)?.descriptor {
                Descriptor::TableColumn { alignment, .. } => Some(alignment),
                _ => return Err(ConstructionError::Corrupt),
            },
        };
        let mut value = Presentation {
            fragment: content.map(|content| Fragment {
                text: String::new(),
                source: ByteRange {
                    start: self.binding.range.start + content.start + offset,
                    end: self.binding.range.start + content.start + offset,
                },
                remaining: ByteRange {
                    start: self.binding.range.start + content.start + offset,
                    end: self.binding.range.start + content.end,
                },
                next_offset: offset,
            }),
            alignment,
            literal_fallback,
        };
        let mut encoded = Encoder::new();
        encode_presentation(&mut encoded, &value)?;
        let overhead = encoded.len();
        if overhead > bytes {
            return Err(ConstructionError::Limit);
        }
        let available = (bytes - overhead).min(8_192);
        if available == 0 && content.is_some_and(|c| offset < c.end - c.start) {
            return Err(ConstructionError::Limit);
        }
        if let Some(content) = content {
            value.fragment = Some(read_fragment(
                records,
                &mut self.ledger,
                &self.binding,
                frontier,
                content,
                offset,
                available.max(1),
            )?);
        }
        Ok(value)
    }
}
