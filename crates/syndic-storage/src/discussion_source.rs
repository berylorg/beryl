use std::{error::Error, fmt};

use beryl_home_store::{
    DomainCallbackError, DomainCallbackSource, DomainHandle, DomainReader, DomainValidator,
    HomeStore, ReadError, ValidationContribution,
};
use beryl_model::{DomainRevision, ProjectionRevision};

use crate::{
    CanonicalItemKind, CurrentTranscriptEntryProof, DiscussionContextSource, DiscussionContextText,
    ItemProjectionHeadRecord, ItemProjectionSetRecord, ProjectionLifecycle, ProjectionSourceRange,
    ProjectionTextSource, SelectedPathProof, SyndicReadError, SyndicStorage,
    TranscriptViewHeadRecord, TurnRecord, TurnStateRevision, codec::*, domain::SyndicDomain,
};

pub struct PreparedDiscussionSource {
    pub(crate) handle: DomainHandle<SyndicDomain>,
    pub(crate) domain_revision: DomainRevision,
    pub(crate) source: DiscussionContextSource,
    pub(crate) text: DiscussionContextText,
    selected_path: SelectedPathProof,
    entry: CurrentTranscriptEntryProof,
    anchors: SourceAnchors,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct SourceAnchors {
    view: TranscriptViewHeadRecord,
    turn: TurnRecord,
    turn_state_revision: TurnStateRevision,
    item_revision: ProjectionRevision,
    head: ItemProjectionHeadRecord,
    set: ItemProjectionSetRecord,
    range: ProjectionSourceRange,
    text_source: ProjectionTextSource,
}

impl PreparedDiscussionSource {
    pub const fn source(&self) -> DiscussionContextSource {
        self.source
    }

    pub fn text(&self) -> &DiscussionContextText {
        &self.text
    }

    pub const fn domain_revision(&self) -> DomainRevision {
        self.domain_revision
    }

    pub fn validation(self) -> ValidationContribution {
        self.handle.clone().validation(self.domain_revision, self)
    }
}

impl SyndicStorage {
    pub fn prepare_discussion_source(
        &self,
        store: &HomeStore,
        source: DiscussionContextSource,
        selected_path: SelectedPathProof,
        entry: CurrentTranscriptEntryProof,
        text: DiscussionContextText,
    ) -> Result<PreparedDiscussionSource, DiscussionSourceError> {
        if source.range().len() != text.len() as u64 {
            return invalid("selected text length disagrees with its source range");
        }
        let before = self.revision(store)?;
        let anchors = authenticate(
            &SourceRead::Ordinary(self, store),
            source,
            selected_path,
            entry,
        )?;
        let mut bytes = vec![0; text.len()];
        crate::read::read_projection_text_source_range_into(
            self,
            store,
            anchors.text_source,
            source.range().start(),
            source.range().end(),
            &mut bytes,
        )
        .map_err(DiscussionSourceError::SourceRead)?;
        if bytes != text.as_str().as_bytes() {
            return invalid("selected text disagrees with its exact source bytes");
        }
        if self.revision(store)? != before {
            return invalid("discussion source changed during preparation");
        }
        Ok(PreparedDiscussionSource {
            handle: self.handle.clone(),
            domain_revision: before,
            source,
            text,
            selected_path,
            entry,
            anchors,
        })
    }
}

impl DomainValidator<SyndicDomain> for PreparedDiscussionSource {
    type Error = DiscussionSourceError;

    fn validate(&self, reader: &DomainReader<'_, SyndicDomain>) -> Result<(), Self::Error> {
        let current = authenticate(
            &SourceRead::Writer(reader),
            self.source,
            self.selected_path,
            self.entry,
        )?;
        if current != self.anchors {
            return invalid("prepared discussion source anchors changed");
        }
        Ok(())
    }
}

enum SourceRead<'a> {
    Ordinary(&'a SyndicStorage, &'a HomeStore),
    Writer(&'a DomainReader<'a, SyndicDomain>),
}

impl SourceRead<'_> {
    fn point<F: Family>(&self, key: &F::Key) -> Result<F::Value, DiscussionSourceError> {
        let value = match self {
            Self::Ordinary(storage, store) => store.read_point::<SyndicDomain, ExactCodec<F>>(
                &storage.handle,
                key,
                family_point_limit::<F>(),
            )?,
            Self::Writer(reader) => {
                reader.point::<ExactCodec<F>>(key, family_point_limit::<F>())?
            }
        };
        value.ok_or(DiscussionSourceError::Ineligible(
            "discussion source record is missing",
        ))
    }
}

fn authenticate(
    read: &SourceRead<'_>,
    source: DiscussionContextSource,
    selected: SelectedPathProof,
    entry_proof: CurrentTranscriptEntryProof,
) -> Result<SourceAnchors, DiscussionSourceError> {
    let thread = read.point::<ThreadsFamily>(&source.thread_id())?;
    let view = read.point::<TranscriptHeadsFamily>(&source.thread_id())?;
    if thread.id() != source.thread_id()
        || thread.revision() != selected.thread_revision()
        || thread.committed_tail() != selected.tail()
        || thread.selected_path_digest() != selected.digest()
        || selected.tail().is_none()
        || view.thread_id() != thread.id()
        || view.lifecycle() != ProjectionLifecycle::Current
        || view.generation() != entry_proof.generation()
        || view.committed_tail() != selected.tail()
        || view.selected_path_digest() != selected.digest()
    {
        return invalid("discussion source selected path or transcript is not current");
    }
    let entry = read.point::<TranscriptEntriesFamily>(&ThreadTranscriptKey {
        thread: source.thread_id(),
        generation: entry_proof.generation(),
        position: entry_proof.position(),
    })?;
    let item = read.point::<CanonicalItemsFamily>(&source.item_id())?;
    let projection = read.point::<ProjectionsFamily>(&source.projection_id())?;
    let turn = read.point::<TurnsFamily>(&source.turn_id())?;
    let state = read.point::<TurnStatesFamily>(&source.turn_id())?;
    if entry.thread_id() != thread.id()
        || entry.generation() != entry_proof.generation()
        || entry.position() != entry_proof.position()
        || entry.item_id() != item.id()
        || entry.item_revision() != item.revision()
        || entry.projection_id() != projection.id()
        || entry.projection_revision() != projection.revision()
        || item.id() != source.item_id()
        || item.turn_id() != source.turn_id()
        || !matches!(item.kind(), CanonicalItemKind::AssistantMessage(_))
        || projection.id() != source.projection_id()
        || projection.item_id() != item.id()
        || projection.turn_id() != source.turn_id()
        || projection.revision() != source.projection_revision()
        || turn.id() != source.turn_id()
        || state.turn_id() != source.turn_id()
        || !state.lifecycle().is_proven_terminal()
        || state.finalized_item_count() < item.ordinal().get()
    {
        return invalid("discussion source is not the exact finalized assistant projection");
    }
    let head = read.point::<ItemProjectionHeadsFamily>(&item.id())?;
    let set = read.point::<ItemProjectionSetsFamily>(&ItemProjectionSetKey {
        item: item.id(),
        generation: head.generation(),
    })?;
    if head.item_id() != item.id()
        || head.lifecycle() != ProjectionLifecycle::Current
        || head.source_item_revision() != item.revision()
        || entry.item_projection_generation() != head.generation()
        || set.item_id() != item.id()
        || set.generation() != head.generation()
        || set.source_item_revision() != item.revision()
        || projection.ordinal().get() > set.projection_count()
    {
        return invalid("discussion projection membership is not current");
    }
    let (member_id, member_revision) =
        if projection.ordinal().get() <= set.stable_projection_count() {
            let member = read.point::<StableItemProjectionsFamily>(&StableItemProjectionKey {
                item: item.id(),
                ordinal: projection.ordinal(),
            })?;
            if member.item_id() != item.id() || member.ordinal() != projection.ordinal() {
                return invalid("discussion stable projection membership disagrees");
            }
            (member.projection_id(), member.projection_revision())
        } else {
            let member = read.point::<ItemProjectionsFamily>(&ItemProjectionKey {
                item: item.id(),
                generation: set.generation(),
                ordinal: projection.ordinal(),
            })?;
            if member.item_id() != item.id()
                || member.generation() != set.generation()
                || member.ordinal() != projection.ordinal()
            {
                return invalid("discussion projection membership disagrees");
            }
            (member.projection_id(), member.projection_revision())
        };
    if member_id != projection.id() || member_revision != projection.revision() {
        return invalid("discussion projection member identity disagrees");
    }
    let range = projection
        .payload()
        .source_range()
        .ok_or(DiscussionSourceError::Ineligible(
            "discussion projection has no selectable text",
        ))?;
    let text_source = item
        .projection_source()
        .ok_or(DiscussionSourceError::Ineligible(
            "discussion source has no text",
        ))?;
    if source.range().start() < range.start()
        || source.range().end() > range.end()
        || source.range().end() > text_source.logical_utf8_bytes()
    {
        return invalid("discussion selection is outside its projection range");
    }
    Ok(SourceAnchors {
        view,
        turn,
        turn_state_revision: state.revision(),
        item_revision: item.revision(),
        head,
        set,
        range,
        text_source,
    })
}

fn invalid<T>(reason: &'static str) -> Result<T, DiscussionSourceError> {
    Err(DiscussionSourceError::Ineligible(reason))
}

#[derive(Debug)]
pub enum DiscussionSourceError {
    Read(ReadError),
    SourceRead(SyndicReadError),
    Ineligible(&'static str),
}

impl fmt::Display for DiscussionSourceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Read(source) => source.fmt(formatter),
            Self::SourceRead(source) => source.fmt(formatter),
            Self::Ineligible(reason) => formatter.write_str(reason),
        }
    }
}

impl Error for DiscussionSourceError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Read(source) => Some(source),
            Self::SourceRead(source) => Some(source),
            Self::Ineligible(_) => None,
        }
    }
}

impl From<ReadError> for DiscussionSourceError {
    fn from(source: ReadError) -> Self {
        Self::Read(source)
    }
}

impl DomainCallbackError for DiscussionSourceError {
    fn into_callback_source(self) -> Result<DomainCallbackSource, Self> {
        match self {
            Self::Read(source) | Self::SourceRead(SyndicReadError::Read(source)) => {
                Ok(DomainCallbackSource::Read(source))
            }
            other => Err(other),
        }
    }
}
