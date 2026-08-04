use crate::ProviderObservationChunkPayload;

mod structured;
mod text;

use super::{ObservationReplayReader, ReplayError};
use crate::provider_observation::compiler::ProviderObservationFrameSemanticError;
use crate::provider_observation::{
    ProviderContainer, ProviderEnumValue, ProviderField, ProviderObservationControl,
    ProviderScalar, ProviderValueContext,
};

const MAX_STRUCTURED_PATH_DEPTH: usize = 128;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::provider_observation::compiler) enum ObjectConstraint {
    Any,
    None,
    Field(ProviderField),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::provider_observation::compiler) struct FieldSelector {
    pub(in crate::provider_observation::compiler) field: ProviderField,
    pub(in crate::provider_observation::compiler) list: Option<(ProviderField, u64)>,
    pub(in crate::provider_observation::compiler) object: ObjectConstraint,
    pub(in crate::provider_observation::compiler) agent_entry: Option<u64>,
}

impl FieldSelector {
    pub(in crate::provider_observation::compiler) const fn top(field: ProviderField) -> Self {
        Self {
            field,
            list: None,
            object: ObjectConstraint::None,
            agent_entry: None,
        }
    }

    pub(in crate::provider_observation::compiler) const fn in_object(
        field: ProviderField,
        object: ProviderField,
    ) -> Self {
        Self {
            field,
            list: None,
            object: ObjectConstraint::Field(object),
            agent_entry: None,
        }
    }

    pub(in crate::provider_observation::compiler) const fn in_list(
        field: ProviderField,
        list: ProviderField,
        index: u64,
    ) -> Self {
        Self {
            field,
            list: Some((list, index)),
            object: ObjectConstraint::Any,
            agent_entry: None,
        }
    }

    pub(in crate::provider_observation::compiler) const fn in_agent_entry(
        field: ProviderField,
        entry: u64,
    ) -> Self {
        Self {
            field,
            list: None,
            object: ObjectConstraint::Any,
            agent_entry: Some(entry),
        }
    }

    pub(in crate::provider_observation::compiler) const fn with_object(
        mut self,
        object: ProviderField,
    ) -> Self {
        self.object = ObjectConstraint::Field(object);
        self
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
// The structured path is an explicitly bounded resident stack. Boxing it would add allocation and
// remove the selector's Copy semantics without reducing logical-data residency.
#[allow(clippy::large_enum_variant)]
pub(in crate::provider_observation::compiler) enum TextSelector {
    Field(FieldSelector),
    AgentStateKey { entry: u64 },
    Structured(StructuredPath),
}

impl From<FieldSelector> for TextSelector {
    fn from(value: FieldSelector) -> Self {
        Self::Field(value)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum StructuredPathStep {
    List(u64),
    ObjectKey(u64),
    ObjectValue(u64),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::provider_observation::compiler) struct StructuredPath {
    root: ProviderField,
    owner: Option<(ProviderField, u64)>,
    depth: u8,
    steps: [Option<StructuredPathStep>; MAX_STRUCTURED_PATH_DEPTH],
}

impl StructuredPath {
    pub(in crate::provider_observation::compiler) const fn new(root: ProviderField) -> Self {
        Self {
            root,
            owner: None,
            depth: 0,
            steps: [None; MAX_STRUCTURED_PATH_DEPTH],
        }
    }

    pub(in crate::provider_observation::compiler) const fn in_list(
        root: ProviderField,
        list: ProviderField,
        index: u64,
    ) -> Self {
        Self {
            root,
            owner: Some((list, index)),
            depth: 0,
            steps: [None; MAX_STRUCTURED_PATH_DEPTH],
        }
    }

    pub(in crate::provider_observation::compiler) fn matches_candidate(
        self,
        candidate: Self,
        location: super::ReplayLocation<'_>,
    ) -> bool {
        let same_path = self.root == candidate.root
            && self.depth == candidate.depth
            && self.steps == candidate.steps;
        same_path
            && self
                .owner
                .is_none_or(|(list, index)| location.typed_list_index(list) == Some(index))
    }

    fn push(
        &mut self,
        step: StructuredPathStep,
    ) -> Result<(), ProviderObservationFrameSemanticError> {
        let slot = self
            .steps
            .get_mut(usize::from(self.depth))
            .ok_or(ProviderObservationFrameSemanticError::StructuredTraversalMismatch)?;
        *slot = Some(step);
        self.depth = self
            .depth
            .checked_add(1)
            .ok_or(ProviderObservationFrameSemanticError::StructuredTraversalMismatch)?;
        Ok(())
    }

    pub(in crate::provider_observation::compiler) fn push_list(
        &mut self,
        index: u64,
    ) -> Result<(), ProviderObservationFrameSemanticError> {
        self.push(StructuredPathStep::List(index))
    }

    pub(in crate::provider_observation::compiler) fn push_object_key(
        &mut self,
        entry: u64,
    ) -> Result<(), ProviderObservationFrameSemanticError> {
        self.push(StructuredPathStep::ObjectKey(entry))
    }

    pub(in crate::provider_observation::compiler) fn push_object_value(
        &mut self,
        entry: u64,
    ) -> Result<(), ProviderObservationFrameSemanticError> {
        self.push(StructuredPathStep::ObjectValue(entry))
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::provider_observation::compiler) enum Presence {
    Missing,
    Null,
    Value,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::provider_observation::compiler) struct TextSummary {
    pub(in crate::provider_observation::compiler) bytes: u64,
    pub(in crate::provider_observation::compiler) digest: [u8; 32],
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::provider_observation::compiler) enum StructuredNode {
    Null,
    Boolean(bool),
    Signed(i64),
    Unsigned(u64),
    FiniteFloat(u64),
    String(TextSummary),
    List(u64),
    Object(u64),
}

impl ObservationReplayReader<'_> {
    pub(in crate::provider_observation::compiler) fn presence(
        &self,
        selector: FieldSelector,
    ) -> Result<Presence, ReplayError> {
        let mut found = Presence::Missing;
        self.scan(|payload, location| {
            let ProviderObservationChunkPayload::Control(control) = payload else {
                return Ok(());
            };
            let (context, presence) = match control {
                ProviderObservationControl::BeginField(context)
                | ProviderObservationControl::BeginContainer { context, .. }
                | ProviderObservationControl::Enum { context, .. } => (*context, Presence::Value),
                ProviderObservationControl::Scalar { context, value } => (
                    *context,
                    if *value == ProviderScalar::Null {
                        Presence::Null
                    } else {
                        Presence::Value
                    },
                ),
                _ => return Ok(()),
            };
            if context == ProviderValueContext::Field(selector.field)
                && location.matches_field(selector)
            {
                if found != Presence::Missing {
                    return Err(duplicate(selector.field));
                }
                found = presence;
            }
            Ok(())
        })
        .map_err(ReplayError::from)?;
        Ok(found)
    }

    pub(in crate::provider_observation::compiler) fn scalar(
        &self,
        selector: FieldSelector,
    ) -> Result<Option<ProviderScalar>, ReplayError> {
        let mut found = None;
        self.scan(|payload, location| {
            let ProviderObservationChunkPayload::Control(ProviderObservationControl::Scalar {
                context,
                value,
            }) = payload
            else {
                return Ok(());
            };
            if *context == ProviderValueContext::Field(selector.field)
                && location.matches_field(selector)
                && found.replace(*value).is_some()
            {
                return Err(duplicate(selector.field));
            }
            Ok(())
        })
        .map_err(ReplayError::from)?;
        Ok(found)
    }

    pub(in crate::provider_observation::compiler) fn enum_value(
        &self,
        selector: FieldSelector,
    ) -> Result<Option<ProviderEnumValue>, ReplayError> {
        let mut found = None;
        self.scan(|payload, location| {
            let ProviderObservationChunkPayload::Control(ProviderObservationControl::Enum {
                context,
                value,
            }) = payload
            else {
                return Ok(());
            };
            if *context == ProviderValueContext::Field(selector.field)
                && location.matches_field(selector)
                && found.replace(*value).is_some()
            {
                return Err(duplicate(selector.field));
            }
            Ok(())
        })
        .map_err(ReplayError::from)?;
        Ok(found)
    }

    pub(in crate::provider_observation::compiler) fn list_count(
        &self,
        selector: FieldSelector,
    ) -> Result<Option<u64>, ReplayError> {
        self.container_count(selector, ProviderContainer::List)
    }

    pub(in crate::provider_observation::compiler) fn object_count(
        &self,
        selector: FieldSelector,
    ) -> Result<Option<u64>, ReplayError> {
        self.container_count(selector, ProviderContainer::Object)
    }

    fn container_count(
        &self,
        selector: FieldSelector,
        container: ProviderContainer,
    ) -> Result<Option<u64>, ReplayError> {
        let mut found = false;
        let mut count = 0_u64;
        self.scan(|payload, location| {
            let ProviderObservationChunkPayload::Control(control) = payload else {
                return Ok(());
            };
            match control {
                ProviderObservationControl::BeginContainer {
                    context,
                    container: actual,
                } if *context == ProviderValueContext::Field(selector.field)
                    && *actual == container
                    && location.matches_field(selector) =>
                {
                    if found {
                        return Err(duplicate(selector.field));
                    }
                    found = true;
                }
                ProviderObservationControl::BeginElement { context, index }
                    if container == ProviderContainer::List
                        && *context == ProviderValueContext::Field(selector.field)
                        && location.matches_field(selector) =>
                {
                    count = index.checked_add(1).ok_or_else(traversal)?;
                }
                ProviderObservationControl::BeginObjectEntry { root, entry, .. }
                    if container == ProviderContainer::Object
                        && *root == selector.field
                        && location.matches_field(selector) =>
                {
                    count = entry.checked_add(1).ok_or_else(traversal)?;
                }
                _ => {}
            }
            Ok(())
        })
        .map_err(ReplayError::from)?;
        Ok(found.then_some(count))
    }
}

fn duplicate(field: ProviderField) -> ProviderObservationFrameSemanticError {
    ProviderObservationFrameSemanticError::DuplicateFieldSelection { field }
}

fn traversal() -> ProviderObservationFrameSemanticError {
    ProviderObservationFrameSemanticError::TraversalMismatch
}

pub(in crate::provider_observation::compiler) enum ReplayWriteError<E> {
    Replay(ReplayError),
    Output(E),
}
