//! Bounded typed fixture contributions available only with `test-faults`.

use std::{error::Error, fmt};

use beryl_home_store::{
    DomainCallbackError, DomainCallbackSource, DomainMutation, DomainReader, MutationBuildError,
    MutationBuilder, MutationContribution,
};
use beryl_model::DomainRevision;

use crate::{codec::*, domain::SyndicDomain, *};

pub(crate) mod metrics;
mod physical;

pub use metrics::{
    CurrentBindingReadMetrics, RecoveryFrontierMetrics, ValidationPageMetrics,
    current_binding_read_metrics, recovery_frontier_metrics, reset_current_binding_read_metrics,
    reset_recovery_frontier_metrics, reset_validation_page_metrics, validation_page_metrics,
};
pub use physical::{
    PhysicalCorruption, PhysicalFamily, RepresentativePhysicalCorruption,
    inject_physical_corruption, inject_representative_physical_corruption,
};

const MAX_FIXTURE_OPERATIONS: usize = 131_072;

/// Returns the exact V1 seed used to fold one item-projection fixture manifest.
#[must_use]
pub fn fixture_item_projection_digest_seed() -> [u8; 32] {
    crate::projection::item_set_digest_seed()
}

/// Folds one projection membership into an exact V1 item-projection fixture digest.
#[must_use]
pub fn fixture_advance_item_projection_digest(
    current: [u8; 32],
    projection: beryl_model::SyndicProjectionId,
    revision: beryl_model::ProjectionRevision,
) -> [u8; 32] {
    crate::projection::advance_item_set_digest(current, projection, revision)
}

/// Folds one resource membership into an exact V1 item-projection fixture digest.
#[must_use]
pub fn fixture_advance_item_projection_resource_digest(
    current: [u8; 32],
    resource: beryl_model::SyndicResourceId,
    revision: beryl_model::ProjectionRevision,
    digest: [u8; 32],
) -> [u8; 32] {
    crate::projection::advance_item_set_resource_digest(current, resource, revision, digest)
}

/// Returns the exact V1 seed used to fold one transcript fixture manifest.
#[must_use]
pub fn fixture_transcript_digest_seed() -> [u8; 32] {
    crate::projection::transcript_entry_digest_seed()
}

/// Folds one transcript entry into an exact V1 transcript fixture digest.
#[must_use]
pub fn fixture_advance_transcript_digest(
    current: [u8; 32],
    entry: &TranscriptViewEntryRecord,
) -> [u8; 32] {
    crate::projection::advance_transcript_entry_digest(
        current,
        entry.thread_id(),
        entry.generation(),
        entry.position(),
        entry.item_id(),
        entry.item_revision(),
        entry.item_projection_generation(),
        entry.projection_id(),
        entry.projection_revision(),
    )
}

/// Builds the exact initial empty live-content fixture owned by one canonical item.
#[must_use]
pub fn fixture_empty_live_content(
    owner: beryl_model::SyndicItemId,
) -> (ContentReference, ContentManifestRecord) {
    let manifest = ContentManifestRecord::live(
        crate::content::live_item_content_id(owner),
        owner,
        beryl_model::ContentRevision::new(1).expect("first live-content revision is nonzero"),
    );
    let content = manifest
        .current_reference()
        .expect("a live manifest always has a current reference");
    (content, manifest)
}

/// Builds the exact deterministic empty projection emitted at EOF for a fixture item.
#[must_use]
pub fn fixture_empty_projection(
    item: beryl_model::SyndicItemId,
    turn: beryl_model::SyndicTurnId,
) -> ProjectionRecord {
    let payload = ProjectionPayload::Empty;
    let (id, revision) = crate::projection::projection_identity(
        ProjectionFormatVersion::V1,
        item,
        0,
        ProjectionOrdinal::FIRST,
        &payload,
    );
    ProjectionRecord::new(id, revision, item, turn, ProjectionOrdinal::FIRST, payload)
}

/// Builds the exact deterministic single-paragraph projection for a UTF-8 fixture item.
#[must_use]
pub fn fixture_inline_paragraph_projection(
    item: beryl_model::SyndicItemId,
    turn: beryl_model::SyndicTurnId,
    source: &str,
) -> ProjectionRecord {
    let format = ProjectionFormatVersion::V1;
    let kind = MarkdownBlockKind::Paragraph;
    let block_start = 0;
    let payload = ProjectionPayload::inline_markdown(
        crate::projection::markdown_block_id(format, item, block_start, kind),
        kind,
        1,
        ProjectionSourceRange::new(0, source.len() as u64)
            .expect("nonempty paragraph fixture has a valid source range"),
        source,
    )
    .expect("bounded paragraph fixture is valid");
    let (id, revision) = crate::projection::projection_identity(
        format,
        item,
        block_start,
        ProjectionOrdinal::FIRST,
        &payload,
    );
    ProjectionRecord::new(id, revision, item, turn, ProjectionOrdinal::FIRST, payload)
}

/// One exact typed V1 family record to insert or replace in a fixture command.
#[derive(Clone, Debug)]
pub enum FixtureRecord {
    Thread(ThreadRecord),
    Draft(DraftRecord),
    ContentManifest(ContentManifestRecord),
    ContentChunk(ContentChunkRecord),
    ContentByteSpan(ContentByteSpanRecord),
    ContentTextSpan(ContentTextSpanRecord),
    ContentPiece(ContentPieceRecord),
    InputMarkerResolution(InputMarkerResolutionRecord),
    ContextEnvelope(ContextEnvelopeRecord),
    Turn(TurnRecord),
    TurnState(TurnStateRecord),
    InputGate(InputGateRecord),
    AcceptedInput(AcceptedInputRecord),
    SourceEvent(SourceEventRecord),
    CanonicalItem(CanonicalItemRecord),
    ItemProjectionHead(ItemProjectionHeadRecord),
    ItemProjectionSet(ItemProjectionSetRecord),
    ItemProjectionBuild(ItemProjectionBuildRecord),
    TranscriptViewHead(TranscriptViewHeadRecord),
    TranscriptBuild(TranscriptBuildRecord),
    Projection(ProjectionRecord),
    Resource(ResourceMetadataRecord),
    HistorySummary(HistorySummaryRecord),
    Binding(BindingRecord),
    ExecutionSnapshot(ExecutionSnapshotRecord),
    ActiveCasTurn(ActiveCasTurnRecord),
    DraftByThread(DraftByThreadRecord),
    ThreadParent(ThreadParentIndexRecord),
    TurnChild(TurnChildIndexRecord),
    AcceptedOrder(AcceptedOrderIndexRecord),
    AcceptedSteering(AcceptedSteeringIndexRecord),
    AcceptedNextTurn(AcceptedNextTurnIndexRecord),
    TurnItem(TurnItemIndexRecord),
    ItemSourceEvent(ItemSourceEventIndexRecord),
    CasItem(CasItemIndexRecord),
    TranscriptPathTurn(TranscriptPathTurnRecord),
    TranscriptViewEntry(TranscriptViewEntryRecord),
    StableItemProjection(StableItemProjectionIndexRecord),
    ItemProjection(ItemProjectionIndexRecord),
    ProjectionResource(ProjectionResourceIndexRecord),
    BindingHead(BindingHeadRecord),
    CasThread(CasThreadIndexRecord),
    CasThreadBinding(CasThreadBindingIndexRecord),
    CasTurn(CasTurnIndexRecord),
}

impl FixtureRecord {
    /// Returns the exact physical V1 family encoded by this fixture record.
    #[must_use]
    pub const fn family(&self) -> PhysicalFamily {
        match self {
            Self::Thread(_) => PhysicalFamily::Threads,
            Self::Draft(_) => PhysicalFamily::Drafts,
            Self::ContentManifest(_) => PhysicalFamily::ContentManifests,
            Self::ContentChunk(_) => PhysicalFamily::ContentChunks,
            Self::ContentByteSpan(_) => PhysicalFamily::ContentByteSpans,
            Self::ContentTextSpan(_) => PhysicalFamily::ContentTextSpans,
            Self::ContentPiece(_) => PhysicalFamily::ContentPieces,
            Self::InputMarkerResolution(_) => PhysicalFamily::InputMarkerResolutions,
            Self::ContextEnvelope(_) => PhysicalFamily::ContextEnvelopes,
            Self::Turn(_) => PhysicalFamily::Turns,
            Self::TurnState(_) => PhysicalFamily::TurnStates,
            Self::InputGate(_) => PhysicalFamily::InputGates,
            Self::AcceptedInput(_) => PhysicalFamily::AcceptedInputs,
            Self::SourceEvent(_) => PhysicalFamily::SourceEvents,
            Self::CanonicalItem(_) => PhysicalFamily::CanonicalItems,
            Self::ItemProjectionHead(_) => PhysicalFamily::ItemProjectionHeads,
            Self::ItemProjectionSet(_) => PhysicalFamily::ItemProjectionSets,
            Self::ItemProjectionBuild(_) => PhysicalFamily::ItemProjectionBuilds,
            Self::TranscriptViewHead(_) => PhysicalFamily::TranscriptViewHeads,
            Self::TranscriptBuild(_) => PhysicalFamily::TranscriptBuilds,
            Self::Projection(_) => PhysicalFamily::Projections,
            Self::Resource(_) => PhysicalFamily::Resources,
            Self::HistorySummary(_) => PhysicalFamily::HistorySummaries,
            Self::Binding(_) => PhysicalFamily::Bindings,
            Self::ExecutionSnapshot(_) => PhysicalFamily::ExecutionSnapshots,
            Self::ActiveCasTurn(_) => PhysicalFamily::ActiveCasTurns,
            Self::DraftByThread(_) => PhysicalFamily::DraftByThread,
            Self::ThreadParent(_) => PhysicalFamily::ThreadParent,
            Self::TurnChild(_) => PhysicalFamily::TurnChildren,
            Self::AcceptedOrder(_) => PhysicalFamily::AcceptedOrder,
            Self::AcceptedSteering(_) => PhysicalFamily::AcceptedSteering,
            Self::AcceptedNextTurn(_) => PhysicalFamily::AcceptedNextTurn,
            Self::TurnItem(_) => PhysicalFamily::TurnItems,
            Self::ItemSourceEvent(_) => PhysicalFamily::ItemSourceEvents,
            Self::CasItem(_) => PhysicalFamily::CasItem,
            Self::TranscriptPathTurn(_) => PhysicalFamily::TranscriptPathTurns,
            Self::TranscriptViewEntry(_) => PhysicalFamily::TranscriptViewEntries,
            Self::StableItemProjection(_) => PhysicalFamily::StableItemProjections,
            Self::ItemProjection(_) => PhysicalFamily::ItemProjections,
            Self::ProjectionResource(_) => PhysicalFamily::ProjectionResources,
            Self::BindingHead(_) => PhysicalFamily::BindingHeads,
            Self::CasThread(_) => PhysicalFamily::CasThread,
            Self::CasThreadBinding(_) => PhysicalFamily::CasThreadBinding,
            Self::CasTurn(_) => PhysicalFamily::CasTurn,
        }
    }
}

/// One exact typed V1 family key to remove in a fixture command.
#[derive(Clone, Debug)]
pub enum FixtureDelete {
    Thread(beryl_model::SyndicThreadId),
    Draft(beryl_model::SyndicDraftId),
    ContentManifest(beryl_model::SyndicContentId),
    ContentChunk {
        content: beryl_model::SyndicContentId,
        ordinal: ContentChunkOrdinal,
    },
    ContentByteSpan {
        content: beryl_model::SyndicContentId,
        start: u64,
    },
    ContentTextSpan {
        content: beryl_model::SyndicContentId,
        logical_start: u64,
    },
    ContentPiece {
        content: beryl_model::SyndicContentId,
        ordinal: ContentPieceOrdinal,
    },
    InputMarkerResolution {
        owner: InputMarkerOwner,
        ordinal: InputMarkerOrdinal,
    },
    ContextEnvelope(beryl_model::DiscussionContextOwnerId),
    Turn(beryl_model::SyndicTurnId),
    TurnState(beryl_model::SyndicTurnId),
    InputGate(beryl_model::SyndicThreadId),
    AcceptedInput(beryl_model::SyndicAcceptedInputId),
    SourceEvent {
        turn: beryl_model::SyndicTurnId,
        sequence: SourceEventSequence,
    },
    CanonicalItem(beryl_model::SyndicItemId),
    ItemProjectionHead(beryl_model::SyndicItemId),
    ItemProjectionSet {
        item: beryl_model::SyndicItemId,
        generation: ItemProjectionGeneration,
    },
    ItemProjectionBuild {
        item: beryl_model::SyndicItemId,
        generation: ItemProjectionGeneration,
    },
    TranscriptViewHead(beryl_model::SyndicThreadId),
    TranscriptBuild {
        thread: beryl_model::SyndicThreadId,
        generation: TranscriptGeneration,
    },
    Projection(beryl_model::SyndicProjectionId),
    Resource(beryl_model::SyndicResourceId),
    HistorySummary(beryl_model::SyndicThreadId),
    Binding {
        thread: beryl_model::SyndicThreadId,
        revision: beryl_model::BindingRevision,
    },
    ExecutionSnapshot(beryl_model::SyndicExecutionSnapshotId),
    ActiveCasTurn(beryl_model::SyndicExecutionSnapshotId),
    DraftByThread(beryl_model::SyndicThreadId),
    ThreadParent {
        parent: beryl_model::SyndicThreadId,
        child: beryl_model::SyndicThreadId,
    },
    TurnChild {
        parent: beryl_model::SyndicTurnId,
        child: beryl_model::SyndicTurnId,
    },
    AcceptedOrder {
        thread: beryl_model::SyndicThreadId,
        ordinal: AcceptedInputOrdinal,
    },
    AcceptedSteering {
        thread: beryl_model::SyndicThreadId,
        turn: beryl_model::SyndicTurnId,
        ordinal: AcceptedInputOrdinal,
    },
    AcceptedNextTurn {
        thread: beryl_model::SyndicThreadId,
        ordinal: AcceptedInputOrdinal,
    },
    TurnItem {
        turn: beryl_model::SyndicTurnId,
        ordinal: TurnItemOrdinal,
    },
    ItemSourceEvent {
        item: beryl_model::SyndicItemId,
        ordinal: ItemSourceEventOrdinal,
    },
    CasItem {
        thread: beryl_model::CasThreadId,
        turn: beryl_model::CasTurnId,
        item: beryl_model::CasItemId,
    },
    TranscriptPathTurn {
        thread: beryl_model::SyndicThreadId,
        generation: TranscriptGeneration,
        depth: TurnDepth,
    },
    TranscriptViewEntry {
        thread: beryl_model::SyndicThreadId,
        generation: TranscriptGeneration,
        position: TranscriptPosition,
    },
    StableItemProjection {
        item: beryl_model::SyndicItemId,
        ordinal: ProjectionOrdinal,
    },
    ItemProjection {
        item: beryl_model::SyndicItemId,
        generation: ItemProjectionGeneration,
        ordinal: ProjectionOrdinal,
    },
    ProjectionResource {
        projection: beryl_model::SyndicProjectionId,
        ordinal: ResourceOrdinal,
    },
    BindingHead(beryl_model::SyndicThreadId),
    CasThread(beryl_model::CasThreadId),
    CasThreadBinding {
        thread: beryl_model::CasThreadId,
        revision: beryl_model::BindingRevision,
    },
    CasTurn {
        thread: beryl_model::CasThreadId,
        turn: beryl_model::CasTurnId,
    },
}

#[derive(Clone, Debug)]
enum FixtureOperation {
    Put(Box<FixtureRecord>),
    Delete(FixtureDelete),
}

/// One bounded exact-domain batch used to seed valid or intentionally inconsistent fixtures.
#[derive(Clone, Debug, Default)]
pub struct FixtureBatch {
    operations: Vec<FixtureOperation>,
}

impl FixtureBatch {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
    pub fn put(&mut self, record: FixtureRecord) -> Result<&mut Self, FixtureBuildError> {
        self.push(FixtureOperation::Put(Box::new(record)))?;
        Ok(self)
    }
    pub fn delete(&mut self, key: FixtureDelete) -> Result<&mut Self, FixtureBuildError> {
        self.push(FixtureOperation::Delete(key))?;
        Ok(self)
    }
    fn push(&mut self, operation: FixtureOperation) -> Result<(), FixtureBuildError> {
        if self.operations.len() == MAX_FIXTURE_OPERATIONS {
            return Err(FixtureBuildError::TooManyOperations);
        }
        self.operations.push(operation);
        Ok(())
    }
}

/// Why a test-only typed fixture batch could not be built.
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum FixtureBuildError {
    #[error("Syndic fixture exceeds its fixed operation bound")]
    TooManyOperations,
}

#[derive(Debug)]
pub enum FixtureMutationError {
    Build(MutationBuildError),
}
impl fmt::Display for FixtureMutationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Build(source) => source.fmt(f),
        }
    }
}
impl Error for FixtureMutationError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Build(source) => Some(source),
        }
    }
}
impl From<MutationBuildError> for FixtureMutationError {
    fn from(source: MutationBuildError) -> Self {
        Self::Build(source)
    }
}
impl DomainCallbackError for FixtureMutationError {
    fn into_callback_source(self) -> Result<DomainCallbackSource, Self> {
        Err(self)
    }
}

impl DomainMutation<SyndicDomain> for FixtureBatch {
    type Error = FixtureMutationError;
    fn validate(&self, _: &DomainReader<'_, SyndicDomain>) -> Result<(), Self::Error> {
        Ok(())
    }
    fn contribute(
        &self,
        _: &DomainReader<'_, SyndicDomain>,
        builder: &mut MutationBuilder<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        for operation in &self.operations {
            match operation {
                FixtureOperation::Put(record) => put_record(builder, record.as_ref())?,
                FixtureOperation::Delete(key) => delete_record(builder, key)?,
            }
        }
        Ok(())
    }
}

impl SyndicStorage {
    /// Seals one bounded typed fixture batch against an exact expected domain revision.
    #[must_use]
    pub fn fixture_contribution(
        &self,
        expected_revision: DomainRevision,
        batch: FixtureBatch,
    ) -> MutationContribution {
        self.handle.contribution(expected_revision, batch)
    }

    /// Reads one immutable CAS-thread binding membership for fault-cut assertions.
    pub fn fixture_cas_thread_binding_membership(
        &self,
        store: &beryl_home_store::HomeStore,
        cas_thread: beryl_model::CasThreadId,
        revision: beryl_model::BindingRevision,
        limit: SyndicPointReadLimit,
    ) -> Result<Option<SyndicStoredRecord<CasThreadBindingIndexRecord>>, SyndicReadError> {
        self.point::<CasThreadBindingIndexFamily>(
            store,
            CasThreadBindingKey::Record(cas_thread, revision),
            limit,
        )
    }
}

fn put_record(
    builder: &mut MutationBuilder<'_, SyndicDomain>,
    record: &FixtureRecord,
) -> Result<(), FixtureMutationError> {
    match record {
        FixtureRecord::Thread(v) => builder.put::<ThreadsCodec>(&v.id(), v)?,
        FixtureRecord::Draft(v) => builder.put::<DraftsCodec>(&v.id(), v)?,
        FixtureRecord::ContentManifest(v) => builder.put::<ContentManifestsCodec>(&v.id(), v)?,
        FixtureRecord::ContentChunk(v) => builder.put::<ContentChunksCodec>(
            &ContentChunkKey {
                owner: v.content_id(),
                ordinal: v.ordinal(),
            },
            v,
        )?,
        FixtureRecord::ContentByteSpan(v) => builder.put::<ContentByteSpansCodec>(
            &ContentByteSpanKey {
                owner: v.content_id(),
                start: v.start(),
            },
            v,
        )?,
        FixtureRecord::ContentTextSpan(v) => builder.put::<ContentTextSpansCodec>(
            &ContentTextSpanKey {
                owner: v.content_id(),
                logical_start: v.logical_start(),
            },
            v,
        )?,
        FixtureRecord::ContentPiece(v) => builder.put::<ContentPiecesCodec>(
            &ContentPieceKey {
                owner: v.content_id(),
                ordinal: v.ordinal(),
            },
            v,
        )?,
        FixtureRecord::InputMarkerResolution(v) => builder.put::<InputMarkerResolutionsCodec>(
            &InputMarkerKey {
                owner: v.owner(),
                ordinal: v.ordinal(),
            },
            v,
        )?,
        FixtureRecord::ContextEnvelope(v) => {
            builder.put::<ContextEnvelopesCodec>(&ContextOwnerKey::from(v.owner()), v)?
        }
        FixtureRecord::Turn(v) => builder.put::<TurnsCodec>(&v.id(), v)?,
        FixtureRecord::TurnState(v) => builder.put::<TurnStatesCodec>(&v.turn_id(), v)?,
        FixtureRecord::InputGate(v) => builder.put::<InputGatesCodec>(&v.thread_id(), v)?,
        FixtureRecord::AcceptedInput(v) => builder.put::<AcceptedInputsCodec>(&v.id(), v)?,
        FixtureRecord::SourceEvent(v) => builder.put::<SourceEventsCodec>(
            &TurnEventKey {
                owner: v.turn_id(),
                ordinal: v.sequence(),
            },
            v,
        )?,
        FixtureRecord::CanonicalItem(v) => builder.put::<CanonicalItemsCodec>(&v.id(), v)?,
        FixtureRecord::ItemProjectionHead(v) => {
            builder.put::<ItemProjectionHeadsCodec>(&v.item_id(), v)?
        }
        FixtureRecord::ItemProjectionSet(v) => builder.put::<ItemProjectionSetsCodec>(
            &ItemProjectionSetKey {
                item: v.item_id(),
                generation: v.generation(),
            },
            v,
        )?,
        FixtureRecord::ItemProjectionBuild(v) => builder.put::<ItemProjectionBuildsCodec>(
            &ItemProjectionSetKey {
                item: v.item_id(),
                generation: v.generation(),
            },
            v,
        )?,
        FixtureRecord::TranscriptViewHead(v) => {
            builder.put::<TranscriptHeadsCodec>(&v.thread_id(), v)?
        }
        FixtureRecord::TranscriptBuild(v) => builder.put::<TranscriptBuildsCodec>(
            &ThreadTranscriptBuildKey {
                thread: v.thread_id(),
                generation: v.generation(),
            },
            v,
        )?,
        FixtureRecord::Projection(v) => builder.put::<ProjectionsCodec>(&v.id(), v)?,
        FixtureRecord::Resource(v) => builder.put::<ResourcesCodec>(&v.id(), v)?,
        FixtureRecord::HistorySummary(v) => {
            builder.put::<HistorySummariesCodec>(&v.thread_id(), v)?
        }
        FixtureRecord::Binding(v) => builder.put::<BindingsCodec>(
            &BindingKey {
                thread: v.thread_id(),
                revision: v.revision(),
            },
            v,
        )?,
        FixtureRecord::ExecutionSnapshot(v) => {
            builder.put::<ExecutionSnapshotsCodec>(&v.id(), v)?
        }
        FixtureRecord::ActiveCasTurn(v) => {
            builder.put::<ActiveCasTurnsCodec>(&v.snapshot_id(), v)?
        }
        FixtureRecord::DraftByThread(v) => builder.put::<DraftByThreadCodec>(&v.thread_id(), v)?,
        FixtureRecord::ThreadParent(v) => builder.put::<ThreadParentCodec>(
            &ThreadPairKey {
                first: v.parent_thread_id(),
                second: v.child_thread_id(),
            },
            v,
        )?,
        FixtureRecord::TurnChild(v) => builder.put::<TurnChildrenCodec>(
            &TurnPairKey {
                parent: v.parent_id(),
                child: v.child_id(),
            },
            v,
        )?,
        FixtureRecord::AcceptedOrder(v) => builder.put::<AcceptedOrderCodec>(
            &ThreadAcceptedKey {
                owner: v.thread_id(),
                ordinal: v.ordinal(),
            },
            v,
        )?,
        FixtureRecord::AcceptedSteering(v) => builder.put::<AcceptedSteeringCodec>(
            &SteeringKey {
                thread: v.thread_id,
                turn: v.turn_id,
                ordinal: v.ordinal,
            },
            v,
        )?,
        FixtureRecord::AcceptedNextTurn(v) => builder.put::<AcceptedNextCodec>(
            &ThreadAcceptedKey {
                owner: v.thread_id,
                ordinal: v.ordinal,
            },
            v,
        )?,
        FixtureRecord::TurnItem(v) => builder.put::<TurnItemsCodec>(
            &TurnItemKey {
                owner: v.turn_id(),
                ordinal: v.ordinal(),
            },
            v,
        )?,
        FixtureRecord::ItemSourceEvent(v) => builder.put::<ItemSourceEventsCodec>(
            &ItemEventKey {
                owner: v.item_id(),
                ordinal: v.ordinal(),
            },
            v,
        )?,
        FixtureRecord::CasItem(v) => builder.put::<CasItemIndexCodec>(
            &CasItemKey::Record(
                v.cas_thread_id.clone(),
                v.cas_turn_id.clone(),
                v.cas_item_id.clone(),
            ),
            v,
        )?,
        FixtureRecord::TranscriptPathTurn(v) => builder.put::<TranscriptPathTurnsCodec>(
            &ThreadTranscriptPathKey {
                thread: v.thread_id(),
                generation: v.generation(),
                depth: v.depth(),
            },
            v,
        )?,
        FixtureRecord::TranscriptViewEntry(v) => builder.put::<TranscriptEntriesCodec>(
            &ThreadTranscriptKey {
                thread: v.thread_id(),
                generation: v.generation(),
                position: v.position(),
            },
            v,
        )?,
        FixtureRecord::StableItemProjection(v) => builder.put::<StableItemProjectionsCodec>(
            &StableItemProjectionKey {
                item: v.item_id(),
                ordinal: v.ordinal(),
            },
            v,
        )?,
        FixtureRecord::ItemProjection(v) => builder.put::<ItemProjectionsCodec>(
            &ItemProjectionKey {
                item: v.item_id,
                generation: v.generation,
                ordinal: v.ordinal,
            },
            v,
        )?,
        FixtureRecord::ProjectionResource(v) => builder.put::<ProjectionResourcesCodec>(
            &ProjectionResourceKey {
                owner: v.projection_id,
                ordinal: v.ordinal,
            },
            v,
        )?,
        FixtureRecord::BindingHead(v) => builder.put::<BindingHeadsCodec>(&v.thread_id(), v)?,
        FixtureRecord::CasThread(v) => builder
            .put::<CasThreadIndexCodec>(&CasThreadKey::Record(v.cas_thread_id().clone()), v)?,
        FixtureRecord::CasThreadBinding(v) => builder.put::<CasThreadBindingIndexCodec>(
            &CasThreadBindingKey::Record(v.cas_thread_id().clone(), v.binding_revision()),
            v,
        )?,
        FixtureRecord::CasTurn(v) => builder.put::<CasTurnIndexCodec>(
            &CasTurnKey::Record(v.cas_thread_id.clone(), v.cas_turn_id.clone()),
            v,
        )?,
    }
    Ok(())
}

fn delete_record(
    builder: &mut MutationBuilder<'_, SyndicDomain>,
    key: &FixtureDelete,
) -> Result<(), FixtureMutationError> {
    match key {
        FixtureDelete::Thread(v) => builder.delete::<ThreadsCodec>(v)?,
        FixtureDelete::Draft(v) => builder.delete::<DraftsCodec>(v)?,
        FixtureDelete::ContentManifest(v) => builder.delete::<ContentManifestsCodec>(v)?,
        FixtureDelete::ContentChunk { content, ordinal } => {
            builder.delete::<ContentChunksCodec>(&ContentChunkKey {
                owner: *content,
                ordinal: *ordinal,
            })?
        }
        FixtureDelete::ContentByteSpan { content, start } => builder
            .delete::<ContentByteSpansCodec>(&ContentByteSpanKey {
                owner: *content,
                start: *start,
            })?,
        FixtureDelete::ContentTextSpan {
            content,
            logical_start,
        } => builder.delete::<ContentTextSpansCodec>(&ContentTextSpanKey {
            owner: *content,
            logical_start: *logical_start,
        })?,
        FixtureDelete::ContentPiece { content, ordinal } => {
            builder.delete::<ContentPiecesCodec>(&ContentPieceKey {
                owner: *content,
                ordinal: *ordinal,
            })?
        }
        FixtureDelete::InputMarkerResolution { owner, ordinal } => {
            builder.delete::<InputMarkerResolutionsCodec>(&InputMarkerKey {
                owner: *owner,
                ordinal: *ordinal,
            })?
        }
        FixtureDelete::ContextEnvelope(v) => {
            builder.delete::<ContextEnvelopesCodec>(&ContextOwnerKey::from(*v))?
        }
        FixtureDelete::Turn(v) => builder.delete::<TurnsCodec>(v)?,
        FixtureDelete::TurnState(v) => builder.delete::<TurnStatesCodec>(v)?,
        FixtureDelete::InputGate(v) => builder.delete::<InputGatesCodec>(v)?,
        FixtureDelete::AcceptedInput(v) => builder.delete::<AcceptedInputsCodec>(v)?,
        FixtureDelete::SourceEvent { turn, sequence } => {
            builder.delete::<SourceEventsCodec>(&TurnEventKey {
                owner: *turn,
                ordinal: *sequence,
            })?
        }
        FixtureDelete::CanonicalItem(v) => builder.delete::<CanonicalItemsCodec>(v)?,
        FixtureDelete::ItemProjectionHead(v) => builder.delete::<ItemProjectionHeadsCodec>(v)?,
        FixtureDelete::ItemProjectionSet { item, generation } => builder
            .delete::<ItemProjectionSetsCodec>(&ItemProjectionSetKey {
                item: *item,
                generation: *generation,
            })?,
        FixtureDelete::ItemProjectionBuild { item, generation } => {
            builder.delete::<ItemProjectionBuildsCodec>(&ItemProjectionSetKey {
                item: *item,
                generation: *generation,
            })?
        }
        FixtureDelete::TranscriptViewHead(v) => builder.delete::<TranscriptHeadsCodec>(v)?,
        FixtureDelete::TranscriptBuild { thread, generation } => builder
            .delete::<TranscriptBuildsCodec>(&ThreadTranscriptBuildKey {
                thread: *thread,
                generation: *generation,
            })?,
        FixtureDelete::Projection(v) => builder.delete::<ProjectionsCodec>(v)?,
        FixtureDelete::Resource(v) => builder.delete::<ResourcesCodec>(v)?,
        FixtureDelete::HistorySummary(v) => builder.delete::<HistorySummariesCodec>(v)?,
        FixtureDelete::Binding { thread, revision } => {
            builder.delete::<BindingsCodec>(&BindingKey {
                thread: *thread,
                revision: *revision,
            })?
        }
        FixtureDelete::ExecutionSnapshot(v) => builder.delete::<ExecutionSnapshotsCodec>(v)?,
        FixtureDelete::ActiveCasTurn(v) => builder.delete::<ActiveCasTurnsCodec>(v)?,
        FixtureDelete::DraftByThread(v) => builder.delete::<DraftByThreadCodec>(v)?,
        FixtureDelete::ThreadParent { parent, child } => {
            builder.delete::<ThreadParentCodec>(&ThreadPairKey {
                first: *parent,
                second: *child,
            })?
        }
        FixtureDelete::TurnChild { parent, child } => {
            builder.delete::<TurnChildrenCodec>(&TurnPairKey {
                parent: *parent,
                child: *child,
            })?
        }
        FixtureDelete::AcceptedOrder { thread, ordinal } => {
            builder.delete::<AcceptedOrderCodec>(&ThreadAcceptedKey {
                owner: *thread,
                ordinal: *ordinal,
            })?
        }
        FixtureDelete::AcceptedSteering {
            thread,
            turn,
            ordinal,
        } => builder.delete::<AcceptedSteeringCodec>(&SteeringKey {
            thread: *thread,
            turn: *turn,
            ordinal: *ordinal,
        })?,
        FixtureDelete::AcceptedNextTurn { thread, ordinal } => builder
            .delete::<AcceptedNextCodec>(&ThreadAcceptedKey {
                owner: *thread,
                ordinal: *ordinal,
            })?,
        FixtureDelete::TurnItem { turn, ordinal } => {
            builder.delete::<TurnItemsCodec>(&TurnItemKey {
                owner: *turn,
                ordinal: *ordinal,
            })?
        }
        FixtureDelete::ItemSourceEvent { item, ordinal } => builder
            .delete::<ItemSourceEventsCodec>(&ItemEventKey {
                owner: *item,
                ordinal: *ordinal,
            })?,
        FixtureDelete::CasItem { thread, turn, item } => builder.delete::<CasItemIndexCodec>(
            &CasItemKey::Record(thread.clone(), turn.clone(), item.clone()),
        )?,
        FixtureDelete::TranscriptPathTurn {
            thread,
            generation,
            depth,
        } => builder.delete::<TranscriptPathTurnsCodec>(&ThreadTranscriptPathKey {
            thread: *thread,
            generation: *generation,
            depth: *depth,
        })?,
        FixtureDelete::TranscriptViewEntry {
            thread,
            generation,
            position,
        } => builder.delete::<TranscriptEntriesCodec>(&ThreadTranscriptKey {
            thread: *thread,
            generation: *generation,
            position: *position,
        })?,
        FixtureDelete::StableItemProjection { item, ordinal } => {
            builder.delete::<StableItemProjectionsCodec>(&StableItemProjectionKey {
                item: *item,
                ordinal: *ordinal,
            })?
        }
        FixtureDelete::ItemProjection {
            item,
            generation,
            ordinal,
        } => builder.delete::<ItemProjectionsCodec>(&ItemProjectionKey {
            item: *item,
            generation: *generation,
            ordinal: *ordinal,
        })?,
        FixtureDelete::ProjectionResource {
            projection,
            ordinal,
        } => builder.delete::<ProjectionResourcesCodec>(&ProjectionResourceKey {
            owner: *projection,
            ordinal: *ordinal,
        })?,
        FixtureDelete::BindingHead(v) => builder.delete::<BindingHeadsCodec>(v)?,
        FixtureDelete::CasThread(v) => {
            builder.delete::<CasThreadIndexCodec>(&CasThreadKey::Record(v.clone()))?
        }
        FixtureDelete::CasThreadBinding { thread, revision } => builder
            .delete::<CasThreadBindingIndexCodec>(
            &CasThreadBindingKey::Record(thread.clone(), *revision),
        )?,
        FixtureDelete::CasTurn { thread, turn } => builder
            .delete::<CasTurnIndexCodec>(&CasTurnKey::Record(thread.clone(), turn.clone()))?,
    }
    Ok(())
}
