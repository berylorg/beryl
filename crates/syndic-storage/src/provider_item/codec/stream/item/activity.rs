use std::io::Read;

use super::super::StreamDecoder;
use crate::provider_item::*;

impl<R: Read, S: ProviderFrameTextSpanSinkV1> StreamDecoder<'_, R, S> {
    pub(super) fn collab_tool_call(&mut self) -> Result<bool, ProviderFrameStreamError<S::Error>> {
        self.enum_tag("collaboration tool", 5)?;
        let in_progress = self.enum_tag("collaboration tool status", 3)? == 0;
        self.cas_thread_id()?;
        let count = self.count("receiver thread ids")?;
        for _ in 0..count {
            self.cas_thread_id()?;
        }
        self.option("collaboration prompt", |decoder| {
            decoder.text(
                "collaboration prompt",
                Some(ProviderLogicalTextRoleV1::Activity),
            )
        })?;
        self.option("collaboration model", |decoder| {
            decoder.text("collaboration model", None)
        })?;
        self.option("collaboration reasoning effort", |decoder| {
            decoder.text("collaboration reasoning effort", None)
        })?;
        let count = self.count("collaboration agent states")?;
        for _ in 0..count {
            self.text("collaboration agent key", None)?;
            self.enum_tag("collaboration agent status", 7)?;
            self.option("collaboration agent message", |decoder| {
                decoder.text(
                    "collaboration agent message",
                    Some(ProviderLogicalTextRoleV1::Activity),
                )
            })?;
        }
        Ok(in_progress)
    }

    pub(super) fn subagent_activity(&mut self) -> Result<(), ProviderFrameStreamError<S::Error>> {
        self.enum_tag("subagent activity kind", 3)?;
        self.cas_thread_id()?;
        self.text("subagent path", Some(ProviderLogicalTextRoleV1::Activity))
    }

    pub(super) fn web_search(
        &mut self,
    ) -> Result<ProviderFrameHistorySupportV1, ProviderFrameStreamError<S::Error>> {
        self.text(
            "web-search query",
            Some(ProviderLogicalTextRoleV1::Activity),
        )?;
        let mut history_support = ProviderFrameHistorySupportV1::Supported;
        self.option("web-search action", |decoder| match decoder.u8()? {
            0 => {
                decoder.option("web-search action query", |decoder| {
                    decoder.text(
                        "web-search action query",
                        Some(ProviderLogicalTextRoleV1::Activity),
                    )
                })?;
                decoder.option("web-search queries", |decoder| {
                    let count = decoder.count("web-search queries")?;
                    for _ in 0..count {
                        decoder.text(
                            "web-search query",
                            Some(ProviderLogicalTextRoleV1::Activity),
                        )?;
                    }
                    Ok(())
                })
            }
            1 => decoder.option("web-search URL", |decoder| {
                decoder.text("web-search URL", Some(ProviderLogicalTextRoleV1::Activity))
            }),
            2 => {
                decoder.option("web-search URL", |decoder| {
                    decoder.text("web-search URL", Some(ProviderLogicalTextRoleV1::Activity))
                })?;
                decoder.option("web-search pattern", |decoder| {
                    decoder.text(
                        "web-search pattern",
                        Some(ProviderLogicalTextRoleV1::Activity),
                    )
                })
            }
            3 => {
                history_support = ProviderFrameHistorySupportV1::Unsupported(
                    crate::UnsupportedHistoryReason::UnsupportedRequiredPayload,
                );
                Ok(())
            }
            tag => Err(ProviderFrameDecodeError::InvalidTag {
                kind: "web-search action",
                tag,
            }
            .into()),
        })?;
        Ok(history_support)
    }

    pub(super) fn image_generation(&mut self) -> Result<bool, ProviderFrameStreamError<S::Error>> {
        let in_progress = self.enum_tag("image-generation status", 3)? == 0;
        self.option("image-generation revised prompt", |decoder| {
            decoder.text(
                "image-generation revised prompt",
                Some(ProviderLogicalTextRoleV1::Activity),
            )
        })?;
        self.option("image-generation saved path", |decoder| {
            decoder.text("image-generation saved path", None)
        })?;
        Ok(in_progress)
    }
}
