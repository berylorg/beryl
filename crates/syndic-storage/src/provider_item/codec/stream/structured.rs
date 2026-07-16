use std::io::Read;

use super::StreamDecoder;
use crate::provider_item::*;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum TextImageProbe {
    Image,
    Referenced,
    Other,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct StructuredProbe {
    direct_text: TextImageProbe,
    object_type: TextImageProbe,
}

impl StructuredProbe {
    const OTHER: Self = Self {
        direct_text: TextImageProbe::Other,
        object_type: TextImageProbe::Other,
    };
}

impl<R: Read, S: ProviderFrameTextSpanSinkV1> StreamDecoder<'_, R, S> {
    fn text_image_probe(
        &mut self,
        kind: &'static str,
    ) -> Result<TextImageProbe, ProviderFrameStreamError<S::Error>> {
        match self.u8()? {
            super::super::tags::TEXT_INLINE => {
                let length = self.u64()?;
                let (_, matches, _) = self.scan_utf8(length, kind, [b"image"], None)?;
                Ok(if matches[0] {
                    TextImageProbe::Image
                } else {
                    TextImageProbe::Other
                })
            }
            super::super::tags::TEXT_REUSED => {
                let start = self.u64()?;
                let end = self.u64()?;
                let digest = self.fixed::<32>()?;
                let reference = ProviderTextReferenceV1::new(start, end, digest)?;
                if reference.end() > self.encoded_start {
                    return Err(ProviderItemValidationError::TextReferenceBeyondFrontier {
                        start,
                        end,
                        frontier: self.encoded_start,
                    }
                    .into());
                }
                Ok(TextImageProbe::Referenced)
            }
            tag => Err(ProviderFrameDecodeError::InvalidTag { kind, tag }.into()),
        }
    }

    pub(super) fn structured(
        &mut self,
        depth: usize,
    ) -> Result<StructuredProbe, ProviderFrameStreamError<S::Error>> {
        match self.u8()? {
            0..=2 => Ok(StructuredProbe::OTHER),
            3 => {
                self.i64()?;
                Ok(StructuredProbe::OTHER)
            }
            4 => {
                self.u64()?;
                Ok(StructuredProbe::OTHER)
            }
            5 => {
                let value = f64::from_bits(self.u64()?);
                if !value.is_finite() {
                    return Err(ProviderItemValidationError::NonFiniteNumber.into());
                }
                Ok(StructuredProbe::OTHER)
            }
            6 => self
                .text_image_probe("structured string")
                .map(|direct_text| StructuredProbe {
                    direct_text,
                    object_type: TextImageProbe::Other,
                }),
            7 => {
                let next = structured_depth(depth)?;
                let count = self.count("structured list")?;
                for _ in 0..count {
                    self.structured(next)?;
                }
                Ok(StructuredProbe::OTHER)
            }
            8 => {
                let next = structured_depth(depth)?;
                let count = self.count("structured object")?;
                let mut item_type = TextImageProbe::Other;
                for _ in 0..count {
                    let matches = self.raw_text_matches("structured object key", [b"type"])?;
                    let value = self.structured(next)?;
                    if matches[0] && item_type == TextImageProbe::Other {
                        item_type = value.direct_text;
                    }
                }
                Ok(StructuredProbe {
                    direct_text: TextImageProbe::Other,
                    object_type: item_type,
                })
            }
            tag => Err(ProviderFrameDecodeError::InvalidTag {
                kind: "structured value",
                tag,
            }
            .into()),
        }
    }

    pub(super) fn mcp_content(&mut self) -> Result<(), ProviderFrameStreamError<S::Error>> {
        match self.u8()? {
            0 => match self.structured(0)?.object_type {
                TextImageProbe::Image => {
                    Err(ProviderItemValidationError::McpInlineImageRequiresAsset.into())
                }
                TextImageProbe::Referenced => {
                    Err(ProviderItemValidationError::McpContentTypeReference.into())
                }
                TextImageProbe::Other => Ok(()),
            },
            1 => {
                self.asset()?;
                let count = self.count("MCP image metadata")?;
                for _ in 0..count {
                    let matches = self.raw_text_matches(
                        "MCP image metadata key",
                        [b"data", b"image_url", b"imageUrl"],
                    )?;
                    if matches[0] {
                        return Err(ProviderItemValidationError::McpImageMetadataContainsBytes {
                            field: "data",
                        }
                        .into());
                    }
                    if matches[1] || matches[2] {
                        return Err(ProviderItemValidationError::McpImageMetadataContainsBytes {
                            field: "image URL",
                        }
                        .into());
                    }
                    self.structured(1)?;
                }
                Ok(())
            }
            tag => Err(ProviderFrameDecodeError::InvalidTag {
                kind: "MCP content",
                tag,
            }
            .into()),
        }
    }
}

fn structured_depth(depth: usize) -> Result<usize, ProviderItemValidationError> {
    let next =
        depth
            .checked_add(1)
            .ok_or(ProviderItemValidationError::StructuredDepthExceeded {
                maximum: PROVIDER_STRUCTURED_VALUE_MAX_DEPTH,
            })?;
    if next > PROVIDER_STRUCTURED_VALUE_MAX_DEPTH {
        Err(ProviderItemValidationError::StructuredDepthExceeded {
            maximum: PROVIDER_STRUCTURED_VALUE_MAX_DEPTH,
        })
    } else {
        Ok(next)
    }
}
