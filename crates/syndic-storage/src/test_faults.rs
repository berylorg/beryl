//! Bounded typed fixture contributions available only with `test-faults`.

use crate::{
    codec::{ActivityQueryEntryKey, activity_entry_stored_bytes},
    domain::SyndicDomain,
    *,
};

mod content_text;
mod draft_composer;
mod draft_edit_history;
mod draft_piece_candidate_drift;
mod draft_piece_corruption;
mod draft_piece_current_drift;
mod draft_piece_staging;
mod fixture_command;
mod fixture_delete;
mod fixture_put;
pub(crate) mod metrics;
mod physical;
mod provider;
mod provider_observation;
mod schema_history;

pub use crate::content::composer_v1::{
    ComposerV1AtomWriter, ComposerV1FoldError, ComposerV1RecordSink,
};
pub use crate::content::{
    ComposerV1FoldOutcome, ComposerV1Plan, fold_composer_v1, plan_composer_v1,
};
pub(crate) use content_text::ContentTextReadResidencyLease;
pub use content_text::{
    ContentTextReadResidency, ContentTextReadResidencySnapshot, ContentTextReadResidencyTracker,
};
pub use draft_composer::{
    DraftComposerBuildCorruption, DraftComposerOutputCorruption,
    delete_draft_composer_origin_build, delete_draft_composer_source,
    draft_composer_build_truncation_is_rejected, draft_composer_full_carry_remaining_bytes,
    draft_composer_mapping_truncation_is_rejected, draft_composer_provisional_output,
    draft_composer_terminal_build_encoded_size, draft_composer_terminal_build_has_maximal_shape,
    inject_draft_composer_build_corruption, inject_draft_composer_chunk_corruption,
    inject_draft_composer_manifest_corruption, inject_draft_composer_mapping_corruption,
    inject_draft_composer_output_corruption, inject_draft_composer_prepared_chunk,
};
pub use draft_edit_history::{
    DraftCandidatePublicationFault, DraftEditHistoryRecordDeletion,
    alternative_ordinary_draft_edit_history, delete_draft_edit_history_frontier,
    delete_draft_edit_history_record, draft_edit_history_accounting_corruption,
    draft_edit_history_availability_corruption, draft_edit_history_first_transition_gap,
    draft_edit_history_no_head_gap, draft_edit_history_overflow_errors,
    draft_edit_history_root_exists, draft_edit_history_stored_charge_components,
    draft_edit_history_transition_exists, draft_edit_history_wrong_head_root,
    inject_draft_candidate_publication_fault, inject_draft_edit_history_frontier_digest_corruption,
    occupy_canonical_empty_draft_edit_history, publish_draft_edit_history_pair,
    replace_draft_edit_history_frontier, replace_draft_edit_history_frontier_and_session,
    replace_draft_edit_history_transition,
};
pub use draft_piece_candidate_drift::arm_draft_piece_candidate_read_fault;
pub(crate) use draft_piece_candidate_drift::run_draft_piece_candidate_read_fault;
pub use draft_piece_corruption::{
    DraftEditorCandidateOpenReceiptCorruption, DraftPieceBuildCorruption,
    DraftPieceCandidateRootCollision, DraftPieceDescendantCorruption, DraftPieceDescendantTarget,
    DraftPieceFragmentCorruption, DraftPieceImmutableDeletion, DraftPieceProgressReceiptCorruption,
    delete_draft_piece_build_progress_receipt, delete_draft_piece_immutable_record,
    delete_draft_piece_terminal_build, draft_piece_fragment_is_stored_exactly,
    draft_piece_fragment_zero_ordinal_codec_rejections, draft_piece_position_record_count,
    inject_draft_editor_candidate_open_receipt_corruption,
    inject_draft_editor_candidate_session_published_beyond_newest,
    inject_draft_piece_build_corruption, inject_draft_piece_candidate_root_collision,
    inject_draft_piece_coordinated_stage_target_replacement,
    inject_draft_piece_custody_endpoint_corruption, inject_draft_piece_descendant_corruption,
    inject_draft_piece_fragment_ahead, inject_draft_piece_fragment_corruption,
    inject_draft_piece_occupied_stage_target, inject_draft_piece_progress_receipt_corruption,
    inject_draft_piece_session_generation_inflation,
    inject_draft_piece_settlement_closure_corruption,
};
pub use draft_piece_current_drift::arm_draft_piece_current_read_fault;
pub(crate) use draft_piece_current_drift::run_draft_piece_current_read_fault;
pub use draft_piece_staging::{
    delete_draft_mutation_staging_head, delete_draft_mutation_staging_page,
    delete_draft_mutation_staging_receipt, draft_mutation_staging_batch_target,
    draft_mutation_staging_batch_target_records, draft_mutation_staging_locally_exact_source_head,
    inject_draft_mutation_staging_batch_prefix, inject_draft_mutation_staging_head_ahead,
    inject_draft_mutation_staging_head_digest_corruption, inject_draft_mutation_staging_head_fork,
    inject_draft_mutation_staging_occupied_page,
    inject_draft_mutation_staging_page_ceiling_corruption,
    inject_draft_mutation_staging_page_digest_corruption,
    inject_draft_mutation_staging_receipt_digest_corruption,
    inject_draft_mutation_terminal_same_operation_custody,
};
pub use fixture_command::{FixtureBatch, FixtureBuildError, FixtureMutationError};
pub use metrics::{
    CurrentBindingReadMetrics, DeliveringSteeringReadMetrics, ReadySteeringReadMetrics,
    RecoveryResidencyMetrics, ValidationPageMetrics, current_binding_read_metrics,
    delivering_steering_read_metrics, ready_steering_read_metrics, recovery_residency_metrics,
    reset_current_binding_read_metrics, reset_delivering_steering_read_metrics,
    reset_ready_steering_read_metrics, reset_recovery_residency_metrics,
    reset_syndic_point_read_count, reset_validation_page_metrics, syndic_point_read_count,
    validation_page_metrics,
};
pub use physical::{
    PhysicalCorruption, PhysicalFamily, RepresentativePhysicalCorruption,
    RetiredWorkerCapacityCodecField, inject_physical_corruption,
    inject_representative_physical_corruption, inject_retired_accepted_input_v2,
    retired_worker_capacity_codec_rejection,
};
pub use provider::{
    PersistedProviderNarrativeCorruption, PersistedProviderNarrativeCorruptionError,
    ProviderFixtureCodecError, ProviderFixtureCorruption, ProviderFixtureFamily,
    ProviderFixtureRecord, decode_corrupted_provider_fixture, encoded_provider_fixture_value_bytes,
    roundtrip_provider_fixture,
};
pub use provider_observation::{ProviderObservationCorruption, ProviderObservationCorruptionError};
pub use schema_history::{
    AwaitingTerminalCodecTags, AwaitingTerminalPredecessorFamily, awaiting_terminal_codec_tags,
    inject_awaiting_terminal_predecessor,
};

pub fn syndic_v5_family_names() -> Vec<&'static str> {
    crate::domain::v5_family_names().collect()
}

pub fn roundtrip_draft_historical_root_adoption(
    value: &DraftHistoricalRootAdoptionV1,
) -> Option<DraftHistoricalRootAdoptionV1> {
    use beryl_home_store::RecordCodec;

    let bytes =
        <DraftHistoricalRootAdoptionsCodec as RecordCodec<SyndicDomain>>::encode_value(value)
            .ok()?;
    <DraftHistoricalRootAdoptionsCodec as RecordCodec<SyndicDomain>>::decode_value(&bytes).ok()
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DraftPieceImmutableSnapshot {
    pub root_records: u64,
    pub node_records: u64,
    pub leaf_records: u64,
    pub canonical_bytes: Vec<u8>,
}

pub fn draft_piece_immutable_snapshot(
    store: &beryl_home_store::HomeStore,
    storage: SyndicStorage,
    reference: DraftPieceRootReferenceV1,
) -> Option<DraftPieceImmutableSnapshot> {
    use beryl_home_store::RecordCodec;

    let root = storage
        .point::<DraftPieceRootsFamily>(store, reference.key(), point_limit())
        .ok()??;
    let mut snapshot = DraftPieceImmutableSnapshot {
        root_records: 1,
        node_records: 0,
        leaf_records: 0,
        canonical_bytes: <DraftPieceRootsCodec as RecordCodec<SyndicDomain>>::encode_value(&root)
            .ok()?,
    };
    let mut pending = reference
        .root_node()
        .map(|id| (id, reference.summary().height()))
        .into_iter()
        .collect::<Vec<_>>();
    while let Some((id, height)) = pending.pop() {
        let key = DraftPieceRecordKeyV1::new(reference.key().draft_id(), id);
        let node = storage
            .point::<DraftPieceNodesFamily>(store, key, point_limit())
            .ok()??;
        snapshot.node_records = snapshot.node_records.checked_add(1)?;
        snapshot
            .canonical_bytes
            .extend(<DraftPieceNodesCodec as RecordCodec<SyndicDomain>>::encode_value(&node).ok()?);
        if height == 1 {
            for child in node.children() {
                let leaf_key = DraftPieceRecordKeyV1::new(reference.key().draft_id(), child.id());
                let leaf = storage
                    .point::<DraftPieceLeavesFamily>(store, leaf_key, point_limit())
                    .ok()??;
                snapshot.leaf_records = snapshot.leaf_records.checked_add(1)?;
                snapshot.canonical_bytes.extend(
                    <DraftPieceLeavesCodec as RecordCodec<SyndicDomain>>::encode_value(&leaf)
                        .ok()?,
                );
            }
        } else {
            pending.extend(
                node.children()
                    .iter()
                    .rev()
                    .map(|child| (child.id(), height - 1)),
            );
        }
    }
    Some(snapshot)
}

/// Computes the exact canonical receipt commitment for deliberate corruption fixtures.
#[must_use]
pub fn compaction_settlement_receipt_commitment(
    receipt: &CompactionSettlementReceiptRecord,
) -> CompactionSettlementReceiptCommitment {
    crate::codec::compaction_settlement_receipt_commitment(receipt)
        .expect("a constructed fixture receipt has a canonical encoding")
}

/// One deliberate fixed-provenance corruption applied to an encoded stop-operation value.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StopProvenanceCodecCorruption {
    MissingAdmissionCause,
    DuplicateLaterCause,
    GappedLaterCause,
    FutureCause,
    ZeroClaimSource,
    FutureClaimPublication,
}

/// Roundtrips one record through the direct V1 stop-operation codec.
#[must_use]
pub fn roundtrip_stop_operation_v1(record: &StopOperationRecord) -> Option<StopOperationRecord> {
    use crate::codec::Family;

    let encoded = codec::StopOperationsFamily::encode_value(record).ok()?;
    codec::StopOperationsFamily::decode_value(&encoded).ok()
}

/// Proves one deliberately corrupted fixed-provenance encoding is rejected.
#[must_use]
pub fn stop_provenance_codec_rejection(
    record: &StopOperationRecord,
    corruption: StopProvenanceCodecCorruption,
) -> bool {
    use codec::StopProvenanceCodecCorruption as Internal;

    let corruption = match corruption {
        StopProvenanceCodecCorruption::MissingAdmissionCause => Internal::MissingAdmissionCause,
        StopProvenanceCodecCorruption::DuplicateLaterCause => Internal::DuplicateLaterCause,
        StopProvenanceCodecCorruption::GappedLaterCause => Internal::GappedLaterCause,
        StopProvenanceCodecCorruption::FutureCause => Internal::FutureCause,
        StopProvenanceCodecCorruption::ZeroClaimSource => Internal::ZeroClaimSource,
        StopProvenanceCodecCorruption::FutureClaimPublication => Internal::FutureClaimPublication,
    };
    codec::stop_provenance_codec_rejects(record, corruption)
}

/// Proves the removed aggregate-only stop-operation encoding is not a V1 predecessor format.
#[must_use]
pub fn old_aggregate_stop_encoding_rejection(record: &StopOperationRecord) -> bool {
    codec::old_aggregate_stop_encoding_is_rejected(record)
}

/// Proves the V1 stop codec rejects a dispatch-claimed record at the admitted first revision.
#[must_use]
pub fn stop_dispatch_claimed_first_codec_rejection(record: &StopOperationRecord) -> bool {
    if record.state() != StopOperationState::DispatchClaimed
        || record.revision().get() != StopOperationRevision::FIRST.get() + 1
    {
        return false;
    }
    matches!(
        StopOperationRecord::new(
            record.id(),
            record.target().clone(),
            record.admission(),
            StopOperationRevision::FIRST,
            record.cause_first_revisions(),
            record.dispatch_claim(),
            record.state(),
        ),
        Err(StopOperationRecordError::ClaimPublicationFuture { .. })
    )
}

/// Identifies the final atomic live-source-event command for scoped physical fault tests.
#[must_use]
pub fn live_source_event_fault_scope() -> beryl_home_store::test_faults::FaultScope {
    crate::mutation::live_source_event_fault_scope()
}

/// Identifies active CAS-turn identity publication for scoped physical fault tests.
#[must_use]
pub fn active_cas_turn_fault_scope() -> beryl_home_store::test_faults::FaultScope {
    crate::mutation::active_cas_turn_fault_scope()
}

/// Reads one exact accepted-route generation for fixture-delta construction.
pub fn accepted_route_generation(
    store: &beryl_home_store::HomeStore,
    storage: SyndicStorage,
    thread_id: beryl_model::SyndicThreadId,
    generation: AcceptedRouteGeneration,
) -> Result<AcceptedRouteGenerationRecord, SyndicReadError> {
    storage.route_generation(
        store,
        crate::codec::ThreadRouteKey {
            thread: thread_id,
            generation,
        },
    )
}

/// Reads the exact current accepted-route generation head for fixture-delta construction.
pub fn accepted_route_generation_head(
    store: &beryl_home_store::HomeStore,
    storage: SyndicStorage,
    thread_id: beryl_model::SyndicThreadId,
) -> Result<Option<AcceptedRouteGenerationHeadRecord>, SyndicReadError> {
    storage.route_generation_head(store, thread_id)
}

/// Identifies one unpublished provider-observation staging command for scoped physical fault tests.
#[must_use]
pub fn provider_observation_stage_fault_scope() -> beryl_home_store::test_faults::FaultScope {
    crate::mutation::provider_observation_stage_fault_scope()
}

/// Opaque observation of one stager's process-local lifetime.
pub struct ProviderObservationStagerLifetimeProbe {
    lifetime: std::sync::Weak<()>,
}

impl ProviderObservationStagerLifetimeProbe {
    /// Reports whether the exact observed stager remains retained.
    #[must_use]
    pub fn is_retained(&self) -> bool {
        self.lifetime.strong_count() != 0
    }
}

/// Observes only the lifetime of one provider-observation stager.
#[must_use]
pub fn provider_observation_stager_lifetime_probe(
    stager: &ProviderObservationStager,
) -> ProviderObservationStagerLifetimeProbe {
    ProviderObservationStagerLifetimeProbe {
        lifetime: stager.lifetime_probe(),
    }
}

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

/// Returns exact stored bytes charged to one activity-query fixture entry.
pub fn fixture_activity_query_entry_stored_bytes(entry: &ActivityQueryEntryRecord) -> u64 {
    activity_entry_stored_bytes(
        &ActivityQueryEntryKey {
            thread: entry.thread_id(),
            work_period: entry.work_period(),
            order: entry.order(),
        },
        entry,
    )
    .expect("valid activity fixture entry encodes")
}

/// Attaches one exact durable transition witness to a route-leaf fixture.
#[must_use]
pub fn fixture_route_leaf_with_transition(
    leaf: AcceptedRouteLeafRecord,
    proof: AcceptedRouteLeafTransitionProof,
) -> AcceptedRouteLeafRecord {
    leaf.with_transition_proof(proof)
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

/// Builds the exact owner-bearing manifest for one published provider frame fixture.
///
/// Finalization preserves every provider-frame fact while advancing only the
/// content revision and manifest lifecycle, matching the production freeze path.
#[must_use]
pub fn fixture_provider_content_manifest(
    owner: beryl_model::SyndicItemId,
    target: &SealedProviderFrameReference,
    finalized: bool,
) -> (SealedProviderFrameReference, ContentManifestRecord) {
    let current = target.content();
    let summary = current.summary();
    let revision = if finalized {
        current
            .revision()
            .checked_next()
            .expect("bounded provider fixture content revision must advance")
    } else {
        current.revision()
    };
    let lifecycle = if finalized {
        ContentLifecycle::Finalized
    } else {
        ContentLifecycle::Live
    };
    let manifest = ContentManifestRecord::with_owner(
        current.id(),
        Some(owner),
        revision,
        current.encoding(),
        lifecycle,
        summary.chunk_count(),
        summary.encoded_bytes(),
        summary.digest(),
        summary,
    );
    let content = manifest
        .current_reference()
        .expect("a published provider fixture manifest has a current reference");
    let target = SealedProviderFrameReference::new(
        content,
        target.frame().clone(),
        target.observation(),
        target.stream_state().clone(),
        target.narrative(),
    )
    .expect("finalization-only provider fixture revision remains valid");
    (target, manifest)
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

/// One exact typed current-domain family record to insert or replace in a fixture command.
#[derive(Clone, Debug)]
pub enum FixtureRecord {
    Thread(ThreadRecord),
    ThreadExecution(ThreadExecutionRecord),
    ThreadAttributes(ThreadAttributesRecord),
    ThreadUsage(ThreadUsageRecord),
    ThreadCatalogSummary(ThreadCatalogSummaryRecord),
    Draft(DraftRecord),
    ContentManifest(ContentManifestRecord),
    ContentChunk(ContentChunkRecord),
    ContentByteSpan(ContentByteSpanRecord),
    ContentTextSpan(ContentTextSpanRecord),
    ContentPiece(ContentPieceRecord),
    ProviderNarrativeSpan(ProviderNarrativeSpanRecord),
    ContextEnvelope(ContextEnvelopeRecord),
    Turn(TurnRecord),
    TurnState(TurnStateRecord),
    InputGate(InputGateRecord),
    AcceptedInput(AcceptedInputRecord),
    StopOperation(StopOperationRecord),
    CompactionOperation(CompactionOperationRecord),
    CompactionSettlementReceipt(CompactionSettlementReceiptRecord),
    AcceptedRouteGenerationHead(AcceptedRouteGenerationHeadRecord),
    AcceptedRouteLeaf(AcceptedRouteLeafRecord),
    SourceEvent(SourceEventRecord),
    CanonicalItem(CanonicalItemRecord),
    ActivityQueryHead(ActivityQueryHeadRecord),
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
    ImageLabelOriginSpan(ImageLabelOriginSpanRecord),
    TurnChild(TurnChildIndexRecord),
    AcceptedOrder(AcceptedOrderIndexRecord),
    AcceptedRouteGeneration(AcceptedRouteGenerationRecord),
    AcceptedReadySource(AcceptedReadySourceRecord),
    AcceptedNextSource(AcceptedNextSourceRecord),
    TurnItem(TurnItemIndexRecord),
    ActivityQueryEntry(ActivityQueryEntryRecord),
    ActivityQuerySource(ActivityQuerySourceRecord),
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

/// Builds the intrinsic open-branch attributes half for a coherent branch-creation fixture.
///
/// Production callers cannot create this state independently; the future branch-creation mutation
/// owns its atomic publication with lineage, context, draft, and property records.
#[doc(hidden)]
#[must_use]
pub fn open_branch_thread_attributes(
    thread_id: beryl_model::SyndicThreadId,
) -> ThreadAttributesRecord {
    ThreadAttributesRecord::branch_discussion_open(thread_id)
}

/// Rebuilds attributes with a deliberate lifecycle revision for reopen-corruption coverage.
#[doc(hidden)]
#[must_use]
pub fn thread_attributes_with_revision(
    record: &ThreadAttributesRecord,
    revision: ThreadAttributesRevision,
) -> ThreadAttributesRecord {
    ThreadAttributesRecord::from_parts(
        record.thread_id(),
        revision,
        record.generated_title().cloned(),
        record.archive(),
    )
}

/// Rebuilds usage with a deliberate lifecycle revision for reopen-corruption coverage.
#[doc(hidden)]
#[must_use]
pub fn thread_usage_with_revision(
    record: &ThreadUsageRecord,
    revision: ThreadUsageRevision,
) -> ThreadUsageRecord {
    ThreadUsageRecord::from_parts(record.thread_id(), revision, record.observation().cloned())
}

impl FixtureRecord {
    /// Returns the exact physical current-domain family encoded by this fixture record.
    #[must_use]
    pub const fn family(&self) -> PhysicalFamily {
        match self {
            Self::Thread(_) => PhysicalFamily::Threads,
            Self::ThreadExecution(_) => PhysicalFamily::ThreadExecutions,
            Self::ThreadAttributes(_) => PhysicalFamily::ThreadAttributes,
            Self::ThreadUsage(_) => PhysicalFamily::ThreadUsage,
            Self::ThreadCatalogSummary(_) => PhysicalFamily::ThreadCatalogSummaries,
            Self::Draft(_) => PhysicalFamily::Drafts,
            Self::ContentManifest(_) => PhysicalFamily::ContentManifests,
            Self::ContentChunk(_) => PhysicalFamily::ContentChunks,
            Self::ContentByteSpan(_) => PhysicalFamily::ContentByteSpans,
            Self::ContentTextSpan(_) => PhysicalFamily::ContentTextSpans,
            Self::ContentPiece(_) => PhysicalFamily::ContentPieces,
            Self::ProviderNarrativeSpan(_) => PhysicalFamily::ProviderNarrativeSpans,
            Self::ContextEnvelope(_) => PhysicalFamily::ContextEnvelopes,
            Self::Turn(_) => PhysicalFamily::Turns,
            Self::TurnState(_) => PhysicalFamily::TurnStates,
            Self::InputGate(_) => PhysicalFamily::InputGates,
            Self::AcceptedInput(_) => PhysicalFamily::AcceptedInputs,
            Self::StopOperation(_) => PhysicalFamily::StopOperations,
            Self::CompactionOperation(_) => PhysicalFamily::CompactionOperations,
            Self::CompactionSettlementReceipt(_) => PhysicalFamily::CompactionSettlementReceipts,
            Self::AcceptedRouteGenerationHead(_) => PhysicalFamily::AcceptedRouteGenerationHeads,
            Self::AcceptedRouteLeaf(_) => PhysicalFamily::AcceptedRouteLeaves,
            Self::SourceEvent(_) => PhysicalFamily::SourceEvents,
            Self::CanonicalItem(_) => PhysicalFamily::CanonicalItems,
            Self::ActivityQueryHead(_) => PhysicalFamily::ActivityQueryHeads,
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
            Self::ImageLabelOriginSpan(_) => PhysicalFamily::ImageLabelOriginSpans,
            Self::TurnChild(_) => PhysicalFamily::TurnChildren,
            Self::AcceptedOrder(_) => PhysicalFamily::AcceptedOrder,
            Self::AcceptedRouteGeneration(_) => PhysicalFamily::AcceptedRouteGenerations,
            Self::AcceptedReadySource(_) => PhysicalFamily::AcceptedReadySources,
            Self::AcceptedNextSource(_) => PhysicalFamily::AcceptedNextSources,
            Self::TurnItem(_) => PhysicalFamily::TurnItems,
            Self::ActivityQueryEntry(_) => PhysicalFamily::ActivityQueryEntries,
            Self::ActivityQuerySource(_) => PhysicalFamily::ActivityQuerySources,
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

/// One exact typed current-domain family key to remove in a fixture command.
#[derive(Clone, Debug)]
pub enum FixtureDelete {
    Thread(beryl_model::SyndicThreadId),
    ThreadExecution(beryl_model::SyndicThreadId),
    ThreadAttributes(beryl_model::SyndicThreadId),
    ThreadUsage(beryl_model::SyndicThreadId),
    ThreadCatalogSummary(beryl_model::SyndicThreadId),
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
    ProviderNarrativeSpan {
        content: beryl_model::SyndicContentId,
        generation: ProviderNarrativeGeneration,
        logical_start: u64,
    },
    ContextEnvelope(beryl_model::DiscussionContextOwnerId),
    Turn(beryl_model::SyndicTurnId),
    TurnState(beryl_model::SyndicTurnId),
    InputGate(beryl_model::SyndicThreadId),
    AcceptedInput(beryl_model::SyndicAcceptedInputId),
    StopOperation(StopOperationId),
    CompactionOperation(CompactionOperationId),
    CompactionSettlementReceipt(CompactionOperationId),
    AcceptedRouteGenerationHead(beryl_model::SyndicThreadId),
    AcceptedRouteLeaf(beryl_model::SyndicAcceptedInputId),
    SourceEvent {
        turn: beryl_model::SyndicTurnId,
        sequence: SourceEventSequence,
    },
    CanonicalItem(beryl_model::SyndicItemId),
    ActivityQueryHead(beryl_model::SyndicThreadId),
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
    ImageLabelOriginSpan {
        thread: beryl_model::SyndicThreadId,
        end_label: ImageLabelOrdinal,
    },
    TurnChild {
        parent: beryl_model::SyndicTurnId,
        child: beryl_model::SyndicTurnId,
    },
    AcceptedOrder {
        thread: beryl_model::SyndicThreadId,
        ordinal: AcceptedInputOrdinal,
    },
    AcceptedRouteGeneration {
        thread: beryl_model::SyndicThreadId,
        generation: AcceptedRouteGeneration,
    },
    AcceptedReadySource {
        thread: beryl_model::SyndicThreadId,
        generation: AcceptedRouteGeneration,
    },
    AcceptedNextSource {
        thread: beryl_model::SyndicThreadId,
        generation: AcceptedRouteGeneration,
    },
    TurnItem {
        turn: beryl_model::SyndicTurnId,
        ordinal: TurnItemOrdinal,
    },
    ActivityQueryEntry {
        thread: beryl_model::SyndicThreadId,
        work_period: ActivityWorkPeriod,
        order: ActivityQueryOrder,
    },
    ActivityQuerySource {
        thread: beryl_model::SyndicThreadId,
        work_period: ActivityWorkPeriod,
        source_thread: beryl_model::SyndicThreadId,
        source_turn: beryl_model::SyndicTurnId,
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
