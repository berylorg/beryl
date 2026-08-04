use std::str;

use beryl_home_store::HomeStore;

use crate::{
    ProjectionTextSource, RecoveryProjectionError, SyndicStorage,
    read::{ReadByteTotals, read_projection_text_source_range_into},
};

/// Maximum UTF-8 payload returned by one recovery cursor page.
pub const RECOVERY_CURSOR_PAGE_MAX_UTF8_BYTES: usize = crate::TRANSCRIPT_PAGE_MAX_BYTES;

pub(super) fn read_recovery_utf8_page(
    storage: &SyndicStorage,
    store: &HomeStore,
    source: ProjectionTextSource,
    offset: u64,
    max_utf8_bytes: usize,
    output: &mut [u8],
) -> Result<(usize, ReadByteTotals), RecoveryProjectionError> {
    if max_utf8_bytes == 0 {
        return Err(RecoveryProjectionError::InvalidCursorPageLimit {
            actual: max_utf8_bytes,
        });
    }
    let remaining = source.logical_utf8_bytes().checked_sub(offset).ok_or(
        RecoveryProjectionError::CursorMismatch {
            reason: "item-local cursor offset exceeds the declared text length",
        },
    )?;
    if remaining == 0 {
        return Err(RecoveryProjectionError::CursorMismatch {
            reason: "recovery cursor attempted to emit an empty text page",
        });
    }
    let requested = remaining
        .min(max_utf8_bytes as u64)
        .min(output.len() as u64)
        .min(RECOVERY_CURSOR_PAGE_MAX_UTF8_BYTES as u64);
    if requested == 0 {
        return Err(RecoveryProjectionError::InvalidCursorPageLimit { actual: 0 });
    }
    let end = offset
        .checked_add(requested)
        .ok_or(RecoveryProjectionError::CursorMismatch {
            reason: "item-local cursor offset overflowed",
        })?;
    let requested =
        usize::try_from(requested).map_err(|_| RecoveryProjectionError::CursorMismatch {
            reason: "bounded recovery page length overflowed",
        })?;
    let bytes = output
        .get_mut(..requested)
        .ok_or(RecoveryProjectionError::CursorMismatch {
            reason: "bounded recovery page exceeds its caller-provided storage",
        })?;
    let totals =
        read_projection_text_source_range_into(storage, store, source, offset, end, bytes)?;

    let valid_len = match str::from_utf8(bytes) {
        Ok(_) => bytes.len(),
        Err(error)
            if end < source.logical_utf8_bytes()
                && error.error_len().is_none()
                && error.valid_up_to() != 0 =>
        {
            error.valid_up_to()
        }
        Err(error)
            if end < source.logical_utf8_bytes()
                && error.error_len().is_none()
                && error.valid_up_to() == 0 =>
        {
            return Err(RecoveryProjectionError::CursorPageLimitTooSmall {
                offset,
                actual: requested,
            });
        }
        Err(_) => {
            return Err(RecoveryProjectionError::CursorMismatch {
                reason: "canonical recovery text is not valid UTF-8",
            });
        }
    };
    if valid_len == 0 {
        return Err(RecoveryProjectionError::CursorMismatch {
            reason: "bounded recovery text page made no UTF-8 progress",
        });
    }
    Ok((valid_len, totals))
}
