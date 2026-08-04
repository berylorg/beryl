use std::io::Read;

use beryl_model::{ContentRevision, ImageLabelOrdinal, SyndicContentDigest, SyndicContentId};

use super::super::StreamDecoder;
use crate::provider_item::*;
use crate::{ContentEncoding, ContentReference, ContentSummary};

impl<R: Read, S: ProviderFrameTextSpanSinkV1> StreamDecoder<'_, R, S> {
    pub(super) fn user_message(
        &mut self,
    ) -> Result<ContentReference, ProviderFrameStreamError<S::Error>> {
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

    pub(super) fn agent_message(
        &mut self,
    ) -> Result<Option<ProviderMessagePhaseV1>, ProviderFrameStreamError<S::Error>> {
        self.text(
            "agent-message text",
            Some(ProviderLogicalTextRoleV1::Narrative),
        )?;
        let phase = self.option_value("agent-message phase", |decoder| {
            decoder
                .enum_tag("agent-message phase", 2)
                .map(|tag| match tag {
                    0 => ProviderMessagePhaseV1::Commentary,
                    1 => ProviderMessagePhaseV1::FinalAnswer,
                    _ => unreachable!("validated agent-message phase tag"),
                })
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
        })?;
        Ok(phase)
    }

    fn content_reference(
        &mut self,
    ) -> Result<ContentReference, ProviderFrameStreamError<S::Error>> {
        let id = SyndicContentId::from_bytes(self.fixed::<16>()?);
        let revision = ContentRevision::new(self.u64()?)
            .map_err(|_| ProviderFrameDecodeError::InvalidContentReference)?;
        let encoding = match self.u8()? {
            0 => ContentEncoding::ComposerV1,
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
        };
        let chunk_count = self.u64()?;
        let piece_count = self.u64()?;
        let encoded_bytes = self.u64()?;
        let logical_utf8_bytes = self.u64()?;
        let atom_count = self.u64()?;
        let marker_count = self.u64()?;
        let marker_digest = self.fixed::<32>()?;
        let maximum_image_label = self.option_value("maximum image label", |decoder| {
            ImageLabelOrdinal::new(decoder.u64()?).map_err(|_| {
                ProviderFrameStreamError::Decode(ProviderFrameDecodeError::InvalidContentReference)
            })
        })?;
        let summary = ContentSummary::new(
            chunk_count,
            piece_count,
            encoded_bytes,
            logical_utf8_bytes,
            atom_count,
            marker_count,
            marker_digest,
            maximum_image_label,
            SyndicContentDigest::from_bytes(self.fixed::<32>()?),
        )
        .map_err(|_| ProviderFrameDecodeError::InvalidContentReference)?;
        Ok(ContentReference::new(id, revision, encoding, summary))
    }
}
