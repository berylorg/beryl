use std::io::Read;

use super::super::StreamDecoder;
use crate::provider_item::*;

impl<R: Read, S: ProviderFrameTextSpanSinkV1> StreamDecoder<'_, R, S> {
    pub(super) fn user_message(&mut self) -> Result<(), ProviderFrameStreamError<S::Error>> {
        self.option("user-message client id", |decoder| {
            decoder.text("user-message client id", None)
        })?;
        self.content_reference()
    }

    pub(super) fn hook_prompt(&mut self) -> Result<(), ProviderFrameStreamError<S::Error>> {
        let count = self.count("hook-prompt fragments")?;
        for _ in 0..count {
            self.text(
                "hook-prompt text",
                Some(ProviderLogicalTextRoleV1::Activity),
            )?;
            self.text("hook-run id", None)?;
        }
        Ok(())
    }

    pub(super) fn agent_message(&mut self) -> Result<(), ProviderFrameStreamError<S::Error>> {
        self.text(
            "agent-message text",
            Some(ProviderLogicalTextRoleV1::Narrative),
        )?;
        self.option("agent-message phase", |decoder| {
            decoder.enum_tag("agent-message phase", 2).map(|_| ())
        })?;
        self.option("memory citation", |decoder| {
            let count = decoder.count("memory-citation entries")?;
            for _ in 0..count {
                decoder.text("memory-citation path", None)?;
                decoder.u32()?;
                decoder.u32()?;
                decoder.text("memory-citation note", None)?;
            }
            let count = decoder.count("memory-citation thread ids")?;
            for _ in 0..count {
                decoder.text("memory-citation thread id", None)?;
            }
            Ok(())
        })
    }

    fn content_reference(&mut self) -> Result<(), ProviderFrameStreamError<S::Error>> {
        self.fixed::<16>()?;
        if self.u64()? == 0 {
            return Err(ProviderFrameDecodeError::InvalidContentReference.into());
        }
        match self.u8()? {
            0 => {}
            1 | 2 => {
                return Err(ProviderItemValidationError::SubmittedContentMustBeComposer.into());
            }
            tag => {
                return Err(ProviderFrameDecodeError::InvalidTag {
                    kind: "content encoding",
                    tag,
                }
                .into());
            }
        }
        for _ in 0..6 {
            self.u64()?;
        }
        self.fixed::<32>()?;
        self.fixed::<32>()?;
        Ok(())
    }
}
