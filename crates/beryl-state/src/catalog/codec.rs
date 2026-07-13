use std::{error::Error, fmt};

use beryl_home_store::{RecordCodec, RecordVersion};
use beryl_model::SyndicThreadId;

use crate::UnixMillis;

use super::{
    CATALOG_RECORD_LIMIT, CatalogArchiveSummary, CatalogDomain, CatalogFacts, CatalogFreshness,
    CatalogRecencyCursor, CatalogRevision, CatalogRow,
};

#[path = "codec/parts.rs"]
mod parts;

use parts::*;

#[derive(Debug)]
pub(super) enum CatalogCodecError {
    Truncated,
    TrailingBytes,
    InvalidTag {
        kind: &'static str,
        tag: u8,
    },
    InvalidLength {
        kind: &'static str,
    },
    InvalidUtf8 {
        kind: &'static str,
    },
    InvalidValue {
        kind: &'static str,
        source: Box<dyn Error + Send + Sync>,
    },
}

pub(super) struct CatalogRowCodec;
pub(super) struct CatalogRecencyCodec;

impl fmt::Display for CatalogCodecError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Truncated => formatter.write_str("catalog record is truncated"),
            Self::TrailingBytes => formatter.write_str("catalog record has trailing bytes"),
            Self::InvalidTag { kind, tag } => write!(formatter, "invalid {kind} tag {tag}"),
            Self::InvalidLength { kind } => write!(formatter, "invalid {kind} byte length"),
            Self::InvalidUtf8 { kind } => write!(formatter, "{kind} is not valid UTF-8"),
            Self::InvalidValue { kind, source } => write!(formatter, "invalid {kind}: {source}"),
        }
    }
}

impl Error for CatalogCodecError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::InvalidValue { source, .. } => Some(source.as_ref()),
            _ => None,
        }
    }
}

struct Encoder {
    bytes: Vec<u8>,
}

impl Encoder {
    fn new() -> Self {
        Self { bytes: Vec::new() }
    }

    fn finish(self) -> Vec<u8> {
        self.bytes
    }

    fn u8(&mut self, value: u8) {
        self.bytes.push(value);
    }

    fn u16(&mut self, value: u16) {
        self.bytes.extend_from_slice(&value.to_be_bytes());
    }

    fn u64(&mut self, value: u64) {
        self.bytes.extend_from_slice(&value.to_be_bytes());
    }

    fn fixed(&mut self, value: &[u8; 16]) {
        self.bytes.extend_from_slice(value);
    }

    fn text(&mut self, value: &str) {
        let length = u32::try_from(value.len()).expect("bounded catalog text fits u32");
        self.bytes.extend_from_slice(&length.to_be_bytes());
        self.bytes.extend_from_slice(value.as_bytes());
    }
}

struct Decoder<'a> {
    remaining: &'a [u8],
}

impl<'a> Decoder<'a> {
    const fn new(bytes: &'a [u8]) -> Self {
        Self { remaining: bytes }
    }

    fn finish(self) -> Result<(), CatalogCodecError> {
        if self.remaining.is_empty() {
            Ok(())
        } else {
            Err(CatalogCodecError::TrailingBytes)
        }
    }

    fn take(&mut self, length: usize) -> Result<&'a [u8], CatalogCodecError> {
        if self.remaining.len() < length {
            return Err(CatalogCodecError::Truncated);
        }
        let (value, remaining) = self.remaining.split_at(length);
        self.remaining = remaining;
        Ok(value)
    }

    fn u8(&mut self) -> Result<u8, CatalogCodecError> {
        Ok(self.take(1)?[0])
    }

    fn u16(&mut self) -> Result<u16, CatalogCodecError> {
        let bytes = self
            .take(2)?
            .try_into()
            .map_err(|_| CatalogCodecError::Truncated)?;
        Ok(u16::from_be_bytes(bytes))
    }

    fn u64(&mut self) -> Result<u64, CatalogCodecError> {
        let bytes = self
            .take(8)?
            .try_into()
            .map_err(|_| CatalogCodecError::Truncated)?;
        Ok(u64::from_be_bytes(bytes))
    }

    fn fixed(&mut self) -> Result<[u8; 16], CatalogCodecError> {
        self.take(16)?
            .try_into()
            .map_err(|_| CatalogCodecError::Truncated)
    }

    fn text(&mut self, kind: &'static str) -> Result<&'a str, CatalogCodecError> {
        let bytes = self
            .take(4)?
            .try_into()
            .map_err(|_| CatalogCodecError::Truncated)?;
        let length = u32::from_be_bytes(bytes) as usize;
        std::str::from_utf8(self.take(length)?).map_err(|_| CatalogCodecError::InvalidUtf8 { kind })
    }
}

fn invalid(kind: &'static str, source: impl Error + Send + Sync + 'static) -> CatalogCodecError {
    CatalogCodecError::InvalidValue {
        kind,
        source: Box::new(source),
    }
}

fn decode_thread_id(encoded: &[u8]) -> Result<SyndicThreadId, CatalogCodecError> {
    encoded
        .try_into()
        .map(SyndicThreadId::from_bytes)
        .map_err(|_| CatalogCodecError::InvalidLength {
            kind: "Syndic thread identity",
        })
}

impl RecordCodec<CatalogDomain> for CatalogRowCodec {
    type Key = SyndicThreadId;
    type Value = CatalogRow;
    type Error = CatalogCodecError;

    const FAMILY: &'static str = "rows";
    const VERSION: RecordVersion = RecordVersion::new(1);
    const MAX_KEY_BYTES: usize = 16;
    const MAX_VALUE_BYTES: usize = CATALOG_RECORD_LIMIT;

    fn encode_key(key: &Self::Key) -> Result<Vec<u8>, Self::Error> {
        Ok(key.as_bytes().to_vec())
    }

    fn decode_key(encoded: &[u8]) -> Result<Self::Key, Self::Error> {
        decode_thread_id(encoded)
    }

    fn encode_value(value: &Self::Value) -> Result<Vec<u8>, Self::Error> {
        encode_row(value)
    }

    fn decode_value(encoded: &[u8]) -> Result<Self::Value, Self::Error> {
        decode_row(encoded)
    }
}

impl RecordCodec<CatalogDomain> for CatalogRecencyCodec {
    type Key = CatalogRecencyCursor;
    type Value = CatalogRow;
    type Error = CatalogCodecError;

    const FAMILY: &'static str = "recency";
    const VERSION: RecordVersion = RecordVersion::new(1);
    const MAX_KEY_BYTES: usize = 24;
    const MAX_VALUE_BYTES: usize = CATALOG_RECORD_LIMIT;

    fn encode_key(key: &Self::Key) -> Result<Vec<u8>, Self::Error> {
        let mut encoded = Vec::with_capacity(24);
        encoded.extend_from_slice(&(!key.last_activity_at().get()).to_be_bytes());
        encoded.extend_from_slice(key.thread_id().as_bytes());
        Ok(encoded)
    }

    fn decode_key(encoded: &[u8]) -> Result<Self::Key, Self::Error> {
        if encoded.len() != 24 {
            return Err(CatalogCodecError::InvalidLength {
                kind: "catalog recency key",
            });
        }
        let activity: [u8; 8] = encoded[..8]
            .try_into()
            .map_err(|_| CatalogCodecError::Truncated)?;
        let thread_id = decode_thread_id(&encoded[8..])?;
        Ok(CatalogRecencyCursor::new(
            UnixMillis::new(!u64::from_be_bytes(activity)),
            thread_id,
        ))
    }

    fn encode_value(value: &Self::Value) -> Result<Vec<u8>, Self::Error> {
        encode_row(value)
    }

    fn decode_value(encoded: &[u8]) -> Result<Self::Value, Self::Error> {
        decode_row(encoded)
    }
}

fn encode_row(row: &CatalogRow) -> Result<Vec<u8>, CatalogCodecError> {
    let mut encoder = Encoder::new();
    encoder.fixed(row.thread_id().as_bytes());
    encode_sources(&mut encoder, row.sources());
    encoder.u8(match row.freshness() {
        CatalogFreshness::Current => 0,
        CatalogFreshness::Stale => 1,
    });
    encode_titles(&mut encoder, row.facts().titles());
    encode_execution(&mut encoder, row.facts().execution());
    encoder.u8(match row.facts().archive() {
        CatalogArchiveSummary::Ordinary => 0,
        CatalogArchiveSummary::BranchDiscussionOpen => 1,
        CatalogArchiveSummary::BranchDiscussionArchived => 2,
    });
    encoder.u64(row.facts().last_activity_at().get());
    encode_claim(&mut encoder, row.facts().claim());
    encode_lineage(&mut encoder, row.facts().lineage());
    encode_search(&mut encoder, row.facts().search());
    encoder.u64(row.revision().get());
    Ok(encoder.finish())
}

fn decode_row(encoded: &[u8]) -> Result<CatalogRow, CatalogCodecError> {
    let mut decoder = Decoder::new(encoded);
    let thread_id = SyndicThreadId::from_bytes(decoder.fixed()?);
    let sources = decode_sources(&mut decoder)?;
    let freshness = match decoder.u8()? {
        0 => CatalogFreshness::Current,
        1 => CatalogFreshness::Stale,
        tag => {
            return Err(CatalogCodecError::InvalidTag {
                kind: "catalog freshness",
                tag,
            });
        }
    };
    let titles = decode_titles(&mut decoder)?;
    let execution = decode_execution(&mut decoder)?;
    let archive = decode_archive(&mut decoder)?;
    let last_activity_at = UnixMillis::new(decoder.u64()?);
    let claim = decode_claim(&mut decoder)?;
    let lineage = decode_lineage(&mut decoder)?;
    let search = decode_search(&mut decoder)?;
    let revision = CatalogRevision::new(decoder.u64()?)
        .map_err(|source| invalid("catalog revision", source))?;
    decoder.finish()?;
    CatalogRow::from_parts(
        thread_id,
        sources,
        freshness,
        CatalogFacts::new(
            titles,
            execution,
            archive,
            last_activity_at,
            claim,
            lineage,
            search,
        ),
        revision,
    )
    .map_err(|source| invalid("catalog row", source))
}
