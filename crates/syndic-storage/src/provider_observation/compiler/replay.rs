mod query;

use beryl_home_store::HomeStore;

use crate::{ProviderObservationChunkPayload, SyndicPointReadLimit, SyndicStorage};

use super::super::{
    CanonicalObservationState, ProviderObservationElementKind, ProviderObservationFrame,
    ProviderObservationValidatorState, ProviderStructuredPosition, ProviderValueContext,
    cursor::ProviderObservationReplay,
};
use super::{ProviderObservationFramePreparationError, ProviderObservationFrameSemanticError};

pub(super) use query::{
    FieldSelector, ObjectConstraint, Presence, ReplayWriteError, StructuredNode, StructuredPath,
    TextSelector,
};

pub(super) struct ObservationReplayReader<'a> {
    storage: &'a SyndicStorage,
    store: &'a HomeStore,
    replay: &'a ProviderObservationReplay,
    limit: SyndicPointReadLimit,
}

impl<'a> ObservationReplayReader<'a> {
    pub(super) const fn new(
        storage: &'a SyndicStorage,
        store: &'a HomeStore,
        replay: &'a ProviderObservationReplay,
        limit: SyndicPointReadLimit,
    ) -> Self {
        Self {
            storage,
            store,
            replay,
            limit,
        }
    }

    pub(super) const fn begin(&self) -> super::super::ProviderObservationBegin {
        self.replay.build().begin()
    }

    pub(super) const fn history_support(&self) -> crate::ProviderFrameHistorySupportV1 {
        self.replay.build().history_support()
    }

    pub(super) fn scan<E>(
        &self,
        mut visitor: impl FnMut(&ProviderObservationChunkPayload, ReplayLocation<'_>) -> Result<(), E>,
    ) -> Result<(), ReplayScanError<E>> {
        let build = self.replay.build();
        let mut cursor = self
            .replay
            .open(self.storage, self.store, self.limit)
            .map_err(ReplayScanError::Cursor)?;
        let mut validator = ProviderObservationValidatorState::initial();
        let mut canonical = CanonicalObservationState::initial(build.begin());
        while let Some(page) = self
            .storage
            .read_provider_observation_cursor_page(self.store, &mut cursor, self.limit)
            .map_err(ReplayScanError::Cursor)?
        {
            let payload = page.payload();
            visitor(
                payload,
                ReplayLocation {
                    frames: &validator.frames,
                },
            )
            .map_err(ReplayScanError::Visitor)?;
            match payload {
                ProviderObservationChunkPayload::Control(control) => {
                    validator
                        .control(build.begin(), *control)
                        .map_err(ReplayScanError::Validation)?;
                }
                ProviderObservationChunkPayload::Fragment { context, bytes } => {
                    for byte in bytes.iter().copied() {
                        validator
                            .fragment_byte(*context, byte)
                            .map_err(ReplayScanError::Validation)?;
                    }
                }
            }
            canonical
                .apply_chunk(payload)
                .map_err(|_| ReplayScanError::FrontierOverflow)?;
        }
        validator
            .finish(build.begin())
            .map_err(ReplayScanError::Validation)?;
        if build.validator() != &validator
            || build.canonical_bytes() != canonical.canonical_bytes()
            || build.digest() != canonical.digest()
        {
            return Err(ReplayScanError::Semantic(
                ProviderObservationFrameSemanticError::ReplayMismatch,
            ));
        }
        Ok(())
    }
}

pub(super) enum ReplayScanError<E> {
    Cursor(super::super::ProviderObservationCursorError),
    Validation(super::super::ProviderObservationValidatorError),
    Semantic(ProviderObservationFrameSemanticError),
    FrontierOverflow,
    Visitor(E),
}

pub(super) enum ReplayError {
    Cursor(super::super::ProviderObservationCursorError),
    Validation(super::super::ProviderObservationValidatorError),
    Semantic(ProviderObservationFrameSemanticError),
    FrontierOverflow,
}

impl ReplayError {
    pub(super) fn preparation(self) -> ProviderObservationFramePreparationError {
        match self {
            Self::Cursor(error) => error.into(),
            Self::Validation(error) => error.into(),
            Self::Semantic(error) => error.into(),
            Self::FrontierOverflow => ProviderObservationFramePreparationError::FrontierOverflow,
        }
    }
}

impl From<ReplayScanError<ProviderObservationFrameSemanticError>> for ReplayError {
    fn from(value: ReplayScanError<ProviderObservationFrameSemanticError>) -> Self {
        match value {
            ReplayScanError::Cursor(error) => Self::Cursor(error),
            ReplayScanError::Validation(error) => Self::Validation(error),
            ReplayScanError::Semantic(error) | ReplayScanError::Visitor(error) => {
                Self::Semantic(error)
            }
            ReplayScanError::FrontierOverflow => Self::FrontierOverflow,
        }
    }
}

#[derive(Clone, Copy)]
pub(super) struct ReplayLocation<'a> {
    frames: &'a [ProviderObservationFrame],
}

impl ReplayLocation<'_> {
    pub(super) fn typed_list_index(self, field: super::super::ProviderField) -> Option<u64> {
        self.frames.iter().rev().find_map(|frame| match frame {
            ProviderObservationFrame::Element {
                context,
                index,
                kind: ProviderObservationElementKind::Typed(_),
                ..
            } if context.root() == field => Some(*index),
            _ => None,
        })
    }

    pub(super) fn enclosing_object(self) -> Option<super::super::ProviderField> {
        self.frames.iter().rev().find_map(|frame| match frame {
            ProviderObservationFrame::Object { context, .. } => Some(context.root()),
            _ => None,
        })
    }

    pub(super) fn agent_state_entry(self) -> Option<u64> {
        self.frames.iter().rev().find_map(|frame| match frame {
            ProviderObservationFrame::AgentStateEntry { entry, .. } => Some(*entry),
            _ => None,
        })
    }

    pub(super) fn structured_container_path(
        self,
        root: super::super::ProviderField,
    ) -> Result<StructuredPath, ProviderObservationFrameSemanticError> {
        let mut path = StructuredPath::new(root);
        for (index, frame) in self.frames.iter().enumerate() {
            match frame {
                ProviderObservationFrame::Element {
                    index: element,
                    kind:
                        ProviderObservationElementKind::Structured {
                            root: frame_root, ..
                        },
                    ..
                } if *frame_root == root && later_structured(self.frames, index, root) => {
                    path.push_list(*element)?;
                }
                ProviderObservationFrame::StructuredEntry {
                    root: frame_root,
                    entry,
                    ..
                } if *frame_root == root && later_structured(self.frames, index, root) => {
                    path.push_object_value(*entry)?;
                }
                _ => {}
            }
        }
        Ok(path)
    }

    pub(super) fn structured_value_path(
        self,
        context: ProviderValueContext,
    ) -> Result<Option<StructuredPath>, ProviderObservationFrameSemanticError> {
        match context {
            ProviderValueContext::Field(root) => Ok(Some(StructuredPath::new(root))),
            ProviderValueContext::Structured { root, position, .. } => {
                let mut path = self.structured_container_path(root)?;
                match position {
                    ProviderStructuredPosition::ListElement { index } => path.push_list(index)?,
                    ProviderStructuredPosition::ObjectKey { entry } => {
                        path.push_object_key(entry)?;
                    }
                    ProviderStructuredPosition::ObjectValue { entry } => {
                        path.push_object_value(entry)?;
                    }
                }
                Ok(Some(path))
            }
        }
    }

    pub(super) fn matches_field(self, selector: FieldSelector) -> bool {
        if let Some((field, index)) = selector.list
            && self.typed_list_index(field) != Some(index)
        {
            return false;
        }
        match selector.object {
            ObjectConstraint::Any => {}
            ObjectConstraint::None if self.enclosing_object().is_some() => return false,
            ObjectConstraint::Field(field) if self.enclosing_object() != Some(field) => {
                return false;
            }
            ObjectConstraint::None | ObjectConstraint::Field(_) => {}
        }
        if let Some(entry) = selector.agent_entry
            && self.agent_state_entry() != Some(entry)
        {
            return false;
        }
        true
    }
}

fn later_structured(
    frames: &[ProviderObservationFrame],
    index: usize,
    root: super::super::ProviderField,
) -> bool {
    frames[index + 1..].iter().any(|frame| {
        matches!(
            frame,
            ProviderObservationFrame::Structured { context, .. } if context.root() == root
        )
    })
}
