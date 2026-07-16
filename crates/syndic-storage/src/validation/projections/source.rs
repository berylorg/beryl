use beryl_home_store::DomainReader;

use crate::{
    AssistantMessagePhase, CanonicalItemKind, ContentChunkOrdinal, ContentChunkRecord,
    ContentEncoding, ContentLifecycle, ItemSourceEventOrdinal, ProviderItemDisposition,
    ProviderItemKind, ProviderItemLifecycle, SourceEventPayload, advance_content_chain, codec::*,
    content_chain_seed, domain::SyndicDomain, error::SyndicValidationError,
};

use super::super::scan::{point, require, scan};

pub(super) fn validate(
    reader: &DomainReader<'_, SyndicDomain>,
) -> Result<(), SyndicValidationError> {
    validate_index_presence(reader)?;
    validate_replay(reader)
}

fn validate_index_presence(
    reader: &DomainReader<'_, SyndicDomain>,
) -> Result<(), SyndicValidationError> {
    scan::<CanonicalItemsFamily>(reader, |_, item| {
        if (item.source_event_count() == 0) != item.source_event().is_none() {
            return invariant("canonical item source-event frontier shape disagrees");
        }
        let first = ItemEventKey {
            owner: item.id(),
            ordinal: ItemSourceEventOrdinal::FIRST,
        };
        if (item.source_event_count() == 0)
            == point::<ItemSourceEventsFamily>(reader, &first)?.is_some()
        {
            return invariant("canonical item source-event index presence disagrees");
        }
        if item.source_event_count() == 0
            && (item.provider_kind() != ProviderItemKind::UserMessage
                || item.provider_lifecycle() != ProviderItemLifecycle::AwaitingCorrelation)
        {
            return invariant("uncorrelated canonical item is not submitted user input");
        }
        Ok(())
    })
}

fn validate_replay(reader: &DomainReader<'_, SyndicDomain>) -> Result<(), SyndicValidationError> {
    let mut replay = None;
    scan::<ItemSourceEventsFamily>(reader, |key, index| {
        if replay
            .as_ref()
            .is_some_and(|state: &Replay| state.item != key.owner)
        {
            finish(reader, replay.take().expect("replay exists"))?;
        }
        if replay.is_none() {
            replay = Some(Replay::new(key.owner));
        }
        replay
            .as_mut()
            .expect("replay exists")
            .push(reader, key, index)
    })?;
    if let Some(state) = replay {
        finish(reader, state)?;
    }
    Ok(())
}

struct Replay {
    item: beryl_model::SyndicItemId,
    next_ordinal: u64,
    last_sequence: Option<crate::SourceEventSequence>,
    chunk_count: u64,
    encoded_bytes: u64,
    chain: beryl_model::SyndicContentDigest,
    kind: Option<ProviderItemKind>,
    disposition: Option<ProviderItemDisposition>,
    assistant_phase: Option<AssistantMessagePhase>,
    completed: bool,
}

impl Replay {
    fn new(item: beryl_model::SyndicItemId) -> Self {
        Self {
            item,
            next_ordinal: 1,
            last_sequence: None,
            chunk_count: 0,
            encoded_bytes: 0,
            chain: content_chain_seed(ContentEncoding::Utf8V1),
            kind: None,
            disposition: None,
            assistant_phase: None,
            completed: false,
        }
    }

    fn push(
        &mut self,
        reader: &DomainReader<'_, SyndicDomain>,
        key: &ItemEventKey,
        index: &crate::ItemSourceEventIndexRecord,
    ) -> Result<(), SyndicValidationError> {
        if key.owner != self.item
            || key.ordinal != index.ordinal()
            || index.item_id() != self.item
            || index.ordinal().get() != self.next_ordinal
            || self
                .last_sequence
                .is_some_and(|previous| index.source_event() <= previous)
        {
            return invariant("item source-event key, order, or sequence disagrees");
        }
        let item = require::<CanonicalItemsFamily>(
            reader,
            &self.item,
            "item source-event owner is missing",
        )?;
        if item.turn_id() != index.turn_id() {
            return invariant("item source-event turn owner disagrees");
        }
        let event = require::<SourceEventsFamily>(
            reader,
            &TurnEventKey {
                owner: index.turn_id(),
                ordinal: index.source_event(),
            },
            "item source event is missing",
        )?;
        if event.payload().item_id() != Some(self.item) {
            return invariant("item source-event payload names another item");
        }
        validate_external_source(&item, &event)?;
        self.apply_event(&item, event.payload())?;
        self.last_sequence = Some(index.source_event());
        self.next_ordinal =
            self.next_ordinal
                .checked_add(1)
                .ok_or(SyndicValidationError::Invariant(
                    "item source-event order exhausted",
                ))?;
        Ok(())
    }
}

impl Replay {
    fn apply_event(
        &mut self,
        item: &crate::CanonicalItemRecord,
        payload: &SourceEventPayload,
    ) -> Result<(), SyndicValidationError> {
        match payload {
            SourceEventPayload::ItemStarted {
                item: descriptor,
                assistant_phase,
            } if self.kind.is_none() && !self.completed && self.next_ordinal == 1 => {
                self.kind = Some(descriptor.kind());
                self.disposition = Some(descriptor.disposition());
                self.assistant_phase = *assistant_phase;
            }
            SourceEventPayload::ItemDelta {
                expected_kind,
                text,
                ..
            } if self.kind == Some(*expected_kind)
                && self.disposition == Some(ProviderItemDisposition::CanonicalText)
                && !self.completed =>
            {
                self.append(item, text.as_str())?;
            }
            SourceEventPayload::ItemCompleted {
                item: descriptor,
                assistant_phase,
            } if !self.completed => {
                if let (Some(kind), Some(disposition)) = (self.kind, self.disposition) {
                    if descriptor.kind() != kind || descriptor.disposition() != disposition {
                        return invariant("completed source item changed kind or disposition");
                    }
                    self.assistant_phase = merge_phase(self.assistant_phase, *assistant_phase)?;
                } else if self.next_ordinal == 1
                    && descriptor.kind().permits_completion_only()
                    && descriptor.disposition() == ProviderItemDisposition::ActivityOnly
                {
                    self.kind = Some(descriptor.kind());
                    self.disposition = Some(descriptor.disposition());
                    self.assistant_phase = *assistant_phase;
                } else {
                    return invariant("completion-only source item lifecycle is not admitted");
                }
                self.completed = true;
            }
            SourceEventPayload::TurnActivated
            | SourceEventPayload::TurnEnded(_)
            | SourceEventPayload::ItemStarted { .. }
            | SourceEventPayload::ItemDelta { .. }
            | SourceEventPayload::ItemCompleted { .. } => {
                return invariant("item source-event lifecycle is not canonical");
            }
        }
        Ok(())
    }

    fn append(
        &mut self,
        item: &crate::CanonicalItemRecord,
        text: &str,
    ) -> Result<(), SyndicValidationError> {
        let content = item
            .payload()
            .content()
            .ok_or(SyndicValidationError::Invariant(
                "text source replay item omitted canonical content",
            ))?;
        for bytes in crate::content::utf8_chunks(text) {
            self.chunk_count =
                self.chunk_count
                    .checked_add(1)
                    .ok_or(SyndicValidationError::Invariant(
                        "canonical replay chunk count overflowed",
                    ))?;
            let chunk = ContentChunkRecord::new(
                content.id(),
                ContentChunkOrdinal::new(self.chunk_count)
                    .map_err(|_| SyndicValidationError::Invariant("canonical replay exhausted"))?,
                bytes,
            )
            .map_err(|_| SyndicValidationError::Invariant("canonical replay chunk is invalid"))?;
            self.chain = advance_content_chain(self.chain, &chunk);
        }
        self.encoded_bytes = self
            .encoded_bytes
            .checked_add(u64::try_from(text.len()).map_err(|_| {
                SyndicValidationError::Invariant("canonical replay text exceeds u64")
            })?)
            .ok_or(SyndicValidationError::Invariant(
                "canonical replay byte count overflowed",
            ))?;
        Ok(())
    }
}

fn finish(
    reader: &DomainReader<'_, SyndicDomain>,
    replay: Replay,
) -> Result<(), SyndicValidationError> {
    let item = require::<CanonicalItemsFamily>(
        reader,
        &replay.item,
        "item source-event owner is missing",
    )?;
    let observed = replay.next_ordinal - 1;
    let expected_lifecycle = if replay.completed {
        ProviderItemLifecycle::Completed
    } else {
        ProviderItemLifecycle::Started
    };
    if item.source_event_count() != observed
        || item.source_event() != replay.last_sequence
        || replay.kind != Some(item.provider_kind())
        || replay.disposition != Some(item.disposition())
        || replay.assistant_phase != item.assistant_phase()
        || item.provider_lifecycle() != expected_lifecycle
    {
        return invariant("canonical item does not equal replayed source lifecycle");
    }
    if item.disposition() == ProviderItemDisposition::CanonicalText {
        finish_text(reader, &item, &replay)?;
    } else if replay.chunk_count != 0 || replay.encoded_bytes != 0 {
        return invariant("content-less item source replay retained text bytes");
    }
    Ok(())
}

fn finish_text(
    reader: &DomainReader<'_, SyndicDomain>,
    item: &crate::CanonicalItemRecord,
    replay: &Replay,
) -> Result<(), SyndicValidationError> {
    let content = item
        .payload()
        .content()
        .ok_or(SyndicValidationError::Invariant(
            "canonical text item omitted content authority",
        ))?;
    let manifest = require::<ContentManifestsFamily>(
        reader,
        &content.id(),
        "item source-event content is missing",
    )?;
    if manifest.chunk_count() != replay.chunk_count
        || manifest.encoded_bytes() != replay.encoded_bytes
        || manifest.expected().logical_utf8_bytes() != replay.encoded_bytes
        || manifest.chain_digest() != replay.chain
        || manifest.expected().digest() != replay.chain
    {
        return invariant("canonical item does not equal replayed source-event text");
    }
    if replay.completed && !manifest.lifecycle().is_immutable() {
        return invariant("completed canonical item retains live content");
    }
    if !replay.completed && manifest.lifecycle() == ContentLifecycle::Finalized {
        let state = require::<TurnStatesFamily>(
            reader,
            &item.turn_id(),
            "finalized canonical item turn state is missing",
        )?;
        if !state.lifecycle().is_proven_terminal() {
            return invariant("unterminated source item finalized before its turn");
        }
    }
    Ok(())
}

fn validate_external_source(
    item: &crate::CanonicalItemRecord,
    event: &crate::SourceEventRecord,
) -> Result<(), SyndicValidationError> {
    match (
        item.cas_source(),
        event.source(),
        event.payload().cas_item_id(),
    ) {
        (Some(item_source), Some(turn_source), Some(item_id))
            if item_source.turn() == turn_source && item_source.item_id() == item_id =>
        {
            Ok(())
        }
        _ => invariant("item source-event external identity disagrees"),
    }
}

fn merge_phase(
    current: Option<AssistantMessagePhase>,
    supplied: Option<AssistantMessagePhase>,
) -> Result<Option<AssistantMessagePhase>, SyndicValidationError> {
    match (current, supplied) {
        (None, None) => Ok(None),
        (Some(AssistantMessagePhase::Unknown), Some(observed)) => Ok(Some(observed)),
        (Some(known), Some(AssistantMessagePhase::Unknown)) => Ok(Some(known)),
        (Some(left), Some(right)) if left == right => Ok(Some(left)),
        (Some(known), None) => Ok(Some(known)),
        _ => invariant("assistant source phases disagree"),
    }
}

pub(super) struct SnapshotReplay {
    replay: Replay,
    source_revision: u64,
}

impl SnapshotReplay {
    pub(super) fn new(item: beryl_model::SyndicItemId) -> Self {
        Self {
            replay: Replay::new(item),
            source_revision: 0,
        }
    }

    pub(super) fn validate(
        &mut self,
        reader: &DomainReader<'_, SyndicDomain>,
        item: &crate::CanonicalItemRecord,
        source_item_revision: beryl_model::ProjectionRevision,
        content: crate::ContentReference,
    ) -> Result<(), SyndicValidationError> {
        let item_content = item
            .payload()
            .content()
            .ok_or(SyndicValidationError::Invariant(
                "projection source item omitted canonical content",
            ))?;
        let manifest = require::<ContentManifestsFamily>(
            reader,
            &content.id(),
            "projection source content is missing",
        )?;
        if item.id() != self.replay.item
            || content.id() != item_content.id()
            || content.encoding() != item_content.encoding()
            || manifest.encoding() != content.encoding()
        {
            return invariant("projection source content identity disagrees");
        }
        if source_item_revision == item.revision() {
            if content != item_content || manifest.current_reference() != Some(content) {
                return invariant("current projection source snapshot disagrees");
            }
            return Ok(());
        }
        if matches!(item.kind(), CanonicalItemKind::UserInput) {
            let Some(current_revision) = item.source_event_count().checked_add(1) else {
                return invariant("historical user projection source snapshot is invalid");
            };
            let source_frontier = source_item_revision.get() - 1;
            if item.revision().get() != current_revision
                || source_item_revision >= item.revision()
                || source_frontier >= item.source_event_count()
                || source_frontier < self.source_revision
                || content != item_content
                || manifest.current_reference() != Some(content)
            {
                return invariant("historical user projection source snapshot is invalid");
            }
            self.replay_to(reader, item, source_frontier)?;
            let lifecycle_matches = match source_frontier {
                0 => {
                    self.replay.kind.is_none()
                        && self.replay.disposition.is_none()
                        && self.replay.assistant_phase.is_none()
                        && !self.replay.completed
                }
                1 => {
                    self.replay.kind == Some(ProviderItemKind::UserMessage)
                        && self.replay.disposition == Some(item.disposition())
                        && self.replay.assistant_phase.is_none()
                        && !self.replay.completed
                }
                _ => false,
            };
            return if lifecycle_matches {
                Ok(())
            } else {
                invariant("historical user projection source snapshot is invalid")
            };
        }
        if content.encoding() != ContentEncoding::Utf8V1
            || manifest.owner() != Some(item.id())
            || source_item_revision >= item.revision()
            || source_item_revision.get() != content.revision().get()
            || source_item_revision.get() > item.source_event_count()
            || source_item_revision.get() <= self.source_revision
        {
            return invariant("historical projection source snapshot is invalid");
        }
        self.replay_to(reader, item, source_item_revision.get())?;
        let expected = crate::ContentSummary::new(
            self.replay.chunk_count,
            self.replay.chunk_count,
            self.replay.encoded_bytes,
            self.replay.encoded_bytes,
            1,
            0,
            crate::content::input_marker_digest(std::iter::empty()),
            self.replay.chain,
        );
        if content.summary() != expected {
            return invariant("historical projection source snapshot disagrees with replay");
        }
        Ok(())
    }

    fn replay_to(
        &mut self,
        reader: &DomainReader<'_, SyndicDomain>,
        item: &crate::CanonicalItemRecord,
        target_revision: u64,
    ) -> Result<(), SyndicValidationError> {
        while self.source_revision < target_revision {
            let ordinal = ItemSourceEventOrdinal::new(self.source_revision.checked_add(1).ok_or(
                SyndicValidationError::Invariant("projection source replay exhausted"),
            )?)
            .map_err(|_| SyndicValidationError::Invariant("projection source replay exhausted"))?;
            let key = ItemEventKey {
                owner: item.id(),
                ordinal,
            };
            let index = require::<ItemSourceEventsFamily>(
                reader,
                &key,
                "projection source event index is missing",
            )?;
            self.replay.push(reader, &key, &index)?;
            self.source_revision = ordinal.get();
        }
        Ok(())
    }
}

fn invariant<T>(message: &'static str) -> Result<T, SyndicValidationError> {
    Err(SyndicValidationError::Invariant(message))
}
