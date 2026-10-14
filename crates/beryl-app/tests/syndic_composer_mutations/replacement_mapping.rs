use super::*;
use beryl_app::composer_host::{
    ComposerHostMutationAdmissionFailure, ComposerHostMutationEvidenceOutcome,
    ComposerHostMutationEvidenceRequest,
};
use beryl_state::AssetState;
use gpui_text_input::{
    MutationLimits, MutationPass, MutationPositionsIntent, MutationProducerIdentity,
    RangeEditCoordinator,
};

#[path = "../composer_marker_evidence/support.rs"]
mod evidence_fixture;

#[path = "replacement_lifecycle.rs"]
mod replacement_lifecycle;

#[path = "local_producer_closure.rs"]
mod local_producer_closure;

#[path = "predecessor_reads.rs"]
mod predecessor_reads;

fn insert_boundary(
    host: &mut SyndicComposerHost,
    store: &beryl_home_store::HomeStore,
    binding: ComposerHostBinding,
    operation: u64,
    anchor: u64,
    object: InlineObjectNeighbor,
    label: u64,
) -> ComposerHostBinding {
    commit_items(
        host,
        store,
        binding,
        operation,
        range(source_position(anchor), source_position(anchor)),
        vec![MutationPageItem::Object(ObjectChange::Insert {
            object: SuccessorObject::new(
                object.id(),
                ByteOffset::new(anchor),
                object.order(),
                17,
                5,
            ),
        })],
        MutationPositions::collapsed(SourcePosition::new(
            ByteOffset::new(anchor),
            InlineObjectGap::after(object),
        )),
        vec![labeled_marker_metadata(
            object.id(),
            ImageLabelOrdinal::new(label).unwrap(),
            asset_id_for_object(object.id()),
        )],
        binding.logical_extent().logical_utf8_bytes(),
        1,
    )
}

fn drive(
    host: &mut SyndicComposerHost,
    store: &beryl_home_store::HomeStore,
    binding: ComposerHostBinding,
    assets: &AssetState,
    mut request: ComposerHostMutationEvidenceRequest,
) -> ComposerHostMutationEvidenceOutcome {
    let key = request.key();
    for _ in 0..64 {
        let result = host
            .dispatch_mutation_evidence(
                store,
                binding,
                assets,
                request,
                &CommandCancellation::new(),
            )
            .unwrap();
        if !matches!(result, ComposerHostMutationEvidenceOutcome::Pending(_)) {
            return result;
        }
        request = ComposerHostMutationEvidenceRequest::Advance(key);
    }
    panic!("replacement evidence exceeded the bounded drive budget")
}

fn prepare(
    host: &mut SyndicComposerHost,
    store: &beryl_home_store::HomeStore,
    binding: ComposerHostBinding,
    assets: &AssetState,
    replacement: SourceRange,
    inserted: &str,
) -> (
    RangeEditCoordinator,
    MutationPass,
    MutationPage,
    MutationFinishInput,
) {
    prepare_with_removals(host, store, binding, assets, replacement, inserted, &[])
}

fn prepare_with_removals(
    host: &mut SyndicComposerHost,
    store: &beryl_home_store::HomeStore,
    binding: ComposerHostBinding,
    assets: &AssetState,
    replacement: SourceRange,
    inserted: &str,
    removals: &[ObjectTarget],
) -> (
    RangeEditCoordinator,
    MutationPass,
    MutationPage,
    MutationFinishInput,
) {
    let key = mutation_key(binding, 2216);
    let begin = MutationBeginRequest::new(
        MutationProposal::new(
            key,
            MutationKind::Edit,
            MutationPositions::collapsed(replacement.start()),
            replacement,
            0,
        ),
        MutationCursor::new(0),
        MutationCursor::new(0),
    )
    .with_replayable_producer(MutationProducerIdentity::new(12216));
    let mut editor = RangeEditCoordinator::new(
        binding.range_binding(),
        MutationLimits::new(16, 65_536)
            .unwrap()
            .with_object_limits(16, 65_536, 65_536)
            .unwrap(),
    );
    editor.begin(begin).unwrap();
    let pass = editor.request_evidence(key).unwrap();
    assert!(
        matches!(drive(host, store, binding, assets, ComposerHostMutationEvidenceRequest::Begin { begin, pass }),
        ComposerHostMutationEvidenceOutcome::Started(actual) if actual == pass)
    );
    let source_items: Vec<_> = removals
        .iter()
        .map(|target| MutationPageItem::Object(ObjectChange::Remove { target: *target }))
        .collect();
    if !source_items.is_empty() {
        let source = MutationPage::new(
            MutationPageKey::new(
                key,
                MutationLane::Source,
                MutationCursor::new(0),
                0,
                MutationIdentity::ROOT,
            ),
            MutationCursor::new(1),
            source_items.clone(),
        )
        .unwrap();
        let acknowledgement = editor.submit_evidence_page(pass, source.clone()).unwrap();
        assert!(
            matches!(drive(host, store, binding, assets, ComposerHostMutationEvidenceRequest::Page { pass, page: source, metadata: Box::new([]) }),
            ComposerHostMutationEvidenceOutcome::PageAccepted(actual) if actual == pass)
        );
        editor.acknowledge_evidence_page(acknowledgement).unwrap();
    }
    let source_finish = editor.stream_finish(key, MutationLane::Source).unwrap();
    let mut proposal_items = source_items;
    proposal_items.push(MutationPageItem::Utf8 {
        inserted_offset: 0,
        text: inserted.into(),
    });
    let page = MutationPage::new(
        MutationPageKey::new(
            key,
            MutationLane::Proposal,
            MutationCursor::new(0),
            0,
            MutationIdentity::ROOT,
        ),
        MutationCursor::new(1),
        proposal_items,
    )
    .unwrap();
    let acknowledgement = editor.submit_evidence_page(pass, page.clone()).unwrap();
    assert!(
        matches!(drive(host, store, binding, assets, ComposerHostMutationEvidenceRequest::Page {
        pass, page: page.clone(), metadata: Box::new([]),
    }), ComposerHostMutationEvidenceOutcome::PageAccepted(actual) if actual == pass)
    );
    editor.acknowledge_evidence_page(acknowledgement).unwrap();
    let finish = MutationFinishInput::with_positions_intent(
        key,
        source_finish,
        MutationStreamFinish {
            next_cursor: page.next_cursor(),
            next_ordinal: 1,
            cumulative_identity: page.cumulative_identity(),
            totals: page.totals(),
        },
        LogicalExtent::new(2 + inserted.len() as u64, 1),
        MutationPositionsIntent::ReplacementEnd {
            replacement,
            inserted_utf8_bytes: inserted.len() as u64,
        },
    );
    editor.finish_evidence(pass, finish).unwrap();
    (editor, pass, page, finish)
}

#[test]
fn mapped_replacement_authenticates_survivors_and_preserves_representable_keys() {
    for (left_order, right_order, inserted) in [(2, 9, ""), (9, 2, "Q"), (9, 2, "é")] {
        eprintln!("mapped replacement keys={left_order}/{right_order} insertion={inserted:?}");
        let fixture = evidence_fixture::fixture("mapped-replacement", 211);
        let store = &fixture.store;
        let storage = fixture.storage.clone();
        let thread = fixture.thread;
        let assets = fixture.assets.clone();
        let (mut host, base) = activated(storage.clone(), &store, thread, 212, 213);
        let text = commit_text(&mut host, &store, base, 214, 0, 0, "ABCD", 4, 1);
        let left = InlineObjectNeighbor::new(
            InlineObjectId::new(0xb101),
            InlineObjectOrder::new(left_order),
        );
        let right = InlineObjectNeighbor::new(
            InlineObjectId::new(0xb102),
            InlineObjectOrder::new(right_order),
        );
        let marked = insert_boundary(&mut host, &store, text, 2141, 1, left, 1);
        let marked = insert_boundary(&mut host, &store, marked, 2142, 3, right, 2);
        let selected = range(
            SourcePosition::new(ByteOffset::new(1), InlineObjectGap::after(left)),
            SourcePosition::new(ByteOffset::new(3), InlineObjectGap::before(right)),
        );
        let (mut editor, pass, page, finish) =
            prepare(&mut host, &store, marked, &assets, selected, inserted);
        assert!(
            matches!(drive(&mut host, &store, marked, &assets, ComposerHostMutationEvidenceRequest::Finish { pass, finish }),
            ComposerHostMutationEvidenceOutcome::Began(actual) if actual == finish.key())
        );
        editor.accept_preflight(finish.key(), None).unwrap();
        let staging = editor.mutation_restart(finish.key()).unwrap();
        editor.acknowledge_restart(staging).unwrap();
        let request = MutationPageRequest::new(page).with_pass(staging);
        host.stage_mutation_page(&store, request.clone(), Box::new([]))
            .unwrap();
        assert_eq!(
            host.stage_mutation_page(&store, request, Box::new([]))
                .unwrap(),
            MutationPageAcceptance::Replay
        );
        if inserted == "Q" {
            host.test_set_mutation_transition_limit(1);
            host.test_arm_mutation_before_execute_fault(|store, storage| {
                support::bump_home_revision(storage.clone(), store, 244);
            });
            assert!(matches!(
                host.finish_mutation_input(&store, finish),
                Err(ComposerHostError::MutationWorkPending)
            ));
            let custody = host.test_mutation_in_flight_custody().unwrap();
            assert!(custody.1);
            assert_eq!(host.test_mutation_in_flight_finish(), Some(finish));
            let different = MutationFinishInput::with_positions_intent(
                finish.key(),
                finish.source(),
                finish.proposal(),
                finish.intended_extent(),
                MutationPositionsIntent::ReplacementEnd {
                    replacement: selected,
                    inserted_utf8_bytes: 2,
                },
            );
            assert!(matches!(
                host.finish_mutation_input(&store, different),
                Err(ComposerHostError::MutationMalformed)
            ));
            assert_eq!(host.test_mutation_in_flight_custody(), Some(custody));
            assert_eq!(host.test_mutation_in_flight_finish(), Some(finish));
            host.test_set_mutation_transition_limit(4096);
        } else if inserted == "é" {
            fixture
                .faults
                .fail_next(beryl_home_store::test_faults::FaultPoint::AfterCommitBeforePersist);
        }
        host.finish_mutation_input(&store, finish).unwrap();
        let (committed, committed_positions) = commit_mapped(&mut host, &store, finish.key());
        assert_eq!(
            candidate_text(storage.clone(), &store, committed),
            format!("A{inserted}D").as_bytes()
        );
        let collapse = 1 + inserted.len() as u64;
        assert_marker(
            storage.clone(),
            &store,
            committed,
            SyndicDraftMarkerId::from_bytes(left.id().get().to_be_bytes()),
            1,
            left_order as u64,
            ImageLabelOrdinal::new(1).unwrap(),
            asset_id_for_object(left.id()),
        );
        assert_marker(
            storage.clone(),
            &store,
            committed,
            SyndicDraftMarkerId::from_bytes(right.id().get().to_be_bytes()),
            collapse,
            right_order as u64,
            ImageLabelOrdinal::new(2).unwrap(),
            asset_id_for_object(right.id()),
        );
        let position = SourcePosition::new(
            ByteOffset::new(collapse),
            if inserted.is_empty() {
                InlineObjectGap::between(left, right).unwrap()
            } else {
                InlineObjectGap::before(right)
            },
        );
        assert_eq!(committed_positions, MutationPositions::collapsed(position));
    }
}

#[test]
fn reversed_coalesced_boundary_keys_refuse_before_mutation_begin_and_preserve_source() {
    let fixture = evidence_fixture::fixture("mapped-replacement-refusal", 221);
    let store = &fixture.store;
    let storage = fixture.storage.clone();
    let thread = fixture.thread;
    let assets = fixture.assets.clone();
    let (mut host, base) = activated(storage.clone(), &store, thread, 222, 223);
    let text = commit_text(&mut host, &store, base, 214, 0, 0, "ABCD", 4, 1);
    let left = InlineObjectNeighbor::new(InlineObjectId::new(0xb201), InlineObjectOrder::new(9));
    let right = InlineObjectNeighbor::new(InlineObjectId::new(0xb202), InlineObjectOrder::new(2));
    let marked = insert_boundary(&mut host, &store, text, 2141, 1, left, 1);
    let marked = insert_boundary(&mut host, &store, marked, 2142, 3, right, 2);
    let selected = range(
        SourcePosition::new(ByteOffset::new(1), InlineObjectGap::after(left)),
        SourcePosition::new(ByteOffset::new(3), InlineObjectGap::before(right)),
    );
    let (_, pass, _, finish) = prepare(&mut host, &store, marked, &assets, selected, "");
    assert!(
        matches!(drive(&mut host, &store, marked, &assets, ComposerHostMutationEvidenceRequest::Finish { pass, finish }),
        ComposerHostMutationEvidenceOutcome::Refused { key, failure }
            if key == finish.key() && matches!(failure.as_ref(), ComposerHostMutationAdmissionFailure::PositionUnrepresentable))
    );
    assert_eq!(host.binding(), Some(marked));
    assert_eq!(candidate_text(storage.clone(), &store, marked), b"ABCD");
    assert_eq!(
        storage
            .draft_mutation_staging_status(&store, staging_identity(marked, 2216))
            .unwrap(),
        DraftMutationStagingStatusV1::Absent
    );
    assert_marker(
        storage.clone(),
        &store,
        marked,
        SyndicDraftMarkerId::from_bytes(left.id().get().to_be_bytes()),
        1,
        9,
        ImageLabelOrdinal::new(1).unwrap(),
        asset_id_for_object(left.id()),
    );
    assert_marker(
        storage.clone(),
        &store,
        marked,
        SyndicDraftMarkerId::from_bytes(right.id().get().to_be_bytes()),
        3,
        2,
        ImageLabelOrdinal::new(2).unwrap(),
        asset_id_for_object(right.id()),
    );
}

fn commit_mapped(
    host: &mut SyndicComposerHost,
    store: &beryl_home_store::HomeStore,
    key: MutationKey,
) -> (ComposerHostBinding, MutationPositions) {
    for _ in 0..64 {
        match host.execute_mutation(
            store,
            MutationCommitRequest::new(key, MutationIdentity::ROOT),
            &CommandCancellation::new(),
        ) {
            Ok(ComposerHostMutationOutcome::Committed {
                binding, positions, ..
            }) => return (binding, positions),
            Err(ComposerHostError::MutationWorkPending) => {}
            other => {
                let diagnostics = host.mutation_build_diagnostics();
                panic!(
                    "mapped replacement did not commit: {other:?}; classification={:?}; result={:?}; local_failure={:?}",
                    diagnostics.as_ref().and_then(|d| d.classification),
                    diagnostics.as_ref().and_then(|d| d.result),
                    diagnostics.as_ref().and_then(|d| d.local_failure)
                );
            }
        }
    }
    panic!("mapped replacement exceeded the bounded drive budget")
}

#[test]
fn mapped_replacement_refuses_a_real_boundary_marker_that_is_not_the_claimed_edge() {
    let fixture = evidence_fixture::fixture("mapped-replacement-forged-edge", 231);
    let store = &fixture.store;
    let storage = fixture.storage.clone();
    let (mut host, base) = activated(storage.clone(), store, fixture.thread, 232, 233);
    let text = commit_text(&mut host, store, base, 214, 0, 0, "ABCD", 4, 1);
    let left = InlineObjectNeighbor::new(InlineObjectId::new(0xb301), InlineObjectOrder::new(2));
    let right = InlineObjectNeighbor::new(InlineObjectId::new(0xb302), InlineObjectOrder::new(9));
    let extra = InlineObjectNeighbor::new(InlineObjectId::new(0xb303), InlineObjectOrder::new(3));
    let marked = insert_boundary(&mut host, store, text, 2141, 1, left, 1);
    let marked = insert_boundary(&mut host, store, marked, 2142, 3, right, 2);
    let after_left = SourcePosition::new(ByteOffset::new(1), InlineObjectGap::after(left));
    let marked = commit_items(
        &mut host,
        store,
        marked,
        2150,
        range(after_left, after_left),
        vec![MutationPageItem::Object(ObjectChange::Insert {
            object: SuccessorObject::new(extra.id(), ByteOffset::new(1), extra.order(), 17, 5),
        })],
        MutationPositions::collapsed(SourcePosition::new(
            ByteOffset::new(1),
            InlineObjectGap::after(extra),
        )),
        vec![labeled_marker_metadata(
            extra.id(),
            ImageLabelOrdinal::new(3).unwrap(),
            asset_id_for_object(extra.id()),
        )],
        4,
        1,
    );
    let selected = range(
        after_left,
        SourcePosition::new(ByteOffset::new(3), InlineObjectGap::before(right)),
    );
    let (_, pass, _, finish) = prepare(&mut host, store, marked, &fixture.assets, selected, "");
    assert!(matches!(
        host.dispatch_mutation_evidence(
            store,
            marked,
            &fixture.assets,
            ComposerHostMutationEvidenceRequest::Finish { pass, finish },
            &CommandCancellation::new()
        ),
        Err(ComposerHostError::MutationMalformed)
    ));
    assert_eq!(host.binding(), Some(marked));
    assert_eq!(
        storage
            .draft_mutation_staging_status(store, staging_identity(marked, 2216))
            .unwrap(),
        DraftMutationStagingStatusV1::Absent
    );
    assert!(
        matches!(drive(&mut host, store, marked, &fixture.assets, ComposerHostMutationEvidenceRequest::Cancel(finish.key())),
        ComposerHostMutationEvidenceOutcome::Refused { failure, .. } if matches!(failure.as_ref(), ComposerHostMutationAdmissionFailure::Cancelled))
    );
    assert_eq!(candidate_text(storage, store, marked), b"ABCD");
}

#[test]
fn mapped_interior_removal_preserves_representable_survivors_with_cross_anchor_keys() {
    for inserted in ["", "Q"] {
        eprintln!("mapped interior replacement insertion={inserted:?}");
        let fixture = evidence_fixture::fixture("mapped-interior-removal", 241);
        let store = &fixture.store;
        let storage = fixture.storage.clone();
        let (mut host, base) = activated(storage.clone(), store, fixture.thread, 242, 243);
        let text = commit_text(&mut host, store, base, 214, 0, 0, "ABCDE", 5, 1);
        let left =
            InlineObjectNeighbor::new(InlineObjectId::new(0xb401), InlineObjectOrder::new(9));
        let middle =
            InlineObjectNeighbor::new(InlineObjectId::new(0xb402), InlineObjectOrder::new(2));
        let right =
            InlineObjectNeighbor::new(InlineObjectId::new(0xb403), InlineObjectOrder::new(12));
        let marked = insert_boundary(&mut host, store, text, 2141, 1, left, 1);
        let marked = insert_boundary(&mut host, store, marked, 2142, 2, middle, 2);
        let marked = insert_boundary(&mut host, store, marked, 2143, 4, right, 3);
        let replacement = range(
            SourcePosition::new(ByteOffset::new(1), InlineObjectGap::after(left)),
            SourcePosition::new(ByteOffset::new(4), InlineObjectGap::before(right)),
        );
        let target = ObjectTarget::new(
            range(
                SourcePosition::new(ByteOffset::new(2), InlineObjectGap::before(middle)),
                SourcePosition::new(ByteOffset::new(2), InlineObjectGap::after(middle)),
            ),
            middle.id(),
            middle.order(),
        )
        .unwrap();
        let (mut editor, pass, page, finish) = prepare_with_removals(
            &mut host,
            store,
            marked,
            &fixture.assets,
            replacement,
            inserted,
            &[target],
        );
        assert!(
            matches!(drive(&mut host, store, marked, &fixture.assets, ComposerHostMutationEvidenceRequest::Finish { pass, finish }),
            ComposerHostMutationEvidenceOutcome::Began(actual) if actual == finish.key())
        );
        editor.accept_preflight(finish.key(), None).unwrap();
        let staging = editor.mutation_restart(finish.key()).unwrap();
        editor.acknowledge_restart(staging).unwrap();
        let source = MutationPage::new(
            MutationPageKey::new(
                finish.key(),
                MutationLane::Source,
                MutationCursor::new(0),
                0,
                MutationIdentity::ROOT,
            ),
            MutationCursor::new(1),
            vec![MutationPageItem::Object(ObjectChange::Remove { target })],
        )
        .unwrap();
        host.stage_mutation_page(
            store,
            MutationPageRequest::new(source).with_pass(staging),
            Box::new([]),
        )
        .unwrap();
        host.stage_mutation_page(
            store,
            MutationPageRequest::new(page).with_pass(staging),
            Box::new([]),
        )
        .unwrap();
        host.finish_mutation_input(store, finish).unwrap();
        let (committed, positions) = commit_mapped(&mut host, store, finish.key());
        assert_eq!(
            candidate_text(storage.clone(), store, committed),
            format!("A{inserted}E").as_bytes()
        );
        assert_eq!(committed.root().summary().marker_count(), 2);
        let collapse = 1 + inserted.len() as u64;
        assert_marker(
            storage.clone(),
            store,
            committed,
            SyndicDraftMarkerId::from_bytes(left.id().get().to_be_bytes()),
            1,
            9,
            ImageLabelOrdinal::new(1).unwrap(),
            asset_id_for_object(left.id()),
        );
        assert_marker(
            storage.clone(),
            store,
            committed,
            SyndicDraftMarkerId::from_bytes(right.id().get().to_be_bytes()),
            collapse,
            12,
            ImageLabelOrdinal::new(3).unwrap(),
            asset_id_for_object(right.id()),
        );
        assert_eq!(positions.caret().byte_offset.get(), collapse);
    }
}
