use beryl_model::{ContentRevision, ImageLabelOrdinal, SyndicContentDigest, SyndicContentId};

use super::super::Decoder;
use crate::{ContentEncoding, ContentReference, ContentSummary, provider_item::*};

impl Decoder<'_> {
    pub(super) fn user_message(
        &mut self,
    ) -> Result<ProviderUserMessageV1, ProviderFrameDecodeError> {
        Ok(ProviderUserMessageV1 {
            client_id: self.option("user-message client id", |decoder| {
                decoder.text("user-message client id")
            })?,
            submitted: ProviderSubmittedContentV1 {
                content: self.content_reference()?,
            },
        })
    }

    pub(super) fn hook_prompt(&mut self) -> Result<ProviderHookPromptV1, ProviderFrameDecodeError> {
        Ok(ProviderHookPromptV1 {
            fragments: self.vector("hook-prompt fragments", |decoder| {
                Ok(ProviderHookPromptFragmentV1 {
                    text: decoder.text("hook-prompt text")?,
                    hook_run_id: decoder.text("hook-run id")?,
                })
            })?,
        })
    }

    pub(super) fn agent_message(
        &mut self,
    ) -> Result<ProviderAgentMessageV1, ProviderFrameDecodeError> {
        Ok(ProviderAgentMessageV1 {
            text: self.text("agent-message text")?,
            phase: self.option("agent-message phase", |decoder| {
                decoder.enum_value(
                    "agent-message phase",
                    &[
                        ProviderMessagePhaseV1::Commentary,
                        ProviderMessagePhaseV1::FinalAnswer,
                    ],
                )
            })?,
            memory_citation: self.option("memory citation", |decoder| {
                Ok(ProviderMemoryCitationV1 {
                    entries: decoder.vector("memory-citation entries", |decoder| {
                        Ok(ProviderMemoryCitationEntryV1 {
                            path: decoder.text("memory-citation path")?,
                            line_start: decoder.u32()?,
                            line_end: decoder.u32()?,
                            note: decoder.text("memory-citation note")?,
                        })
                    })?,
                    thread_ids: decoder.vector("memory-citation thread ids", |decoder| {
                        decoder.text("memory-citation thread id")
                    })?,
                })
            })?,
        })
    }

    fn content_reference(&mut self) -> Result<ContentReference, ProviderFrameDecodeError> {
        let id = SyndicContentId::from_bytes(
            self.take(16)?.try_into().expect("exact 16-byte content id"),
        );
        let revision = ContentRevision::new(self.u64()?)
            .map_err(|_| ProviderFrameDecodeError::InvalidContentReference)?;
        let encoding = match self.u8()? {
            0 => ContentEncoding::ComposerV1,
            1 => ContentEncoding::Utf8V1,
            2 => ContentEncoding::ProviderItemV1,
            tag => {
                return Err(ProviderFrameDecodeError::InvalidTag {
                    kind: "content encoding",
                    tag,
                });
            }
        };
        let chunk_count = self.u64()?;
        let piece_count = self.u64()?;
        let encoded_bytes = self.u64()?;
        let logical_utf8_bytes = self.u64()?;
        let atom_count = self.u64()?;
        let marker_count = self.u64()?;
        let marker_digest = self.take(32)?.try_into().expect("exact marker digest");
        let maximum_image_label = self.option("maximum image label", |decoder| {
            ImageLabelOrdinal::new(decoder.u64()?)
                .map_err(|_| ProviderFrameDecodeError::InvalidContentReference)
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
            SyndicContentDigest::from_bytes(
                self.take(32)?.try_into().expect("exact content digest"),
            ),
        )
        .map_err(|_| ProviderFrameDecodeError::InvalidContentReference)?;
        Ok(ContentReference::new(id, revision, encoding, summary))
    }
}
