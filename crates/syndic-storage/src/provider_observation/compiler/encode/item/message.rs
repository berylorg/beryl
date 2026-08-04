use crate::provider_observation::{ProviderEnumValue as E, ProviderField as F};
use crate::{ProviderFrameSinkV1, ProviderLogicalTextRoleV1};

use super::super::{Encoder, ObservationEncodeError};
use crate::provider_observation::compiler::replay::{FieldSelector, Presence};

impl<S: ProviderFrameSinkV1> Encoder<'_, '_, S> {
    pub(super) fn hook_prompt(&mut self) -> Result<(), ObservationEncodeError<S::Error>> {
        let list = FieldSelector::top(F::HookFragments);
        let count = self.list_count(list, true)?;
        self.u64(count)?;
        for index in 0..count {
            self.text(
                FieldSelector::in_list(F::HookFragmentText, F::HookFragments, index).into(),
                Some(ProviderLogicalTextRoleV1::Activity),
            )?;
            self.text(
                FieldSelector::in_list(F::HookRunId, F::HookFragments, index).into(),
                None,
            )?;
        }
        Ok(())
    }

    pub(super) fn agent_message(&mut self) -> Result<(), ObservationEncodeError<S::Error>> {
        self.text(
            FieldSelector::top(F::AgentMessageText).into(),
            Some(ProviderLogicalTextRoleV1::Narrative),
        )?;
        let phase = self.optional_enum(FieldSelector::top(F::MessagePhase))?;
        self.option(phase.is_some(), |encoder| {
            encoder.enum_tag(phase.unwrap(), &[E::Commentary, E::FinalAnswer])
        })?;

        let citation = FieldSelector::top(F::MemoryCitation);
        let present = self.field_presence(citation)? == Presence::Value;
        self.option(present, |encoder| encoder.memory_citation())
    }

    fn memory_citation(&mut self) -> Result<(), ObservationEncodeError<S::Error>> {
        let entries = FieldSelector::in_object(F::MemoryCitationEntries, F::MemoryCitation);
        let count = self.list_count(entries, true)?;
        self.u64(count)?;
        for index in 0..count {
            self.text(
                FieldSelector::in_list(F::MemoryCitationPath, F::MemoryCitationEntries, index)
                    .into(),
                None,
            )?;
            let start = self.required_unsigned(FieldSelector::in_list(
                F::MemoryCitationLineStart,
                F::MemoryCitationEntries,
                index,
            ))?;
            self.u32(
                u32::try_from(start)
                    .map_err(|_| super::value_mismatch(F::MemoryCitationLineStart))?,
            )?;
            let end = self.required_unsigned(FieldSelector::in_list(
                F::MemoryCitationLineEnd,
                F::MemoryCitationEntries,
                index,
            ))?;
            self.u32(
                u32::try_from(end).map_err(|_| super::value_mismatch(F::MemoryCitationLineEnd))?,
            )?;
            self.text(
                FieldSelector::in_list(F::MemoryCitationNote, F::MemoryCitationEntries, index)
                    .into(),
                None,
            )?;
        }
        let threads = FieldSelector::in_object(F::MemoryCitationThreadIds, F::MemoryCitation);
        let count = self.list_count(threads, true)?;
        self.u64(count)?;
        for index in 0..count {
            self.text(
                FieldSelector::in_list(
                    F::MemoryCitationThreadId,
                    F::MemoryCitationThreadIds,
                    index,
                )
                .into(),
                None,
            )?;
        }
        Ok(())
    }

    pub(super) fn reasoning(&mut self) -> Result<(), ObservationEncodeError<S::Error>> {
        let count = self.list_count(FieldSelector::top(F::ReasoningSummaries), false)?;
        self.u64(count)?;
        for index in 0..count {
            self.text(
                FieldSelector::in_list(F::ReasoningSummary, F::ReasoningSummaries, index).into(),
                Some(ProviderLogicalTextRoleV1::Activity),
            )?;
        }
        Ok(())
    }
}
