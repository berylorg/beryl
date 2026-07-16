mod binding;
mod content;
mod digest;
mod history;
mod index;
mod input_gate;
mod payload;
mod projection;

pub use binding::*;
pub use content::*;
pub use digest::*;
pub use history::*;
pub use index::*;
pub use input_gate::*;
pub use payload::*;
pub use projection::*;

pub(crate) const MAX_COMPOSER_IMAGE_MARKERS: usize = 1_024;
pub(crate) const MAX_LARGE_TEXT_BYTES: usize = 262_144;
pub(crate) const MAX_INLINE_TEXT_BYTES: usize = 65_536;
pub(crate) const MAX_REASON_BYTES: usize = 1_024;
pub(crate) const MAX_MEDIA_TYPE_BYTES: usize = 256;

pub(crate) fn validate_text(
    kind: &'static str,
    value: &str,
    maximum: usize,
    allow_empty: bool,
) -> Result<Box<str>, crate::SyndicRecordError> {
    if !allow_empty && value.is_empty() {
        return Err(crate::SyndicRecordError::Empty { kind });
    }
    if value.len() > maximum {
        return Err(crate::SyndicRecordError::BytesTooLong {
            kind,
            maximum,
            actual: value.len(),
        });
    }
    if let Some(index) = value.as_bytes().iter().position(|byte| *byte == 0) {
        return Err(crate::SyndicRecordError::NulByte { kind, index });
    }
    Ok(value.into())
}
