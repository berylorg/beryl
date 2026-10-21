use std::num::NonZeroU64;

use beryl_home_store::{PointReadLimit, RecordCodec, RecordVersion};
use beryl_model::{AssetId, SyndicItemId, SyndicResourceId, SyndicThreadId, SyndicTurnId};

use super::{
    AssetDimensions, AssetDomain, AssetMediaType, GENERATED_MEDIA_ADMISSION_MAX_BYTES,
    GeneratedMediaFacts, GeneratedMediaLifecycle, GeneratedMediaRecord,
};
use crate::{RecordRevision, encoding::CodecError};

pub(crate) struct GeneratedMediaCodec;

impl RecordCodec<AssetDomain> for GeneratedMediaCodec {
    type Key = SyndicResourceId;
    type Value = GeneratedMediaRecord;
    type Error = CodecError;
    const FAMILY: &'static str = "generated-media-admissions";
    const VERSION: RecordVersion = RecordVersion::new(1);
    const MAX_KEY_BYTES: usize = 16;
    const MAX_VALUE_BYTES: usize = GENERATED_MEDIA_ADMISSION_MAX_BYTES;

    fn encode_key(key: &Self::Key) -> Result<Vec<u8>, Self::Error> {
        Ok(key.as_bytes().to_vec())
    }
    fn decode_key(bytes: &[u8]) -> Result<Self::Key, Self::Error> {
        Ok(SyndicResourceId::from_bytes(
            bytes.try_into().map_err(|_| invalid())?,
        ))
    }
    fn encode_value(value: &Self::Value) -> Result<Vec<u8>, Self::Error> {
        let mut out = Vec::with_capacity(Self::MAX_VALUE_BYTES);
        out.extend_from_slice(value.thread.as_bytes());
        out.extend_from_slice(value.turn.as_bytes());
        out.extend_from_slice(value.item.as_bytes());
        out.extend_from_slice(&value.fingerprint);
        out.push(1);
        out.extend_from_slice(&value.facts.asset.digest());
        out.extend_from_slice(&value.facts.asset.length().get().to_le_bytes());
        let media = value.facts.media_type.as_str().as_bytes();
        out.push(u8::try_from(media.len()).map_err(|_| invalid())?);
        out.extend_from_slice(media);
        match value.facts.dimensions {
            None => out.push(0),
            Some(d) => {
                out.push(1);
                out.extend_from_slice(
                    &u32::try_from(d.width().get())
                        .map_err(|_| invalid())?
                        .to_le_bytes(),
                );
                out.extend_from_slice(
                    &u32::try_from(d.height().get())
                        .map_err(|_| invalid())?
                        .to_le_bytes(),
                );
            }
        }
        out.push(match value.lifecycle {
            GeneratedMediaLifecycle::Prepared => 1,
            GeneratedMediaLifecycle::Published => 2,
        });
        out.extend_from_slice(&value.revision.get().to_le_bytes());
        Ok(out)
    }
    fn decode_value(bytes: &[u8]) -> Result<Self::Value, Self::Error> {
        if bytes.len() > Self::MAX_VALUE_BYTES {
            return Err(invalid());
        }
        let mut d = Bytes(bytes);
        let thread = SyndicThreadId::from_bytes(d.fixed()?);
        let turn = SyndicTurnId::from_bytes(d.fixed()?);
        let item = SyndicItemId::from_bytes(d.fixed()?);
        let fingerprint = d.fixed()?;
        if d.byte()? != 1 {
            return Err(invalid());
        }
        let asset = AssetId::sha256_v1(
            d.fixed()?,
            NonZeroU64::new(u64::from_le_bytes(d.fixed()?)).ok_or_else(invalid)?,
        );
        let media_len = usize::from(d.byte()?);
        if media_len == 0 || media_len > 127 {
            return Err(invalid());
        }
        let media_type =
            AssetMediaType::new(std::str::from_utf8(d.take(media_len)?).map_err(|_| invalid())?)
                .map_err(|_| invalid())?;
        let dimensions = match d.byte()? {
            0 => None,
            1 => {
                let width = NonZeroU64::new(u64::from(u32::from_le_bytes(d.fixed()?)))
                    .ok_or_else(invalid)?;
                let height = NonZeroU64::new(u64::from(u32::from_le_bytes(d.fixed()?)))
                    .ok_or_else(invalid)?;
                Some(AssetDimensions::new(width, height))
            }
            _ => return Err(invalid()),
        };
        let lifecycle = match d.byte()? {
            1 => GeneratedMediaLifecycle::Prepared,
            2 => GeneratedMediaLifecycle::Published,
            _ => return Err(invalid()),
        };
        let revision =
            RecordRevision::new(u64::from_le_bytes(d.fixed()?)).map_err(|_| invalid())?;
        if !d.0.is_empty()
            || (lifecycle == GeneratedMediaLifecycle::Prepared
                && revision != RecordRevision::INITIAL)
            || (lifecycle == GeneratedMediaLifecycle::Published && revision.get() != 2)
        {
            return Err(invalid());
        }
        Ok(GeneratedMediaRecord {
            thread,
            turn,
            item,
            fingerprint,
            facts: GeneratedMediaFacts {
                asset,
                media_type,
                dimensions,
            },
            lifecycle,
            revision,
        })
    }
}

fn invalid() -> CodecError {
    CodecError::InvalidLength {
        kind: "generated media admission facts",
    }
}
struct Bytes<'a>(&'a [u8]);
impl<'a> Bytes<'a> {
    fn take(&mut self, len: usize) -> Result<&'a [u8], CodecError> {
        if self.0.len() < len {
            return Err(CodecError::Truncated);
        }
        let (out, remaining) = self.0.split_at(len);
        self.0 = remaining;
        Ok(out)
    }
    fn byte(&mut self) -> Result<u8, CodecError> {
        Ok(self.take(1)?[0])
    }
    fn fixed<const N: usize>(&mut self) -> Result<[u8; N], CodecError> {
        self.take(N)?.try_into().map_err(|_| invalid())
    }
}
pub(super) fn point_limit() -> PointReadLimit {
    PointReadLimit::new(
        GENERATED_MEDIA_ADMISSION_MAX_BYTES + beryl_home_store::RECORD_VERSION_BYTES,
    )
    .expect("fixed admission bound")
}
