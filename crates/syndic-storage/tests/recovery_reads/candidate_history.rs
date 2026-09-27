use beryl_home_store::{
    HomeOpenCandidate, HomeOpenOptions, HomeSchemaVersion,
    test_faults::{FaultController, FaultPoint},
};
use beryl_model::{
    ProjectionRevision, SyndicContentId, SyndicItemId, SyndicResourceId, SyndicTurnId,
};
use syndic_storage::{
    ItemProjectionBuildPhase, ItemProjectionBuildRecord, ItemProjectionGeneration,
    MarkdownParserCheckpoint, ProjectionFormatVersion, ProjectionSourceRange,
    ProjectionTextSourceCursor, ResourceBacking, ResourceKind, ResourceMetadataRecord,
    ResourceOrdinal, ResourceStructure, SyndicPointReadLimit, SyndicReadError, SyndicStorage,
    TranscriptGeneration, prepare_lifecycle_continuation_content, test_faults::FixtureRecord,
};

use crate::support::{
    TestHome, batch, commit, id, open,
    populated::{source_item, source_resource, source_resource_projection, source_turn},
    seed_populated,
};

#[test]
fn candidate_history_metadata_preserves_exact_values_bounds_and_generation_fences() {
    let home = TestHome::new("candidate-history-metadata");
    let mut candidate = open(home.path());
    let mut storage = SyndicStorage::register(&mut candidate).unwrap();
    let mut store = candidate
        .prepare_publication(SyndicStorage::required_domains().unwrap())
        .unwrap()
        .publish()
        .unwrap();
    seed_populated(&store, storage.clone());
    let limit = SyndicPointReadLimit::new(65_536).unwrap();
    let item = storage
        .canonical_item(&store, source_item(), limit)
        .unwrap()
        .unwrap();
    let resource = ResourceMetadataRecord::new(
        source_resource(),
        ProjectionRevision::new(1).unwrap(),
        source_resource_projection(),
        source_item(),
        ResourceOrdinal::FIRST,
        ResourceKind::Attachment,
        "text/plain",
        ResourceBacking::TextRange {
            source: item.projection_source().unwrap(),
            range: ProjectionSourceRange::new(0, 9).unwrap(),
        },
        [50; 32],
        Some(ProjectionSourceRange::new(0, 9).unwrap()),
        ResourceStructure::Opaque,
    )
    .unwrap();
    commit(
        &store,
        storage.clone(),
        batch([FixtureRecord::Resource(resource)]),
    );
    let foreign_home = TestHome::new("candidate-history-foreign");
    let mut foreign = open(foreign_home.path());
    let foreign_storage = SyndicStorage::register(&mut foreign).unwrap();
    let item_generation = storage
        .item_projection_head(&store, source_item(), limit)
        .unwrap()
        .unwrap()
        .generation();
    let transcript_generation = storage
        .transcript_view_head(&store, id(30), limit)
        .unwrap()
        .unwrap()
        .generation();
    let content_id = storage
        .canonical_item(&store, source_item(), limit)
        .unwrap()
        .unwrap()
        .provider_content()
        .unwrap()
        .id();

    let build_generation = ItemProjectionGeneration::new(777).unwrap();
    let build = ItemProjectionBuildRecord::new(
        source_item(),
        build_generation,
        ProjectionRevision::new(1).unwrap(),
        ProjectionFormatVersion::V1,
        item.revision(),
        item.projection_source().unwrap(),
        0,
        0,
        0,
        syndic_storage::test_faults::fixture_item_projection_digest_seed(),
        ItemProjectionBuildPhase::Parsing(MarkdownParserCheckpoint::new(
            0,
            0,
            ProjectionTextSourceCursor::ProviderNarrative { logical_start: 0 },
            0,
            Box::<str>::default(),
            false,
            None,
        )),
    );
    commit(
        &store,
        storage.clone(),
        batch([FixtureRecord::ItemProjectionBuild(build)]),
    );

    macro_rules! check {
        ($ordinary:ident, $candidate:ident, [$($key:expr),+], [$($missing:expr),+]) => {{
            let expected = storage.$ordinary(&store, $($key,)+ limit).unwrap();
            assert!(expected.is_some(), stringify!($ordinary));
            store.close().unwrap();
            let faults = FaultController::new();
            let mut candidate = HomeOpenCandidate::open_with_faults(
                HomeOpenOptions::new(home.path(), HomeSchemaVersion::CURRENT), faults.clone(),
            ).unwrap();
            let fresh = SyndicStorage::register(&mut candidate).unwrap();
            let mut publication = candidate.prepare_publication(SyndicStorage::required_domains().unwrap()).unwrap();
            let access = publication.recovery_access().unwrap();
            assert_eq!(fresh.$candidate(&access, $($key,)+ limit).unwrap(), expected);
            assert_eq!(fresh.$candidate(&access, $($missing,)+ limit).unwrap(), None);
            assert!(fresh.$candidate(&access, $($key,)+ SyndicPointReadLimit::new(1).unwrap()).is_err());
            for invalid in [&storage, &foreign_storage] {
                assert!(matches!(invalid.$candidate(&access, $($key,)+ limit), Err(SyndicReadError::Read(_))));
            }
            beryl_home_store::test_faults::with_initial_publication_store(&publication, |store| {
                assert!(fresh.$ordinary(store, $($key,)+ limit).is_err());
            });
            store = publication.publish().unwrap();
            assert_eq!(fresh.$ordinary(&store, $($key,)+ limit).unwrap(), expected);
            faults.fail_next(FaultPoint::BeforeReadConfirmation);
            assert!(store.home_revision().is_err());
            let mut recovered = store.recover_same_home().unwrap();
            storage = SyndicStorage::reacquire_candidate(&recovered).unwrap();
            let access = recovered.recovery_access().unwrap();
            assert!(matches!(fresh.$candidate(&access, $($key,)+ limit), Err(SyndicReadError::Read(_))));
            assert_eq!(storage.$candidate(&access, $($key,)+ limit).unwrap(), expected);
            store = recovered.publish().unwrap();
            assert_eq!(storage.$ordinary(&store, $($key,)+ limit).unwrap(), expected);
        }};
    }
    check!(thread, thread_candidate, [id(30)], [id(250)]);
    check!(current_draft, current_draft_candidate, [id(30)], [id(250)]);
    check!(
        turn,
        turn_candidate,
        [source_turn()],
        [SyndicTurnId::from_bytes([250; 16])]
    );
    check!(
        turn_state,
        turn_state_candidate,
        [source_turn()],
        [SyndicTurnId::from_bytes([250; 16])]
    );
    check!(input_gate, input_gate_candidate, [id(30)], [id(250)]);
    check!(
        canonical_item,
        canonical_item_candidate,
        [source_item()],
        [SyndicItemId::from_bytes([250; 16])]
    );
    check!(
        content_manifest,
        content_manifest_candidate,
        [content_id],
        [SyndicContentId::from_bytes([250; 16])]
    );
    check!(
        resource,
        resource_candidate,
        [source_resource()],
        [SyndicResourceId::from_bytes([250; 16])]
    );
    check!(
        item_projection_head,
        item_projection_head_candidate,
        [source_item()],
        [SyndicItemId::from_bytes([250; 16])]
    );
    check!(
        item_projection_set,
        item_projection_set_candidate,
        [source_item(), item_generation],
        [source_item(), ItemProjectionGeneration::new(999).unwrap()]
    );
    check!(
        item_projection_build,
        item_projection_build_candidate,
        [source_item(), build_generation],
        [source_item(), ItemProjectionGeneration::new(999).unwrap()]
    );
    check!(
        transcript_view_head,
        transcript_view_head_candidate,
        [id(30)],
        [id(250)]
    );
    check!(
        transcript_build,
        transcript_build_candidate,
        [id(30), transcript_generation],
        [id(30), TranscriptGeneration::new(999).unwrap()]
    );
    store.close().unwrap();
    foreign.close().unwrap();
}

#[test]
fn candidate_content_metadata_preserves_ownerless_unsealed_rejection() {
    let home = TestHome::new("candidate-history-unsealed");
    let mut candidate = open(home.path());
    let storage = SyndicStorage::register(&mut candidate).unwrap();
    let store = candidate
        .prepare_publication(SyndicStorage::required_domains().unwrap())
        .unwrap()
        .publish()
        .unwrap();
    let prepared = prepare_lifecycle_continuation_content().unwrap();
    commit(
        &store,
        storage.clone(),
        batch([FixtureRecord::ContentManifest(prepared.building_manifest())]),
    );
    let limit = SyndicPointReadLimit::new(65_536).unwrap();
    assert!(matches!(
        storage.content_manifest(&store, prepared.id(), limit),
        Err(SyndicReadError::Invariant(
            "ownerless content is unavailable before seal"
        ))
    ));
    store.close().unwrap();
    let mut candidate = open(home.path());
    let storage = SyndicStorage::register(&mut candidate).unwrap();
    let mut publication = candidate
        .prepare_publication(SyndicStorage::required_domains().unwrap())
        .unwrap();
    assert!(matches!(
        storage.content_manifest_candidate(
            &publication.recovery_access().unwrap(),
            prepared.id(),
            limit
        ),
        Err(SyndicReadError::Invariant(
            "ownerless content is unavailable before seal"
        ))
    ));
    publication.close().unwrap();
}
