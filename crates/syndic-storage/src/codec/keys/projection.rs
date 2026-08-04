use super::*;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ItemProjectionSetKey {
    pub(crate) item: SyndicItemId,
    pub(crate) generation: ItemProjectionGeneration,
}

impl ScanKey for ItemProjectionSetKey {
    fn first() -> Self {
        Self {
            item: SyndicItemId::from_bytes([0; 16]),
            generation: ItemProjectionGeneration::FIRST,
        }
    }

    fn last() -> Self {
        Self {
            item: SyndicItemId::from_bytes([u8::MAX; 16]),
            generation: ItemProjectionGeneration::new(u64::MAX).expect("maximum is nonzero"),
        }
    }
}

impl ItemProjectionSetKey {
    pub(crate) fn encode(&self) -> Vec<u8> {
        let mut e = Encoder::new();
        enc_item(&mut e, self.item);
        enc_item_projection_generation(&mut e, self.generation);
        e.finish()
    }

    pub(crate) fn decode(bytes: &[u8]) -> Result<Self, CodecError> {
        let mut d = Decoder::new(bytes);
        let key = Self {
            item: dec_item(&mut d)?,
            generation: dec_item_projection_generation(&mut d)?,
        };
        d.finish()?;
        Ok(key)
    }

    pub(crate) fn first_for_item(item: SyndicItemId) -> Self {
        Self {
            item,
            generation: ItemProjectionGeneration::FIRST,
        }
    }

    pub(crate) fn last_for_item(item: SyndicItemId) -> Self {
        Self {
            item,
            generation: ItemProjectionGeneration::new(u64::MAX).expect("maximum is nonzero"),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct StableItemProjectionKey {
    pub(crate) item: SyndicItemId,
    pub(crate) ordinal: ProjectionOrdinal,
}

impl ScanKey for StableItemProjectionKey {
    fn first() -> Self {
        Self {
            item: SyndicItemId::from_bytes([0; 16]),
            ordinal: ProjectionOrdinal::FIRST,
        }
    }

    fn last() -> Self {
        Self {
            item: SyndicItemId::from_bytes([u8::MAX; 16]),
            ordinal: ProjectionOrdinal::new(u64::MAX).expect("maximum is nonzero"),
        }
    }
}

impl StableItemProjectionKey {
    pub(crate) fn encode(&self) -> Vec<u8> {
        let mut e = Encoder::new();
        enc_item(&mut e, self.item);
        enc_projection_ord(&mut e, self.ordinal);
        e.finish()
    }

    pub(crate) fn decode(bytes: &[u8]) -> Result<Self, CodecError> {
        let mut d = Decoder::new(bytes);
        let key = Self {
            item: dec_item(&mut d)?,
            ordinal: dec_projection_ord(&mut d)?,
        };
        d.finish()?;
        Ok(key)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ItemProjectionKey {
    pub(crate) item: SyndicItemId,
    pub(crate) generation: ItemProjectionGeneration,
    pub(crate) ordinal: ProjectionOrdinal,
}

impl ScanKey for ItemProjectionKey {
    fn first() -> Self {
        Self {
            item: SyndicItemId::from_bytes([0; 16]),
            generation: ItemProjectionGeneration::FIRST,
            ordinal: ProjectionOrdinal::FIRST,
        }
    }

    fn last() -> Self {
        Self {
            item: SyndicItemId::from_bytes([u8::MAX; 16]),
            generation: ItemProjectionGeneration::new(u64::MAX).expect("maximum is nonzero"),
            ordinal: ProjectionOrdinal::new(u64::MAX).expect("maximum is nonzero"),
        }
    }
}

impl ItemProjectionKey {
    pub(crate) fn encode(&self) -> Vec<u8> {
        let mut e = Encoder::new();
        enc_item(&mut e, self.item);
        enc_item_projection_generation(&mut e, self.generation);
        enc_projection_ord(&mut e, self.ordinal);
        e.finish()
    }

    pub(crate) fn decode(bytes: &[u8]) -> Result<Self, CodecError> {
        let mut d = Decoder::new(bytes);
        let key = Self {
            item: dec_item(&mut d)?,
            generation: dec_item_projection_generation(&mut d)?,
            ordinal: dec_projection_ord(&mut d)?,
        };
        d.finish()?;
        Ok(key)
    }
}
