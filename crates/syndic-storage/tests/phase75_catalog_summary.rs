#![cfg(feature = "test-faults")]

mod support;

use beryl_home_store::{CommandError, HomeCommand};
use beryl_model::{
    AssetReferenceSetDigest, AssetReferenceSetId, ProjectionRevision, SealedAssetReferenceSetProof,
    SyndicDraftId, SyndicDraftMarkerId, SyndicItemId, SyndicThreadId,
};
use syndic_storage::test_faults::FixtureRecord;
use syndic_storage::{
    AcceptGeneratedThreadTitle, CanonicalItemRecord, ComposerAtom, ComposerPayload, ContentAppend,
    ContentBuild, CreateThread, DraftPayloadUpdate, DraftPayloadUpdateDecision,
    GeneratedThreadTitle, IdleSubmission, ImageLabelOrdinal, PreparedContent, SourceEventPayload,
    SyndicPointReadLimit, SyndicStorage, ThreadAttributesRevision, ThreadCatalogSummaryPreparation,
    TurnEndStatus, TurnItemIndexRecord, TurnItemOrdinal, TurnTerminalOutcome,
};

use support::{TestHome, batch, commit, draft_id, id, open, timestamp};

fn limit() -> SyndicPointReadLimit {
    SyndicPointReadLimit::new(1_000_000).unwrap()
}

fn execute(
    store: &beryl_home_store::HomeStore,
    contribution: beryl_home_store::MutationContribution,
) -> Result<(), CommandError> {
    let mut command = HomeCommand::new(store.home_revision().unwrap());
    command.add(contribution).unwrap();
    store.execute(command).map(|_| ())
}

fn stage_content(
    store: &beryl_home_store::HomeStore,
    storage: SyndicStorage,
    content: &PreparedContent,
) {
    execute(
        store,
        storage.begin_content(
            storage.revision(store).unwrap(),
            ContentBuild::from_prepared(content),
        ),
    )
    .unwrap();
    let mut manifest = content.building_manifest();
    while let Some(append) = ContentAppend::prepare(&manifest, content).unwrap() {
        let next = append.next_manifest().clone();
        execute(
            store,
            storage.append_content(storage.revision(store).unwrap(), append),
        )
        .unwrap();
        manifest = next;
    }
}

fn asset_proof(content: &PreparedContent, seed: u8) -> Option<SealedAssetReferenceSetProof> {
    (content.summary().image_marker_count() != 0).then(|| {
        let source = content
            .reference(beryl_model::ContentRevision::new(1).unwrap())
            .sealed_marker_summary()
            .unwrap();
        SealedAssetReferenceSetProof::new(
            AssetReferenceSetId::from_bytes([seed; 16]),
            source,
            source.marker_count(),
            AssetReferenceSetDigest::from_bytes([seed.wrapping_add(1); 32]),
        )
        .unwrap()
    })
}

fn submit_payload(
    store: &beryl_home_store::HomeStore,
    storage: SyndicStorage,
    thread: SyndicThreadId,
    next_draft: SyndicDraftId,
    item: SyndicItemId,
    payload: ComposerPayload,
    seed: u8,
    at: u64,
) {
    let prepared = PreparedContent::composer(&payload).unwrap();
    let proof = asset_proof(&prepared, seed);
    stage_content(store, storage, &prepared);
    let current = storage
        .current_draft(store, thread, limit())
        .unwrap()
        .unwrap();
    let DraftPayloadUpdateDecision::Update(update) =
        DraftPayloadUpdate::prepare(&current, &prepared, timestamp(at)).unwrap()
    else {
        panic!("test payload must change the draft")
    };
    execute(
        store,
        storage.update_draft_payload(storage.revision(store).unwrap(), update),
    )
    .unwrap();
    let current = storage
        .current_draft(store, thread, limit())
        .unwrap()
        .unwrap();
    let gate = storage.input_gate(store, thread, limit()).unwrap().unwrap();
    execute(
        store,
        storage.submit_idle_draft(
            storage.revision(store).unwrap(),
            IdleSubmission::new(
                thread,
                current.thread().revision(),
                current.draft().id(),
                current.draft().revision(),
                current.draft().content(),
                gate.revision(),
                next_draft,
                item,
                proof,
                timestamp(at + 1),
            ),
        ),
    )
    .unwrap();
}

fn title_for_payload(name: &str, payload: ComposerPayload) -> Option<String> {
    let home = TestHome::new(name);
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    let thread = id(10);
    execute(
        &store,
        storage.create_thread(
            storage.revision(&store).unwrap(),
            CreateThread::ordinary(
                thread,
                draft_id(11),
                support::exact_cas::execution_binding(),
                timestamp(1),
            ),
        ),
    )
    .unwrap();
    submit_payload(
        &store,
        storage,
        thread,
        draft_id(12),
        SyndicItemId::from_bytes([13; 16]),
        payload,
        14,
        2,
    );
    let prepared = storage
        .prepare_thread_catalog_summary(&store, thread)
        .unwrap()
        .unwrap();
    let replacement = match prepared {
        ThreadCatalogSummaryPreparation::PreparedReplacement(prepared) => prepared,
        ThreadCatalogSummaryPreparation::ExactCurrent(_) => {
            panic!("submitted history must stale the initial summary")
        }
    };
    let expected = replacement.replacement().title().map(|title| {
        assert_eq!(
            title.source(),
            syndic_storage::ThreadCatalogTitleSource::HistoryDerived
        );
        title.text().to_owned()
    });
    execute(&store, storage.rebuild_thread_catalog_summary(replacement)).unwrap();
    let revision = storage.revision(&store).unwrap();
    let exact = storage
        .prepare_thread_catalog_summary(&store, thread)
        .unwrap()
        .unwrap();
    let ThreadCatalogSummaryPreparation::ExactCurrent(exact) = exact else {
        panic!("rebuilt summary must prepare as an exact no-op")
    };
    assert_eq!(storage.revision(&store).unwrap(), revision);
    assert_eq!(
        exact.summary().title().map(|title| title.text()),
        expected.as_deref()
    );
    store.validate_registered_domains().unwrap();
    expected
}

#[test]
fn logical_lines_controls_and_unicode_whitespace_are_normalized_exactly() {
    let title = title_for_payload(
        "phase75-title-lines-controls",
        ComposerPayload::new(vec![
            ComposerAtom::text("---\r\n\tFirst\u{2003}\u{00a0}line\u{0007} \rbare").unwrap(),
        ])
        .unwrap(),
    );
    assert_eq!(title.as_deref(), Some(" First line bare"));
}

#[test]
fn image_markers_are_zero_width_even_across_text_piece_boundaries() {
    let title = title_for_payload(
        "phase75-title-markers",
        ComposerPayload::new(vec![
            ComposerAtom::text("Mark").unwrap(),
            ComposerAtom::image_marker(
                SyndicDraftMarkerId::from_bytes([21; 16]),
                ImageLabelOrdinal::FIRST,
            ),
            ComposerAtom::text("er title").unwrap(),
        ])
        .unwrap(),
    );
    assert_eq!(title.as_deref(), Some("Marker title"));
}

#[test]
fn scalar_utf8_and_scan_limits_stop_without_an_ellipsis() {
    let source = format!("{}界tail", "a".repeat(79));
    let title = title_for_payload(
        "phase75-title-scalar-limit",
        ComposerPayload::new(vec![ComposerAtom::text(source).unwrap()]).unwrap(),
    )
    .unwrap();
    assert_eq!(title.chars().count(), 80);
    assert_eq!(title, format!("{}界", "a".repeat(79)));

    let outside = format!("{}\nOutside", ".".repeat(4_096));
    assert_eq!(
        title_for_payload(
            "phase75-title-scan-limit",
            ComposerPayload::new(vec![ComposerAtom::text(outside).unwrap()]).unwrap(),
        ),
        None
    );
}

#[test]
fn truncated_history_title_may_lack_an_alphanumeric_character() {
    let source = format!("{}A", ".".repeat(80));
    let title = title_for_payload(
        "phase75-title-truncated-prefix",
        ComposerPayload::new(vec![ComposerAtom::text(source).unwrap()]).unwrap(),
    )
    .unwrap();
    assert_eq!(title, ".".repeat(80));
    assert!(!title.chars().any(char::is_alphanumeric));
}

#[test]
fn generated_title_precedes_history_and_invalidates_an_older_preparation() {
    let home = TestHome::new("phase75-generated-precedence");
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    let thread = id(30);
    execute(
        &store,
        storage.create_thread(
            storage.revision(&store).unwrap(),
            CreateThread::ordinary(
                thread,
                draft_id(31),
                support::exact_cas::execution_binding(),
                timestamp(1),
            ),
        ),
    )
    .unwrap();
    let item = SyndicItemId::from_bytes([32; 16]);
    let turn = support::exact_cas::submit_current_draft(
        &store,
        storage,
        thread,
        draft_id(33),
        item,
        "history title",
        timestamp(2),
    );
    let stale = match storage
        .prepare_thread_catalog_summary(&store, thread)
        .unwrap()
        .unwrap()
    {
        ThreadCatalogSummaryPreparation::PreparedReplacement(prepared) => prepared,
        ThreadCatalogSummaryPreparation::ExactCurrent(_) => panic!("history must stale catalog"),
    };
    let content = storage
        .canonical_item(&store, item, limit())
        .unwrap()
        .unwrap()
        .presentation_content()
        .unwrap();
    let current_thread = storage.thread(&store, thread, limit()).unwrap().unwrap();
    let generated = GeneratedThreadTitle::new(
        "Generated winner",
        turn,
        content,
        current_thread.selected_path_digest(),
        current_thread.revision(),
        timestamp(3),
    )
    .unwrap();
    execute(
        &store,
        storage.accept_generated_thread_title(
            storage.revision(&store).unwrap(),
            AcceptGeneratedThreadTitle::new(thread, ThreadAttributesRevision::FIRST, generated),
        ),
    )
    .unwrap();
    assert!(execute(&store, storage.rebuild_thread_catalog_summary(stale),).is_err());
    let prepared = match storage
        .prepare_thread_catalog_summary(&store, thread)
        .unwrap()
        .unwrap()
    {
        ThreadCatalogSummaryPreparation::PreparedReplacement(prepared) => prepared,
        ThreadCatalogSummaryPreparation::ExactCurrent(_) => panic!("attributes must stale catalog"),
    };
    let title = prepared.replacement().title().unwrap();
    assert_eq!(title.text(), "Generated winner");
    assert_eq!(
        title.source(),
        syndic_storage::ThreadCatalogTitleSource::Generated
    );
    execute(&store, storage.rebuild_thread_catalog_summary(prepared)).unwrap();
    store.validate_registered_domains().unwrap();
}

#[test]
fn reopen_accepts_a_valid_stale_summary_then_prepares_its_exact_rebuild() {
    let home = TestHome::new("phase75-stale-reopen");
    let thread = id(40);
    {
        let mut store = open(home.path());
        let storage = SyndicStorage::register(&mut store).unwrap();
        execute(
            &store,
            storage.create_thread(
                storage.revision(&store).unwrap(),
                CreateThread::ordinary(
                    thread,
                    draft_id(41),
                    support::exact_cas::execution_binding(),
                    timestamp(1),
                ),
            ),
        )
        .unwrap();
        support::exact_cas::submit_current_draft(
            &store,
            storage,
            thread,
            draft_id(42),
            SyndicItemId::from_bytes([43; 16]),
            "stale but valid",
            timestamp(2),
        );
        store.close().unwrap();
    }
    let mut reopened = open(home.path());
    let storage = SyndicStorage::register(&mut reopened).unwrap();
    assert!(matches!(
        storage
            .prepare_thread_catalog_summary(&reopened, thread)
            .unwrap()
            .unwrap(),
        ThreadCatalogSummaryPreparation::PreparedReplacement(_)
    ));
    reopened.validate_registered_domains().unwrap();
}

#[test]
fn from_tail_creation_publishes_the_entire_selected_path_fallback_immediately() {
    let home = TestHome::new("phase75-from-tail-title");
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    let source_thread = id(50);
    execute(
        &store,
        storage.create_thread(
            storage.revision(&store).unwrap(),
            CreateThread::ordinary(
                source_thread,
                draft_id(51),
                support::exact_cas::execution_binding(),
                timestamp(1),
            ),
        ),
    )
    .unwrap();
    let item = SyndicItemId::from_bytes([52; 16]);
    let turn = support::exact_cas::submit_current_draft(
        &store,
        storage,
        source_thread,
        draft_id(53),
        item,
        "Inherited fallback",
        timestamp(2),
    );
    let source =
        support::exact_cas::establish_turn(&store, storage, source_thread, turn, timestamp(3));
    support::exact_cas::admit_event(
        &store,
        storage,
        source_thread,
        turn,
        &source,
        SourceEventPayload::TurnActivated,
        timestamp(3),
    );
    support::exact_cas::correlate_user_item(
        &store,
        storage,
        source_thread,
        turn,
        item,
        &source,
        timestamp(4),
    );
    support::exact_cas::admit_event(
        &store,
        storage,
        source_thread,
        turn,
        &source,
        SourceEventPayload::TurnEnded(
            TurnEndStatus::new(TurnTerminalOutcome::Complete, None).unwrap(),
        ),
        timestamp(5),
    );
    support::converge_and_release_terminal_history(&store, storage, source_thread, turn);
    let tail = storage
        .thread_tail(&store, source_thread, limit())
        .unwrap()
        .unwrap();
    assert_eq!(
        tail.entire_selected_path_title().unwrap().text(),
        "Inherited fallback"
    );
    let child = id(54);
    execute(
        &store,
        storage.create_thread(
            storage.revision(&store).unwrap(),
            CreateThread::from_tail(child, draft_id(55), timestamp(10), tail).unwrap(),
        ),
    )
    .unwrap();
    let prepared = storage
        .prepare_thread_catalog_summary(&store, child)
        .unwrap()
        .unwrap();
    let ThreadCatalogSummaryPreparation::ExactCurrent(exact) = prepared else {
        panic!("from-tail creation must publish its current fallback")
    };
    assert_eq!(
        exact.summary().title().unwrap().text(),
        "Inherited fallback"
    );
    assert_eq!(
        exact.summary().sources().history_summary_revision(),
        ProjectionRevision::new(1).unwrap()
    );
    store.validate_registered_domains().unwrap();
}

#[test]
fn branch_discussion_uses_branch_local_input_instead_of_inherited_history() {
    let home = TestHome::new("phase75-branch-local-title");
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    commit(
        &store,
        storage,
        batch(support::populated::populated_records()),
    );

    // Extend the inherited fixture root with a real canonical user input. The source-tail read
    // proves that the entire-path builder sees it, while the branch builder must exclude it by
    // origin-thread ownership.
    let inherited_turn = beryl_model::SyndicTurnId::from_bytes([29; 16]);
    let inherited_item = SyndicItemId::from_bytes([60; 16]);
    let inherited_payload =
        ComposerPayload::new(vec![ComposerAtom::text("Inherited title").unwrap()]).unwrap();
    let (inherited_content, mut inherited_records) =
        support::composer_content_records(&inherited_payload);
    inherited_records.extend([
        FixtureRecord::CanonicalItem(CanonicalItemRecord::local_user_input(
            inherited_item,
            inherited_turn,
            TurnItemOrdinal::FIRST,
            ProjectionRevision::new(1).unwrap(),
            inherited_content,
            None,
        )),
        FixtureRecord::TurnItem(TurnItemIndexRecord::new(
            inherited_turn,
            TurnItemOrdinal::FIRST,
            inherited_item,
            ProjectionRevision::new(1).unwrap(),
        )),
    ]);
    commit(&store, storage, batch(inherited_records));
    assert_eq!(
        storage
            .thread_tail(&store, id(30), limit())
            .unwrap()
            .unwrap()
            .entire_selected_path_title()
            .unwrap()
            .text(),
        "Inherited title"
    );

    let branch = id(36);
    submit_payload(
        &store,
        storage,
        branch,
        draft_id(241),
        SyndicItemId::from_bytes([242; 16]),
        ComposerPayload::new(vec![ComposerAtom::text("Branch local title").unwrap()]).unwrap(),
        243,
        6,
    );
    let prepared = storage
        .prepare_thread_catalog_summary(&store, branch)
        .unwrap()
        .unwrap();
    let ThreadCatalogSummaryPreparation::PreparedReplacement(prepared) = prepared else {
        panic!("first branch-local input must stale the branch summary")
    };
    assert_eq!(
        prepared.replacement().title().unwrap().text(),
        "Branch local title"
    );
}
