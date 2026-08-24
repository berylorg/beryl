use super::*;

fn begin_empty_edit(
    host: &mut SyndicComposerHost,
    store: &beryl_home_store::HomeStore,
    binding: ComposerHostBinding,
    operation: u64,
) -> MutationKey {
    let key = mutation_key(binding, operation);
    let zero = source_position(0);
    host.begin_mutation(
        store,
        binding,
        MutationBeginRequest::new(
            MutationProposal::new(
                key,
                MutationKind::Edit,
                MutationPositions::collapsed(zero),
                range(zero, zero),
                0,
            ),
            MutationCursor::new(0),
            MutationCursor::new(0),
        ),
    )
    .unwrap();
    key
}

fn initial_page(
    key: MutationKey,
    lane: MutationLane,
    items: Vec<MutationPageItem>,
) -> MutationPage {
    MutationPage::new(
        MutationPageKey::new(key, lane, MutationCursor::new(0), 0, MutationIdentity::ROOT),
        MutationCursor::new(1),
        items,
    )
    .unwrap()
}

#[test]
fn source_page_rejects_marker_metadata_before_frontier_or_storage_effect() {
    let (_home, store, storage, thread) = fixture("phase155-source-marker-metadata", 231);
    let (mut host, binding) = activated(storage, &store, thread, 232, 233);
    let key = begin_empty_edit(&mut host, &store, binding, 234);
    let page = initial_page(
        key,
        MutationLane::Source,
        vec![MutationPageItem::Utf8 {
            inserted_offset: 0,
            text: "source".into(),
        }],
    );
    let object_id = InlineObjectId::new(235);
    let label = ImageLabelOrdinal::new(1).unwrap();
    let asset_id = asset_id_for_object(object_id);

    assert!(matches!(
        host.stage_mutation_page(
            &store,
            MutationPageRequest::new(page.clone()),
            Box::new([ComposerHostImageMarkerMetadata::new(
                object_id, label, asset_id,
            )]),
        ),
        Err(ComposerHostError::MutationMalformed)
    ));
    let head = storage
        .draft_mutation_staging_head(&store, staging_identity(binding, 234))
        .unwrap()
        .unwrap();
    assert_eq!(head.source().next_cursor(), 0);
    assert_eq!(head.source().item_total(), 0);

    assert!(matches!(
        host.stage_mutation_page(&store, MutationPageRequest::new(page), Box::new([])),
        Ok(MutationPageAcceptance::Accepted { .. })
    ));
}

#[test]
fn mixed_proposal_requires_exact_insert_metadata_count_before_admission() {
    let (_home, store, storage, thread) = fixture("phase155-mixed-marker-metadata", 241);
    let (mut host, binding) = activated(storage, &store, thread, 242, 243);
    let key = begin_empty_edit(&mut host, &store, binding, 244);
    let object_id = InlineObjectId::new(245);
    let extra_id = InlineObjectId::new(246);
    let label = ImageLabelOrdinal::new(2).unwrap();
    let asset_id = asset_id_for_object(object_id);
    let page = initial_page(
        key,
        MutationLane::Proposal,
        vec![
            MutationPageItem::Utf8 {
                inserted_offset: 0,
                text: "a".into(),
            },
            MutationPageItem::Object(ObjectChange::Insert {
                object: SuccessorObject::new(
                    object_id,
                    ByteOffset::new(1),
                    InlineObjectOrder::new(1),
                    17,
                    5,
                ),
            }),
            MutationPageItem::Utf8 {
                inserted_offset: 1,
                text: "b".into(),
            },
        ],
    );
    let exact = ComposerHostImageMarkerMetadata::new(object_id, label, asset_id);

    assert!(matches!(
        host.stage_mutation_page(&store, MutationPageRequest::new(page.clone()), Box::new([]),),
        Err(ComposerHostError::MutationMalformed)
    ));

    assert!(matches!(
        host.stage_mutation_page(
            &store,
            MutationPageRequest::new(page.clone()),
            Box::new([
                exact,
                ComposerHostImageMarkerMetadata::new(
                    extra_id,
                    label,
                    asset_id_for_object(extra_id),
                ),
            ]),
        ),
        Err(ComposerHostError::MutationMalformed)
    ));
    let head = storage
        .draft_mutation_staging_head(&store, staging_identity(binding, 244))
        .unwrap()
        .unwrap();
    assert_eq!(head.proposal().next_cursor(), 0);
    assert_eq!(head.proposal().item_total(), 0);

    assert!(matches!(
        host.stage_mutation_page(&store, MutationPageRequest::new(page), Box::new([exact]),),
        Ok(MutationPageAcceptance::Accepted { .. })
    ));
    let admitted = storage
        .draft_mutation_staging_head(&store, staging_identity(binding, 244))
        .unwrap()
        .unwrap();
    assert_eq!(admitted.proposal().next_cursor(), 3);
    assert_eq!(admitted.proposal().item_total(), 3);
}

#[test]
fn proposal_rejects_duplicate_marker_metadata_identity_before_admission() {
    let (_home, store, storage, thread) = fixture("phase169-duplicate-marker-metadata", 247);
    let (mut host, binding) = activated(storage, &store, thread, 248, 249);
    let key = begin_empty_edit(&mut host, &store, binding, 250);
    let first_id = InlineObjectId::new(251);
    let second_id = InlineObjectId::new(252);
    let first_label = ImageLabelOrdinal::new(1).unwrap();
    let second_label = ImageLabelOrdinal::new(2).unwrap();
    let page = initial_page(
        key,
        MutationLane::Proposal,
        vec![
            MutationPageItem::Object(ObjectChange::Insert {
                object: SuccessorObject::new(
                    first_id,
                    ByteOffset::new(0),
                    InlineObjectOrder::new(1),
                    17,
                    5,
                ),
            }),
            MutationPageItem::Object(ObjectChange::Insert {
                object: SuccessorObject::new(
                    second_id,
                    ByteOffset::new(0),
                    InlineObjectOrder::new(2),
                    17,
                    5,
                ),
            }),
        ],
    );

    assert!(matches!(
        host.stage_mutation_page(
            &store,
            MutationPageRequest::new(page.clone()),
            Box::new([
                ComposerHostImageMarkerMetadata::new(
                    first_id,
                    first_label,
                    asset_id_for_object(first_id),
                ),
                ComposerHostImageMarkerMetadata::new(
                    first_id,
                    second_label,
                    asset_id_for_object(second_id),
                ),
            ]),
        ),
        Err(ComposerHostError::MutationMalformed)
    ));
    let head = storage
        .draft_mutation_staging_head(&store, staging_identity(binding, 250))
        .unwrap()
        .unwrap();
    assert_eq!(head.proposal().next_cursor(), 0);
    assert_eq!(head.proposal().item_total(), 0);

    assert!(matches!(
        host.stage_mutation_page(
            &store,
            MutationPageRequest::new(page),
            Box::new([
                ComposerHostImageMarkerMetadata::new(
                    first_id,
                    first_label,
                    asset_id_for_object(first_id),
                ),
                ComposerHostImageMarkerMetadata::new(
                    second_id,
                    second_label,
                    asset_id_for_object(second_id),
                ),
            ]),
        ),
        Ok(MutationPageAcceptance::Accepted { .. })
    ));
}
