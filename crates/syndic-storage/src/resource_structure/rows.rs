use super::*;
use crate::table_syntax;

pub(crate) fn advance_rows(
    checkpoint: &Checkpoint,
    source: &[u8],
    backing_length: u64,
    header: ByteRange,
    columns: u64,
) -> Result<DerivationStep, DerivationError> {
    if header.start != 0
        || header.end == 0
        || header.end >= backing_length
        || checkpoint.pass != Pass::TableRows
        || columns == 0
        || checkpoint.phase != ParsePhase::Consuming
    {
        return Err(DerivationError::InvalidFrontier);
    }
    if checkpoint.role == RowRole::Header {
        let remaining = header
            .end
            .checked_sub(checkpoint.next_byte)
            .ok_or(DerivationError::InvalidFrontier)?;
        if remaining == 0 {
            return Err(DerivationError::InvalidFrontier);
        }
        if source.len() > 65_536 {
            return Err(DerivationError::InvalidAllowance);
        }
        let consumed = source
            .len()
            .min(usize::try_from(remaining).unwrap_or(usize::MAX));
        let step = advance_row(checkpoint, &source[..consumed], backing_length, columns)?;
        if let Some(Descriptor::TableRow { raw, .. }) = step.descriptors.first() {
            if *raw != header {
                return Err(DerivationError::InvalidSyntax);
            }
        } else if step.checkpoint.next_byte == header.end {
            return Err(DerivationError::InvalidSyntax);
        }
        return Ok(step);
    }
    if checkpoint.next_row == 0 && checkpoint.line_start == header.end {
        let consumed = source
            .iter()
            .position(|byte| *byte == b'\n')
            .map(|position| position + 1)
            .unwrap_or(source.len());
        if source.len() > 65_536 {
            return Err(DerivationError::InvalidAllowance);
        }
        let end = checkpoint
            .next_byte
            .checked_add(consumed as u64)
            .ok_or(DerivationError::Overflow)?;
        let closed = source.get(consumed.wrapping_sub(1)) == Some(&b'\n') || end == backing_length;
        let mut next = checkpoint.clone();
        if next.syntax.is_empty() && next.utf8_carry.is_empty() && next.next_byte == header.end {
            next.syntax_start = header.end;
        }
        if next.syntax_start != header.end {
            return Err(DerivationError::InvalidFrontier);
        }
        let (mut next, consumed) = table::admit(
            &next,
            &source[..consumed],
            if closed { end } else { backing_length },
            backing_length,
        )?;
        if closed {
            let syntax =
                std::str::from_utf8(&next.syntax).map_err(|_| DerivationError::InvalidUtf8)?;
            if table_syntax::delimiter_columns(syntax) != Some(columns) {
                return Err(DerivationError::InvalidSyntax);
            }
            next.syntax.clear();
            next.syntax_start = 0;
            next.line_start = next.next_byte;
            next.phase = ParsePhase::Consuming;
            if next.next_byte == backing_length {
                next.line_start = 0;
                next.phase = ParsePhase::Eof;
            }
        }
        return Ok(DerivationStep {
            checkpoint: next,
            descriptors: Vec::new(),
            consumed,
        });
    }
    if checkpoint.line_start <= header.end {
        return Err(DerivationError::InvalidFrontier);
    }
    advance_row(checkpoint, source, backing_length, columns)
}

pub(crate) fn advance_row(
    checkpoint: &Checkpoint,
    source: &[u8],
    backing_length: u64,
    columns: u64,
) -> Result<DerivationStep, DerivationError> {
    if source.len() > 65_536 {
        return Err(DerivationError::InvalidAllowance);
    }
    codec::validate_checkpoint(checkpoint, backing_length)
        .map_err(|_| DerivationError::InvalidFrontier)?;
    if checkpoint.pass != Pass::TableRows
        || checkpoint.phase != ParsePhase::Consuming
        || columns == 0
        || checkpoint.pending_cr
        || (checkpoint.role == RowRole::Header && checkpoint.next_row != 0)
        || !(checkpoint.syntax.is_empty() || checkpoint.syntax == b"|")
        || (checkpoint.syntax.is_empty() && checkpoint.output_position != 0)
        || checkpoint.line_start > checkpoint.next_byte - checkpoint.utf8_carry.len() as u64
        || (checkpoint.next_column == 0
            && (!checkpoint.syntax.is_empty() || checkpoint.escaped || checkpoint.in_code))
    {
        return Err(DerivationError::InvalidFrontier);
    }
    let offered_end = checkpoint
        .next_byte
        .checked_add(source.len() as u64)
        .ok_or(DerivationError::Overflow)?;
    if offered_end > backing_length || (source.is_empty() && offered_end != backing_length) {
        return Err(DerivationError::InvalidFrontier);
    }
    let consumed = source
        .iter()
        .position(|byte| *byte == b'\n')
        .map(|position| position + 1)
        .unwrap_or(source.len());
    let mut joined = checkpoint.utf8_carry.clone();
    joined.extend_from_slice(&source[..consumed]);
    let complete = match std::str::from_utf8(&joined) {
        Ok(_) => joined.len(),
        Err(error) if error.error_len().is_none() => error.valid_up_to(),
        Err(_) => return Err(DerivationError::InvalidUtf8),
    };
    let text =
        std::str::from_utf8(&joined[..complete]).map_err(|_| DerivationError::InvalidUtf8)?;
    let mut next = checkpoint.clone();
    next.next_byte = checkpoint
        .next_byte
        .checked_add(consumed as u64)
        .ok_or(DerivationError::Overflow)?;
    next.utf8_carry = joined[complete..].to_vec();
    let text_start = checkpoint.next_byte - checkpoint.utf8_carry.len() as u64;
    for (offset, character) in text.char_indices() {
        if character == '\n' {
            break;
        }
        if next.next_column == 0 {
            if character.is_whitespace() {
                continue;
            }
            next.next_column = 1;
            if character == '|' {
                continue;
            }
        }
        let mut separator = false;
        let mut encoded = [0_u8; 4];
        for byte in character.encode_utf8(&mut encoded).bytes() {
            separator |= table_syntax::separator(byte, &mut next.escaped, &mut next.in_code);
        }
        if separator {
            next.next_column = next
                .next_column
                .checked_add(1)
                .ok_or(DerivationError::Overflow)?;
        }
        if !character.is_whitespace() {
            next.syntax.clear();
            next.syntax_start = 0;
            next.output_position = 0;
            if character == '|' {
                // A final outer pipe is removed even when escaped or inside backticks.
                next.syntax.push(b'|');
                next.syntax_start = text_start
                    .checked_add(offset as u64)
                    .ok_or(DerivationError::Overflow)?;
                next.output_position = u64::from(separator);
            }
        }
    }
    let closed =
        source.get(consumed.wrapping_sub(1)) == Some(&b'\n') || next.next_byte == backing_length;
    let mut descriptors = Vec::new();
    if closed {
        if !next.utf8_carry.is_empty() {
            return Err(DerivationError::InvalidUtf8);
        }
        let authored_cells = next
            .next_column
            .checked_sub(next.output_position)
            .ok_or(DerivationError::InvalidFrontier)?;
        if authored_cells < 2
            || (next.role == RowRole::Header && authored_cells != columns)
            || next.line_start == next.next_byte
        {
            return Err(DerivationError::InvalidSyntax);
        }
        let raw = ByteRange {
            start: next.line_start,
            end: next.next_byte,
        };
        let disposition = if raw.end - raw.start > 16_384 || authored_cells > columns {
            RowDisposition::WholeRowFallback
        } else {
            RowDisposition::Ordinary
        };
        descriptors.push(Descriptor::TableRow {
            role: next.role,
            ordinal: next.next_row,
            raw,
            authored_cells,
            disposition,
        });
        if next.role == RowRole::Header {
            next.role = RowRole::Body;
        } else {
            next.next_row = next
                .next_row
                .checked_add(1)
                .ok_or(DerivationError::Overflow)?;
        }
        next.next_column = 0;
        next.syntax.clear();
        next.syntax_start = 0;
        next.output_position = 0;
        next.escaped = false;
        next.in_code = false;
        next.line_start = next.next_byte;
        if next.next_byte == backing_length {
            next.phase = ParsePhase::Eof;
            next.line_start = 0;
        }
    }
    codec::validate_checkpoint(&next, backing_length)
        .map_err(|_| DerivationError::InvalidFrontier)?;
    Ok(DerivationStep {
        checkpoint: next,
        descriptors,
        consumed,
    })
}
