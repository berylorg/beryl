use crate::codec::parts::Encoder;

use super::{
    codec::{binding_bytes, enc_build_key, enc_descriptor, enc_range, enc_role, lp},
    construction::authenticate_source,
    construction::{AuthenticationRecords, ConstructionError},
    display::Fragment,
    query::{CellLookup, IndexedDescriptor, IndexedQuery, QueryRecords},
    source::SourceRecords,
    *,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OrdinalRange {
    pub start: u64,
    pub end: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Request {
    Code {
        lines: OrdinalRange,
    },
    Table {
        role: RowRole,
        rows: OrdinalRange,
        columns: OrdinalRange,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Position {
    Code(u64),
    Table { row: u64, column: u64 },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Cursor {
    pub build: BuildKey,
    pub root_digest: [u8; 32],
    pub request: Request,
    pub next: Position,
    pub fragment_offset: u64,
}

#[derive(Clone, Copy)]
pub struct ResponseLimits {
    pub descriptors: usize,
    pub bytes: usize,
}

impl Default for ResponseLimits {
    fn default() -> Self {
        Self {
            descriptors: 256,
            bytes: 65_536,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Item {
    Descriptor(IndexedDescriptor),
    Missing {
        role: RowRole,
        row: u64,
        column: u64,
    },
}

pub struct Response {
    pub binding: Binding,
    pub cursor: Cursor,
    pub items: Vec<Item>,
    pub request_complete: bool,
    pub resource_complete: bool,
    pub encoded_bytes: usize,
    pub presentations: Option<Vec<Presentation>>,
}

pub struct Presentation {
    pub fragment: Option<Fragment>,
    pub alignment: Option<Alignment>,
    pub literal_fallback: bool,
}

fn encode_cursor(e: &mut Encoder, cursor: Cursor) {
    enc_build_key(e, cursor.build);
    e.fixed32(&cursor.root_digest);
    match cursor.request {
        Request::Code { lines } => {
            e.u8(0);
            e.u64(lines.start);
            e.u64(lines.end);
        }
        Request::Table {
            role,
            rows,
            columns,
        } => {
            e.u8(1);
            enc_role(e, role);
            for value in [rows.start, rows.end, columns.start, columns.end] {
                e.u64(value);
            }
        }
    }
    match cursor.next {
        Position::Code(line) => e.u64(line),
        Position::Table { row, column } => {
            e.u64(row);
            e.u64(column);
        }
    }
    e.u64(cursor.fragment_offset);
}

fn encode_item(e: &mut Encoder, item: &Item) -> Result<(), ConstructionError> {
    match item {
        Item::Descriptor(value) => {
            e.u8(0);
            enc_descriptor(e, &value.descriptor)?;
            e.u8(u8::from(value.absolute_raw.is_some()));
            if let Some(raw) = value.absolute_raw {
                enc_range(e, raw)?;
            }
        }
        Item::Missing { role, row, column } => {
            e.u8(1);
            enc_role(e, *role);
            e.u64(*row);
            e.u64(*column);
        }
    }
    Ok(())
}

impl Response {
    fn header(&self, e: &mut Encoder) -> Result<(), ConstructionError> {
        lp(e, &binding_bytes(&self.binding)?)?;
        encode_cursor(e, self.cursor);
        e.u8(u8::from(self.request_complete));
        e.u8(u8::from(self.resource_complete));
        e.u64(self.items.len() as u64);
        e.u8(u8::from(self.presentations.is_some()));
        Ok(())
    }

    pub(super) fn encode(&self) -> Result<Vec<u8>, ConstructionError> {
        let mut e = Encoder::new();
        self.header(&mut e)?;
        for (index, item) in self.items.iter().enumerate() {
            encode_item(&mut e, item)?;
            if let Some(presentations) = &self.presentations {
                encode_presentation(&mut e, &presentations[index])?;
            }
        }
        Ok(e.finish())
    }
}

pub(super) fn encode_presentation(
    e: &mut Encoder,
    value: &Presentation,
) -> Result<(), ConstructionError> {
    e.u8(u8::from(value.literal_fallback));
    e.u8(match value.alignment {
        None => 0,
        Some(Alignment::Unspecified) => 1,
        Some(Alignment::Left) => 2,
        Some(Alignment::Center) => 3,
        Some(Alignment::Right) => 4,
    });
    e.u8(u8::from(value.fragment.is_some()));
    if let Some(fragment) = &value.fragment {
        lp(e, fragment.text.as_bytes())?;
        enc_range(e, fragment.source)?;
        enc_range(e, fragment.remaining)?;
        e.u64(fragment.next_offset);
    }
    Ok(())
}

impl IndexedQuery {
    pub(super) fn range(
        &mut self,
        records: &(impl QueryRecords + SourceRecords),
        request: Request,
        cursor: Option<Cursor>,
        limits: ResponseLimits,
    ) -> Result<Response, ConstructionError> {
        self.range_with_display(records, request, cursor, limits, None)
    }

    fn range_bounds(
        &self,
        request: Request,
        cursor: Option<Cursor>,
        limits: ResponseLimits,
        display: bool,
    ) -> Result<(u64, u64, u64, u64, Position), ConstructionError> {
        if limits.descriptors == 0
            || limits.descriptors > 256
            || limits.bytes == 0
            || limits.bytes > 65_536
        {
            return Err(ConstructionError::Limit);
        }
        let (start, end, column_start, column_end, total) = match request {
            Request::Code { lines } => {
                if lines.start > lines.end
                    || !matches!(
                        self.binding.metadata.structure(),
                        crate::ResourceStructure::Code { .. }
                    )
                {
                    return Err(ConstructionError::Unavailable);
                }
                (
                    lines.start.min(self.root.code_lines),
                    lines.end.min(self.root.code_lines),
                    0,
                    0,
                    self.root.code_lines,
                )
            }
            Request::Table {
                role,
                rows,
                columns,
            } => {
                if rows.start > rows.end
                    || columns.start > columns.end
                    || !matches!(
                        self.binding.metadata.structure(),
                        crate::ResourceStructure::Table { .. }
                    )
                {
                    return Err(ConstructionError::Unavailable);
                }
                let total = match role {
                    RowRole::Header => 1,
                    RowRole::Body => self.root.body_rows,
                };
                (
                    rows.start.min(total),
                    rows.end.min(total),
                    columns.start.min(self.root.columns),
                    columns.end.min(self.root.columns),
                    total,
                )
            }
        };
        let first = match request {
            Request::Code { .. } => Position::Code(start),
            Request::Table { .. } => Position::Table {
                row: if column_start == column_end {
                    end
                } else {
                    start
                },
                column: column_start,
            },
        };
        let mut next = first;
        if let Some(cursor) = cursor {
            if cursor.build != self.root.key
                || cursor.root_digest != self.root.digest
                || cursor.request != request
            {
                return Err(ConstructionError::Collision);
            }
            let valid = match cursor.next {
                Position::Code(line) => {
                    matches!(request, Request::Code { .. }) && start <= line && line <= end
                }
                Position::Table { row, column } => {
                    matches!(request, Request::Table { .. })
                        && start <= row
                        && row <= end
                        && if row == end || column_start == column_end {
                            row == end && column == column_start
                        } else {
                            column_start <= column && column < column_end
                        }
                }
            };
            if !valid {
                return Err(ConstructionError::Unavailable);
            }
            if cursor.fragment_offset != 0
                && (!display || !matches!(cursor.next, Position::Code(line) if line < end))
            {
                return Err(ConstructionError::Unavailable);
            }
            next = cursor.next;
        }
        Ok((end, column_start, column_end, total, next))
    }

    pub(super) fn range_display(
        &mut self,
        reader: &(impl AuthenticationRecords + SourceRecords),
        request: Request,
        cursor: Option<Cursor>,
        limits: ResponseLimits,
    ) -> Result<Response, ConstructionError> {
        self.range_bounds(request, cursor, limits, true)?;
        let frontier = authenticate_source(reader, &mut self.ledger, &self.binding)?;
        self.authenticated_range_display(reader, frontier, request, cursor, limits)
    }

    pub(super) fn authenticated_range_display(
        &mut self,
        records: &(impl QueryRecords + SourceRecords),
        encoded_frontier: u64,
        request: Request,
        cursor: Option<Cursor>,
        limits: ResponseLimits,
    ) -> Result<Response, ConstructionError> {
        self.range_with_display(records, request, cursor, limits, Some(encoded_frontier))
    }

    fn range_with_display(
        &mut self,
        records: &(impl QueryRecords + SourceRecords),
        request: Request,
        cursor: Option<Cursor>,
        limits: ResponseLimits,
        frontier: Option<u64>,
    ) -> Result<Response, ConstructionError> {
        let (end, column_start, column_end, total, next) =
            self.range_bounds(request, cursor, limits, frontier.is_some())?;
        // Reserve the complete response and its temporary encoding before any demanded lookup.
        self.ledger.reserve_read(
            0,
            0,
            limits.bytes * 2
                + 128
                + limits.descriptors * std::mem::size_of::<Item>()
                + usize::from(frontier.is_some())
                    * limits.descriptors
                    * std::mem::size_of::<Presentation>()
                + binding_bytes(&self.binding)?.len() * 2,
        )?;
        let mut response = Response {
            binding: self.binding.clone(),
            cursor: Cursor {
                build: self.root.key,
                root_digest: self.root.digest,
                request,
                next,
                fragment_offset: cursor.map_or(0, |cursor| cursor.fragment_offset),
            },
            items: Vec::with_capacity(limits.descriptors),
            request_complete: false,
            resource_complete: false,
            encoded_bytes: 0,
            presentations: frontier.map(|_| Vec::with_capacity(limits.descriptors)),
        };
        let mut header = Encoder::new();
        response.header(&mut header)?;
        response.encoded_bytes = header.len();
        if response.encoded_bytes > limits.bytes {
            return Err(ConstructionError::Limit);
        }
        while response.items.len() < limits.descriptors {
            let candidate = (|| -> Result<_, ConstructionError> {
                Ok(Some(match response.cursor.next {
                    Position::Code(line) if line < end => (
                        Item::Descriptor(self.line(records, line)?),
                        Position::Code(line + 1),
                    ),
                    Position::Table { row, column } if row < end => {
                        let Request::Table { role, .. } = request else {
                            return Err(ConstructionError::Corrupt);
                        };
                        let cell = self.cell(records, role, row, column)?;
                        let fallback = matches!(cell, CellLookup::RowFallback(_));
                        let item = match cell {
                            CellLookup::Authored(value) | CellLookup::RowFallback(value) => {
                                Item::Descriptor(value)
                            }
                            CellLookup::Missing { role, row, column } => {
                                Item::Missing { role, row, column }
                            }
                        };
                        let following = if fallback || column + 1 == column_end {
                            Position::Table {
                                row: row + 1,
                                column: column_start,
                            }
                        } else {
                            Position::Table {
                                row,
                                column: column + 1,
                            }
                        };
                        (item, following)
                    }
                    _ => return Ok(None),
                }))
            })();
            let (item, mut following) = match candidate {
                Ok(Some(value)) => value,
                Ok(None) => break,
                Err(ConstructionError::Limit) if !response.items.is_empty() => break,
                Err(error) => return Err(error),
            };
            let mut encoded = Encoder::new();
            encode_item(&mut encoded, &item)?;
            let mut next_offset = 0;
            let presentation = if let Some(frontier) = frontier {
                let value = match self.range_presentation(
                    records,
                    frontier,
                    &item,
                    response.cursor.next,
                    response.cursor.fragment_offset,
                    limits
                        .bytes
                        .saturating_sub(response.encoded_bytes + encoded.len()),
                ) {
                    Ok(value) => value,
                    Err(ConstructionError::Limit) if !response.items.is_empty() => break,
                    Err(error) => return Err(error),
                };
                if let Some(fragment) = &value.fragment {
                    if matches!(request, Request::Code { .. })
                        && fragment.remaining.start < fragment.remaining.end
                    {
                        following = response.cursor.next;
                        next_offset = fragment.next_offset;
                    }
                }
                encode_presentation(&mut encoded, &value)?;
                Some(value)
            } else {
                None
            };
            if response.encoded_bytes + encoded.len() > limits.bytes {
                if response.items.is_empty() {
                    return Err(ConstructionError::Limit);
                }
                break;
            }
            response.encoded_bytes += encoded.len();
            response.items.push(item);
            if let (Some(values), Some(value)) = (&mut response.presentations, presentation) {
                values.push(value);
            }
            response.cursor.next = following;
            response.cursor.fragment_offset = next_offset;
            if next_offset != 0 {
                break;
            }
        }
        response.request_complete = match response.cursor.next {
            Position::Code(line) => line == end,
            Position::Table { row, .. } => row == end,
        };
        response.resource_complete = response.request_complete
            && end == total
            && match request {
                Request::Code { .. } => true,
                Request::Table { role, .. } => {
                    column_end == self.root.columns
                        && (role == RowRole::Body || self.root.body_rows == 0)
                }
            };
        Ok(response)
    }
}
