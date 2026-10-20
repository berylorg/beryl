use super::*;
use crate::{
    CanonicalItemKind, ContentEncoding, ContentLifecycle, ContextInjectionObservation,
    ContextLineageProof, ProjectionTextSource, ProviderNarrativeCompletionDisposition,
    TurnItemOrdinal,
};
use beryl_model::{RecoveryItemSequenceAccumulator, RecoveryItemSequenceRole};

pub(super) fn validate_replay(
    reader: &DomainReader<'_, SyndicDomain>,
    target: SyndicThreadId,
    proof: ContextLineageProof,
    observation: ContextInjectionObservation,
) -> Result<(), SyndicValidationError> {
    let tail = require::<TurnsFamily>(
        reader,
        &proof
            .established_prefix()
            .tail()
            .ok_or(SyndicValidationError::Invariant(
                "context replay prefix empty",
            ))?,
        "context replay tail missing",
    )?;
    let mut sequence = ReplaySequence {
        accumulator: RecoveryItemSequenceAccumulator::new(
            u64::from(observation.item_count().get()),
            observation.utf8_bytes().get(),
        ),
        ordinal: 0,
    };
    let mut previous = None;
    for depth in 1..=tail.depth().get() {
        let turn = crate::selected_path::lift_to_depth(
            tail.clone(),
            crate::TurnDepth::new(depth).unwrap(),
            |id| require::<TurnsFamily>(reader, &id, "context replay ancestor missing"),
            SyndicValidationError::Invariant,
        )?;
        if previous != Some(turn.origin_thread_id()) {
            if let Some(frame) =
                super::context_frame(reader, proof.home_id(), turn.origin_thread_id())?
            {
                sequence.frame(reader, frame)?;
            }
            previous = Some(turn.origin_thread_id());
        }
        let state =
            require::<TurnStatesFamily>(reader, &turn.id(), "context replay state missing")?;
        if !matches!(
            turn.kind(),
            crate::TurnKind::OrdinaryUser
                | crate::TurnKind::BerylLifecycleContinuation
                | crate::TurnKind::BerylDiscussionHandoff
        ) || state.item_count() == 0
            || state.item_count() != state.finalized_item_count()
            || state.open_item_count() != 0
            || state.history_blocking_item_count() != 0
        {
            return invariant("context replay canonical frontier is not complete");
        }
        for ordinal in 1..=state.item_count() {
            let index = require::<TurnItemsFamily>(
                reader,
                &TurnItemKey {
                    owner: turn.id(),
                    ordinal: TurnItemOrdinal::new(ordinal).map_err(|_| {
                        SyndicValidationError::Invariant("context replay ordinal invalid")
                    })?,
                },
                "context replay item index missing",
            )?;
            let item = require::<CanonicalItemsFamily>(
                reader,
                &index.item_id(),
                "context replay item missing",
            )?;
            if item.turn_id() != turn.id()
                || item.ordinal() != index.ordinal()
                || item.revision() != index.item_revision()
            {
                return invariant("context replay canonical item/index disagree");
            }
            sequence.canonical(reader, &item)?;
        }
    }
    if tail.origin_thread_id() != target
        && let Some(frame) = super::context_frame(reader, proof.home_id(), target)?
    {
        sequence.frame(reader, frame)?;
    }
    if sequence
        .accumulator
        .finish()
        .map_err(|_| SyndicValidationError::Invariant("context replay totals disagree"))?
        != observation.sequence_digest()
    {
        return invariant("context replay sequence digest disagrees");
    }
    Ok(())
}

struct ReplaySequence {
    accumulator: RecoveryItemSequenceAccumulator,
    ordinal: u64,
}
impl ReplaySequence {
    fn begin(
        &mut self,
        role: RecoveryItemSequenceRole,
        bytes: u64,
    ) -> Result<(), SyndicValidationError> {
        self.ordinal = self
            .ordinal
            .checked_add(1)
            .ok_or(SyndicValidationError::Invariant(
                "context replay ordinal overflow",
            ))?;
        self.accumulator
            .begin_item(self.ordinal, role, bytes)
            .map_err(|_| {
                SyndicValidationError::Invariant("context replay item header/totals disagree")
            })
    }
    fn frame(
        &mut self,
        reader: &DomainReader<'_, SyndicDomain>,
        frame: crate::context_projection::frame::ContextFrameSource,
    ) -> Result<(), SyndicValidationError> {
        let record = require::<ContextEnvelopesFamily>(
            reader,
            &ContextOwnerKey::from(frame.owner),
            "context replay envelope missing",
        )?;
        self.begin(
            RecoveryItemSequenceRole::AssistantOutputText,
            frame.utf8_bytes,
        )?;
        crate::context_projection::frame::update_sequence_text(&record, &mut self.accumulator)
            .map_err(|_| SyndicValidationError::Invariant("context replay frame bytes disagree"))?;
        self.accumulator
            .finish_item()
            .map_err(|_| SyndicValidationError::Invariant("context replay frame length disagrees"))
    }
    fn canonical(
        &mut self,
        reader: &DomainReader<'_, SyndicDomain>,
        item: &crate::CanonicalItemRecord,
    ) -> Result<(), SyndicValidationError> {
        if item.is_history_blocking() {
            return invariant("context replay canonical item blocks history");
        }
        let (role, source) = match item.kind() {
            CanonicalItemKind::UserInput | CanonicalItemKind::DiscussionHandoff => {
                let content =
                    item.presentation_content()
                        .ok_or(SyndicValidationError::Invariant(
                            "context replay user content missing",
                        ))?;
                let manifest = require::<ContentManifestsFamily>(
                    reader,
                    &content.id(),
                    "context replay user manifest missing",
                )?;
                if item.presentation().asset_reference_set().is_some()
                    || content.summary().image_marker_count() != 0
                    || content.encoding() != ContentEncoding::ComposerV1
                    || manifest.lifecycle() != ContentLifecycle::Sealed
                    || manifest.owner().is_some()
                    || manifest.sealed_reference() != Some(content)
                {
                    return invariant("context replay user source is not closed text");
                }
                (
                    RecoveryItemSequenceRole::UserInputText,
                    ProjectionTextSource::composer(content),
                )
            }
            CanonicalItemKind::AssistantMessage(_) => {
                let source = item
                    .projection_source()
                    .ok_or(SyndicValidationError::Invariant(
                        "context replay assistant narrative missing",
                    ))?;
                let content = item
                    .provider_content()
                    .ok_or(SyndicValidationError::Invariant(
                        "context replay provider content missing",
                    ))?;
                let manifest = require::<ContentManifestsFamily>(
                    reader,
                    &content.id(),
                    "context replay provider manifest missing",
                )?;
                if !matches!(source, ProjectionTextSource::ProviderNarrative(_))
                    || item.narrative_completion()
                        != Some(ProviderNarrativeCompletionDisposition::Equal)
                    || manifest.lifecycle() != ContentLifecycle::Finalized
                    || manifest.owner() != Some(item.id())
                    || manifest.current_reference() != Some(content)
                {
                    return invariant("context replay assistant source is not finalized");
                }
                (RecoveryItemSequenceRole::AssistantOutputText, source)
            }
            _ => return invariant("context replay contains unsupported canonical item"),
        };
        self.begin(role, source.logical_utf8_bytes())?;
        let mut offset = 0;
        while offset < source.logical_utf8_bytes() {
            let end = (offset + 65_536).min(source.logical_utf8_bytes());
            let bytes = crate::validation::read_projection_text_range(reader, source, offset, end)?;
            self.accumulator.update_text(&bytes).map_err(|_| {
                SyndicValidationError::Invariant("context replay text totals disagree")
            })?;
            offset = end;
        }
        self.accumulator
            .finish_item()
            .map_err(|_| SyndicValidationError::Invariant("context replay item length disagrees"))
    }
}
