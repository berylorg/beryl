use beryl_model::SyndicContentId;

use crate::ContentPieceOrdinal;

use super::{
    CodecError, Decoder, Encoder, ScanKey, dec_content, dec_content_piece_ord, enc_content,
    enc_content_piece_ord,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ContentByteSpanKey {
    pub(crate) owner: SyndicContentId,
    pub(crate) start: u64,
}

impl ScanKey for ContentByteSpanKey {
    fn first() -> Self {
        Self {
            owner: SyndicContentId::from_bytes([0; 16]),
            start: 0,
        }
    }

    fn last() -> Self {
        Self {
            owner: SyndicContentId::from_bytes([u8::MAX; 16]),
            start: u64::MAX,
        }
    }
}

impl ContentByteSpanKey {
    pub(crate) fn encode(self) -> Vec<u8> {
        let mut encoder = Encoder::new();
        enc_content(&mut encoder, self.owner);
        encoder.u64(self.start);
        encoder.finish()
    }

    pub(crate) fn decode(encoded: &[u8]) -> Result<Self, CodecError> {
        let mut decoder = Decoder::new(encoded);
        let key = Self {
            owner: dec_content(&mut decoder)?,
            start: decoder.u64()?,
        };
        decoder.finish()?;
        Ok(key)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ContentTextSpanKey {
    pub(crate) owner: SyndicContentId,
    pub(crate) logical_start: u64,
}

impl ScanKey for ContentTextSpanKey {
    fn first() -> Self {
        Self {
            owner: SyndicContentId::from_bytes([0; 16]),
            logical_start: 0,
        }
    }

    fn last() -> Self {
        Self {
            owner: SyndicContentId::from_bytes([u8::MAX; 16]),
            logical_start: u64::MAX,
        }
    }
}

impl ContentTextSpanKey {
    pub(crate) fn encode(self) -> Vec<u8> {
        let mut encoder = Encoder::new();
        enc_content(&mut encoder, self.owner);
        encoder.u64(self.logical_start);
        encoder.finish()
    }

    pub(crate) fn decode(encoded: &[u8]) -> Result<Self, CodecError> {
        let mut decoder = Decoder::new(encoded);
        let key = Self {
            owner: dec_content(&mut decoder)?,
            logical_start: decoder.u64()?,
        };
        decoder.finish()?;
        Ok(key)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ContentPieceKey {
    pub(crate) owner: SyndicContentId,
    pub(crate) ordinal: ContentPieceOrdinal,
}

impl ScanKey for ContentPieceKey {
    fn first() -> Self {
        Self {
            owner: SyndicContentId::from_bytes([0; 16]),
            ordinal: ContentPieceOrdinal::FIRST,
        }
    }

    fn last() -> Self {
        Self {
            owner: SyndicContentId::from_bytes([u8::MAX; 16]),
            ordinal: ContentPieceOrdinal::new(u64::MAX).expect("maximum is nonzero"),
        }
    }
}

impl ContentPieceKey {
    pub(crate) fn encode(self) -> Vec<u8> {
        let mut encoder = Encoder::new();
        enc_content(&mut encoder, self.owner);
        enc_content_piece_ord(&mut encoder, self.ordinal);
        encoder.finish()
    }

    pub(crate) fn decode(encoded: &[u8]) -> Result<Self, CodecError> {
        let mut decoder = Decoder::new(encoded);
        let key = Self {
            owner: dec_content(&mut decoder)?,
            ordinal: dec_content_piece_ord(&mut decoder)?,
        };
        decoder.finish()?;
        Ok(key)
    }
}
