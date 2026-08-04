use thiserror::Error;

use crate::{ProviderFrameHistorySupportV1, UnsupportedHistoryReason};

use super::{
    ProviderContainer, ProviderEnumValue, ProviderField, ProviderIdentityValidatorState,
    ProviderObservationBegin, ProviderObservationControl, ProviderObservationElementKind,
    ProviderObservationFrame, ProviderObservationItemKind, ProviderObservationItemLifecycle,
    ProviderScalar, ProviderStructuredPosition, ProviderValueContext, Utf8ValidatorState,
    schema::{self, FieldSpec, ValueKind},
};

const MAX_STRUCTURED_DEPTH: usize = 128;
const MAX_TYPED_ENCLOSING_FRAMES: usize = 3;
pub(crate) const PROVIDER_OBSERVATION_MAX_FRAME_DEPTH: usize =
    MAX_STRUCTURED_DEPTH * 2 + MAX_TYPED_ENCLOSING_FRAMES;

#[derive(Clone, Copy)]
enum ValueControl {
    Text,
    Container(ProviderContainer),
    Enum(ProviderEnumValue),
    Scalar(ProviderScalar),
}

/// Compact resumable semantic and structural state persisted with an unpublished build.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ProviderObservationValidatorState {
    pub(crate) active_text: Option<ProviderValueContext>,
    pub(crate) active_identity: Option<ProviderIdentityValidatorState>,
    pub(crate) utf8: Utf8ValidatorState,
    pub(crate) frames: Vec<ProviderObservationFrame>,
    pub(crate) token_count: u64,
    pub(crate) text_bytes: u64,
    pub(crate) seen_fields: [u64; 2],
    pub(crate) item_status: Option<ProviderEnumValue>,
    pub(crate) history_support: ProviderFrameHistorySupportV1,
}

impl ProviderObservationValidatorState {
    pub(crate) const fn initial() -> Self {
        Self {
            active_text: None,
            active_identity: None,
            utf8: Utf8ValidatorState::new(),
            frames: Vec::new(),
            token_count: 0,
            text_bytes: 0,
            seen_fields: [0; 2],
            item_status: None,
            history_support: ProviderFrameHistorySupportV1::Supported,
        }
    }

    pub(crate) fn control(
        &mut self,
        begin: ProviderObservationBegin,
        control: ProviderObservationControl,
    ) -> Result<(), ProviderObservationValidatorError> {
        if self.active_text.is_some() {
            match control {
                ProviderObservationControl::EndField(context) => self.end_text(context)?,
                _ => return Err(ProviderObservationValidatorError::ControlDuringText),
            }
            return self.count_token();
        }
        match control {
            ProviderObservationControl::BeginField(context) => {
                let value = self.claim_value(begin, context, ValueControl::Text)?;
                self.active_text = Some(context);
                self.active_identity = matches!(value, ValueKind::Identity)
                    .then_some(ProviderIdentityValidatorState::new());
            }
            ProviderObservationControl::EndField(_) => {
                return Err(ProviderObservationValidatorError::TextNotOpen);
            }
            ProviderObservationControl::BeginContainer { context, container } => {
                let value = self.claim_value(begin, context, ValueControl::Container(container))?;
                self.begin_container(context, container, value)?;
            }
            ProviderObservationControl::EndContainer { context, container } => {
                self.end_container(context, container)?;
            }
            ProviderObservationControl::BeginElement { context, index } => {
                self.begin_element(context, index)?;
            }
            ProviderObservationControl::EndElement { context, index } => {
                self.end_element(context, index)?;
            }
            ProviderObservationControl::BeginObjectEntry { root, depth, entry } => {
                self.begin_object_entry(root, depth, entry)?;
            }
            ProviderObservationControl::EndObjectEntry { root, depth, entry } => {
                self.end_object_entry(root, depth, entry)?;
            }
            ProviderObservationControl::Enum { context, value } => {
                self.claim_value(begin, context, ValueControl::Enum(value))?;
                if matches!(
                    context,
                    ProviderValueContext::Field(
                        ProviderField::CommandStatus
                            | ProviderField::FileChangeStatus
                            | ProviderField::McpStatus
                            | ProviderField::DynamicStatus
                            | ProviderField::CollabStatus
                            | ProviderField::ImageGenerationStatus
                    )
                ) {
                    self.item_status = Some(value);
                }
                if value == ProviderEnumValue::Other {
                    self.history_support =
                        self.history_support
                            .merge(ProviderFrameHistorySupportV1::Unsupported(
                                UnsupportedHistoryReason::UnsupportedRequiredPayload,
                            ));
                }
                self.complete_value()?;
            }
            ProviderObservationControl::Scalar { context, value } => {
                self.claim_value(begin, context, ValueControl::Scalar(value))?;
                self.complete_value()?;
            }
        }
        self.count_token()
    }

    pub(crate) fn fragment_byte(
        &mut self,
        context: ProviderValueContext,
        byte: u8,
    ) -> Result<(), ProviderObservationValidatorError> {
        if self.active_text != Some(context) {
            return Err(ProviderObservationValidatorError::TextContextMismatch);
        }
        let scalar = self.utf8.push(byte)?;
        if let Some(identity) = self.active_identity.as_mut() {
            identity.push(scalar)?;
        }
        self.text_bytes = self
            .text_bytes
            .checked_add(1)
            .ok_or(ProviderObservationValidatorError::FrontierOverflow)?;
        Ok(())
    }

    pub(crate) fn finish(
        &self,
        begin: ProviderObservationBegin,
    ) -> Result<(), ProviderObservationValidatorError> {
        if self.active_text.is_some() || self.utf8.remaining != 0 {
            return Err(ProviderObservationValidatorError::IncompleteText);
        }
        if !self.frames.is_empty() {
            return Err(ProviderObservationValidatorError::IncompleteStructure);
        }
        if !self.field_seen(ProviderField::ItemId) {
            return Err(ProviderObservationValidatorError::MissingItemIdentity);
        }
        if matches!(begin, ProviderObservationBegin::Item { .. })
            && !self.field_seen(ProviderField::LifecycleObservedAt)
        {
            return Err(ProviderObservationValidatorError::MissingLifecycleTimestamp);
        }
        if !schema::required_fields_present(schema::top_fields(begin), self.seen_fields) {
            return Err(ProviderObservationValidatorError::MissingRequiredField);
        }
        if matches!(
            begin,
            ProviderObservationBegin::Item {
                lifecycle: ProviderObservationItemLifecycle::Completed,
                kind: ProviderObservationItemKind::CommandExecution
                    | ProviderObservationItemKind::FileChange
                    | ProviderObservationItemKind::McpToolCall
                    | ProviderObservationItemKind::DynamicToolCall
                    | ProviderObservationItemKind::CollabAgentToolCall
                    | ProviderObservationItemKind::StandaloneImageGeneration,
            }
        ) && self.item_status == Some(ProviderEnumValue::InProgress)
        {
            return Err(ProviderObservationValidatorError::InvalidLifecycle);
        }
        Ok(())
    }

    pub(crate) fn field_seen(&self, field: ProviderField) -> bool {
        schema::field_seen(self.seen_fields, field)
    }

    pub(crate) const fn history_support(&self) -> ProviderFrameHistorySupportV1 {
        self.history_support
    }

    fn count_token(&mut self) -> Result<(), ProviderObservationValidatorError> {
        self.token_count = self
            .token_count
            .checked_add(1)
            .ok_or(ProviderObservationValidatorError::FrontierOverflow)?;
        Ok(())
    }
}

include!("validator/value.rs");
include!("validator/structure.rs");

#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum ProviderObservationValidatorError {
    #[error("provider observation control appeared while text was open")]
    ControlDuringText,
    #[error("provider observation text was not open")]
    TextNotOpen,
    #[error("provider observation text context changed")]
    TextContextMismatch,
    #[error("provider observation UTF-8 was malformed")]
    InvalidUtf8,
    #[error("provider observation identity did not satisfy its bounded external contract")]
    InvalidIdentity,
    #[error("provider observation structure did not match its selected schema")]
    StructureMismatch,
    #[error("provider observation structured index or depth did not match")]
    IndexMismatch,
    #[error("provider observation structured depth exceeded 128")]
    StructuredDepthExceeded,
    #[error("provider observation frontier overflowed")]
    FrontierOverflow,
    #[error("provider observation ended inside text")]
    IncompleteText,
    #[error("provider observation ended inside a container")]
    IncompleteStructure,
    #[error("provider observation omitted its item identity")]
    MissingItemIdentity,
    #[error("provider lifecycle observation omitted its timestamp")]
    MissingLifecycleTimestamp,
    #[error("provider observation field is not part of the active typed schema")]
    FieldNotAllowed,
    #[error("provider observation repeated a field in the same typed object")]
    DuplicateField,
    #[error("provider observation value control or scalar type did not match its field")]
    ValueMismatch,
    #[error("provider observation enum token did not belong to its field vocabulary")]
    EnumMismatch,
    #[error("provider observation omitted a field required by the selected schema")]
    MissingRequiredField,
    #[error("provider `Other` marker is not the pinned Web-search action marker")]
    OtherMarkerMismatch,
    #[error("provider observation item kind is not valid for this lifecycle")]
    InvalidLifecycle,
}
