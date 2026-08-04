use crate::provider_observation::{ProviderEnumValue as E, ProviderField as F};
use crate::{ProviderFrameSinkV1, ProviderLogicalTextRoleV1};

use super::super::{Encoder, ObservationEncodeError};
use crate::provider_observation::compiler::replay::{FieldSelector, Presence, TextSelector};

impl<S: ProviderFrameSinkV1> Encoder<'_, '_, S> {
    pub(super) fn collab_tool_call(&mut self) -> Result<(), ObservationEncodeError<S::Error>> {
        let tool = self.required_enum(FieldSelector::top(F::CollabTool))?;
        self.enum_tag(
            tool,
            &[
                E::SpawnAgent,
                E::SendInput,
                E::ResumeAgent,
                E::Wait,
                E::CloseAgent,
            ],
        )?;
        let status = self.required_enum(FieldSelector::top(F::CollabStatus))?;
        self.status3(status)?;
        self.cas_thread_id(FieldSelector::top(F::CollabSenderThreadId).into())?;

        let count = self.list_count(FieldSelector::top(F::CollabReceiverThreadIds), true)?;
        self.u64(count)?;
        for index in 0..count {
            self.cas_thread_id(
                FieldSelector::in_list(
                    F::CollabReceiverThreadId,
                    F::CollabReceiverThreadIds,
                    index,
                )
                .into(),
            )?;
        }
        self.optional_text(
            FieldSelector::top(F::CollabPrompt),
            Some(ProviderLogicalTextRoleV1::Activity),
        )?;
        self.optional_text(FieldSelector::top(F::CollabModel), None)?;
        self.optional_text(FieldSelector::top(F::CollabReasoningEffort), None)?;

        let states = FieldSelector::top(F::CollabAgentStates);
        let count = self
            .reader
            .object_count(states)?
            .ok_or_else(|| super::missing(F::CollabAgentStates))?;
        self.u64(count)?;
        for entry in 0..count {
            self.text(TextSelector::AgentStateKey { entry }, None)?;
            let status = self.required_enum(FieldSelector::in_agent_entry(
                F::CollabAgentStateStatus,
                entry,
            ))?;
            self.enum_tag(
                status,
                &[
                    E::PendingInit,
                    E::Running,
                    E::Interrupted,
                    E::Completed,
                    E::Errored,
                    E::Shutdown,
                    E::NotFound,
                ],
            )?;
            self.optional_text(
                FieldSelector::in_agent_entry(F::CollabAgentStateMessage, entry),
                Some(ProviderLogicalTextRoleV1::Activity),
            )?;
        }
        Ok(())
    }

    pub(super) fn subagent_activity(&mut self) -> Result<(), ObservationEncodeError<S::Error>> {
        let kind = self.required_enum(FieldSelector::top(F::SubAgentKind))?;
        self.enum_tag(
            kind,
            &[
                E::SubAgentStarted,
                E::SubAgentInteracted,
                E::SubAgentInterrupted,
            ],
        )?;
        self.cas_thread_id(FieldSelector::top(F::SubAgentThreadId).into())?;
        self.text(
            FieldSelector::top(F::SubAgentPath).into(),
            Some(ProviderLogicalTextRoleV1::Activity),
        )
    }

    pub(super) fn web_search(&mut self) -> Result<(), ObservationEncodeError<S::Error>> {
        self.text(
            FieldSelector::top(F::WebSearchQuery).into(),
            Some(ProviderLogicalTextRoleV1::Activity),
        )?;
        let action = FieldSelector::top(F::WebSearchAction);
        let present = self.field_presence(action)? == Presence::Value;
        self.option(present, |encoder| encoder.web_search_action())
    }

    fn web_search_action(&mut self) -> Result<(), ObservationEncodeError<S::Error>> {
        let field = |value| FieldSelector::in_object(value, F::WebSearchAction);
        match self.required_enum(field(F::WebSearchActionKind))? {
            E::Search => {
                self.u8(0)?;
                self.optional_text(
                    field(F::WebSearchActionQuery),
                    Some(ProviderLogicalTextRoleV1::Activity),
                )?;
                let queries = field(F::WebSearchActionQueryList);
                let present = self.field_presence(queries)? == Presence::Value;
                self.option(present, |encoder| encoder.web_search_queries())
            }
            E::OpenPage => {
                self.u8(1)?;
                self.optional_text(
                    field(F::WebSearchUrl),
                    Some(ProviderLogicalTextRoleV1::Activity),
                )
            }
            E::FindInPage => {
                self.u8(2)?;
                self.optional_text(
                    field(F::WebSearchUrl),
                    Some(ProviderLogicalTextRoleV1::Activity),
                )?;
                self.optional_text(
                    field(F::WebSearchPattern),
                    Some(ProviderLogicalTextRoleV1::Activity),
                )
            }
            E::Other => self.u8(3),
            _ => Err(super::value_mismatch(F::WebSearchActionKind)),
        }
    }

    fn web_search_queries(&mut self) -> Result<(), ObservationEncodeError<S::Error>> {
        let list = FieldSelector::in_object(F::WebSearchActionQueryList, F::WebSearchAction);
        let count = self.list_count(list, true)?;
        self.u64(count)?;
        for index in 0..count {
            self.text(
                FieldSelector::in_list(
                    F::WebSearchActionQueries,
                    F::WebSearchActionQueryList,
                    index,
                )
                .into(),
                Some(ProviderLogicalTextRoleV1::Activity),
            )?;
        }
        Ok(())
    }

    pub(super) fn image_generation(&mut self) -> Result<(), ObservationEncodeError<S::Error>> {
        let status = self.required_enum(FieldSelector::top(F::ImageGenerationStatus))?;
        self.enum_tag(status, &[E::InProgress, E::Failed, E::Completed])?;
        self.optional_text(
            FieldSelector::top(F::ImageGenerationRevisedPrompt),
            Some(ProviderLogicalTextRoleV1::Activity),
        )?;
        self.optional_text(FieldSelector::top(F::ImageGenerationSavedPath), None)
    }
}
