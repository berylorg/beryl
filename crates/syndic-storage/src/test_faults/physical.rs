use beryl_home_store::{HomeStore, RecordCodec};
use beryl_model::*;

use crate::{SyndicStorage, codec::*, domain::SyndicDomain};

mod family;

pub use family::PhysicalFamily;

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

/// Codec field whose retired worker-capacity tag must remain invalid.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RetiredWorkerCapacityCodecField {
    NextTurnReason,
    AcceptedRouteLeafTransition,
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
    let observation = ProviderObservationId::from_bytes([12; 16]);
    let projection = SyndicProjectionId::from_bytes([8; 16]);
    let resource = SyndicResourceId::from_bytes([9; 16]);
    let snapshot = SyndicExecutionSnapshotId::from_bytes([10; 16]);
    let binding_revision = BindingRevision::new(1).expect("one is nonzero");
    let accepted_ordinal = crate::AcceptedInputOrdinal::FIRST;
    let content_ordinal = crate::ContentChunkOrdinal::FIRST;
    let narrative_generation = crate::ProviderNarrativeGeneration::FIRST;
    let image_label = crate::ImageLabelOrdinal::FIRST;
    let activity_order =
        crate::ActivityQueryOrder::new(true, crate::SyndicTimestamp::from_unix_millis(1), item);
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
        PhysicalFamily::ImageLabelAuthorityHeads => {
            inject::<ImageLabelAuthorityHeadsFamily>(store, storage, thread, corruption)
        }
        PhysicalFamily::ThreadExecutions => {
            inject::<ThreadExecutionsFamily>(store, storage, thread, corruption)
        }
        PhysicalFamily::ThreadAttributes => {
            inject::<ThreadAttributesFamily>(store, storage, thread, corruption)
        }
        PhysicalFamily::ThreadUsage => {
            inject::<ThreadUsageFamily>(store, storage, thread, corruption)
        }
        PhysicalFamily::ThreadCatalogSummaries => {
            inject::<ThreadCatalogSummariesFamily>(store, storage, thread, corruption)
        }
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
        PhysicalFamily::ProviderNarrativeSpans => inject::<ProviderNarrativeSpansFamily>(
            store,
            storage,
            ProviderNarrativeSpanKey::new(content, narrative_generation, 0),
            corruption,
        ),
        PhysicalFamily::ProviderItemBuilds => {
            inject::<ProviderItemBuildsFamily>(store, storage, item, corruption)
        }
        PhysicalFamily::ProviderObservationBuilds => {
            inject::<ProviderObservationBuildsFamily>(store, storage, observation, corruption)
        }
        PhysicalFamily::ProviderObservationChunks => inject::<ProviderObservationChunksFamily>(
            store,
            storage,
            ProviderObservationChunkKey::new(observation, 1),
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
        PhysicalFamily::StopOperations => inject::<StopOperationsFamily>(
            store,
            storage,
            crate::StopOperationId::new(thread, crate::StopOperationNonce::from_bytes([13; 16])),
            corruption,
        ),
        PhysicalFamily::CompactionOperations => inject::<CompactionOperationsFamily>(
            store,
            storage,
            crate::CompactionOperationId::new(
                thread,
                crate::CompactionOperationNonce::from_bytes([14; 16]),
            ),
            corruption,
        ),
        PhysicalFamily::CompactionSettlementReceipts => {
            inject::<CompactionSettlementReceiptsFamily>(
                store,
                storage,
                crate::CompactionOperationId::new(
                    thread,
                    crate::CompactionOperationNonce::from_bytes([15; 16]),
                ),
                corruption,
            )
        }
        PhysicalFamily::AcceptedRouteGenerationHeads => {
            inject::<AcceptedRouteGenerationHeadsFamily>(store, storage, thread, corruption)
        }
        PhysicalFamily::AcceptedRouteLeaves => {
            inject::<AcceptedRouteLeavesFamily>(store, storage, accepted, corruption)
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
        PhysicalFamily::ActivityQueryHeads => {
            inject::<ActivityQueryHeadsFamily>(store, storage, thread, corruption)
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
        PhysicalFamily::ImageLabelOriginSpans => inject::<ImageLabelOriginSpansFamily>(
            store,
            storage,
            ImageLabelOriginSpanKey {
                thread,
                end_label: image_label,
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
        PhysicalFamily::AcceptedRouteGenerations => inject::<AcceptedRouteGenerationsFamily>(
            store,
            storage,
            ThreadRouteKey {
                thread,
                generation: crate::AcceptedRouteGeneration::FIRST,
            },
            corruption,
        ),
        PhysicalFamily::AcceptedReadySources => inject::<AcceptedReadySourcesFamily>(
            store,
            storage,
            ThreadRouteKey {
                thread,
                generation: crate::AcceptedRouteGeneration::FIRST,
            },
            corruption,
        ),
        PhysicalFamily::AcceptedNextSources => inject::<AcceptedNextSourcesFamily>(
            store,
            storage,
            ThreadRouteKey {
                thread,
                generation: crate::AcceptedRouteGeneration::FIRST,
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
        PhysicalFamily::ActivityQueryEntries => inject::<ActivityQueryEntriesFamily>(
            store,
            storage,
            ActivityQueryEntryKey {
                thread,
                work_period: crate::ActivityWorkPeriod::FIRST,
                order: activity_order,
            },
            corruption,
        ),
        PhysicalFamily::ActivityQuerySources => inject::<ActivityQuerySourcesFamily>(
            store,
            storage,
            ActivityQuerySourceKey {
                thread,
                work_period: crate::ActivityWorkPeriod::FIRST,
                source_thread: thread,
                source_turn: turn,
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

/// Installs one retired accepted-input V2 envelope to prove there is no compatibility decoder.
pub fn inject_retired_accepted_input_v2(
    store: &HomeStore,
    storage: SyndicStorage,
) -> Result<(), beryl_home_store::test_faults::PersistedCorruptionError> {
    let source = SyndicDraftId::from_bytes([0xB4; 16]);
    let replacement = SyndicDraftId::from_bytes([0xB5; 16]);
    let input = source.accepted_input_id();
    let content = crate::PreparedContent::composer(&crate::ComposerPayload::default())
        .expect("empty composer fixture is valid")
        .reference(ContentRevision::new(1).expect("one is nonzero"));
    let value = crate::AcceptedInputRecord::new(
        input,
        SyndicThreadId::from_bytes([0xB6; 16]),
        crate::AcceptedInputOrdinal::FIRST,
        crate::AcceptedInputAdmissionProof::new(
            ThreadRevision::new(1).expect("one is nonzero"),
            source,
            DraftRevision::new(1).expect("one is nonzero"),
            InputGateRevision::new(1).expect("one is nonzero"),
            replacement,
        )
        .expect("retired fixture proof is valid"),
        crate::AcceptedRouteGeneration::FIRST,
        content,
        None,
        crate::SyndicTimestamp::from_unix_millis(1),
    )
    .expect("retired fixture input is valid");
    let current = versioned_value::<AcceptedInputsFamily>(&value);
    let payload = &current[4..];
    let mut retired = Vec::with_capacity(4 + payload.len() - 48);
    retired.extend_from_slice(&2_u32.to_be_bytes());
    retired.extend_from_slice(&payload[..40]);
    retired.extend_from_slice(&payload[72..80]);
    retired.extend_from_slice(&payload[96..]);
    inject_encoded::<AcceptedInputsFamily>(store, storage, input, retired)
}

/// Returns the exact invalid-tag diagnostic produced for one retired worker-capacity codec field.
#[must_use]
pub fn retired_worker_capacity_codec_rejection(
    field: RetiredWorkerCapacityCodecField,
) -> Option<(&'static str, u8)> {
    let result = match field {
        RetiredWorkerCapacityCodecField::NextTurnReason => {
            let encoded = retired_next_turn_reason_payload();
            <AcceptedRouteGenerationsFamily as Family>::decode_value(&encoded).map(|_| ())
        }
        RetiredWorkerCapacityCodecField::AcceptedRouteLeafTransition => {
            let encoded = retired_leaf_transition_payload();
            <AcceptedRouteLeavesFamily as Family>::decode_value(&encoded).map(|_| ())
        }
    };
    match result {
        Err(CodecError::InvalidTag { kind, tag }) => Some((kind, tag)),
        Ok(()) | Err(_) => None,
    }
}

fn retired_next_turn_reason_payload() -> Vec<u8> {
    let thread = SyndicThreadId::from_bytes([0xB1; 16]);
    let generation = crate::AcceptedRouteGeneration::FIRST;
    let value = crate::AcceptedRouteGenerationRecord::new(
        thread,
        generation,
        crate::AcceptedRouteRevision::FIRST,
        crate::AcceptedRouteTarget::NextTurn(crate::NextTurnReason::ProjectionLost),
        None,
        None,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
    )
    .expect("empty retired-tag route generation is valid");
    let mut encoded = <AcceptedRouteGenerationsFamily as Family>::encode_value(&value)
        .expect("retired-tag route generation must encode");
    const REASON_TAG_OFFSET: usize = 16 + 8 + 8 + 1;
    assert_eq!(encoded[REASON_TAG_OFFSET], 5);
    encoded[REASON_TAG_OFFSET] = 4;
    encoded
}

fn retired_leaf_transition_payload() -> Vec<u8> {
    let generation = crate::AcceptedRouteGeneration::FIRST;
    let input_revision = AcceptedInputRevision::new(1).expect("one is nonzero");
    let value = crate::AcceptedRouteLeafRecord::new(
        SyndicAcceptedInputId::from_bytes([0xB2; 16]),
        SyndicThreadId::from_bytes([0xB3; 16]),
        generation,
        crate::AcceptedInputOrdinal::FIRST,
        input_revision,
        crate::AcceptedRouteLeafState::NextTurn(crate::NextTurnReason::ProjectionLost),
        crate::AcceptedInputLifecycle::Retryable,
    )
    .with_transition_proof(crate::AcceptedRouteLeafTransitionProof::new(
        InputGateRevision::new(1).expect("one is nonzero"),
        crate::AcceptedRouteHeadProof::new(generation, crate::AcceptedRouteRevision::FIRST),
        input_revision,
        crate::AcceptedRouteLeafTransitionKind::ProjectionLostExactRejection,
    ));
    let mut encoded = <AcceptedRouteLeavesFamily as Family>::encode_value(&value)
        .expect("retired-tag route leaf must encode");
    let transition_tag_offset = encoded
        .len()
        .checked_sub(2)
        .expect("transition kind precedes promotion-presence byte");
    let transition_tag = encoded
        .get_mut(transition_tag_offset)
        .expect("route leaf transition tag is present");
    assert_eq!(*transition_tag, 5);
    *transition_tag = 4;
    encoded
}

fn representative_thread(thread: SyndicThreadId, draft: SyndicDraftId) -> crate::ThreadRecord {
    crate::ThreadRecord::new(
        thread,
        crate::SelectedPathProof::new(
            None,
            ThreadRevision::new(1).expect("one is nonzero"),
            crate::empty_selected_path_digest(),
        ),
        draft,
        crate::ThreadLineageProof::new(
            None,
            None,
            crate::ThreadLineageDepth::FIRST,
            crate::root_thread_lineage_digest(thread),
        ),
        None,
    )
}

fn versioned_value<F: Family>(value: &F::Value) -> Vec<u8> {
    let payload = <ExactCodec<F> as RecordCodec<SyndicDomain>>::encode_value(value)
        .expect("representative physical fixture value must encode");
    let mut encoded = Vec::with_capacity(4 + payload.len());
    encoded.extend_from_slice(&F::RECORD_VERSION.get().to_be_bytes());
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
        PhysicalCorruption::UnsupportedRecordVersion => F::RECORD_VERSION
            .get()
            .checked_add(1)
            .expect("test record version must leave room for an unsupported successor")
            .to_be_bytes(),
        PhysicalCorruption::MalformedStoredKey | PhysicalCorruption::MalformedCodecPayload => {
            F::RECORD_VERSION.get().to_be_bytes()
        }
    };
    store.inject_persisted_corrupt_record::<SyndicDomain, ExactCodec<F>>(
        storage.handle,
        &encoded_key,
        &encoded_value,
    )
}
