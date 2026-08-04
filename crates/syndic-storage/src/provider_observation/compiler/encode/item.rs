mod activity;
mod execution;
mod message;

use crate::provider_observation::{
    ProviderDeltaKind, ProviderEnumValue, ProviderField as F, ProviderObservationBegin,
    ProviderObservationItemKind, ProviderObservationItemLifecycle, ProviderScalar,
};
use crate::{ProviderFrameSinkV1, ProviderLogicalTextRoleV1};

use super::{Encoder, ObservationEncodeError};
use crate::provider_observation::compiler::ProviderObservationFrameSemanticError;
use crate::provider_observation::compiler::replay::{
    FieldSelector, Presence, ReplayError, TextSelector,
};

impl<S: ProviderFrameSinkV1> Encoder<'_, '_, S> {
    pub(super) fn observation(&mut self) -> Result<(), ObservationEncodeError<S::Error>> {
        match self.reader.begin() {
            ProviderObservationBegin::Item { lifecycle, kind } => {
                self.u8(match lifecycle {
                    ProviderObservationItemLifecycle::Started => 0,
                    ProviderObservationItemLifecycle::Completed => 2,
                })?;
                self.u64(self.required_unsigned(FieldSelector::top(F::LifecycleObservedAt))?)?;
                self.item(kind)
            }
            ProviderObservationBegin::Delta { kind } => {
                self.u8(1)?;
                self.delta(kind)
            }
        }
    }

    fn item(
        &mut self,
        kind: ProviderObservationItemKind,
    ) -> Result<(), ObservationEncodeError<S::Error>> {
        match kind {
            ProviderObservationItemKind::HookPrompt => {
                self.u8(1)?;
                self.hook_prompt()
            }
            ProviderObservationItemKind::AgentMessage => {
                self.u8(2)?;
                self.agent_message()
            }
            ProviderObservationItemKind::Plan => {
                self.u8(3)?;
                self.text(
                    FieldSelector::top(F::PlanText).into(),
                    Some(ProviderLogicalTextRoleV1::Narrative),
                )
            }
            ProviderObservationItemKind::Reasoning => {
                self.u8(4)?;
                self.reasoning()
            }
            ProviderObservationItemKind::CommandExecution => {
                self.u8(5)?;
                self.command_execution()
            }
            ProviderObservationItemKind::FileChange => {
                self.u8(6)?;
                self.file_change(F::FileChanges)
            }
            ProviderObservationItemKind::McpToolCall => {
                self.u8(7)?;
                self.mcp_tool_call()
            }
            ProviderObservationItemKind::DynamicToolCall => {
                self.u8(8)?;
                self.dynamic_tool_call()
            }
            ProviderObservationItemKind::CollabAgentToolCall => {
                self.u8(9)?;
                self.collab_tool_call()
            }
            ProviderObservationItemKind::SubAgentActivity => {
                self.u8(10)?;
                self.subagent_activity()
            }
            ProviderObservationItemKind::WebSearch => {
                self.u8(11)?;
                self.web_search()
            }
            ProviderObservationItemKind::ImageView => {
                self.u8(12)?;
                self.text(
                    FieldSelector::top(F::ImageViewPath).into(),
                    Some(ProviderLogicalTextRoleV1::Activity),
                )
            }
            ProviderObservationItemKind::Sleep => {
                self.u8(13)?;
                let duration = self.required_unsigned(FieldSelector::top(F::SleepDurationMs))?;
                self.u64(duration)
            }
            ProviderObservationItemKind::StandaloneImageGeneration => {
                self.u8(14)?;
                self.image_generation()
            }
            ProviderObservationItemKind::EnteredReviewMode => {
                self.u8(15)?;
                self.text(
                    FieldSelector::top(F::EnteredReview).into(),
                    Some(ProviderLogicalTextRoleV1::Activity),
                )
            }
            ProviderObservationItemKind::ExitedReviewMode => {
                self.u8(16)?;
                self.text(
                    FieldSelector::top(F::ExitedReview).into(),
                    Some(ProviderLogicalTextRoleV1::Activity),
                )
            }
            ProviderObservationItemKind::ContextCompaction => self.u8(17),
        }
    }

    fn delta(&mut self, kind: ProviderDeltaKind) -> Result<(), ObservationEncodeError<S::Error>> {
        let (tag, text, role) = match kind {
            ProviderDeltaKind::AgentMessage => (
                0,
                Some(F::DeltaText),
                Some(ProviderLogicalTextRoleV1::Narrative),
            ),
            ProviderDeltaKind::Plan => (
                1,
                Some(F::DeltaText),
                Some(ProviderLogicalTextRoleV1::Narrative),
            ),
            ProviderDeltaKind::ReasoningSummaryPartAdded => (2, None, None),
            ProviderDeltaKind::ReasoningSummaryText => (
                3,
                Some(F::DeltaText),
                Some(ProviderLogicalTextRoleV1::Activity),
            ),
            ProviderDeltaKind::ReasoningTextObserved => (4, None, None),
            ProviderDeltaKind::CommandExecutionOutput => (
                5,
                Some(F::DeltaText),
                Some(ProviderLogicalTextRoleV1::Operational),
            ),
            ProviderDeltaKind::FileChangeOutput => (
                6,
                Some(F::DeltaText),
                Some(ProviderLogicalTextRoleV1::Operational),
            ),
            ProviderDeltaKind::FileChangePatchUpdated => (7, None, None),
            ProviderDeltaKind::McpToolCallProgress => (
                8,
                Some(F::McpProgressMessage),
                Some(ProviderLogicalTextRoleV1::Operational),
            ),
        };
        self.u8(tag)?;
        match kind {
            ProviderDeltaKind::ReasoningSummaryPartAdded => {
                let value = self.required_unsigned(FieldSelector::top(F::DeltaSummaryIndex))?;
                self.u64(value)
            }
            ProviderDeltaKind::ReasoningSummaryText => {
                let value = self.required_unsigned(FieldSelector::top(F::DeltaSummaryIndex))?;
                self.u64(value)?;
                self.text(FieldSelector::top(text.unwrap()).into(), role)
            }
            ProviderDeltaKind::ReasoningTextObserved => {
                let value = self.required_unsigned(FieldSelector::top(F::DeltaContentIndex))?;
                self.u64(value)
            }
            ProviderDeltaKind::FileChangePatchUpdated => self.file_changes(F::DeltaChanges),
            _ => self.text(FieldSelector::top(text.unwrap()).into(), role),
        }
    }

    pub(super) fn field_presence(
        &self,
        selector: FieldSelector,
    ) -> Result<Presence, ObservationEncodeError<S::Error>> {
        self.reader.presence(selector).map_err(Into::into)
    }

    pub(super) fn required_unsigned(
        &self,
        selector: FieldSelector,
    ) -> Result<u64, ObservationEncodeError<S::Error>> {
        match self.reader.scalar(selector)? {
            Some(ProviderScalar::Unsigned(value)) => Ok(value),
            _ => Err(missing_or_mismatch(selector.field)),
        }
    }

    pub(super) fn optional_signed(
        &self,
        selector: FieldSelector,
    ) -> Result<Option<i64>, ObservationEncodeError<S::Error>> {
        match self.reader.scalar(selector)? {
            None | Some(ProviderScalar::Null) => Ok(None),
            Some(ProviderScalar::Signed(value)) => Ok(Some(value)),
            _ => Err(value_mismatch(selector.field)),
        }
    }

    pub(super) fn optional_boolean(
        &self,
        selector: FieldSelector,
    ) -> Result<Option<bool>, ObservationEncodeError<S::Error>> {
        match self.reader.scalar(selector)? {
            None | Some(ProviderScalar::Null) => Ok(None),
            Some(ProviderScalar::Boolean(value)) => Ok(Some(value)),
            _ => Err(value_mismatch(selector.field)),
        }
    }

    pub(super) fn optional_text(
        &mut self,
        selector: FieldSelector,
        role: Option<ProviderLogicalTextRoleV1>,
    ) -> Result<(), ObservationEncodeError<S::Error>> {
        let present = self.field_presence(selector)? == Presence::Value;
        self.option(present, |encoder| {
            encoder.text(TextSelector::Field(selector), role)
        })
    }

    pub(super) fn required_enum(
        &self,
        selector: FieldSelector,
    ) -> Result<ProviderEnumValue, ObservationEncodeError<S::Error>> {
        self.reader
            .enum_value(selector)?
            .ok_or_else(|| missing(selector.field))
    }

    pub(super) fn optional_enum(
        &self,
        selector: FieldSelector,
    ) -> Result<Option<ProviderEnumValue>, ObservationEncodeError<S::Error>> {
        match self.field_presence(selector)? {
            Presence::Missing | Presence::Null => Ok(None),
            Presence::Value => self
                .reader
                .enum_value(selector)?
                .map(Some)
                .ok_or_else(|| value_mismatch(selector.field)),
        }
    }

    pub(super) fn list_count(
        &self,
        selector: FieldSelector,
        required: bool,
    ) -> Result<u64, ObservationEncodeError<S::Error>> {
        match self.reader.list_count(selector)? {
            Some(count) => Ok(count),
            None if !required => Ok(0),
            None => Err(missing(selector.field)),
        }
    }

    pub(super) fn enum_tag(
        &mut self,
        value: ProviderEnumValue,
        values: &[ProviderEnumValue],
    ) -> Result<(), ObservationEncodeError<S::Error>> {
        let Some(tag) = values.iter().position(|candidate| candidate == &value) else {
            return Err(value_mismatch(F::ItemId));
        };
        self.u8(u8::try_from(tag).expect("closed provider enum tag fits u8"))
    }
}

fn missing<E>(field: F) -> ObservationEncodeError<E> {
    ObservationEncodeError::Replay(ReplayError::Semantic(
        ProviderObservationFrameSemanticError::MissingField { field },
    ))
}

fn value_mismatch<E>(field: F) -> ObservationEncodeError<E> {
    ObservationEncodeError::Replay(ReplayError::Semantic(
        ProviderObservationFrameSemanticError::ValueMismatch { field },
    ))
}

fn missing_or_mismatch<E>(field: F) -> ObservationEncodeError<E> {
    value_mismatch(field)
}
