use super::*;

pub fn source_turn() -> SyndicTurnId {
    SyndicTurnId::from_bytes([32; 16])
}

pub fn source_item() -> SyndicItemId {
    SyndicItemId::from_bytes([33; 16])
}

pub(super) fn source_cas_thread() -> CasThreadId {
    CasThreadId::new("source-history-thread").unwrap()
}

pub(super) fn source_cas_turn() -> CasTurnId {
    CasTurnId::new("source-history-turn").unwrap()
}

pub(super) fn source_cas_item() -> CasItemId {
    CasItemId::new("source-history-item").unwrap()
}

pub fn source_cas_authority() -> CasTurnSource {
    CasTurnSource::new(source_cas_thread(), source_cas_turn())
}

pub(super) fn source_snapshot() -> SyndicExecutionSnapshotId {
    SyndicExecutionSnapshotId::from_bytes([35; 16])
}

pub fn source_projection() -> SyndicProjectionId {
    syndic_storage::test_faults::fixture_inline_paragraph_projection(
        source_item(),
        source_turn(),
        "assistant",
    )
    .id()
}

pub fn correlate_source_user_item(
    records: &mut Vec<FixtureRecord>,
    item: SyndicItemId,
    _projection_revision: ProjectionRevision,
    content: ContentReference,
    asset_reference_set: Option<beryl_model::SealedAssetReferenceSetProof>,
    updated_at: SyndicTimestamp,
) {
    let source = source_turn();
    let source_thread = id(30);
    let source_digest = child_turn_chain_digest(
        source,
        SyndicTurnId::from_bytes([29; 16]),
        root_turn_chain_digest(SyndicTurnId::from_bytes([29; 16])),
    );
    records.retain(|record| {
        !matches!(record, FixtureRecord::TurnState(state) if state.turn_id() == source)
            && !matches!(record, FixtureRecord::SourceEvent(event)
                if event.turn_id() == source && event.sequence().get() >= 5)
            && !matches!(record, FixtureRecord::CanonicalItem(existing) if existing.id() == item)
            && !matches!(record, FixtureRecord::TurnItem(index)
                if index.turn_id() == source && index.ordinal() == TurnItemOrdinal::new(2).unwrap())
            && !matches!(record, FixtureRecord::ItemSourceEvent(index) if index.item_id() == item)
            && !matches!(record, FixtureRecord::CasItem(index) if index.item_id() == item)
            && !matches!(record, FixtureRecord::TranscriptPathTurn(path)
                if path.thread_id() == source_thread
                    && path.generation() == TranscriptGeneration::FIRST
                    && path.depth() == TurnDepth::new(2).unwrap())
    });
    let cas_thread = source_cas_thread();
    let cas_turn = source_cas_turn();
    let cas_item = CasItemId::new(format!("source-user-{item}")).unwrap();
    let source_authority = CasTurnSource::new(cas_thread.clone(), cas_turn.clone());
    let cas_source = CasItemSource::new(source_authority.clone(), cas_item.clone());
    let provider = correlated_user_item_fixture(
        item,
        source,
        cas_source.clone(),
        SourceEventSequence::new(5).unwrap(),
        content,
    );
    let item_revision = ProjectionRevision::new(4).unwrap();
    records.extend(provider.records);
    records.extend([
        FixtureRecord::TurnState(fixture_turn_state(
            source,
            TurnStateRevision::FIRST,
            TurnLifecycle::Interrupted,
            7,
            2,
            updated_at,
        )),
        FixtureRecord::SourceEvent(
            SourceEventRecord::new(
                source,
                SourceEventSequence::new(5).unwrap(),
                Some(source_authority.clone()),
                SourceEventPayload::ItemFrame {
                    item_id: item,
                    frame: Box::new(provider.frames[0].clone()),
                },
            )
            .unwrap(),
        ),
        FixtureRecord::SourceEvent(
            SourceEventRecord::new(
                source,
                SourceEventSequence::new(6).unwrap(),
                Some(source_authority.clone()),
                SourceEventPayload::ItemFrame {
                    item_id: item,
                    frame: Box::new(provider.frames[1].clone()),
                },
            )
            .unwrap(),
        ),
        FixtureRecord::SourceEvent(
            SourceEventRecord::new(
                source,
                SourceEventSequence::new(7).unwrap(),
                Some(source_authority.clone()),
                SourceEventPayload::TurnEnded(
                    TurnEndStatus::new(TurnTerminalOutcome::Interrupted, None).unwrap(),
                ),
            )
            .unwrap(),
        ),
        FixtureRecord::CanonicalItem(
            CanonicalItemRecord::with_provider_state(
                item,
                source,
                TurnItemOrdinal::new(2).unwrap(),
                item_revision,
                SourceEventSequence::new(6).unwrap(),
                2,
                cas_source,
                None,
                provider.canonical,
                None,
                CanonicalItemPresentation::user_input(content, asset_reference_set),
            )
            .unwrap(),
        ),
        FixtureRecord::TurnItem(TurnItemIndexRecord::new(
            source,
            TurnItemOrdinal::new(2).unwrap(),
            item,
            item_revision,
        )),
        FixtureRecord::ItemSourceEvent(ItemSourceEventIndexRecord::new(
            item,
            ItemSourceEventOrdinal::FIRST,
            source,
            SourceEventSequence::new(5).unwrap(),
        )),
        FixtureRecord::ItemSourceEvent(ItemSourceEventIndexRecord::new(
            item,
            ItemSourceEventOrdinal::new(2).unwrap(),
            source,
            SourceEventSequence::new(6).unwrap(),
        )),
        FixtureRecord::CasItem(CasItemIndexRecord::new(
            cas_thread,
            cas_turn,
            cas_item,
            item,
            item_revision,
        )),
        FixtureRecord::TranscriptPathTurn(TranscriptPathTurnRecord::new(
            source_thread,
            TranscriptGeneration::FIRST,
            TurnDepth::new(2).unwrap(),
            source,
            source_digest,
            TurnStateRevision::FIRST,
            TurnLifecycle::Interrupted,
            7,
            2,
            2,
            updated_at,
        )),
    ]);
}

pub fn source_resource() -> SyndicResourceId {
    SyndicResourceId::from_bytes([35; 16])
}

pub fn source_resource_projection() -> SyndicProjectionId {
    SyndicProjectionId::from_bytes([34; 16])
}

pub fn active_turn() -> SyndicTurnId {
    SyndicTurnId::from_bytes([42; 16])
}

pub fn active_item() -> SyndicItemId {
    SyndicItemId::from_bytes([43; 16])
}

pub fn active_projection() -> SyndicProjectionId {
    syndic_storage::test_faults::fixture_inline_paragraph_projection(
        active_item(),
        active_turn(),
        "active",
    )
    .id()
}

pub fn suffix_item() -> SyndicItemId {
    SyndicItemId::from_bytes([60; 16])
}

pub fn build_item() -> SyndicItemId {
    SyndicItemId::from_bytes([61; 16])
}

pub fn activity_item() -> SyndicItemId {
    SyndicItemId::from_bytes([62; 16])
}

pub fn suffix_projection() -> SyndicProjectionId {
    syndic_storage::test_faults::fixture_empty_projection(suffix_item(), active_turn()).id()
}

pub fn active_snapshot() -> SyndicExecutionSnapshotId {
    SyndicExecutionSnapshotId::from_bytes([45; 16])
}

pub fn steering_input() -> SyndicAcceptedInputId {
    SyndicAcceptedInputId::from_bytes([46; 16])
}

pub fn next_input() -> SyndicAcceptedInputId {
    SyndicAcceptedInputId::from_bytes([47; 16])
}

pub fn cas_thread() -> CasThreadId {
    CasThreadId::new("populated-thread").unwrap()
}

pub fn cas_turn() -> CasTurnId {
    CasTurnId::new("populated-turn").unwrap()
}

pub fn cas_item() -> CasItemId {
    CasItemId::new("populated-item").unwrap()
}

pub(super) fn execution_binding() -> beryl_model::ExecutionBinding {
    let path = beryl_model::RuntimeNativePath::from_admitted(
        beryl_model::RuntimeMode::host(),
        beryl_model::PathFlavor::Windows,
        "C:\\populated",
    )
    .unwrap();
    beryl_model::ExecutionBinding::new(
        beryl_model::RuntimeId::from_bytes([48; 16]),
        beryl_model::RootId::from_bytes([49; 16]),
        path,
    )
}
