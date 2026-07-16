use beryl_home_store::{HomeStore, RecordCodec};
use beryl_model::*;

use crate::{SyndicStorage, codec::*, domain::SyndicDomain};

/// One exact Syndic V1 family selected for a bounded physical corruption fixture.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PhysicalFamily {
    Threads,
    Drafts,
    ContentManifests,
    ContentChunks,
    ContentByteSpans,
    ContentTextSpans,
    ContentPieces,
    InputMarkerResolutions,
    ContextEnvelopes,
    Turns,
    TurnStates,
    InputGates,
    AcceptedInputs,
    SourceEvents,
    CanonicalItems,
    ItemProjectionHeads,
    ItemProjectionSets,
    ItemProjectionBuilds,
    TranscriptViewHeads,
    TranscriptBuilds,
    Projections,
    Resources,
    HistorySummaries,
    Bindings,
    ExecutionSnapshots,
    ActiveCasTurns,
    DraftByThread,
    ThreadParent,
    TurnChildren,
    AcceptedOrder,
    AcceptedSteering,
    AcceptedNextTurn,
    TurnItems,
    ItemSourceEvents,
    CasItem,
    TranscriptPathTurns,
    TranscriptViewEntries,
    StableItemProjections,
    ItemProjections,
    ProjectionResources,
    BindingHeads,
    CasThread,
    CasThreadBinding,
    CasTurn,
}

impl PhysicalFamily {
    pub const ALL: [Self; 44] = [
        Self::Threads,
        Self::Drafts,
        Self::ContentManifests,
        Self::ContentChunks,
        Self::ContentByteSpans,
        Self::ContentTextSpans,
        Self::ContentPieces,
        Self::InputMarkerResolutions,
        Self::ContextEnvelopes,
        Self::Turns,
        Self::TurnStates,
        Self::InputGates,
        Self::AcceptedInputs,
        Self::SourceEvents,
        Self::CanonicalItems,
        Self::ItemProjectionHeads,
        Self::ItemProjectionSets,
        Self::ItemProjectionBuilds,
        Self::TranscriptViewHeads,
        Self::TranscriptBuilds,
        Self::Projections,
        Self::Resources,
        Self::HistorySummaries,
        Self::Bindings,
        Self::ExecutionSnapshots,
        Self::ActiveCasTurns,
        Self::DraftByThread,
        Self::ThreadParent,
        Self::TurnChildren,
        Self::AcceptedOrder,
        Self::AcceptedSteering,
        Self::AcceptedNextTurn,
        Self::TurnItems,
        Self::ItemSourceEvents,
        Self::CasItem,
        Self::TranscriptPathTurns,
        Self::TranscriptViewEntries,
        Self::StableItemProjections,
        Self::ItemProjections,
        Self::ProjectionResources,
        Self::BindingHeads,
        Self::CasThread,
        Self::CasThreadBinding,
        Self::CasTurn,
    ];

    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Threads => "threads",
            Self::Drafts => "drafts",
            Self::ContentManifests => "content-manifests",
            Self::ContentChunks => "content-chunks",
            Self::ContentByteSpans => "content-byte-spans",
            Self::ContentTextSpans => "content-text-spans",
            Self::ContentPieces => "content-pieces",
            Self::InputMarkerResolutions => "input-marker-resolutions",
            Self::ContextEnvelopes => "context-envelopes",
            Self::Turns => "turns",
            Self::TurnStates => "turn-states",
            Self::InputGates => "input-gates",
            Self::AcceptedInputs => "accepted-inputs",
            Self::SourceEvents => "source-events",
            Self::CanonicalItems => "canonical-items",
            Self::ItemProjectionHeads => "item-projection-heads",
            Self::ItemProjectionSets => "item-projection-sets",
            Self::ItemProjectionBuilds => "item-projection-builds",
            Self::TranscriptViewHeads => "transcript-view-heads",
            Self::TranscriptBuilds => "transcript-builds",
            Self::Projections => "projections",
            Self::Resources => "resources",
            Self::HistorySummaries => "history-summaries",
            Self::Bindings => "bindings",
            Self::ExecutionSnapshots => "execution-snapshots",
            Self::ActiveCasTurns => "active-cas-turns",
            Self::DraftByThread => "draft-by-thread",
            Self::ThreadParent => "thread-parent-index",
            Self::TurnChildren => "turn-children",
            Self::AcceptedOrder => "accepted-order",
            Self::AcceptedSteering => "accepted-steering",
            Self::AcceptedNextTurn => "accepted-next-turn",
            Self::TurnItems => "turn-items",
            Self::ItemSourceEvents => "item-source-events",
            Self::CasItem => "cas-item-index",
            Self::TranscriptPathTurns => "transcript-path-turns",
            Self::TranscriptViewEntries => "transcript-view-entries",
            Self::StableItemProjections => "stable-item-projections",
            Self::ItemProjections => "item-projections",
            Self::ProjectionResources => "projection-resources",
            Self::BindingHeads => "binding-heads",
            Self::CasThread => "cas-thread-index",
            Self::CasThreadBinding => "cas-thread-bindings",
            Self::CasTurn => "cas-turn-index",
        }
    }
}

/// Exact bounded codec rejection shape applied to one selected family.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PhysicalCorruption {
    UnsupportedRecordVersion,
    MalformedStoredKey,
    MalformedCodecPayload,
}

/// Representative strict-decoder rejection beyond truncation and version/key failures.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RepresentativePhysicalCorruption {
    UnknownTag,
    TrailingBytes,
    NoncanonicalOption,
}

/// Installs one bounded exact-codec-rejected physical envelope through the home-store seam.
pub fn inject_physical_corruption(
    store: &HomeStore,
    storage: SyndicStorage,
    family: PhysicalFamily,
    corruption: PhysicalCorruption,
) -> Result<(), beryl_home_store::test_faults::PersistedCorruptionError> {
    let thread = SyndicThreadId::from_bytes([1; 16]);
    let thread_two = SyndicThreadId::from_bytes([2; 16]);
    let draft = SyndicDraftId::from_bytes([3; 16]);
    let content = SyndicContentId::from_bytes([11; 16]);
    let turn = SyndicTurnId::from_bytes([4; 16]);
    let turn_two = SyndicTurnId::from_bytes([5; 16]);
    let accepted = SyndicAcceptedInputId::from_bytes([6; 16]);
    let item = SyndicItemId::from_bytes([7; 16]);
    let projection = SyndicProjectionId::from_bytes([8; 16]);
    let resource = SyndicResourceId::from_bytes([9; 16]);
    let snapshot = SyndicExecutionSnapshotId::from_bytes([10; 16]);
    let binding_revision = BindingRevision::new(1).expect("one is nonzero");
    let accepted_ordinal = crate::AcceptedInputOrdinal::FIRST;
    let content_ordinal = crate::ContentChunkOrdinal::FIRST;
    let marker_ordinal = crate::InputMarkerOrdinal::FIRST;
    let source_sequence = crate::SourceEventSequence::FIRST;
    let item_ordinal = crate::TurnItemOrdinal::FIRST;
    let item_event_ordinal = crate::ItemSourceEventOrdinal::FIRST;
    let transcript_generation = crate::TranscriptGeneration::FIRST;
    let transcript_position = crate::TranscriptPosition::FIRST;
    let item_projection_generation = crate::ItemProjectionGeneration::FIRST;
    let projection_ordinal = crate::ProjectionOrdinal::FIRST;
    let resource_ordinal = crate::ResourceOrdinal::FIRST;
    let cas_thread = CasThreadId::new("physical-thread").expect("fixture id is valid");
    let cas_turn = CasTurnId::new("physical-turn").expect("fixture id is valid");
    let cas_item = CasItemId::new("physical-item").expect("fixture id is valid");

    match family {
        PhysicalFamily::Threads => inject::<ThreadsFamily>(store, storage, thread, corruption),
        PhysicalFamily::Drafts => inject::<DraftsFamily>(store, storage, draft, corruption),
        PhysicalFamily::ContentManifests => {
            inject::<ContentManifestsFamily>(store, storage, content, corruption)
        }
        PhysicalFamily::ContentChunks => inject::<ContentChunksFamily>(
            store,
            storage,
            ContentChunkKey {
                owner: content,
                ordinal: content_ordinal,
            },
            corruption,
        ),
        PhysicalFamily::ContentByteSpans => inject::<ContentByteSpansFamily>(
            store,
            storage,
            ContentByteSpanKey {
                owner: content,
                start: 0,
            },
            corruption,
        ),
        PhysicalFamily::ContentTextSpans => inject::<ContentTextSpansFamily>(
            store,
            storage,
            ContentTextSpanKey {
                owner: content,
                logical_start: 0,
            },
            corruption,
        ),
        PhysicalFamily::ContentPieces => inject::<ContentPiecesFamily>(
            store,
            storage,
            ContentPieceKey {
                owner: content,
                ordinal: crate::ContentPieceOrdinal::FIRST,
            },
            corruption,
        ),
        PhysicalFamily::InputMarkerResolutions => inject::<InputMarkerResolutionsFamily>(
            store,
            storage,
            InputMarkerKey {
                owner: crate::InputMarkerOwner::AcceptedInput(accepted),
                ordinal: marker_ordinal,
            },
            corruption,
        ),
        PhysicalFamily::ContextEnvelopes => inject::<ContextEnvelopesFamily>(
            store,
            storage,
            ContextOwnerKey::Draft(draft),
            corruption,
        ),
        PhysicalFamily::Turns => inject::<TurnsFamily>(store, storage, turn, corruption),
        PhysicalFamily::TurnStates => inject::<TurnStatesFamily>(store, storage, turn, corruption),
        PhysicalFamily::InputGates => {
            inject::<InputGatesFamily>(store, storage, thread, corruption)
        }
        PhysicalFamily::AcceptedInputs => {
            inject::<AcceptedInputsFamily>(store, storage, accepted, corruption)
        }
        PhysicalFamily::SourceEvents => inject::<SourceEventsFamily>(
            store,
            storage,
            TurnEventKey {
                owner: turn,
                ordinal: source_sequence,
            },
            corruption,
        ),
        PhysicalFamily::CanonicalItems => {
            inject::<CanonicalItemsFamily>(store, storage, item, corruption)
        }
        PhysicalFamily::ItemProjectionHeads => {
            inject::<ItemProjectionHeadsFamily>(store, storage, item, corruption)
        }
        PhysicalFamily::ItemProjectionSets => inject::<ItemProjectionSetsFamily>(
            store,
            storage,
            ItemProjectionSetKey {
                item,
                generation: item_projection_generation,
            },
            corruption,
        ),
        PhysicalFamily::ItemProjectionBuilds => inject::<ItemProjectionBuildsFamily>(
            store,
            storage,
            ItemProjectionSetKey {
                item,
                generation: item_projection_generation,
            },
            corruption,
        ),
        PhysicalFamily::TranscriptViewHeads => {
            inject::<TranscriptHeadsFamily>(store, storage, thread, corruption)
        }
        PhysicalFamily::TranscriptBuilds => inject::<TranscriptBuildsFamily>(
            store,
            storage,
            ThreadTranscriptBuildKey {
                thread,
                generation: transcript_generation,
            },
            corruption,
        ),
        PhysicalFamily::Projections => {
            inject::<ProjectionsFamily>(store, storage, projection, corruption)
        }
        PhysicalFamily::Resources => {
            inject::<ResourcesFamily>(store, storage, resource, corruption)
        }
        PhysicalFamily::HistorySummaries => {
            inject::<HistorySummariesFamily>(store, storage, thread, corruption)
        }
        PhysicalFamily::Bindings => inject::<BindingsFamily>(
            store,
            storage,
            BindingKey {
                thread,
                revision: binding_revision,
            },
            corruption,
        ),
        PhysicalFamily::ExecutionSnapshots => {
            inject::<ExecutionSnapshotsFamily>(store, storage, snapshot, corruption)
        }
        PhysicalFamily::ActiveCasTurns => {
            inject::<ActiveCasTurnsFamily>(store, storage, snapshot, corruption)
        }
        PhysicalFamily::DraftByThread => {
            inject::<DraftByThreadFamily>(store, storage, thread, corruption)
        }
        PhysicalFamily::ThreadParent => inject::<ThreadParentFamily>(
            store,
            storage,
            ThreadPairKey {
                first: thread,
                second: thread_two,
            },
            corruption,
        ),
        PhysicalFamily::TurnChildren => inject::<TurnChildrenFamily>(
            store,
            storage,
            TurnPairKey {
                parent: turn,
                child: turn_two,
            },
            corruption,
        ),
        PhysicalFamily::AcceptedOrder => inject::<AcceptedOrderFamily>(
            store,
            storage,
            ThreadAcceptedKey {
                owner: thread,
                ordinal: accepted_ordinal,
            },
            corruption,
        ),
        PhysicalFamily::AcceptedSteering => inject::<AcceptedSteeringFamily>(
            store,
            storage,
            SteeringKey {
                thread,
                turn,
                ordinal: accepted_ordinal,
            },
            corruption,
        ),
        PhysicalFamily::AcceptedNextTurn => inject::<AcceptedNextFamily>(
            store,
            storage,
            ThreadAcceptedKey {
                owner: thread,
                ordinal: accepted_ordinal,
            },
            corruption,
        ),
        PhysicalFamily::TurnItems => inject::<TurnItemsFamily>(
            store,
            storage,
            TurnItemKey {
                owner: turn,
                ordinal: item_ordinal,
            },
            corruption,
        ),
        PhysicalFamily::ItemSourceEvents => inject::<ItemSourceEventsFamily>(
            store,
            storage,
            ItemEventKey {
                owner: item,
                ordinal: item_event_ordinal,
            },
            corruption,
        ),
        PhysicalFamily::CasItem => inject::<CasItemIndexFamily>(
            store,
            storage,
            CasItemKey::Record(cas_thread.clone(), cas_turn.clone(), cas_item),
            corruption,
        ),
        PhysicalFamily::TranscriptPathTurns => inject::<TranscriptPathTurnsFamily>(
            store,
            storage,
            ThreadTranscriptPathKey {
                thread,
                generation: transcript_generation,
                depth: crate::TurnDepth::FIRST,
            },
            corruption,
        ),
        PhysicalFamily::TranscriptViewEntries => inject::<TranscriptEntriesFamily>(
            store,
            storage,
            ThreadTranscriptKey {
                thread,
                generation: transcript_generation,
                position: transcript_position,
            },
            corruption,
        ),
        PhysicalFamily::StableItemProjections => inject::<StableItemProjectionsFamily>(
            store,
            storage,
            StableItemProjectionKey {
                item,
                ordinal: projection_ordinal,
            },
            corruption,
        ),
        PhysicalFamily::ItemProjections => inject::<ItemProjectionsFamily>(
            store,
            storage,
            ItemProjectionKey {
                item,
                generation: item_projection_generation,
                ordinal: projection_ordinal,
            },
            corruption,
        ),
        PhysicalFamily::ProjectionResources => inject::<ProjectionResourcesFamily>(
            store,
            storage,
            ProjectionResourceKey {
                owner: projection,
                ordinal: resource_ordinal,
            },
            corruption,
        ),
        PhysicalFamily::BindingHeads => {
            inject::<BindingHeadsFamily>(store, storage, thread, corruption)
        }
        PhysicalFamily::CasThread => inject::<CasThreadIndexFamily>(
            store,
            storage,
            CasThreadKey::Record(cas_thread),
            corruption,
        ),
        PhysicalFamily::CasThreadBinding => inject::<CasThreadBindingIndexFamily>(
            store,
            storage,
            CasThreadBindingKey::Record(cas_thread.clone(), binding_revision),
            corruption,
        ),
        PhysicalFamily::CasTurn => inject::<CasTurnIndexFamily>(
            store,
            storage,
            CasTurnKey::Record(cas_thread, cas_turn),
            corruption,
        ),
    }
}

/// Installs one representative strict-decoder failure through the persisted-corruption seam.
pub fn inject_representative_physical_corruption(
    store: &HomeStore,
    storage: SyndicStorage,
    corruption: RepresentativePhysicalCorruption,
) -> Result<(), beryl_home_store::test_faults::PersistedCorruptionError> {
    let thread = SyndicThreadId::from_bytes([0xA1; 16]);
    let draft = SyndicDraftId::from_bytes([0xA2; 16]);
    match corruption {
        RepresentativePhysicalCorruption::UnknownTag => {
            let turn = SyndicTurnId::from_bytes([0xA3; 16]);
            let value = crate::TurnStateRecord::new(
                turn,
                crate::TurnStateRevision::FIRST,
                crate::TurnLifecycle::Incomplete,
                0,
                0,
                Some(crate::TurnEndStatus::incomplete(
                    crate::TurnIncompleteReason::ItemAuditFailed,
                )),
                crate::SyndicTimestamp::from_unix_millis(1),
            )
            .expect("representative incomplete turn state is valid");
            let mut encoded = versioned_value::<TurnStatesFamily>(&value);
            encoded[28] = u8::MAX;
            inject_encoded::<TurnStatesFamily>(store, storage, turn, encoded)
        }
        RepresentativePhysicalCorruption::TrailingBytes => {
            let value = representative_thread(thread, draft);
            let mut encoded = versioned_value::<ThreadsFamily>(&value);
            encoded.push(0);
            inject_encoded::<ThreadsFamily>(store, storage, thread, encoded)
        }
        RepresentativePhysicalCorruption::NoncanonicalOption => {
            let value = representative_thread(thread, draft);
            let mut encoded = versioned_value::<ThreadsFamily>(&value);
            encoded[28] = 2;
            inject_encoded::<ThreadsFamily>(store, storage, thread, encoded)
        }
    }
}

fn representative_thread(thread: SyndicThreadId, draft: SyndicDraftId) -> crate::ThreadRecord {
    crate::ThreadRecord::new(
        thread,
        ThreadRevision::new(1).expect("one is nonzero"),
        None,
        draft,
        None,
        None,
        crate::empty_selected_path_digest(),
    )
}

fn versioned_value<F: Family>(value: &F::Value) -> Vec<u8> {
    let payload = <ExactCodec<F> as RecordCodec<SyndicDomain>>::encode_value(value)
        .expect("representative physical fixture value must encode");
    let mut encoded = Vec::with_capacity(4 + payload.len());
    encoded.extend_from_slice(&1_u32.to_be_bytes());
    encoded.extend_from_slice(&payload);
    encoded
}

fn inject_encoded<F: Family>(
    store: &HomeStore,
    storage: SyndicStorage,
    key: F::Key,
    encoded_value: Vec<u8>,
) -> Result<(), beryl_home_store::test_faults::PersistedCorruptionError> {
    let encoded_key = <ExactCodec<F> as RecordCodec<SyndicDomain>>::encode_key(&key)
        .expect("representative physical fixture key must encode");
    store.inject_persisted_corrupt_record::<SyndicDomain, ExactCodec<F>>(
        storage.handle,
        &encoded_key,
        &encoded_value,
    )
}

fn inject<F: Family>(
    store: &HomeStore,
    storage: SyndicStorage,
    key: F::Key,
    corruption: PhysicalCorruption,
) -> Result<(), beryl_home_store::test_faults::PersistedCorruptionError> {
    let encoded_key = match corruption {
        PhysicalCorruption::MalformedStoredKey => vec![0xFF],
        PhysicalCorruption::UnsupportedRecordVersion
        | PhysicalCorruption::MalformedCodecPayload => {
            <ExactCodec<F> as RecordCodec<SyndicDomain>>::encode_key(&key)
                .expect("bounded physical fixture key must encode")
        }
    };
    let encoded_value = match corruption {
        PhysicalCorruption::UnsupportedRecordVersion => 2_u32.to_be_bytes(),
        PhysicalCorruption::MalformedStoredKey | PhysicalCorruption::MalformedCodecPayload => {
            1_u32.to_be_bytes()
        }
    };
    store.inject_persisted_corrupt_record::<SyndicDomain, ExactCodec<F>>(
        storage.handle,
        &encoded_key,
        &encoded_value,
    )
}
