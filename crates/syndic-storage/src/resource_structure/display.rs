use super::{
    Binding, ByteRange, DerivationError,
    construction::ConstructionError,
    ledger::Ledger,
    source::{SourceRecords, read_authenticated_window},
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Fragment {
    pub text: String,
    pub source: ByteRange,
    pub remaining: ByteRange,
    pub next_offset: u64,
}

pub(super) fn read_fragment(
    records: &impl SourceRecords,
    ledger: &mut Ledger,
    binding: &Binding,
    encoded_frontier: u64,
    content: ByteRange,
    offset: u64,
    limit: usize,
) -> Result<Fragment, ConstructionError> {
    if limit == 0 || limit > 8_192 {
        return Err(ConstructionError::Limit);
    }
    if !binding.range.is_valid()
        || !content.is_valid()
        || content.end > binding.range.end - binding.range.start
        || offset > content.end - content.start
    {
        return Err(ConstructionError::Unavailable);
    }
    let requested = content.start + offset;
    if requested == content.end {
        ledger.reserve_read(0, 0, std::mem::size_of::<Fragment>())?;
        let end = binding.range.start + content.end;
        return Ok(Fragment {
            text: String::new(),
            source: ByteRange { start: end, end },
            remaining: ByteRange { start: end, end },
            next_offset: offset,
        });
    }
    let start = requested.saturating_sub(3).max(content.start);
    let end = requested.saturating_add(limit as u64).min(content.end);
    ledger.reserve_read(0, 0, limit + std::mem::size_of::<Fragment>())?;
    let bytes = read_authenticated_window(
        records,
        ledger,
        binding,
        encoded_frontier,
        start,
        (end - start) as usize,
    )?;
    let mut first = (requested - start) as usize;
    if requested != content.end {
        while first > 0 && bytes[first] & 0xc0 == 0x80 {
            first -= 1;
        }
    }
    let last = (first + limit).min(bytes.len());
    let selected = &bytes[first..last];
    let valid = match std::str::from_utf8(selected) {
        Ok(_) => selected.len(),
        Err(error) if error.error_len().is_none() && start + (last as u64) < content.end => {
            error.valid_up_to()
        }
        Err(_) => return Err(DerivationError::InvalidUtf8.into()),
    };
    if valid == 0 && requested != content.end {
        return Err(ConstructionError::Limit);
    }
    let relative_start = start + first as u64;
    let relative_end = relative_start + valid as u64;
    let absolute_end = binding.range.start + relative_end;
    Ok(Fragment {
        text: std::str::from_utf8(&selected[..valid])
            .map_err(|_| DerivationError::InvalidUtf8)?
            .to_owned(),
        source: ByteRange {
            start: binding.range.start + relative_start,
            end: absolute_end,
        },
        remaining: ByteRange {
            start: absolute_end,
            end: binding.range.start + content.end,
        },
        next_offset: relative_end - content.start,
    })
}
