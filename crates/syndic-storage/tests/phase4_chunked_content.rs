#![cfg(feature = "test-faults")]

mod support;

use beryl_home_store::{
    CommandError, CursorReadLimits, HomeCommand, HomeOpenOptions, HomeSchemaVersion, HomeStore,
};
use beryl_model::{
    AcceptedInputRevision, ContentRevision, InputGateRevision, ProjectionRevision,
    SyndicAcceptedInputId, SyndicItemId, SyndicTurnId,
};
use syndic_storage::test_faults::FixtureRecord;
use syndic_storage::{
    AcceptedInputDisposition, AcceptedInputLifecycle, AcceptedInputOrdinal, AcceptedInputRecord,
    AcceptedNextTurnIndexRecord, AcceptedOrderIndexRecord, AdvanceItemProjectionBuild,
    CONTENT_APPEND_MAX_CHUNKS, CONTENT_CHUNK_MAX_BYTES, CanonicalItemRecord, ComposerAtom,
    ComposerPayload, ContentAppend, ContentBuild, ContentLifecycle, ContentManifestRecord,
    DraftPayloadUpdate, DraftPayloadUpdateDecision, InputGateRecord, InputGateState,
    ItemProjectionGeneration, NextTurnReason, PreparedContent, StartItemProjectionBuild,
    SyndicMutationError, SyndicPointReadLimit, SyndicStorage, TurnDepth, TurnEndStatus,
    TurnIncompleteReason, TurnItemIndexRecord, TurnItemOrdinal, TurnKind, TurnLifecycle,
    TurnRecord, TurnStateRecord, TurnStateRevision, TurnTerminalOutcome,
};

use support::*;

fn point_limit() -> SyndicPointReadLimit {
    SyndicPointReadLimit::new(1_000_000).unwrap()
}

fn execute(
    store: &HomeStore,
    storage: SyndicStorage,
    contribution: beryl_home_store::MutationContribution,
) -> Result<(), CommandError> {
    let mut command = HomeCommand::new(store.home_revision().unwrap());
    command.add(contribution).unwrap();
    let receipt = store.execute(command)?;
    assert!(
        storage
            .committed_revision(store, &receipt)
            .unwrap()
            .is_some()
    );
    Ok(())
}

fn create_thread(
    store: &HomeStore,
    storage: SyndicStorage,
    thread: beryl_model::SyndicThreadId,
    draft: beryl_model::SyndicDraftId,
) {
    let creation = syndic_storage::CreateThread::ordinary(thread, draft, timestamp(1));
    execute(
        store,
        storage,
        storage.create_thread(storage.revision(store).unwrap(), creation),
    )
    .unwrap();
}

fn project_item(store: &HomeStore, storage: SyndicStorage, item: SyndicItemId) {
    let canonical = storage
        .canonical_item(store, item, point_limit())
        .unwrap()
        .unwrap();
    let generation = ItemProjectionGeneration::FIRST;
    execute(
        store,
        storage,
        storage.start_item_projection_build(
            storage.revision(store).unwrap(),
            StartItemProjectionBuild::new(item, canonical.record().revision(), generation),
        ),
    )
    .unwrap();
    loop {
        if storage
            .item_projection_set(store, item, generation, point_limit())
            .unwrap()
            .is_some()
        {
            return;
        }
        let build = storage
            .item_projection_build(store, item, generation, point_limit())
            .unwrap()
            .unwrap();
        execute(
            store,
            storage,
            storage.advance_item_projection_build(
                storage.revision(store).unwrap(),
                AdvanceItemProjectionBuild::new(item, generation, build.record().revision()),
            ),
        )
        .unwrap();
    }
}

fn append_one_batch(
    store: &HomeStore,
    storage: SyndicStorage,
    manifest: &ContentManifestRecord,
    content: &PreparedContent,
) -> Option<ContentManifestRecord> {
    let append = ContentAppend::prepare(manifest, content).unwrap()?;
    let next = append.next_manifest().clone();
    let advanced = next.chunk_count() - manifest.chunk_count();
    assert!(advanced > 0);
    assert!(advanced <= u64::try_from(CONTENT_APPEND_MAX_CHUNKS).unwrap());
    execute(
        store,
        storage,
        storage.append_content(storage.revision(store).unwrap(), append),
    )
    .unwrap();
    Some(next)
}

fn huge_boundary_payload() -> ComposerPayload {
    let mut boundary = "a".repeat(CONTENT_CHUNK_MAX_BYTES - 19);
    boundary.push('🧵');
    let large = "word ".repeat(2_000_000);
    ComposerPayload::new(vec![
        ComposerAtom::text(boundary).unwrap(),
        ComposerAtom::text(large).unwrap(),
    ])
    .unwrap()
}

#[test]
fn multi_million_token_scale_draft_stages_reopens_and_publishes_exactly() {
    let home = TestHome::new("phase4-large-content");
    let mut store = HomeStore::open(HomeOpenOptions::new(
        home.path(),
        HomeSchemaVersion::CURRENT,
    ))
    .unwrap();
    let storage = SyndicStorage::register(&mut store).unwrap();
    let thread = id(1);
    let draft = draft_id(2);
    create_thread(&store, storage, thread, draft);

    let payload = huge_boundary_payload();
    assert!(payload.utf8_bytes() > 10_000_000);
    let content = PreparedContent::composer(&payload).unwrap();
    assert!(content.chunks().len() > CONTENT_APPEND_MAX_CHUNKS);
    assert!(
        content
            .chunks()
            .iter()
            .all(|chunk| chunk.bytes().len() <= CONTENT_CHUNK_MAX_BYTES)
    );

    execute(
        &store,
        storage,
        storage.begin_content(
            storage.revision(&store).unwrap(),
            ContentBuild::from_prepared(&content),
        ),
    )
    .unwrap();
    let mut manifest = content.building_manifest();
    manifest = append_one_batch(&store, storage, &manifest, &content).unwrap();
    assert_eq!(manifest.lifecycle(), ContentLifecycle::Building);
    assert_eq!(
        read_composer_payload(
            &store,
            storage,
            &storage
                .current_draft(&store, thread, point_limit())
                .unwrap()
                .unwrap(),
        ),
        ComposerPayload::default()
    );
    store.validate_registered_domains().unwrap();
    store.close().unwrap();

    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    let stored = storage
        .content_manifest(&store, content.id(), point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(stored.record(), &manifest);
    while let Some(next) = append_one_batch(&store, storage, &manifest, &content) {
        manifest = next;
    }
    assert_eq!(manifest.expected(), content.summary());

    let current = storage
        .current_draft(&store, thread, point_limit())
        .unwrap()
        .unwrap();
    let update = match DraftPayloadUpdate::prepare(&current, &content, timestamp(2)).unwrap() {
        DraftPayloadUpdateDecision::Update(update) => update,
        DraftPayloadUpdateDecision::NoChange => unreachable!(),
    };
    execute(
        &store,
        storage,
        storage.update_draft_payload(storage.revision(&store).unwrap(), update),
    )
    .unwrap();
    let current = storage
        .current_draft(&store, thread, point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(current.draft().content().id(), content.id());
    assert_eq!(read_composer_payload(&store, storage, &current), payload);
    assert_eq!(current.content().lifecycle(), ContentLifecycle::Sealed);
    assert!(
        storage
            .draft(&store, draft, point_limit())
            .unwrap()
            .unwrap()
            .stored_bytes()
            < 512
    );

    let mut after = None;
    let mut observed_chunks = 0_u64;
    loop {
        let page = storage
            .content_chunks(
                &store,
                content.id(),
                after,
                CursorReadLimits::new(1, CONTENT_CHUNK_MAX_BYTES + 256).unwrap(),
            )
            .unwrap();
        assert!(page.records().len() <= 1);
        for chunk in page.records() {
            observed_chunks += 1;
            after = Some(chunk.ordinal());
        }
        if !page.has_more() {
            break;
        }
    }
    assert_eq!(observed_chunks, content.summary().chunk_count());

    let second_thread = id(3);
    create_thread(&store, storage, second_thread, draft_id(4));
    let second = storage
        .current_draft(&store, second_thread, point_limit())
        .unwrap()
        .unwrap();
    let update = match DraftPayloadUpdate::prepare(&second, &content, timestamp(3)).unwrap() {
        DraftPayloadUpdateDecision::Update(update) => update,
        DraftPayloadUpdateDecision::NoChange => unreachable!(),
    };
    execute(
        &store,
        storage,
        storage.update_draft_payload(storage.revision(&store).unwrap(), update),
    )
    .unwrap();
    let second = storage
        .current_draft(&store, second_thread, point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(second.draft().content(), current.draft().content());

    let duplicate = execute(
        &store,
        storage,
        storage.begin_content(
            storage.revision(&store).unwrap(),
            ContentBuild::from_prepared(&content),
        ),
    )
    .unwrap_err();
    let CommandError::ContributorValidation { source, .. } = duplicate else {
        panic!("expected content identity rejection");
    };
    assert!(matches!(
        source.downcast_ref::<SyndicMutationError>(),
        Some(SyndicMutationError::ContentIdentityCollision)
    ));
    assert!(
        ContentAppend::prepare(current.content(), &content)
            .unwrap()
            .is_none()
    );
    store.validate_registered_domains().unwrap();
    store.close().unwrap();

    let mut reopened = open(home.path());
    let storage = SyndicStorage::register(&mut reopened).unwrap();
    let current = storage
        .current_draft(&reopened, thread, point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(read_composer_payload(&reopened, storage, &current), payload);
    reopened.close().unwrap();
}

#[test]
fn accepted_and_canonical_owners_remain_small_metadata_records() {
    let home = TestHome::new("phase4-small-owners");
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    let thread = id(10);
    let draft = draft_id(11);
    let input = SyndicAcceptedInputId::from_bytes([12; 16]);
    let turn = SyndicTurnId::from_bytes([13; 16]);
    let item = SyndicItemId::from_bytes([14; 16]);
    let revision = AcceptedInputRevision::new(1).unwrap();
    let projection_revision = ProjectionRevision::new(1).unwrap();
    let ordinal = AcceptedInputOrdinal::FIRST;
    let payload =
        ComposerPayload::new(vec![ComposerAtom::text("large ".repeat(200_000)).unwrap()]).unwrap();
    let (content, content_records) = composer_content_records(&payload);
    let digest = syndic_storage::root_turn_chain_digest(turn);

    let mut records = empty_thread_records(thread, draft);
    records.retain(|record| !matches!(record, FixtureRecord::InputGate(_)));
    records.extend(content_records);
    records.extend([
        FixtureRecord::AcceptedInput(AcceptedInputRecord::new(
            input,
            thread,
            revision,
            ordinal,
            InputGateRevision::new(2).unwrap(),
            AcceptedInputDisposition::NextTurn(NextTurnReason::PendingTurn),
            AcceptedInputLifecycle::Admitted,
            content,
            0,
            timestamp(2),
        )),
        FixtureRecord::AcceptedOrder(AcceptedOrderIndexRecord::new(
            thread, ordinal, input, revision,
        )),
        FixtureRecord::AcceptedNextTurn(AcceptedNextTurnIndexRecord::new(
            thread, ordinal, input, revision,
        )),
        FixtureRecord::InputGate(
            InputGateRecord::new(
                thread,
                InputGateRevision::new(2).unwrap(),
                InputGateState::Idle,
                1,
                0,
                1,
                content.summary().logical_utf8_bytes(),
            )
            .unwrap(),
        ),
        FixtureRecord::Turn(TurnRecord::new(
            turn,
            thread,
            TurnKind::OrdinaryUser,
            syndic_storage::ConversationParent::Root,
            None,
            TurnDepth::FIRST,
            digest,
            timestamp(2),
        )),
        FixtureRecord::TurnState(
            TurnStateRecord::with_capture_frontiers(
                turn,
                TurnStateRevision::FIRST,
                TurnLifecycle::Interrupted,
                0,
                1,
                1,
                1,
                0,
                Some(
                    TurnEndStatus::new(
                        TurnTerminalOutcome::Interrupted,
                        Some(TurnIncompleteReason::ItemAuditFailed),
                    )
                    .unwrap(),
                ),
                timestamp(2),
            )
            .unwrap(),
        ),
        FixtureRecord::CanonicalItem(CanonicalItemRecord::local_user_input(
            item,
            turn,
            TurnItemOrdinal::FIRST,
            projection_revision,
            content,
            0,
        )),
        FixtureRecord::TurnItem(TurnItemIndexRecord::new(
            turn,
            TurnItemOrdinal::FIRST,
            item,
            projection_revision,
        )),
    ]);
    commit(&store, storage, batch(records));
    project_item(&store, storage, item);
    store.validate_registered_domains().unwrap();

    let accepted = storage
        .accepted_input(&store, input, point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(accepted.record().content(), content);
    assert!(accepted.stored_bytes() < 512);
    let canonical = storage
        .canonical_item(&store, item, point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(canonical.record().payload().content(), Some(content));
    assert!(canonical.stored_bytes() < 512);
    assert!(content.summary().encoded_bytes() > 1_000_000);
    store.close().unwrap();

    let mut reopened = open(home.path());
    SyndicStorage::register(&mut reopened).unwrap();
    reopened.validate_registered_domains().unwrap();
    reopened.close().unwrap();
}

#[test]
fn prepared_identity_mismatch_is_rejected_before_any_append() {
    let expected = PreparedContent::composer(
        &ComposerPayload::new(vec![ComposerAtom::text("expected").unwrap()]).unwrap(),
    )
    .unwrap();
    let other = PreparedContent::composer(
        &ComposerPayload::new(vec![ComposerAtom::text("other").unwrap()]).unwrap(),
    )
    .unwrap();
    let other_manifest = other.building_manifest();
    let forged = ContentManifestRecord::new(
        expected.id(),
        ContentRevision::new(1).unwrap(),
        other_manifest.encoding(),
        ContentLifecycle::Building,
        other_manifest.chunk_count(),
        other_manifest.encoded_bytes(),
        other_manifest.chain_digest(),
        other_manifest.expected(),
    );
    assert!(matches!(
        ContentAppend::prepare(&forged, &expected),
        Err(SyndicMutationError::ContentIdentityCollision)
    ));
}
