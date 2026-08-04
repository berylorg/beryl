use super::*;

fn started_agent_observation(
    identity_byte: u8,
    item_id: &str,
    message: &[u8],
    callback: &mut impl ProviderObservationStageCallback<Error = CommandError>,
) -> BoundProviderObservation {
    let mut stager = ProviderObservationStager::begin(
        ProviderObservationId::from_bytes([identity_byte; 16]),
        ProviderObservationBegin::Item {
            lifecycle: ProviderObservationItemLifecycle::Started,
            kind: ProviderObservationItemKind::AgentMessage,
        },
        callback,
    )
    .unwrap();

    // The compiler must select by field identity, not the provider's object-field order.
    field_text(
        &mut stager,
        ProviderField::AgentMessageText,
        &[message],
        callback,
    );
    enum_value(
        &mut stager,
        ProviderField::MessagePhase,
        ProviderEnumValue::FinalAnswer,
        callback,
    );
    scalar(
        &mut stager,
        ProviderField::LifecycleObservedAt,
        ProviderScalar::Unsigned(42),
        callback,
    );
    field_text(
        &mut stager,
        ProviderField::ItemId,
        &[item_id.as_bytes()],
        callback,
    );
    bind_sealed(stager, callback)
}

#[test]
fn arbitrary_order_agent_observation_matches_materialized_encoding_and_staging() {
    let home = TestHome::new("agent-parity");
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    let bound = {
        let mut callback = observation_callback(&store, storage);
        started_agent_observation(1, "agent-item", "hÃ©llo 🦀".as_bytes(), &mut callback)
    };

    let prepared = prepare_first(
        &storage,
        &store,
        bound,
        "agent-item",
        ProviderItemKind::AgentMessage,
        4,
    );
    let expected = ProviderItemFrameV1::new(
        ProviderFrameOrdinalV1::FIRST,
        CasItemId::new("agent-item").unwrap(),
        ProviderItemObservationV1::Started {
            observed_at: ProviderLifecycleTimestampMsV1::new(42),
            item: ProviderItemV1::AgentMessage(ProviderAgentMessageV1 {
                text: ProviderTextV1::inline("hÃ©llo 🦀"),
                phase: Some(ProviderMessagePhaseV1::FinalAnswer),
                memory_citation: None,
            }),
        },
    );
    let (materialized, expected_reference) = materialized(&expected);

    assert_eq!(prepared.target().frame(), &expected_reference);
    assert_eq!(prepared.target().narrative().unwrap().span_count(), 1);
    let (compiled, final_build) = stage_compiler(&storage, &store, &prepared);
    assert_eq!(compiled.bytes, materialized.bytes);
    assert_eq!(compiled.narrative_spans, 1);
    assert_eq!(final_build.lifecycle(), ProviderItemBuildLifecycle::Sealed);
    assert!(final_build.frame_staged());
    store.validate_registered_domains().unwrap();
    store.close().unwrap();
}

#[test]
fn destination_item_disagreement_has_a_distinct_semantic_error() {
    let home = TestHome::new("item-mismatch");
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    let bound = {
        let mut callback = observation_callback(&store, storage);
        started_agent_observation(2, "observed-item", b"text", &mut callback)
    };
    let inspected = inspect_provider_observation(&storage, &store, bound, limit()).unwrap();
    let result = prepare_provider_observation_frame(
        &storage,
        &store,
        inspected,
        ProviderObservationFramePreparationPlan::first(
            SyndicItemId::from_bytes([7; 16]),
            SyndicTurnId::from_bytes([8; 16]),
            source("other-item"),
            SourceEventSequence::FIRST,
            SyndicContentId::from_bytes([9; 16]),
        ),
        limit(),
    );
    assert!(matches!(
        result,
        Err(ProviderObservationFramePreparationError::Semantic(
            ProviderObservationFrameSemanticError::ItemIdentityMismatch
        ))
    ));
    store.validate_registered_domains().unwrap();
    store.close().unwrap();
}
