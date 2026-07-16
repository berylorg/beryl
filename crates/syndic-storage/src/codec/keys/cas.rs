use beryl_model::{BindingRevision, CasItemId, CasThreadId, CasTurnId};

use super::{
    super::{CodecError, parts::*},
    ScanKey,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum CasThreadKey {
    Lower,
    Record(CasThreadId),
    Upper,
}
impl ScanKey for CasThreadKey {
    fn first() -> Self {
        Self::Lower
    }
    fn last() -> Self {
        Self::Upper
    }
}
impl CasThreadKey {
    pub(crate) fn encode(&self) -> Vec<u8> {
        let mut e = Encoder::new();
        match self {
            Self::Lower => e.u8(0),
            Self::Record(id) => {
                e.u8(1);
                enc_external(&mut e, id.as_str());
            }
            Self::Upper => e.u8(2),
        }
        e.finish()
    }
    pub(crate) fn decode(bytes: &[u8]) -> Result<Self, CodecError> {
        let mut d = Decoder::new(bytes);
        let value = match d.u8()? {
            0 => Self::Lower,
            1 => Self::Record(dec_cas_thread(&mut d)?),
            2 => Self::Upper,
            tag => {
                return Err(CodecError::InvalidTag {
                    kind: "CAS thread key",
                    tag,
                });
            }
        };
        d.finish()?;
        Ok(value)
    }
    pub(crate) fn stored(&self) -> Result<(), CodecError> {
        if matches!(self, Self::Record(_)) {
            Ok(())
        } else {
            Err(CodecError::CursorSentinel)
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum CasThreadBindingKey {
    Lower,
    Record(CasThreadId, BindingRevision),
    Upper,
}

impl ScanKey for CasThreadBindingKey {
    fn first() -> Self {
        Self::Lower
    }

    fn last() -> Self {
        Self::Upper
    }
}

impl CasThreadBindingKey {
    pub(crate) fn encode(&self) -> Vec<u8> {
        let mut e = Encoder::new();
        match self {
            Self::Lower => e.u8(0),
            Self::Record(thread, revision) => {
                e.u8(1);
                enc_external(&mut e, thread.as_str());
                enc_binding_rev(&mut e, *revision);
            }
            Self::Upper => e.u8(2),
        }
        e.finish()
    }

    pub(crate) fn decode(bytes: &[u8]) -> Result<Self, CodecError> {
        let mut d = Decoder::new(bytes);
        let value = match d.u8()? {
            0 => Self::Lower,
            1 => Self::Record(dec_cas_thread(&mut d)?, dec_binding_rev(&mut d)?),
            2 => Self::Upper,
            tag => {
                return Err(CodecError::InvalidTag {
                    kind: "CAS thread binding key",
                    tag,
                });
            }
        };
        d.finish()?;
        Ok(value)
    }

    pub(crate) fn stored(&self) -> Result<(), CodecError> {
        if matches!(self, Self::Record(..)) {
            Ok(())
        } else {
            Err(CodecError::CursorSentinel)
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum CasTurnKey {
    Lower,
    Record(CasThreadId, CasTurnId),
    Upper,
}
impl ScanKey for CasTurnKey {
    fn first() -> Self {
        Self::Lower
    }
    fn last() -> Self {
        Self::Upper
    }
}
impl CasTurnKey {
    pub(crate) fn encode(&self) -> Vec<u8> {
        let mut e = Encoder::new();
        match self {
            Self::Lower => e.u8(0),
            Self::Record(thread, turn) => {
                e.u8(1);
                enc_external(&mut e, thread.as_str());
                enc_external(&mut e, turn.as_str());
            }
            Self::Upper => e.u8(2),
        }
        e.finish()
    }
    pub(crate) fn decode(bytes: &[u8]) -> Result<Self, CodecError> {
        let mut d = Decoder::new(bytes);
        let value = match d.u8()? {
            0 => Self::Lower,
            1 => Self::Record(dec_cas_thread(&mut d)?, dec_cas_turn(&mut d)?),
            2 => Self::Upper,
            tag => {
                return Err(CodecError::InvalidTag {
                    kind: "CAS turn key",
                    tag,
                });
            }
        };
        d.finish()?;
        Ok(value)
    }
    pub(crate) fn stored(&self) -> Result<(), CodecError> {
        if matches!(self, Self::Record(..)) {
            Ok(())
        } else {
            Err(CodecError::CursorSentinel)
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum CasItemKey {
    Lower,
    Record(CasThreadId, CasTurnId, CasItemId),
    Upper,
}
impl ScanKey for CasItemKey {
    fn first() -> Self {
        Self::Lower
    }
    fn last() -> Self {
        Self::Upper
    }
}
impl CasItemKey {
    pub(crate) fn encode(&self) -> Vec<u8> {
        let mut e = Encoder::new();
        match self {
            Self::Lower => e.u8(0),
            Self::Record(thread, turn, item) => {
                e.u8(1);
                enc_external(&mut e, thread.as_str());
                enc_external(&mut e, turn.as_str());
                enc_external(&mut e, item.as_str());
            }
            Self::Upper => e.u8(2),
        }
        e.finish()
    }
    pub(crate) fn decode(bytes: &[u8]) -> Result<Self, CodecError> {
        let mut d = Decoder::new(bytes);
        let value = match d.u8()? {
            0 => Self::Lower,
            1 => Self::Record(
                dec_cas_thread(&mut d)?,
                dec_cas_turn(&mut d)?,
                dec_cas_item(&mut d)?,
            ),
            2 => Self::Upper,
            tag => {
                return Err(CodecError::InvalidTag {
                    kind: "CAS item key",
                    tag,
                });
            }
        };
        d.finish()?;
        Ok(value)
    }
    pub(crate) fn stored(&self) -> Result<(), CodecError> {
        if matches!(self, Self::Record(..)) {
            Ok(())
        } else {
            Err(CodecError::CursorSentinel)
        }
    }
}
