use super::*;

#[test]
fn multi_page_text_replays_with_bounded_compiler_batches() {
    let home = TestHome::new("large-replay");
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    let text_bytes = vec![b'x'; CONTENT_CHUNK_MAX_BYTES * (CONTENT_APPEND_MAX_CHUNKS + 2)];
    let pieces = text_bytes
        .chunks(PROVIDER_OBSERVATION_CHUNK_MAX_BYTES)
        .collect::<Vec<_>>();
    let bound = {
        let mut callback = observation_callback(&store, storage);
        let mut stager = committed_stage_value(ProviderObservationStager::begin(
            ProviderObservationId::from_bytes([3; 16]),
            ProviderObservationBegin::Item {
                lifecycle: ProviderObservationItemLifecycle::Started,
                kind: ProviderObservationItemKind::AgentMessage,
            },
            &mut callback,
        )
        .unwrap());
        field_text(
            &mut stager,
            ProviderField::ItemId,
            &[b"large-item"],
            &mut callback,
        );
        scalar(
            &mut stager,
            ProviderField::LifecycleObservedAt,
            ProviderScalar::Unsigned(77),
            &mut callback,
        );
        field_text(
            &mut stager,
            ProviderField::AgentMessageText,
            &pieces,
            &mut callback,
        );
        bind_sealed(stager, &mut callback)
    };

    let prepared = prepare_first(
        &storage,
        &store,
        bound,
        "large-item",
        ProviderItemKind::AgentMessage,
        10,
    );
    let (compiled, final_build) = stage_compiler(&storage, &store, &prepared);
    assert!(compiled.batches >= 2);
    assert_eq!(compiled.narrative_spans, 1);
    assert_eq!(
        compiled.bytes.len() as u64,
        prepared.target().content().summary().encoded_bytes()
    );
    assert_eq!(
        prepared.target().frame().logical_utf8_bytes(),
        text_bytes.len() as u64
    );
    assert_eq!(final_build.lifecycle(), ProviderItemBuildLifecycle::Sealed);
    assert!(final_build.frame_staged());
    store.validate_registered_domains().unwrap();
    store.close().unwrap();
}
