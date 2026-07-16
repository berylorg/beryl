mod item;

use std::num::NonZeroU64;

use beryl_model::{AssetId, CasItemId, CasThreadId};

use super::{PROVIDER_FRAME_BOUNDED_DECODE_MAX_BYTES, ProviderFrameDecodeError, tags};
use crate::provider_item::*;

pub fn decode_bounded_provider_item_frame_v1(
    encoded: &[u8],
    maximum_bytes: usize,
    prior_content_frontier: u64,
) -> Result<ProviderItemFrameV1, ProviderFrameDecodeError> {
    let maximum = maximum_bytes.min(PROVIDER_FRAME_BOUNDED_DECODE_MAX_BYTES);
    if encoded.len() > maximum {
        return Err(ProviderFrameDecodeError::FrameTooLarge {
            maximum,
            actual: encoded.len(),
        });
    }
    let mut decoder = Decoder::new(encoded);
    if decoder.take(4)? != tags::MAGIC {
        return Err(ProviderFrameDecodeError::InvalidTag {
            kind: "magic/version",
            tag: encoded.first().copied().unwrap_or_default(),
        });
    }
    let ordinal = ProviderFrameOrdinalV1::new(decoder.u64()?)?;
    let item_id = decoder.cas_item_id()?;
    let observation = match decoder.u8()? {
        tags::OBSERVATION_STARTED => ProviderItemObservationV1::Started {
            observed_at: ProviderLifecycleTimestampMsV1::new(decoder.u64()?),
            item: decoder.item()?,
        },
        tags::OBSERVATION_DELTA => ProviderItemObservationV1::Delta(decoder.delta()?),
        tags::OBSERVATION_COMPLETED => ProviderItemObservationV1::Completed {
            observed_at: ProviderLifecycleTimestampMsV1::new(decoder.u64()?),
            item: decoder.item()?,
        },
        tag => {
            return Err(ProviderFrameDecodeError::InvalidTag {
                kind: "observation",
                tag,
            });
        }
    };
    decoder.finish()?;
    let frame = ProviderItemFrameV1::new(ordinal, item_id, observation);
    frame.validate(prior_content_frontier)?;
    Ok(frame)
}

pub(super) struct Decoder<'a> {
    remaining: &'a [u8],
}

impl<'a> Decoder<'a> {
    fn new(encoded: &'a [u8]) -> Self {
        Self { remaining: encoded }
    }

    fn finish(self) -> Result<(), ProviderFrameDecodeError> {
        if self.remaining.is_empty() {
            Ok(())
        } else {
            Err(ProviderFrameDecodeError::TrailingBytes)
        }
    }

    pub(super) fn take(&mut self, length: usize) -> Result<&'a [u8], ProviderFrameDecodeError> {
        if self.remaining.len() < length {
            return Err(ProviderFrameDecodeError::Truncated);
        }
        let (value, remaining) = self.remaining.split_at(length);
        self.remaining = remaining;
        Ok(value)
    }

    pub(super) fn u8(&mut self) -> Result<u8, ProviderFrameDecodeError> {
        Ok(self.take(1)?[0])
    }

    pub(super) fn u32(&mut self) -> Result<u32, ProviderFrameDecodeError> {
        Ok(u32::from_be_bytes(
            self.take(4)?.try_into().expect("exact four-byte slice"),
        ))
    }

    pub(super) fn i32(&mut self) -> Result<i32, ProviderFrameDecodeError> {
        Ok(i32::from_be_bytes(
            self.take(4)?.try_into().expect("exact four-byte slice"),
        ))
    }

    pub(super) fn u64(&mut self) -> Result<u64, ProviderFrameDecodeError> {
        Ok(u64::from_be_bytes(
            self.take(8)?.try_into().expect("exact eight-byte slice"),
        ))
    }

    pub(super) fn i64(&mut self) -> Result<i64, ProviderFrameDecodeError> {
        Ok(i64::from_be_bytes(
            self.take(8)?.try_into().expect("exact eight-byte slice"),
        ))
    }

    pub(super) fn count(&mut self, kind: &'static str) -> Result<usize, ProviderFrameDecodeError> {
        let count = usize::try_from(self.u64()?)
            .map_err(|_| ProviderFrameDecodeError::InvalidLength { kind })?;
        if count > self.remaining.len() {
            return Err(ProviderFrameDecodeError::InvalidLength { kind });
        }
        Ok(count)
    }

    pub(super) fn raw_text(
        &mut self,
        kind: &'static str,
    ) -> Result<String, ProviderFrameDecodeError> {
        let length = self.count(kind)?;
        let bytes = self.take(length)?;
        std::str::from_utf8(bytes)
            .map(str::to_owned)
            .map_err(|_| ProviderFrameDecodeError::InvalidUtf8 { kind })
    }

    pub(super) fn text(
        &mut self,
        kind: &'static str,
    ) -> Result<ProviderTextV1, ProviderFrameDecodeError> {
        match self.u8()? {
            tags::TEXT_INLINE => self.raw_text(kind).map(ProviderTextV1::Inline),
            tags::TEXT_REUSED => {
                let start = self.u64()?;
                let end = self.u64()?;
                let digest = self
                    .take(32)?
                    .try_into()
                    .expect("exact 32-byte digest slice");
                Ok(ProviderTextV1::reused(ProviderTextReferenceV1::new(
                    start, end, digest,
                )?))
            }
            tag => Err(ProviderFrameDecodeError::InvalidTag { kind, tag }),
        }
    }

    pub(super) fn option<T>(
        &mut self,
        kind: &'static str,
        decode: impl FnOnce(&mut Self) -> Result<T, ProviderFrameDecodeError>,
    ) -> Result<Option<T>, ProviderFrameDecodeError> {
        match self.u8()? {
            tags::OPTION_NONE => Ok(None),
            tags::OPTION_SOME => decode(self).map(Some),
            tag => Err(ProviderFrameDecodeError::InvalidTag { kind, tag }),
        }
    }

    pub(super) fn vector<T>(
        &mut self,
        kind: &'static str,
        mut decode: impl FnMut(&mut Self) -> Result<T, ProviderFrameDecodeError>,
    ) -> Result<Vec<T>, ProviderFrameDecodeError> {
        let count = self.count(kind)?;
        let mut values = Vec::new();
        values
            .try_reserve_exact(count)
            .map_err(|_| ProviderFrameDecodeError::InvalidLength { kind })?;
        for _ in 0..count {
            values.push(decode(self)?);
        }
        Ok(values)
    }

    fn bounded_identity(
        &mut self,
        kind: &'static str,
    ) -> Result<&'a str, ProviderFrameDecodeError> {
        let length = usize::try_from(self.u32()?)
            .map_err(|_| ProviderFrameDecodeError::InvalidLength { kind })?;
        std::str::from_utf8(self.take(length)?)
            .map_err(|_| ProviderFrameDecodeError::InvalidUtf8 { kind })
    }

    fn cas_item_id(&mut self) -> Result<CasItemId, ProviderFrameDecodeError> {
        CasItemId::new(self.bounded_identity("CAS item id")?).map_err(|_| {
            ProviderFrameDecodeError::InvalidIdentity {
                kind: "CAS item id",
            }
        })
    }

    pub(super) fn cas_thread_id(&mut self) -> Result<CasThreadId, ProviderFrameDecodeError> {
        CasThreadId::new(self.bounded_identity("CAS thread id")?).map_err(|_| {
            ProviderFrameDecodeError::InvalidIdentity {
                kind: "CAS thread id",
            }
        })
    }

    pub(super) fn asset(&mut self) -> Result<AssetId, ProviderFrameDecodeError> {
        match self.u8()? {
            1 => {
                let digest = self
                    .take(32)?
                    .try_into()
                    .expect("exact 32-byte digest slice");
                let length = NonZeroU64::new(self.u64()?)
                    .ok_or(ProviderFrameDecodeError::InvalidIdentity { kind: "asset id" })?;
                Ok(AssetId::sha256_v1(digest, length))
            }
            tag => Err(ProviderFrameDecodeError::InvalidTag {
                kind: "asset identity version",
                tag,
            }),
        }
    }

    pub(super) fn structured(
        &mut self,
        depth: usize,
    ) -> Result<ProviderStructuredValueV1, ProviderFrameDecodeError> {
        match self.u8()? {
            0 => Ok(ProviderStructuredValueV1::Null),
            1 => Ok(ProviderStructuredValueV1::Boolean(false)),
            2 => Ok(ProviderStructuredValueV1::Boolean(true)),
            3 => Ok(ProviderStructuredValueV1::Number(ProviderNumberV1::Signed(
                self.i64()?,
            ))),
            4 => Ok(ProviderStructuredValueV1::Number(
                ProviderNumberV1::Unsigned(self.u64()?),
            )),
            5 => Ok(ProviderStructuredValueV1::Number(
                ProviderNumberV1::FiniteFloat(ProviderFiniteF64V1::from_bits(self.u64()?)?),
            )),
            6 => self
                .text("structured string")
                .map(ProviderStructuredValueV1::String),
            7 => {
                let next = self.structured_depth(depth)?;
                self.vector("structured list", |decoder| decoder.structured(next))
                    .map(ProviderStructuredValueV1::List)
            }
            8 => {
                let next = self.structured_depth(depth)?;
                self.object_entries(next)
                    .map(ProviderStructuredValueV1::Object)
            }
            tag => Err(ProviderFrameDecodeError::InvalidTag {
                kind: "structured value",
                tag,
            }),
        }
    }

    fn structured_depth(&self, depth: usize) -> Result<usize, ProviderFrameDecodeError> {
        let next =
            depth
                .checked_add(1)
                .ok_or(ProviderItemValidationError::StructuredDepthExceeded {
                    maximum: PROVIDER_STRUCTURED_VALUE_MAX_DEPTH,
                })?;
        if next > PROVIDER_STRUCTURED_VALUE_MAX_DEPTH {
            return Err(ProviderItemValidationError::StructuredDepthExceeded {
                maximum: PROVIDER_STRUCTURED_VALUE_MAX_DEPTH,
            }
            .into());
        }
        Ok(next)
    }

    pub(super) fn object_entries(
        &mut self,
        depth: usize,
    ) -> Result<Vec<ProviderObjectEntryV1>, ProviderFrameDecodeError> {
        self.vector("structured object", |decoder| {
            Ok(ProviderObjectEntryV1 {
                key: decoder.raw_text("structured object key")?,
                value: decoder.structured(depth)?,
            })
        })
    }

    pub(super) fn mcp_content(&mut self) -> Result<ProviderMcpContentV1, ProviderFrameDecodeError> {
        match self.u8()? {
            0 => ProviderMcpContentV1::structured(self.structured(0)?).map_err(Into::into),
            1 => {
                let asset = ProviderInlineImageAssetV1::new(self.asset()?);
                let metadata = self.object_entries(1)?;
                ProviderMcpInlineImageV1::new(asset, metadata)
                    .map(ProviderMcpContentV1::inline_image)
                    .map_err(Into::into)
            }
            tag => Err(ProviderFrameDecodeError::InvalidTag {
                kind: "MCP content",
                tag,
            }),
        }
    }

    pub(super) fn enum_value<T: Copy>(
        &mut self,
        kind: &'static str,
        values: &[T],
    ) -> Result<T, ProviderFrameDecodeError> {
        let tag = self.u8()?;
        values
            .get(usize::from(tag))
            .copied()
            .ok_or(ProviderFrameDecodeError::InvalidTag { kind, tag })
    }
}
