use super::*;

pub(crate) fn decode_composer_content(bytes: &[u8]) -> Result<ComposerPayload, SyndicRecordError> {
    let mut decoder = ContentDecoder::new(bytes);
    if decoder.u8()? != 1 {
        return Err(SyndicRecordError::InvalidContentEncoding);
    }
    let count = decoder.u64()?;
    if count > u64::try_from(bytes.len()).unwrap_or(u64::MAX) {
        return Err(SyndicRecordError::InvalidContentEncoding);
    }
    let mut atoms = Vec::new();
    for _ in 0..count {
        atoms.push(match decoder.u8()? {
            0 => {
                let length = usize::try_from(decoder.u64()?)
                    .map_err(|_| SyndicRecordError::InvalidContentEncoding)?;
                let text = std::str::from_utf8(decoder.take(length)?)
                    .map_err(|_| SyndicRecordError::InvalidContentEncoding)?;
                ComposerAtom::text(text)?
            }
            1 => {
                let marker_id = SyndicDraftMarkerId::from_bytes(
                    decoder
                        .take(16)?
                        .try_into()
                        .map_err(|_| SyndicRecordError::InvalidContentEncoding)?,
                );
                let label = ImageLabelOrdinal::new(decoder.u64()?)
                    .map_err(|_| SyndicRecordError::InvalidContentEncoding)?;
                ComposerAtom::image_marker(marker_id, label)
            }
            _ => return Err(SyndicRecordError::InvalidContentEncoding),
        });
    }
    if !decoder.is_empty() {
        return Err(SyndicRecordError::InvalidContentEncoding);
    }
    ComposerPayload::new(atoms)
}

struct ContentDecoder<'a> {
    remaining: &'a [u8],
}

impl<'a> ContentDecoder<'a> {
    const fn new(bytes: &'a [u8]) -> Self {
        Self { remaining: bytes }
    }

    fn take(&mut self, length: usize) -> Result<&'a [u8], SyndicRecordError> {
        if self.remaining.len() < length {
            return Err(SyndicRecordError::InvalidContentEncoding);
        }
        let (value, rest) = self.remaining.split_at(length);
        self.remaining = rest;
        Ok(value)
    }

    fn u8(&mut self) -> Result<u8, SyndicRecordError> {
        Ok(self.take(1)?[0])
    }

    fn u64(&mut self) -> Result<u64, SyndicRecordError> {
        Ok(u64::from_be_bytes(
            self.take(8)?
                .try_into()
                .map_err(|_| SyndicRecordError::InvalidContentEncoding)?,
        ))
    }

    const fn is_empty(&self) -> bool {
        self.remaining.is_empty()
    }
}
