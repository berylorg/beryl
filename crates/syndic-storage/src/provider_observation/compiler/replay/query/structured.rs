use crate::ProviderObservationChunkPayload;
use crate::provider_observation::{
    ProviderContainer, ProviderField, ProviderObservationControl, ProviderScalar,
};

use super::{
    ObservationReplayReader, ReplayError, StructuredNode, StructuredPath, TextSelector, duplicate,
    traversal,
};
use crate::provider_observation::compiler::ProviderObservationFrameSemanticError;

impl ObservationReplayReader<'_> {
    pub(in crate::provider_observation::compiler) fn structured_node(
        &self,
        path: StructuredPath,
    ) -> Result<StructuredNode, ReplayError> {
        #[derive(Clone, Copy, PartialEq)]
        enum Kind {
            Scalar(ProviderScalar),
            Text,
            Container(ProviderContainer),
        }

        let mut kind = None;
        let mut count = 0_u64;
        self.scan(|payload, location| {
            let ProviderObservationChunkPayload::Control(control) = payload else {
                return Ok(());
            };
            match control {
                ProviderObservationControl::BeginField(context)
                    if location
                        .structured_value_path(*context)?
                        .is_some_and(|candidate| path.matches_candidate(candidate, location)) =>
                {
                    set_structured_kind(&mut kind, Kind::Text, path.root)?;
                }
                ProviderObservationControl::Scalar { context, value }
                    if location
                        .structured_value_path(*context)?
                        .is_some_and(|candidate| path.matches_candidate(candidate, location)) =>
                {
                    set_structured_kind(&mut kind, Kind::Scalar(*value), path.root)?;
                }
                ProviderObservationControl::BeginContainer { context, container }
                    if location
                        .structured_value_path(*context)?
                        .is_some_and(|candidate| path.matches_candidate(candidate, location)) =>
                {
                    set_structured_kind(&mut kind, Kind::Container(*container), path.root)?;
                }
                ProviderObservationControl::BeginElement { index, .. }
                    if path.matches_candidate(
                        location.structured_container_path(path.root)?,
                        location,
                    ) =>
                {
                    count = index.checked_add(1).ok_or_else(traversal)?;
                }
                ProviderObservationControl::BeginObjectEntry { root, entry, .. }
                    if *root == path.root
                        && path.matches_candidate(
                            location.structured_container_path(path.root)?,
                            location,
                        ) =>
                {
                    count = entry.checked_add(1).ok_or_else(traversal)?;
                }
                _ => {}
            }
            Ok(())
        })
        .map_err(ReplayError::from)?;

        match kind.ok_or_else(|| {
            ReplayError::Semantic(ProviderObservationFrameSemanticError::MissingField {
                field: path.root,
            })
        })? {
            Kind::Scalar(ProviderScalar::Null) => Ok(StructuredNode::Null),
            Kind::Scalar(ProviderScalar::Boolean(value)) => Ok(StructuredNode::Boolean(value)),
            Kind::Scalar(ProviderScalar::Signed(value)) => Ok(StructuredNode::Signed(value)),
            Kind::Scalar(ProviderScalar::Unsigned(value)) => Ok(StructuredNode::Unsigned(value)),
            Kind::Scalar(ProviderScalar::FiniteFloat(value)) => {
                Ok(StructuredNode::FiniteFloat(value.bits()))
            }
            Kind::Text => self
                .text_summary(TextSelector::Structured(path))
                .map(StructuredNode::String),
            Kind::Container(ProviderContainer::List) => Ok(StructuredNode::List(count)),
            Kind::Container(ProviderContainer::Object) => Ok(StructuredNode::Object(count)),
        }
    }
}

fn set_structured_kind<T: Copy + PartialEq>(
    target: &mut Option<T>,
    value: T,
    root: ProviderField,
) -> Result<(), ProviderObservationFrameSemanticError> {
    if target.replace(value).is_some() {
        Err(duplicate(root))
    } else {
        Ok(())
    }
}
