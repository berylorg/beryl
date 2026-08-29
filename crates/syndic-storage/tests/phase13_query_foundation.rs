#![cfg(feature = "test-faults")]

mod support;

use beryl_home_store::{CommandOutcome, CursorReadLimits, HomeCommand, HomeStore};
use beryl_model::{
    AssetReferenceSetDigest, AssetReferenceSetId, SealedAssetReferenceSetProof, SyndicDraftId,
    SyndicItemId, SyndicThreadId, ThreadRevision,
};
use syndic_storage::test_faults::{FixtureBatch, FixtureRecord};
use syndic_storage::*;

use support::{TestHome, batch, commit, open, timestamp};

fn identity(value: u128) -> SyndicThreadId {
    SyndicThreadId::from_bytes(value.to_be_bytes())
}

fn draft_identity(value: u128) -> SyndicDraftId {
    SyndicDraftId::from_bytes(value.to_be_bytes())
}

fn point_limit() -> SyndicPointReadLimit {
    SyndicPointReadLimit::new(1 << 20).unwrap()
}

fn page_limits(items: usize) -> CursorReadLimits {
    CursorReadLimits::new(items, QUERY_PAGE_MAX_STORED_BYTES).unwrap()
}

fn execute(store: &HomeStore, contribution: beryl_home_store::MutationContribution) {
    let mut command = HomeCommand::new(store.home_revision().unwrap());
    command.add(contribution).unwrap();
    match store.execute(command) {
        CommandOutcome::Committed {
            later_failure: None,
            ..
        } => {}
        outcome => panic!("expected clean phase13 fixture command, got {outcome:?}"),
    }
}

fn submit_image_draft(
    store: &HomeStore,
    storage: SyndicStorage,
    thread: SyndicThreadId,
    next_draft: SyndicDraftId,
    item: SyndicItemId,
    payload: ComposerPayload,
    asset_reference_set: SealedAssetReferenceSetProof,
) {
    let prepared = PreparedContent::composer(&payload).unwrap();
    support::exact_cas::submit_prepared_current_draft(
        store,
        storage,
        thread,
        next_draft,
        item,
        &prepared,
        Some(asset_reference_set),
        timestamp(4),
    );
}

#[test]
fn deep_thread_lineage_is_top_to_bottom_fixed_page_and_revision_bound() {
    const DEPTH: u64 = 300;
    let home = TestHome::new("phase13-deep-thread-lineage");
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    let mut records: Vec<ThreadRecord> = Vec::new();
    let mut fixture_batch = FixtureBatch::new();

    for depth in 1..=DEPTH {
        let thread_id = identity(u128::from(depth));
        let parent = (depth > 1).then(|| identity(u128::from(depth - 1)));
        let digest = match parent {
            None => root_thread_lineage_digest(thread_id),
            Some(parent_id) => child_thread_lineage_digest(
                thread_id,
                parent_id,
                records[usize::try_from(depth - 2).unwrap()].lineage_digest(),
            ),
        };
        let skip = (depth > 1).then(|| {
            let target = (depth & (depth - 1)).max(1);
            records[usize::try_from(target - 1).unwrap()].id()
        });
        let record = ThreadRecord::new(
            thread_id,
            SelectedPathProof::new(
                None,
                ThreadRevision::new(1).unwrap(),
                empty_selected_path_digest(),
            ),
            draft_identity(u128::from(depth)),
            ThreadLineageProof::new(
                parent,
                skip,
                ThreadLineageDepth::new(depth).unwrap(),
                digest,
            ),
            None,
        );
        fixture_batch
            .put(FixtureRecord::Thread(record.clone()))
            .unwrap();
        records.push(record);
    }
    commit(&store, storage.clone(), fixture_batch);

    let head = storage
        .clone()
        .thread_lineage_head(&store, identity(DEPTH.into()), point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(head.total_parent_count(), DEPTH - 1);
    let first_cursor = head.cursor().unwrap();
    let measured_first = storage
        .clone()
        .thread_lineage_page(&store, &head, first_cursor, page_limits(1))
        .unwrap();
    let root_stored_bytes = measured_first.stored_bytes();
    let exact_first = storage
        .clone()
        .thread_lineage_page(
            &store,
            &head,
            first_cursor,
            CursorReadLimits::new(1, root_stored_bytes).unwrap(),
        )
        .unwrap();
    assert_eq!(exact_first.records().len(), 1);
    assert_eq!(exact_first.stored_bytes(), root_stored_bytes);
    assert!(
        storage
            .clone()
            .thread_lineage_page(
                &store,
                &head,
                first_cursor,
                CursorReadLimits::new(1, root_stored_bytes - 1).unwrap(),
            )
            .is_err()
    );
    let mut cursor = head.cursor();
    let mut observed = Vec::new();
    while let Some(current) = cursor {
        let page = storage
            .clone()
            .thread_lineage_page(&store, &head, current, page_limits(1_000))
            .unwrap();
        assert!(page.records().len() <= QUERY_PAGE_MAX_RECORDS);
        observed.extend(page.records().iter().map(|entry| entry.thread_id()));
        cursor = page.next_cursor();
    }
    assert_eq!(observed.len(), usize::try_from(DEPTH - 1).unwrap());
    assert_eq!(observed.first(), Some(&identity(1)));
    assert_eq!(observed.last(), Some(&identity((DEPTH - 1).into())));

    let stale_cursor = head.cursor().unwrap();
    let leaf = records.last().unwrap();
    let replacement = ThreadRecord::new(
        leaf.id(),
        SelectedPathProof::new(
            leaf.committed_tail(),
            ThreadRevision::new(2).unwrap(),
            leaf.selected_path_digest(),
        ),
        leaf.current_draft_id(),
        leaf.lineage(),
        leaf.context_owner_id(),
    );
    commit(
        &store,
        storage.clone(),
        batch([FixtureRecord::Thread(replacement)]),
    );
    assert!(matches!(
        storage
            .clone()
            .thread_lineage_page(&store, &head, stale_cursor, page_limits(4)),
        Err(SyndicReadError::StaleThreadLineage)
    ));
}

#[test]
fn inherited_label_origin_and_activity_pages_keep_compact_revision_authority() {
    let home = TestHome::new("phase13-label-activity-query");
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    let parent_id = support::id(30);
    let child_id = support::id(36);
    support::seed_populated(&store, storage.clone());
    support::converge_and_release_terminal_history(
        &store,
        storage.clone(),
        parent_id,
        support::populated::source_turn(),
    );
    let marker_id = beryl_model::SyndicDraftMarkerId::from_bytes([88; 16]);
    let gap_label = ImageLabelOrdinal::FIRST;
    let label = ImageLabelOrdinal::new(37).unwrap();
    let payload = ComposerPayload::new(vec![ComposerAtom::image_marker(marker_id, label)]).unwrap();
    let source = PreparedContent::composer(&payload)
        .unwrap()
        .reference(beryl_model::ContentRevision::new(1).unwrap())
        .sealed_marker_summary()
        .unwrap();
    let proof = SealedAssetReferenceSetProof::new(
        AssetReferenceSetId::from_bytes([89; 16]),
        source.sequential(),
        beryl_model::OrderedMarkerAssetSummaryV1::new([91; 32], source.sequential().marker_count()),
        source.sequential().marker_count(),
        AssetReferenceSetDigest::from_bytes([90; 32]),
    )
    .unwrap();
    let item = SyndicItemId::from_bytes([91; 16]);
    submit_image_draft(
        &store,
        storage.clone(),
        parent_id,
        draft_identity(92),
        item,
        payload,
        proof,
    );
    let parent = storage
        .clone()
        .thread(&store, parent_id, point_limit())
        .unwrap()
        .unwrap();
    let child = storage
        .clone()
        .thread(&store, child_id, point_limit())
        .unwrap()
        .unwrap();
    let parent = ThreadRecord::new(
        parent.id(),
        parent.selected_path(),
        parent.current_draft_id(),
        parent.lineage(),
        parent.context_owner_id(),
    );
    let child = ThreadRecord::new(
        child.id(),
        child.selected_path(),
        child.current_draft_id(),
        child.lineage(),
        child.context_owner_id(),
    );
    let origin = ImageLabelOriginSpanRecord::new(
        parent_id,
        gap_label,
        label,
        ImageLabelOriginOwner::CanonicalItem(item),
        proof,
    )
    .unwrap();
    commit(
        &store,
        storage.clone(),
        batch([
            FixtureRecord::Thread(parent),
            FixtureRecord::ImageLabelAuthorityHead(
                ImageLabelAuthorityHeadV1::new(
                    parent_id,
                    2,
                    ImageLabelFrontier::EMPTY,
                    ImageLabelFrontier::from_raw(label.get()),
                )
                .unwrap(),
            ),
            FixtureRecord::Thread(child),
            FixtureRecord::ImageLabelAuthorityHead(
                ImageLabelAuthorityHeadV1::new(
                    child_id,
                    2,
                    ImageLabelFrontier::from_raw(label.get()),
                    ImageLabelFrontier::from_raw(label.get()),
                )
                .unwrap(),
            ),
            FixtureRecord::ImageLabelOriginSpan(origin),
        ]),
    );
    let resolved = storage
        .clone()
        .resolve_image_label_origin_span(&store, child_id, gap_label, point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(resolved.span().thread_id(), parent_id);
    assert_eq!(resolved.origin_depth(), ThreadLineageDepth::FIRST);
    assert_eq!(resolved.span().asset_reference_set(), proof);
    assert!(resolved.span().contains(gap_label));
    let current = storage
        .clone()
        .resolve_image_label_origin_span(&store, parent_id, label, point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(current.span().thread_id(), parent_id);
    assert_eq!(current.origin_depth(), ThreadLineageDepth::FIRST);

    let thread = support::id(40);
    let head = storage
        .clone()
        .activity_query_head(&store, thread, point_limit())
        .unwrap();
    let head = head.unwrap().clone();
    let page = storage
        .clone()
        .activity_query_page(&store, &head, None, page_limits(1))
        .unwrap();
    assert_eq!(page.records().len(), 1);
    assert_eq!(
        page.records()[0].item_id(),
        support::populated::activity_item()
    );
    assert!(page.next_cursor().is_none());
    let revised = ActivityQueryHeadRecord::new(
        head.thread_id(),
        head.work_period(),
        head.source(),
        head.source_active(),
        head.source_frontier(),
        head.revision().checked_next().unwrap(),
        head.source_count(),
        head.logical_row_count(),
        head.running_row_count(),
        head.completed_row_count(),
        head.completed_stored_bytes(),
        head.completed_retention_cutoff(),
        head.lifecycle(),
    )
    .unwrap();
    commit(
        &store,
        storage.clone(),
        batch([FixtureRecord::ActivityQueryHead(revised)]),
    );
    assert!(matches!(
        storage.activity_query_page(&store, &head, None, page_limits(1)),
        Err(SyndicReadError::StaleActivityQuery)
    ));
}
