use super::*;

pub(super) fn prepare(
    fixture: &support::Fixture,
    host: &mut SyndicComposerHost,
    binding: beryl_app::composer_host::ComposerHostBinding,
    operation: u64,
    replacement: SourceRange,
    items: Vec<MutationPageItem>,
    metadata: Box<[ComposerHostImageMarkerMetadata]>,
    intended: SourcePosition,
) -> (
    RangeEditCoordinator,
    MutationPass,
    MutationPage,
    MutationFinishInput,
) {
    let (mut editor, begin, key) = begin_edit(binding, operation, replacement, operation + 1000);
    let evidence = editor.request_evidence(key).unwrap();
    assert!(matches!(
        drive_evidence(host, &fixture.store, binding, &fixture.assets,
            ComposerHostMutationEvidenceRequest::Begin { begin, pass: evidence }),
        ComposerHostMutationEvidenceOutcome::Started(actual) if actual == evidence
    ));
    let proposal = page(&editor, key, MutationLane::Proposal, items);
    let acknowledgement = editor
        .submit_evidence_page(evidence, proposal.clone())
        .unwrap();
    assert!(matches!(
        drive_evidence(host, &fixture.store, binding, &fixture.assets,
            ComposerHostMutationEvidenceRequest::Page {
                pass: evidence, page: proposal.clone(), metadata,
            }),
        ComposerHostMutationEvidenceOutcome::PageAccepted(actual) if actual == evidence
    ));
    editor.acknowledge_evidence_page(acknowledgement).unwrap();
    let finish = finish_input(&editor, key, intended);
    editor.finish_evidence(evidence, finish).unwrap();
    assert!(matches!(
        drive_evidence(host, &fixture.store, binding, &fixture.assets,
            ComposerHostMutationEvidenceRequest::Finish { pass: evidence, finish }),
        ComposerHostMutationEvidenceOutcome::Began(actual) if actual == key
    ));
    editor.accept_preflight(key, None).unwrap();
    let staging = editor.mutation_restart(key).unwrap();
    editor.acknowledge_restart(staging).unwrap();
    (editor, staging, proposal, finish)
}

fn commit(
    fixture: &support::Fixture,
    host: &mut SyndicComposerHost,
    binding: beryl_app::composer_host::ComposerHostBinding,
    operation: u64,
    replacement: SourceRange,
    items: Vec<MutationPageItem>,
    metadata: Box<[ComposerHostImageMarkerMetadata]>,
    intended: SourcePosition,
) -> beryl_app::composer_host::ComposerHostBinding {
    let (mut editor, staging, proposal, finish) = prepare(
        fixture,
        host,
        binding,
        operation,
        replacement,
        items,
        metadata.clone(),
        intended,
    );
    stage(
        host,
        &fixture.store,
        &mut editor,
        staging,
        proposal,
        metadata,
    );
    editor.finish_pass_input(staging, finish).unwrap();
    host.finish_mutation_input(&fixture.store, finish).unwrap();
    settle(host, &fixture.store, finish.key())
}

#[test]
fn evidence_replacement_and_removal_preserve_exact_marker_identity_and_history() {
    let fixture = fixture("composer-marker-replacement", 40);
    let first_asset = publish_image_asset(&fixture.store, fixture.assets.clone(), b"replace-first");
    let (mut host, binding) = activate(
        fixture.storage.clone(),
        &fixture.store,
        fixture.thread,
        41,
        42,
    );
    let id = InlineObjectId::new(401);
    let first_order = InlineObjectOrder::new(1);
    let inserted = commit(
        &fixture,
        &mut host,
        binding,
        43,
        SourceRange::new(origin(), origin()).unwrap(),
        vec![MutationPageItem::Object(ObjectChange::Insert {
            object: SuccessorObject::new(id, ByteOffset::new(0), first_order, 1, 1),
        })],
        Box::new([ComposerHostImageMarkerMetadata::new(id, first_asset)]),
        after(id, first_order),
    );
    let original = occurrence(fixture.storage.clone(), &fixture.store, inserted, id);
    assert_eq!(original.asset_id(), first_asset);
    let second_order = InlineObjectOrder::new(2);
    let previous = target(id, first_order);
    let replaced = commit(
        &fixture,
        &mut host,
        inserted,
        44,
        previous.range(),
        vec![MutationPageItem::Object(ObjectChange::Replace {
            target: previous,
            object: SuccessorObject::new(id, ByteOffset::new(0), second_order, 1, 1),
        })],
        Box::new([ComposerHostImageMarkerMetadata::from_source(
            id,
            first_asset,
            syndic_storage::DraftMarkerReadinessSourceSelectorV1::Candidate(
                syndic_storage::DraftMarkerReadinessCandidateSourceV1::new(
                    inserted.candidate().draft_id(),
                    inserted.candidate().session_id(),
                    inserted.candidate().candidate_generation(),
                    inserted.root(),
                    SyndicDraftMarkerId::from_bytes(id.get().to_be_bytes()),
                ),
            ),
        )]),
        after(id, second_order),
    );
    assert_eq!(replaced.root().summary().marker_count(), 1);
    let replacement = occurrence(fixture.storage.clone(), &fixture.store, replaced, id);
    assert_eq!(replacement.asset_id(), first_asset);
    assert_eq!(replacement.label(), original.label());
    assert_eq!(
        occurrence(fixture.storage.clone(), &fixture.store, inserted, id),
        original
    );
    let previous = target(id, second_order);
    let removed = commit(
        &fixture,
        &mut host,
        replaced,
        45,
        previous.range(),
        vec![MutationPageItem::Object(ObjectChange::Remove {
            target: previous,
        })],
        Box::new([]),
        origin(),
    );
    assert_eq!(removed.root().summary().marker_count(), 0);
    assert!(
        fixture
            .storage
            .draft_marker_identity(
                &fixture.store,
                removed.root(),
                SyndicDraftMarkerId::from_bytes(id.get().to_be_bytes()),
            )
            .unwrap()
            .is_none()
    );
    assert_eq!(
        occurrence(fixture.storage.clone(), &fixture.store, replaced, id),
        replacement
    );
    assert_ne!(removed.history(), replaced.history());
    assert_eq!(host.binding(), Some(removed));
}

#[test]
fn evidence_finish_rejects_a_different_replayed_lane_without_adopting_a_prefix() {
    let fixture = fixture("composer-marker-replay-mismatch", 50);
    let asset = publish_image_asset(&fixture.store, fixture.assets.clone(), b"replay-image");
    let (mut host, binding) = activate(
        fixture.storage.clone(),
        &fixture.store,
        fixture.thread,
        51,
        52,
    );
    let id = InlineObjectId::new(501);
    let order = InlineObjectOrder::new(1);
    let (_editor, staging, proposal, finish) = prepare(
        &fixture,
        &mut host,
        binding,
        53,
        SourceRange::new(origin(), origin()).unwrap(),
        vec![MutationPageItem::Object(ObjectChange::Insert {
            object: SuccessorObject::new(id, ByteOffset::new(0), order, 1, 1),
        })],
        Box::new([ComposerHostImageMarkerMetadata::new(id, asset)]),
        after(id, order),
    );
    let changed = MutationPage::new(
        proposal.key(),
        proposal.next_cursor(),
        vec![MutationPageItem::Object(ObjectChange::Insert {
            object: SuccessorObject::new(id, ByteOffset::new(0), order, 2, 1),
        })],
    )
    .unwrap();
    host.stage_mutation_page(
        &fixture.store,
        MutationPageRequest::new(changed).with_pass(staging),
        Box::new([ComposerHostImageMarkerMetadata::new(id, asset)]),
    )
    .unwrap();
    assert!(matches!(
        host.finish_mutation_input(&fixture.store, finish),
        Err(ComposerHostError::MutationMalformed)
    ));
    assert_eq!(host.binding(), Some(binding));
    assert_eq!(binding.root().summary().marker_count(), 0);
    let cancellation = CommandCancellation::new();
    cancellation.cancel();
    for _ in 0..64 {
        match host.execute_mutation(
            &fixture.store,
            MutationCommitRequest::new(finish.key(), MutationIdentity::ROOT),
            &cancellation,
        ) {
            Ok(ComposerHostMutationOutcome::Cancelled) => {
                let drained = host.binding().unwrap();
                assert_eq!(drained.root(), binding.root());
                assert_eq!(drained.history(), binding.history());
                assert_eq!(drained.range_binding(), binding.range_binding());
                assert!(
                    drained.candidate().session_generation()
                        > binding.candidate().session_generation()
                );
                return;
            }
            Err(ComposerHostError::MutationWorkPending) => {}
            other => panic!("mismatched replay cleanup did not settle: {other:?}"),
        }
    }
    panic!("mismatched replay cleanup exceeded its bounded budget");
}
