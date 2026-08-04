use sha2::{Digest, Sha256};

use crate::ProviderObservationChunkPayload;
use crate::provider_observation::{
    ProviderField, ProviderObservationControl, ProviderStructuredPosition, ProviderValueContext,
};

use super::{
    ObservationReplayReader, ReplayError, ReplayWriteError, TextSelector, TextSummary, duplicate,
    traversal,
};
use crate::provider_observation::compiler::ProviderObservationFrameSemanticError;

enum TextVisitorError<E> {
    Semantic(ProviderObservationFrameSemanticError),
    Output(E),
}

impl ObservationReplayReader<'_> {
    pub(in crate::provider_observation::compiler) fn text_summary(
        &self,
        selector: TextSelector,
    ) -> Result<TextSummary, ReplayError> {
        let mut active = false;
        let mut found = false;
        let mut bytes = 0_u64;
        let mut hasher = Sha256::new();
        self.scan(|payload, location| {
            match payload {
                ProviderObservationChunkPayload::Control(
                    ProviderObservationControl::BeginField(context),
                ) if matches_text(selector, *context, location)? => {
                    if found || active {
                        return Err(duplicate(selector.root_field()));
                    }
                    active = true;
                    found = true;
                }
                ProviderObservationChunkPayload::Fragment {
                    context: _,
                    bytes: fragment,
                } if active => {
                    bytes = bytes
                        .checked_add(u64::try_from(fragment.len()).map_err(|_| traversal())?)
                        .ok_or_else(traversal)?;
                    hasher.update(fragment);
                }
                ProviderObservationChunkPayload::Control(ProviderObservationControl::EndField(
                    context,
                )) if active && matches_text(selector, *context, location)? => {
                    active = false;
                }
                _ => {}
            }
            Ok(())
        })
        .map_err(ReplayError::from)?;
        if !found || active {
            return Err(ReplayError::Semantic(
                ProviderObservationFrameSemanticError::MissingField {
                    field: selector.root_field(),
                },
            ));
        }
        Ok(TextSummary {
            bytes,
            digest: hasher.finalize().into(),
        })
    }
}

impl ObservationReplayReader<'_> {
    pub(in crate::provider_observation::compiler) fn write_text<E>(
        &self,
        selector: TextSelector,
        mut output: impl FnMut(&[u8]) -> Result<(), E>,
    ) -> Result<(), ReplayWriteError<E>> {
        let mut active = false;
        let mut found = false;
        let result = self.scan(|payload, location| {
            match payload {
                ProviderObservationChunkPayload::Control(
                    ProviderObservationControl::BeginField(context),
                ) if matches_text(selector, *context, location)
                    .map_err(TextVisitorError::Semantic)? =>
                {
                    if found || active {
                        return Err(TextVisitorError::Semantic(duplicate(selector.root_field())));
                    }
                    active = true;
                    found = true;
                }
                ProviderObservationChunkPayload::Fragment { context: _, bytes } if active => {
                    output(bytes).map_err(TextVisitorError::Output)?
                }
                ProviderObservationChunkPayload::Control(ProviderObservationControl::EndField(
                    context,
                )) if active
                    && matches_text(selector, *context, location)
                        .map_err(TextVisitorError::Semantic)? =>
                {
                    active = false;
                }
                _ => {}
            }
            Ok(())
        });
        match result {
            Ok(()) if found && !active => Ok(()),
            Ok(()) => Err(ReplayWriteError::Replay(ReplayError::Semantic(
                ProviderObservationFrameSemanticError::MissingField {
                    field: selector.root_field(),
                },
            ))),
            Err(super::super::ReplayScanError::Visitor(TextVisitorError::Output(error))) => {
                Err(ReplayWriteError::Output(error))
            }
            Err(super::super::ReplayScanError::Visitor(TextVisitorError::Semantic(error))) => {
                Err(ReplayWriteError::Replay(ReplayError::Semantic(error)))
            }
            Err(super::super::ReplayScanError::Cursor(error)) => {
                Err(ReplayWriteError::Replay(ReplayError::Cursor(error)))
            }
            Err(super::super::ReplayScanError::Validation(error)) => {
                Err(ReplayWriteError::Replay(ReplayError::Validation(error)))
            }
            Err(super::super::ReplayScanError::Semantic(error)) => {
                Err(ReplayWriteError::Replay(ReplayError::Semantic(error)))
            }
            Err(super::super::ReplayScanError::FrontierOverflow) => {
                Err(ReplayWriteError::Replay(ReplayError::FrontierOverflow))
            }
        }
    }
}

impl TextSelector {
    pub(in crate::provider_observation::compiler) const fn root_field(self) -> ProviderField {
        match self {
            Self::Field(selector) => selector.field,
            Self::AgentStateKey { .. } => ProviderField::CollabAgentStateKey,
            Self::Structured(path) => path.root,
        }
    }
}

fn matches_text(
    selector: TextSelector,
    context: ProviderValueContext,
    location: super::super::ReplayLocation<'_>,
) -> Result<bool, ProviderObservationFrameSemanticError> {
    match selector {
        TextSelector::Field(selector) => Ok(context == ProviderValueContext::Field(selector.field)
            && location.matches_field(selector)),
        TextSelector::AgentStateKey { entry } => Ok(matches!(
            context,
            ProviderValueContext::Structured {
                root: ProviderField::CollabAgentStates,
                depth: 0,
                position: ProviderStructuredPosition::ObjectKey { entry: actual },
            } if actual == entry && location.agent_state_entry() == Some(entry)
        )),
        TextSelector::Structured(path) => Ok(location
            .structured_value_path(context)?
            .is_some_and(|candidate| path.matches_candidate(candidate, location))),
    }
}
