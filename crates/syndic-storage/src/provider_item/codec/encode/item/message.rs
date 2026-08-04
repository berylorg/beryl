use super::super::Encoder;
use crate::{ContentEncoding, ContentReference, provider_item::*};

impl<S: ProviderFrameSinkV1> Encoder<'_, S> {
    pub(super) fn user_message(
        &mut self,
        value: &ProviderUserMessageV1,
    ) -> Result<(), ProviderFrameEncodeError<S::Error>> {
        self.option(&value.client_id, |encoder, value| encoder.text(value, None))?;
        self.content_reference(value.submitted.content)
    }

    pub(super) fn hook_prompt(
        &mut self,
        value: &ProviderHookPromptV1,
    ) -> Result<(), ProviderFrameEncodeError<S::Error>> {
        self.count(value.fragments.len())?;
        for fragment in &value.fragments {
            self.text(&fragment.text, Some(ProviderLogicalTextRoleV1::Activity))?;
            self.text(&fragment.hook_run_id, None)?;
        }
        Ok(())
    }

    pub(super) fn agent_message(
        &mut self,
        value: &ProviderAgentMessageV1,
    ) -> Result<(), ProviderFrameEncodeError<S::Error>> {
        self.text(&value.text, Some(ProviderLogicalTextRoleV1::Narrative))?;
        self.option(&value.phase, |encoder, value| {
            encoder.enum_tag(
                *value,
                &[
                    ProviderMessagePhaseV1::Commentary,
                    ProviderMessagePhaseV1::FinalAnswer,
                ],
            )
        })?;
        self.option(&value.memory_citation, |encoder, citation| {
            encoder.count(citation.entries.len())?;
            for entry in &citation.entries {
                encoder.text(&entry.path, None)?;
                encoder.u32(entry.line_start)?;
                encoder.u32(entry.line_end)?;
                encoder.text(&entry.note, None)?;
            }
            encoder.count(citation.thread_ids.len())?;
            for thread_id in &citation.thread_ids {
                encoder.text(thread_id, None)?;
            }
            Ok(())
        })
    }

    fn content_reference(
        &mut self,
        value: ContentReference,
    ) -> Result<(), ProviderFrameEncodeError<S::Error>> {
        self.bytes(value.id().as_bytes())?;
        self.u64(value.revision().get())?;
        self.u8(match value.encoding() {
            ContentEncoding::ComposerV1 => 0,
            ContentEncoding::Utf8V1 => 1,
            ContentEncoding::ProviderItemV1 => 2,
        })?;
        let summary = value.summary();
        self.u64(summary.chunk_count())?;
        self.u64(summary.piece_count())?;
        self.u64(summary.encoded_bytes())?;
        self.u64(summary.logical_utf8_bytes())?;
        self.u64(summary.atom_count())?;
        self.u64(summary.image_marker_count())?;
        self.bytes(&summary.marker_digest())?;
        self.option(&summary.maximum_image_label(), |encoder, label| {
            encoder.u64(label.get())
        })?;
        self.bytes(summary.digest().as_bytes())
    }
}
