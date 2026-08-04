use beryl_home_store::DomainReader;
use sha2::{Digest, Sha256};

use crate::{ContentReference, domain::SyndicDomain, error::SyndicValidationError};

use super::FrameSpanError;

pub(super) fn validate_source_range(
    reader: &DomainReader<'_, SyndicDomain>,
    content: ContentReference,
    available_encoded_bytes: u64,
    source_start: u64,
    source_end: u64,
    source_digest: [u8; 32],
) -> Result<(), FrameSpanError> {
    const PAGE_BYTES: u64 = 65_536;
    if source_end > available_encoded_bytes {
        return Err(FrameSpanError::Invariant(
            "provider span source range exceeds its staged content frontier",
        ));
    }
    let mut hash = Sha256::new();
    let mut utf8 = Utf8Pages::default();
    let mut start = source_start;
    while start < source_end {
        let end = start.saturating_add(PAGE_BYTES).min(source_end);
        let bytes = super::super::content::read_encoded_range(
            reader,
            content.id(),
            available_encoded_bytes,
            start,
            end,
        )
        .map_err(frame_span_validation_error)?;
        hash.update(&bytes);
        if !utf8.observe(&bytes) {
            return Err(FrameSpanError::Invariant(
                "provider span source range is not UTF-8",
            ));
        }
        start = end;
    }
    let digest: [u8; 32] = hash.finalize().into();
    if !utf8.finish() || digest != source_digest {
        return Err(FrameSpanError::Invariant(
            "provider span source range digest disagrees",
        ));
    }
    Ok(())
}

fn frame_span_validation_error(error: SyndicValidationError) -> FrameSpanError {
    match error {
        SyndicValidationError::Read(source) => FrameSpanError::Read(source),
        SyndicValidationError::Invariant(message) => FrameSpanError::Invariant(message),
    }
}

#[derive(Default)]
struct Utf8Pages(Vec<u8>);

impl Utf8Pages {
    fn observe(&mut self, bytes: &[u8]) -> bool {
        let mut combined = std::mem::take(&mut self.0);
        combined.extend_from_slice(bytes);
        match std::str::from_utf8(&combined) {
            Ok(_) => true,
            Err(error) if error.error_len().is_none() => {
                self.0.extend_from_slice(&combined[error.valid_up_to()..]);
                self.0.len() <= 3
            }
            Err(_) => false,
        }
    }

    fn finish(&self) -> bool {
        self.0.is_empty()
    }
}
