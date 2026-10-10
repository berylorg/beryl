use super::*;

pub(crate) fn advance_code(
    checkpoint: &Checkpoint,
    source: &[u8],
    backing_length: u64,
    maximum_descriptors: usize,
) -> Result<DerivationStep, DerivationError> {
    if source.len() > 65_536 || !(1..=256).contains(&maximum_descriptors) {
        return Err(DerivationError::InvalidAllowance);
    }
    codec::validate_checkpoint(checkpoint, backing_length)
        .map_err(|_| DerivationError::InvalidFrontier)?;
    if checkpoint.pass != Pass::CodeLines || checkpoint.phase != ParsePhase::Consuming {
        return Err(DerivationError::InvalidFrontier);
    }
    let offered_end = checkpoint
        .next_byte
        .checked_add(source.len() as u64)
        .ok_or(DerivationError::Overflow)?;
    if offered_end > backing_length || (source.is_empty() && offered_end != backing_length) {
        return Err(DerivationError::InvalidFrontier);
    }
    let mut next = checkpoint.clone();
    let mut descriptors = Vec::with_capacity(maximum_descriptors);
    let mut consumed = 0;
    for byte in source {
        if descriptors.len() == maximum_descriptors {
            break;
        }
        let position = next.next_byte;
        next.next_byte = position.checked_add(1).ok_or(DerivationError::Overflow)?;
        consumed += 1;
        if *byte == b'\n' {
            let content_end = if next.pending_cr {
                position
                    .checked_sub(1)
                    .ok_or(DerivationError::InvalidFrontier)?
            } else {
                position
            };
            descriptors.push(Descriptor::CodeLine {
                ordinal: next.next_row,
                raw: ByteRange {
                    start: next.line_start,
                    end: next.next_byte,
                },
                content: ByteRange {
                    start: next.line_start,
                    end: content_end,
                },
                terminator: ByteRange {
                    start: content_end,
                    end: next.next_byte,
                },
            });
            next.next_row = next
                .next_row
                .checked_add(1)
                .ok_or(DerivationError::Overflow)?;
            next.line_start = next.next_byte;
        }
        next.pending_cr = *byte == b'\r';
    }
    let mut utf8 = Vec::with_capacity(checkpoint.utf8_carry.len() + consumed);
    utf8.extend_from_slice(&checkpoint.utf8_carry);
    utf8.extend_from_slice(&source[..consumed]);
    next.utf8_carry = match std::str::from_utf8(&utf8) {
        Ok(_) => Vec::new(),
        Err(error) if error.error_len().is_none() => utf8[error.valid_up_to()..].to_vec(),
        Err(_) => return Err(DerivationError::InvalidUtf8),
    };
    if next.next_byte == backing_length {
        if !next.utf8_carry.is_empty() {
            return Err(DerivationError::InvalidUtf8);
        }
        if next.line_start < next.next_byte && descriptors.len() < maximum_descriptors {
            descriptors.push(Descriptor::CodeLine {
                ordinal: next.next_row,
                raw: ByteRange {
                    start: next.line_start,
                    end: next.next_byte,
                },
                content: ByteRange {
                    start: next.line_start,
                    end: next.next_byte,
                },
                terminator: ByteRange {
                    start: next.next_byte,
                    end: next.next_byte,
                },
            });
            next.next_row = next
                .next_row
                .checked_add(1)
                .ok_or(DerivationError::Overflow)?;
            next.line_start = next.next_byte;
            next.pending_cr = false;
        }
        if next.line_start == next.next_byte {
            next.phase = ParsePhase::Eof;
            next.pending_cr = false;
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
