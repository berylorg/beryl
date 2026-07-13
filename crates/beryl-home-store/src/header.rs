use std::{fmt, num::NonZeroU32};

use beryl_model::BerylHomeId;

const HEADER_MAGIC: &[u8; 8] = b"BRYLHOME";
const HEADER_ENCODING_VERSION: u16 = 1;
const HEADER_LENGTH: usize = HEADER_MAGIC.len() + 2 + 4 + 16;

/// Exact durable schema understood by one Beryl-home opener.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct HomeSchemaVersion(NonZeroU32);

impl HomeSchemaVersion {
    /// Initial Beryl-home schema implemented by this rework.
    pub const CURRENT: Self = Self(NonZeroU32::MIN);

    /// Constructs a nonzero durable schema version.
    #[must_use]
    pub const fn new(value: u32) -> Option<Self> {
        match NonZeroU32::new(value) {
            Some(value) => Some(Self(value)),
            None => None,
        }
    }

    /// Returns the durable numeric schema version.
    #[must_use]
    pub const fn get(self) -> u32 {
        self.0.get()
    }
}

impl fmt::Display for HomeSchemaVersion {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.get().fmt(formatter)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct HomeHeader {
    pub(crate) schema: HomeSchemaVersion,
    pub(crate) home_id: BerylHomeId,
}

impl HomeHeader {
    pub(crate) fn encode(self) -> [u8; HEADER_LENGTH] {
        let mut encoded = [0; HEADER_LENGTH];
        encoded[..8].copy_from_slice(HEADER_MAGIC);
        encoded[8..10].copy_from_slice(&HEADER_ENCODING_VERSION.to_be_bytes());
        encoded[10..14].copy_from_slice(&self.schema.get().to_be_bytes());
        encoded[14..].copy_from_slice(self.home_id.as_bytes());
        encoded
    }

    pub(crate) fn decode(encoded: &[u8]) -> Result<Self, HeaderDecodeError> {
        if encoded.len() != HEADER_LENGTH {
            return Err(HeaderDecodeError::WrongLength {
                actual: encoded.len(),
            });
        }
        if &encoded[..8] != HEADER_MAGIC {
            return Err(HeaderDecodeError::WrongMagic);
        }

        let encoding = u16::from_be_bytes([encoded[8], encoded[9]]);
        if encoding != HEADER_ENCODING_VERSION {
            return Err(HeaderDecodeError::UnsupportedEncoding { found: encoding });
        }

        let schema = u32::from_be_bytes([encoded[10], encoded[11], encoded[12], encoded[13]]);
        let schema = HomeSchemaVersion::new(schema).ok_or(HeaderDecodeError::ZeroSchema)?;
        let mut home_id = [0; 16];
        home_id.copy_from_slice(&encoded[14..]);

        Ok(Self {
            schema,
            home_id: BerylHomeId::from_bytes(home_id),
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub(crate) enum HeaderDecodeError {
    #[error("home header has {actual} bytes instead of {HEADER_LENGTH}")]
    WrongLength { actual: usize },
    #[error("home header magic is invalid")]
    WrongMagic,
    #[error("home header encoding {found} is unsupported")]
    UnsupportedEncoding { found: u16 },
    #[error("home schema version must be nonzero")]
    ZeroSchema,
}
