use super::{
    codec::{binding_bytes, enc_build_key, enc_descriptor, enc_range, enc_role, lp},
    construction::{AuthenticationRecords, ConstructionError, authenticate_source},
    display::{Fragment, read_fragment},
    query::{CellLookup, IndexedDescriptor, IndexedQuery, QueryRecords},
    source::SourceRecords,
    *,
};
use crate::codec::parts::Encoder;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Target {
    Code(u64),
    Cell {
        role: RowRole,
        row: u64,
        column: u64,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DisplayRequest {
    pub target: Target,
    pub offset: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DisplayCursor {
    pub build: BuildKey,
    pub root: [u8; 32],
    pub request: DisplayRequest,
    pub next_offset: u64,
}

pub struct DisplayResponse {
    pub binding: Binding,
    pub cursor: DisplayCursor,
    pub descriptor: Option<IndexedDescriptor>,
    pub fragment: Option<Fragment>,
    pub alignment: Option<Alignment>,
    pub literal_fallback: bool,
    pub complete: bool,
    pub encoded_bytes: usize,
}

impl DisplayResponse {
    pub(super) fn encode(&self) -> Result<Vec<u8>, ConstructionError> {
        let mut e = Encoder::new();
        lp(&mut e, &binding_bytes(&self.binding)?)?;
        enc_build_key(&mut e, self.cursor.build);
        e.fixed32(&self.cursor.root);
        match self.cursor.request.target {
            Target::Code(line) => {
                e.u8(0);
                e.u64(line);
            }
            Target::Cell { role, row, column } => {
                e.u8(1);
                enc_role(&mut e, role);
                e.u64(row);
                e.u64(column);
            }
        }
        e.u64(self.cursor.request.offset);
        e.u64(self.cursor.next_offset);
        e.u8(u8::from(self.complete));
        e.u8(u8::from(self.literal_fallback));
        e.u8(match self.alignment {
            None => 0,
            Some(Alignment::Unspecified) => 1,
            Some(Alignment::Left) => 2,
            Some(Alignment::Center) => 3,
            Some(Alignment::Right) => 4,
        });
        e.u8(u8::from(self.descriptor.is_some()));
        if let Some(value) = &self.descriptor {
            enc_descriptor(&mut e, &value.descriptor)?;
            e.u8(u8::from(value.absolute_raw.is_some()));
            if let Some(raw) = value.absolute_raw {
                enc_range(&mut e, raw)?;
            }
        }
        e.u8(u8::from(self.fragment.is_some()));
        if let Some(fragment) = &self.fragment {
            lp(&mut e, fragment.text.as_bytes())?;
            enc_range(&mut e, fragment.source)?;
            enc_range(&mut e, fragment.remaining)?;
            e.u64(fragment.next_offset);
        }
        Ok(e.finish())
    }
}

impl IndexedQuery {
    pub(super) fn display(
        &mut self,
        reader: &(impl AuthenticationRecords + SourceRecords),
        request: DisplayRequest,
        cursor: Option<DisplayCursor>,
        bytes: usize,
    ) -> Result<DisplayResponse, ConstructionError> {
        Self::validate_display_cursor(self, request, cursor, bytes)?;
        let frontier = authenticate_source(reader, &mut self.ledger, &self.binding)?;
        self.authenticated_display(reader, frontier, request, cursor, bytes)
    }

    fn validate_display_cursor(
        &self,
        request: DisplayRequest,
        cursor: Option<DisplayCursor>,
        bytes: usize,
    ) -> Result<(), ConstructionError> {
        if bytes == 0 || bytes > 65_536 {
            return Err(ConstructionError::Limit);
        }
        if cursor.is_some_and(|cursor| {
            cursor.build != self.root.key
                || cursor.root != self.root.digest
                || cursor.request != request
                || cursor.next_offset < request.offset
        }) {
            return Err(ConstructionError::Collision);
        }
        Ok(())
    }

    pub(super) fn authenticated_display(
        &mut self,
        records: &(impl QueryRecords + SourceRecords),
        encoded_frontier: u64,
        request: DisplayRequest,
        cursor: Option<DisplayCursor>,
        bytes: usize,
    ) -> Result<DisplayResponse, ConstructionError> {
        self.validate_display_cursor(request, cursor, bytes)?;
        self.ledger
            .reserve_read(0, 0, 2 * bytes + std::mem::size_of::<DisplayResponse>())?;
        let offset = cursor.map_or(request.offset, |cursor| cursor.next_offset);
        let (descriptor, alignment, literal_fallback) = match request.target {
            Target::Code(line) => (Some(self.line(records, line)?), None, false),
            Target::Cell { role, row, column } => {
                let lookup = self.cell(records, role, row, column)?;
                let alignment = match self.column(records, column)?.descriptor {
                    Descriptor::TableColumn { alignment, .. } => Some(alignment),
                    _ => return Err(ConstructionError::Corrupt),
                };
                let descriptor = match lookup {
                    CellLookup::Authored(value) | CellLookup::RowFallback(value) => Some(value),
                    CellLookup::Missing { .. } => None,
                };
                let fallback = descriptor.as_ref().is_some_and(|value| {
                    matches!(
                        value.descriptor,
                        Descriptor::TableCell {
                            presentation: CellPresentation::LiteralFallback,
                            ..
                        } | Descriptor::TableRow {
                            disposition: RowDisposition::WholeRowFallback,
                            ..
                        }
                    )
                });
                (descriptor, alignment, fallback)
            }
        };
        let content = descriptor
            .as_ref()
            .map(|value| match value.descriptor {
                Descriptor::CodeLine { content, .. } => Ok(content),
                Descriptor::TableCell { trimmed, .. } => Ok(trimmed),
                Descriptor::TableRow {
                    raw,
                    disposition: RowDisposition::WholeRowFallback,
                    ..
                } => Ok(raw),
                _ => Err(ConstructionError::Corrupt),
            })
            .transpose()?;
        if content.map_or(offset != 0, |range| offset > range.end - range.start) {
            return Err(ConstructionError::Unavailable);
        }
        let placeholder = content.map(|content| Fragment {
            text: String::new(),
            source: ByteRange {
                start: self.binding.range.start + content.start,
                end: self.binding.range.start + content.start,
            },
            remaining: ByteRange {
                start: self.binding.range.start + content.start,
                end: self.binding.range.start + content.end,
            },
            next_offset: offset,
        });
        let mut response = DisplayResponse {
            binding: self.binding.clone(),
            cursor: DisplayCursor {
                build: self.root.key,
                root: self.root.digest,
                request,
                next_offset: offset,
            },
            descriptor,
            fragment: placeholder,
            alignment,
            literal_fallback,
            complete: content.is_none(),
            encoded_bytes: 0,
        };
        let overhead = response.encode()?.len();
        if overhead > bytes {
            return Err(ConstructionError::Limit);
        }
        if let Some(content) = content {
            let limit = (bytes - overhead).min(8_192);
            if limit == 0 && offset != content.end - content.start {
                return Err(ConstructionError::Limit);
            }
            let fragment = read_fragment(
                records,
                &mut self.ledger,
                &self.binding,
                encoded_frontier,
                content,
                offset,
                limit.max(1),
            )?;
            response.cursor.next_offset = fragment.next_offset;
            response.complete = fragment.remaining.start == fragment.remaining.end;
            response.fragment = Some(fragment);
        }
        response.encoded_bytes = response.encode()?.len();
        if response.encoded_bytes > bytes {
            return Err(ConstructionError::Limit);
        }
        Ok(response)
    }
}
