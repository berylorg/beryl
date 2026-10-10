use super::*;
use crate::table_syntax;

pub(super) fn admit(
    checkpoint: &Checkpoint,
    source: &[u8],
    end: u64,
    backing_length: u64,
) -> Result<(Checkpoint, usize), DerivationError> {
    if source.len() > 65_536 {
        return Err(DerivationError::InvalidAllowance);
    }
    codec::validate_checkpoint(checkpoint, backing_length)
        .map_err(|_| DerivationError::InvalidFrontier)?;
    if checkpoint.phase != ParsePhase::Consuming
        || checkpoint.next_byte > end
        || end > backing_length
        || checkpoint.next_column != 0
        || checkpoint.output_position != 0
        || checkpoint.pending_cr
        || checkpoint.escaped
        || checkpoint.in_code
        || checkpoint
            .syntax_start
            .checked_add(checkpoint.syntax.len() as u64)
            .and_then(|position| position.checked_add(checkpoint.utf8_carry.len() as u64))
            != Some(checkpoint.next_byte)
    {
        return Err(DerivationError::InvalidFrontier);
    }
    let remaining = end - checkpoint.next_byte;
    let consumed = source
        .len()
        .min(usize::try_from(remaining).unwrap_or(usize::MAX));
    if consumed == 0 && remaining != 0 {
        return Err(DerivationError::InvalidFrontier);
    }
    if checkpoint.syntax.len() + checkpoint.utf8_carry.len() + consumed > 16_384 {
        return Err(DerivationError::InvalidAllowance);
    }
    let mut next = checkpoint.clone();
    let mut joined = checkpoint.utf8_carry.clone();
    joined.extend_from_slice(&source[..consumed]);
    let complete = match std::str::from_utf8(&joined) {
        Ok(_) => joined.len(),
        Err(error) if error.error_len().is_none() => error.valid_up_to(),
        Err(_) => return Err(DerivationError::InvalidUtf8),
    };
    next.syntax.extend_from_slice(&joined[..complete]);
    next.utf8_carry = joined[complete..].to_vec();
    next.next_byte = next
        .next_byte
        .checked_add(consumed as u64)
        .ok_or(DerivationError::Overflow)?;
    if next.next_byte == end {
        if !next.utf8_carry.is_empty() {
            return Err(DerivationError::InvalidUtf8);
        }
        next.phase = ParsePhase::DrainingSyntax;
    }
    codec::validate_checkpoint(&next, backing_length)
        .map_err(|_| DerivationError::InvalidFrontier)?;
    Ok((next, consumed))
}

pub(crate) fn advance_cells(
    checkpoint: &Checkpoint,
    row: &Descriptor,
    source: &[u8],
    backing_length: u64,
    maximum_descriptors: usize,
) -> Result<DerivationStep, DerivationError> {
    if source.len() > 65_536 {
        return Err(DerivationError::InvalidAllowance);
    }
    if checkpoint.pass != Pass::TableCells {
        return Err(DerivationError::InvalidFrontier);
    }
    if checkpoint.phase == ParsePhase::DrainingSyntax {
        return drain_cells(checkpoint, row, backing_length, maximum_descriptors);
    }
    if !(1..=256).contains(&maximum_descriptors) {
        return Err(DerivationError::InvalidAllowance);
    }
    let Descriptor::TableRow {
        role,
        ordinal,
        raw,
        disposition,
        ..
    } = *row
    else {
        return Err(DerivationError::InvalidFrontier);
    };
    if !raw.is_valid()
        || raw.start == raw.end
        || raw.end > backing_length
        || role != checkpoint.role
        || ordinal != checkpoint.next_row
        || (role == RowRole::Header && ordinal != 0)
    {
        return Err(DerivationError::InvalidFrontier);
    }
    let mut next = checkpoint.clone();
    if next.syntax.is_empty() && next.utf8_carry.is_empty() {
        if next.next_byte != raw.start {
            return Err(DerivationError::InvalidFrontier);
        }
        next.syntax_start = raw.start;
    }
    if next.syntax_start != raw.start {
        return Err(DerivationError::InvalidFrontier);
    }
    if disposition == RowDisposition::WholeRowFallback {
        codec::validate_checkpoint(checkpoint, backing_length)
            .map_err(|_| DerivationError::InvalidFrontier)?;
        if !source.is_empty()
            || !checkpoint.syntax.is_empty()
            || !checkpoint.utf8_carry.is_empty()
            || checkpoint.phase != ParsePhase::Consuming
            || checkpoint.next_column != 0
            || checkpoint.output_position != 0
            || checkpoint.escaped
            || checkpoint.in_code
            || checkpoint.pending_cr
        {
            return Err(DerivationError::InvalidFrontier);
        }
        next.next_byte = raw.end;
        clear_window(&mut next);
        finish_row(&mut next, role)?;
        return Ok(DerivationStep {
            checkpoint: next,
            descriptors: Vec::new(),
            consumed: 0,
        });
    }
    let (next, consumed) = admit(&next, source, raw.end, backing_length)?;
    if next.phase == ParsePhase::DrainingSyntax {
        let mut step = drain_cells(&next, row, backing_length, maximum_descriptors)?;
        step.consumed = consumed;
        Ok(step)
    } else {
        Ok(DerivationStep {
            checkpoint: next,
            descriptors: Vec::new(),
            consumed,
        })
    }
}

pub(crate) fn advance_columns(
    checkpoint: &Checkpoint,
    source: &[u8],
    columns: u64,
    backing_length: u64,
    maximum_descriptors: usize,
) -> Result<DerivationStep, DerivationError> {
    if source.len() > 65_536 {
        return Err(DerivationError::InvalidAllowance);
    }
    if checkpoint.pass != Pass::Columns {
        return Err(DerivationError::InvalidFrontier);
    }
    if checkpoint.phase == ParsePhase::DrainingSyntax {
        return drain_columns(checkpoint, columns, backing_length, maximum_descriptors);
    }
    if !(1..=256).contains(&maximum_descriptors) || source.len() > 65_536 {
        return Err(DerivationError::InvalidAllowance);
    }
    let mut next = checkpoint.clone();
    if next.syntax.is_empty() && next.utf8_carry.is_empty() {
        next.syntax_start = next.next_byte;
    }
    let consumed = source
        .iter()
        .position(|byte| *byte == b'\n')
        .map(|position| position + 1)
        .unwrap_or(source.len());
    let offered_end = next
        .next_byte
        .checked_add(consumed as u64)
        .ok_or(DerivationError::Overflow)?;
    if offered_end > backing_length {
        return Err(DerivationError::InvalidFrontier);
    }
    let complete =
        consumed < source.len() || source.last() == Some(&b'\n') || offered_end == backing_length;
    let end = if complete {
        offered_end
    } else {
        backing_length
    };
    let (mut next, consumed) = admit(&next, &source[..consumed], end, backing_length)?;
    if complete {
        next.phase = ParsePhase::DrainingSyntax;
        let mut step = drain_columns(&next, columns, backing_length, maximum_descriptors)?;
        step.consumed = consumed;
        Ok(step)
    } else {
        Ok(DerivationStep {
            checkpoint: next,
            descriptors: Vec::new(),
            consumed,
        })
    }
}

fn finish_row(next: &mut Checkpoint, role: RowRole) -> Result<(), DerivationError> {
    next.phase = ParsePhase::Consuming;
    if role == RowRole::Header {
        next.role = RowRole::Body;
        next.next_row = 0;
    } else {
        next.next_row = next
            .next_row
            .checked_add(1)
            .ok_or(DerivationError::Overflow)?;
    }
    Ok(())
}

pub(crate) fn table_row(
    source: &str,
    raw: ByteRange,
    role: RowRole,
    ordinal: u64,
    columns: u64,
) -> Result<Descriptor, DerivationError> {
    if source.len() > 16_384 {
        return Err(DerivationError::InvalidAllowance);
    }
    if !raw.is_valid()
        || raw.end - raw.start != source.len() as u64
        || columns == 0
        || (role == RowRole::Header && ordinal != 0)
        || table_syntax::content(source).contains('\n')
    {
        return Err(DerivationError::InvalidFrontier);
    }
    let authored_cells = table_syntax::columns(source).ok_or(DerivationError::InvalidSyntax)?;
    if role == RowRole::Header && authored_cells != columns {
        return Err(DerivationError::InvalidSyntax);
    }
    Ok(Descriptor::TableRow {
        role,
        ordinal,
        raw,
        authored_cells,
        disposition: if authored_cells > columns {
            RowDisposition::WholeRowFallback
        } else {
            RowDisposition::Ordinary
        },
    })
}

fn window<'a>(
    checkpoint: &'a Checkpoint,
    length: u64,
    pass: Pass,
    maximum: usize,
) -> Result<&'a str, DerivationError> {
    if !(1..=256).contains(&maximum) {
        return Err(DerivationError::InvalidAllowance);
    }
    codec::validate_checkpoint(checkpoint, length).map_err(|_| DerivationError::InvalidFrontier)?;
    if checkpoint.pass != pass
        || checkpoint.phase != ParsePhase::DrainingSyntax
        || !checkpoint.utf8_carry.is_empty()
        || checkpoint.pending_cr
        || checkpoint
            .syntax_start
            .checked_add(checkpoint.syntax.len() as u64)
            != Some(checkpoint.next_byte)
    {
        return Err(DerivationError::InvalidFrontier);
    }
    let source =
        std::str::from_utf8(&checkpoint.syntax).map_err(|_| DerivationError::InvalidUtf8)?;
    if table_syntax::content(source).contains('\n') {
        return Err(DerivationError::InvalidSyntax);
    }
    Ok(source)
}

fn clear_window(next: &mut Checkpoint) {
    next.line_start = 0;
    next.syntax_start = 0;
    next.syntax.clear();
    next.output_position = 0;
    next.escaped = false;
    next.in_code = false;
    next.next_column = 0;
}

pub(crate) fn drain_cells(
    checkpoint: &Checkpoint,
    row: &Descriptor,
    backing_length: u64,
    maximum_descriptors: usize,
) -> Result<DerivationStep, DerivationError> {
    let source = window(
        checkpoint,
        backing_length,
        Pass::TableCells,
        maximum_descriptors,
    )?;
    let Descriptor::TableRow {
        role,
        ordinal,
        raw,
        authored_cells,
        disposition,
    } = *row
    else {
        return Err(DerivationError::InvalidFrontier);
    };
    if disposition != RowDisposition::Ordinary
        || role != checkpoint.role
        || ordinal != checkpoint.next_row
        || raw.start != checkpoint.syntax_start
        || raw.end != checkpoint.next_byte
        || table_syntax::columns(source) != Some(authored_cells)
        || (role == RowRole::Header && ordinal != 0)
        || checkpoint.next_column >= authored_cells
    {
        return Err(DerivationError::InvalidFrontier);
    }
    let bounds = table_syntax::cell_bounds(source);
    let mut position = if checkpoint.next_column == 0 {
        if checkpoint.output_position != 0 || checkpoint.escaped || checkpoint.in_code {
            return Err(DerivationError::InvalidFrontier);
        }
        bounds.start
    } else {
        usize::try_from(checkpoint.output_position).map_err(|_| DerivationError::Overflow)?
    };
    if position < bounds.start || position > bounds.end || !source.is_char_boundary(position) {
        return Err(DerivationError::InvalidFrontier);
    }
    let mut next = checkpoint.clone();
    let mut descriptors = Vec::with_capacity(maximum_descriptors);
    while descriptors.len() < maximum_descriptors {
        let start = position;
        while position < bounds.end {
            if table_syntax::separator(
                source.as_bytes()[position],
                &mut next.escaped,
                &mut next.in_code,
            ) {
                break;
            }
            position += 1;
        }
        let cell = &source[start..position];
        let leading = cell.len() - cell.trim_start_matches([' ', '\t']).len();
        let trimmed = cell[leading..].trim_end_matches([' ', '\t']);
        let absolute = |offset: usize| {
            raw.start
                .checked_add(offset as u64)
                .ok_or(DerivationError::Overflow)
        };
        descriptors.push(Descriptor::TableCell {
            role,
            row: ordinal,
            column: next.next_column,
            raw: ByteRange {
                start: absolute(start)?,
                end: absolute(position)?,
            },
            trimmed: ByteRange {
                start: absolute(start + leading)?,
                end: absolute(start + leading + trimmed.len())?,
            },
            presentation: if trimmed.bytes().any(|byte| {
                matches!(
                    byte,
                    b'\\' | b'`' | b'*' | b'_' | b'[' | b']' | b'<' | b'>' | b'&' | b'~'
                )
            }) {
                CellPresentation::LiteralFallback
            } else {
                CellPresentation::Literal
            },
        });
        next.next_column = next
            .next_column
            .checked_add(1)
            .ok_or(DerivationError::Overflow)?;
        if position == bounds.end {
            if next.next_column != authored_cells {
                return Err(DerivationError::InvalidSyntax);
            }
            clear_window(&mut next);
            finish_row(&mut next, role)?;
            break;
        }
        position += 1;
        next.output_position = position as u64;
    }
    codec::validate_checkpoint(&next, backing_length)
        .map_err(|_| DerivationError::InvalidFrontier)?;
    Ok(DerivationStep {
        checkpoint: next,
        descriptors,
        consumed: 0,
    })
}

pub(crate) fn drain_columns(
    checkpoint: &Checkpoint,
    columns: u64,
    backing_length: u64,
    maximum_descriptors: usize,
) -> Result<DerivationStep, DerivationError> {
    let source = window(
        checkpoint,
        backing_length,
        Pass::Columns,
        maximum_descriptors,
    )?;
    if table_syntax::delimiter_columns(source) != Some(columns)
        || checkpoint.next_column >= columns
        || checkpoint.next_row != 0
        || checkpoint.escaped
        || checkpoint.in_code
    {
        return Err(DerivationError::InvalidSyntax);
    }
    let bounds = table_syntax::cell_bounds(source);
    let mut position = if checkpoint.next_column == 0 {
        if checkpoint.output_position != 0 {
            return Err(DerivationError::InvalidFrontier);
        }
        bounds.start
    } else {
        checkpoint.output_position as usize
    };
    if position < bounds.start || position > bounds.end || !source.is_char_boundary(position) {
        return Err(DerivationError::InvalidFrontier);
    }
    let mut next = checkpoint.clone();
    let mut descriptors = Vec::with_capacity(maximum_descriptors);
    while descriptors.len() < maximum_descriptors {
        let end = source[position..bounds.end]
            .find('|')
            .map(|offset| position + offset)
            .unwrap_or(bounds.end);
        let (left, right) = table_syntax::delimiter_alignment(&source[position..end])
            .ok_or(DerivationError::InvalidSyntax)?;
        descriptors.push(Descriptor::TableColumn {
            ordinal: next.next_column,
            alignment: match (left, right) {
                (false, false) => Alignment::Unspecified,
                (true, false) => Alignment::Left,
                (false, true) => Alignment::Right,
                (true, true) => Alignment::Center,
            },
        });
        next.next_column = next
            .next_column
            .checked_add(1)
            .ok_or(DerivationError::Overflow)?;
        if end == bounds.end {
            if next.next_column != columns {
                return Err(DerivationError::InvalidSyntax);
            }
            clear_window(&mut next);
            next.phase = ParsePhase::Eof;
            break;
        }
        position = end + 1;
        next.output_position = position as u64;
    }
    codec::validate_checkpoint(&next, backing_length)
        .map_err(|_| DerivationError::InvalidFrontier)?;
    Ok(DerivationStep {
        checkpoint: next,
        descriptors,
        consumed: 0,
    })
}
