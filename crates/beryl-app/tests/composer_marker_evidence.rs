#![cfg(feature = "test-faults")]

#[path = "composer_marker_evidence/support.rs"]
mod support;

#[path = "composer_marker_evidence/replay.rs"]
mod replay;

#[path = "composer_marker_evidence/refusal.rs"]
mod refusal;

#[path = "composer_marker_evidence/flights.rs"]
mod flights;

use beryl_app::composer_host::{
    ComposerHostError, ComposerHostImageMarkerMetadata, ComposerHostMutationAdmissionFailure,
    ComposerHostMutationEvidenceOutcome, ComposerHostMutationEvidenceRequest,
    ComposerHostMutationOutcome, SyndicComposerHost,
};
use beryl_home_store::{CommandCancellation, HomeStore};
use beryl_model::SyndicDraftMarkerId;
use beryl_state::AssetState;
use gpui_text_input::{
    ByteOffset, InlineObjectGap, InlineObjectId, InlineObjectNeighbor, InlineObjectOrder,
    LogicalExtent, MutationBeginRequest, MutationCommitRequest, MutationCursor,
    MutationFinishInput, MutationIdentity, MutationKey, MutationKind, MutationLane, MutationLimits,
    MutationPage, MutationPageAcceptance, MutationPageItem, MutationPageKey, MutationPageRequest,
    MutationPass, MutationPositions, MutationProducerIdentity, MutationProposal, ObjectChange,
    ObjectTarget, OperationId, RangeEditCoordinator, SourcePosition, SourceRange, SourceRevision,
    SuccessorObject,
};
use syndic_storage::{DraftMarkerIdentityOccurrenceV1, SyndicStorage};

use support::{activate, fixture, publish_image_asset};

fn origin() -> SourcePosition {
    SourcePosition::new(ByteOffset::new(0), InlineObjectGap::NoObjects)
}

fn key(binding: beryl_app::composer_host::ComposerHostBinding, operation: u64) -> MutationKey {
    MutationKey::new(
        gpui_text_input::BindingId::new(binding.host_generation().get()),
        SourceRevision::new(binding.candidate().candidate_generation()),
        OperationId::new(operation),
    )
}

fn begin_edit(
    binding: beryl_app::composer_host::ComposerHostBinding,
    operation: u64,
    replacement: SourceRange,
    producer: u64,
) -> (RangeEditCoordinator, MutationBeginRequest, MutationKey) {
    let key = key(binding, operation);
    let proposal = MutationProposal::new(
        key,
        MutationKind::Edit,
        MutationPositions::collapsed(replacement.start()),
        replacement,
        0,
    );
    let begin = MutationBeginRequest::new(proposal, MutationCursor::new(0), MutationCursor::new(0))
        .with_replayable_producer(MutationProducerIdentity::new(producer));
    let mut editor = RangeEditCoordinator::new(
        binding.range_binding(),
        MutationLimits::new(16, 65_536)
            .unwrap()
            .with_object_limits(16, 65_536, 65_536)
            .unwrap(),
    );
    editor.begin(begin).unwrap();
    (editor, begin, key)
}

fn page(
    editor: &RangeEditCoordinator,
    key: MutationKey,
    lane: MutationLane,
    items: Vec<MutationPageItem>,
) -> MutationPage {
    let frontier = editor.stream_finish(key, lane).unwrap();
    MutationPage::new(
        MutationPageKey::new(
            key,
            lane,
            frontier.next_cursor,
            frontier.next_ordinal,
            frontier.cumulative_identity,
        ),
        MutationCursor::new(frontier.next_cursor.get() + 1),
        items,
    )
    .unwrap()
}

fn finish_input(
    editor: &RangeEditCoordinator,
    key: MutationKey,
    intended_position: SourcePosition,
) -> MutationFinishInput {
    MutationFinishInput::new(
        key,
        editor.stream_finish(key, MutationLane::Source).unwrap(),
        editor.stream_finish(key, MutationLane::Proposal).unwrap(),
        LogicalExtent::new(0, 0),
        MutationPositions::collapsed(intended_position),
    )
}

fn drive_evidence(
    host: &mut SyndicComposerHost,
    store: &HomeStore,
    binding: beryl_app::composer_host::ComposerHostBinding,
    assets: &AssetState,
    request: ComposerHostMutationEvidenceRequest,
) -> ComposerHostMutationEvidenceOutcome {
    let cancellation = CommandCancellation::new();
    let mut request = Some(request);
    for _ in 0..64 {
        let outcome = host
            .dispatch_mutation_evidence(
                store,
                binding,
                assets,
                request.take().unwrap(),
                &cancellation,
            )
            .unwrap();
        match outcome {
            ComposerHostMutationEvidenceOutcome::Pending(key) => {
                request = Some(ComposerHostMutationEvidenceRequest::Advance(key));
            }
            outcome => return outcome,
        }
    }
    panic!("marker evidence did not settle within its bounded drive budget");
}

fn cancel_evidence(
    host: &mut SyndicComposerHost,
    store: &HomeStore,
    binding: beryl_app::composer_host::ComposerHostBinding,
    assets: &AssetState,
    key: MutationKey,
) -> ComposerHostMutationEvidenceOutcome {
    drive_evidence(
        host,
        store,
        binding,
        assets,
        ComposerHostMutationEvidenceRequest::Cancel(key),
    )
}

fn settle(
    host: &mut SyndicComposerHost,
    store: &HomeStore,
    key: MutationKey,
) -> beryl_app::composer_host::ComposerHostBinding {
    for _ in 0..64 {
        match host.execute_mutation(
            store,
            MutationCommitRequest::new(key, MutationIdentity::ROOT),
            &CommandCancellation::new(),
        ) {
            Ok(ComposerHostMutationOutcome::Committed { binding, .. }) => return binding,
            Err(ComposerHostError::MutationWorkPending) => {}
            other => panic!("admitted mutation did not settle: {other:?}"),
        }
    }
    panic!("admitted mutation remained pending within its bounded drive budget");
}

fn occurrence(
    storage: SyndicStorage,
    store: &HomeStore,
    binding: beryl_app::composer_host::ComposerHostBinding,
    object: InlineObjectId,
) -> DraftMarkerIdentityOccurrenceV1 {
    storage
        .draft_marker_identity(
            store,
            binding.root(),
            SyndicDraftMarkerId::from_bytes(object.get().to_be_bytes()),
        )
        .unwrap()
        .unwrap_or_else(|| panic!("marker {object:?} was not settled"))
}

fn target(id: InlineObjectId, order: InlineObjectOrder) -> ObjectTarget {
    let neighbor = InlineObjectNeighbor::new(id, order);
    ObjectTarget::new(
        SourceRange::new(
            SourcePosition::new(ByteOffset::new(0), InlineObjectGap::before(neighbor)),
            SourcePosition::new(ByteOffset::new(0), InlineObjectGap::after(neighbor)),
        )
        .unwrap(),
        id,
        order,
    )
    .unwrap()
}

fn after(id: InlineObjectId, order: InlineObjectOrder) -> SourcePosition {
    SourcePosition::new(
        ByteOffset::new(0),
        InlineObjectGap::after(InlineObjectNeighbor::new(id, order)),
    )
}

fn stage(
    host: &mut SyndicComposerHost,
    store: &HomeStore,
    editor: &mut RangeEditCoordinator,
    pass: MutationPass,
    page: MutationPage,
    metadata: Box<[ComposerHostImageMarkerMetadata]>,
) {
    assert!(matches!(
        editor.accept_pass_page(pass, page.clone()),
        Ok(MutationPageAcceptance::Accepted { .. })
    ));
    host.stage_mutation_page(
        store,
        MutationPageRequest::new(page).with_pass(pass),
        metadata,
    )
    .unwrap();
}

#[test]
fn evidence_admission_replays_fresh_and_existing_markers_across_closed_lanes() {
    let fixture = fixture("composer-marker-evidence", 1);
    let first_asset = publish_image_asset(&fixture.store, fixture.assets.clone(), b"first-image");
    let second_asset = publish_image_asset(&fixture.store, fixture.assets.clone(), b"second-image");
    let third_asset = publish_image_asset(&fixture.store, fixture.assets.clone(), b"third-image");
    let fourth_asset = publish_image_asset(&fixture.store, fixture.assets.clone(), b"fourth-image");
    let (mut host, binding) = activate(
        fixture.storage.clone(),
        &fixture.store,
        fixture.thread,
        2,
        3,
    );

    let first = InlineObjectId::new(101);
    let second = InlineObjectId::new(102);
    let third = InlineObjectId::new(103);
    let first_order = InlineObjectOrder::new(1);
    let second_order = InlineObjectOrder::new(2);
    let third_order = InlineObjectOrder::new(3);
    let (mut editor, begin, key) =
        begin_edit(binding, 4, SourceRange::new(origin(), origin()).unwrap(), 5);
    let evidence = editor.request_evidence(key).unwrap();
    assert!(matches!(
        drive_evidence(
            &mut host,
            &fixture.store,
            binding,
            &fixture.assets,
            ComposerHostMutationEvidenceRequest::Begin { begin, pass: evidence },
        ),
        ComposerHostMutationEvidenceOutcome::Started(pass) if pass == evidence
    ));

    let source = page(
        &editor,
        key,
        MutationLane::Source,
        vec![MutationPageItem::Utf8 {
            inserted_offset: 0,
            text: "".into(),
        }],
    );
    let acknowledgement = editor
        .submit_evidence_page(evidence, source.clone())
        .unwrap();
    assert!(matches!(
        drive_evidence(
            &mut host,
            &fixture.store,
            binding,
            &fixture.assets,
            ComposerHostMutationEvidenceRequest::Page {
                pass: evidence,
                page: source.clone(),
                metadata: Box::new([]),
            },
        ),
        ComposerHostMutationEvidenceOutcome::PageAccepted(pass) if pass == evidence
    ));
    editor.acknowledge_evidence_page(acknowledgement).unwrap();

    let proposal = page(
        &editor,
        key,
        MutationLane::Proposal,
        vec![
            MutationPageItem::Object(ObjectChange::Insert {
                object: SuccessorObject::new(first, ByteOffset::new(0), first_order, 1, 1),
            }),
            MutationPageItem::Object(ObjectChange::Insert {
                object: SuccessorObject::new(second, ByteOffset::new(0), second_order, 1, 1),
            }),
        ],
    );
    let acknowledgement = editor
        .submit_evidence_page(evidence, proposal.clone())
        .unwrap();
    assert!(matches!(
        drive_evidence(
            &mut host,
            &fixture.store,
            binding,
            &fixture.assets,
            ComposerHostMutationEvidenceRequest::Page {
                pass: evidence,
                page: proposal.clone(),
                metadata: Box::new([
                    ComposerHostImageMarkerMetadata::new(first, first_asset),
                    ComposerHostImageMarkerMetadata::new(second, second_asset),
                ]),
            },
        ),
        ComposerHostMutationEvidenceOutcome::PageAccepted(pass) if pass == evidence
    ));
    editor.acknowledge_evidence_page(acknowledgement).unwrap();

    let continuation = page(
        &editor,
        key,
        MutationLane::Proposal,
        vec![MutationPageItem::Object(ObjectChange::Insert {
            object: SuccessorObject::new(third, ByteOffset::new(0), third_order, 1, 1),
        })],
    );
    let acknowledgement = editor
        .submit_evidence_page(evidence, continuation.clone())
        .unwrap();
    assert!(matches!(
        drive_evidence(
            &mut host,
            &fixture.store,
            binding,
            &fixture.assets,
            ComposerHostMutationEvidenceRequest::Page {
                pass: evidence,
                page: continuation.clone(),
                metadata: Box::new([ComposerHostImageMarkerMetadata::new(third, third_asset)]),
            },
        ),
        ComposerHostMutationEvidenceOutcome::PageAccepted(pass) if pass == evidence
    ));
    editor.acknowledge_evidence_page(acknowledgement).unwrap();

    let finish = finish_input(&editor, key, after(third, third_order));
    editor.finish_evidence(evidence, finish).unwrap();
    assert!(matches!(
        drive_evidence(
            &mut host,
            &fixture.store,
            binding,
            &fixture.assets,
            ComposerHostMutationEvidenceRequest::Finish { pass: evidence, finish },
        ),
        ComposerHostMutationEvidenceOutcome::Began(actual) if actual == key
    ));
    editor.accept_preflight(key, None).unwrap();
    let staging = editor.mutation_restart(key).unwrap();
    editor.acknowledge_restart(staging).unwrap();
    stage(
        &mut host,
        &fixture.store,
        &mut editor,
        staging,
        source,
        Box::new([]),
    );
    stage(
        &mut host,
        &fixture.store,
        &mut editor,
        staging,
        proposal,
        Box::new([
            ComposerHostImageMarkerMetadata::new(first, first_asset),
            ComposerHostImageMarkerMetadata::new(second, second_asset),
        ]),
    );
    stage(
        &mut host,
        &fixture.store,
        &mut editor,
        staging,
        continuation,
        Box::new([ComposerHostImageMarkerMetadata::new(third, third_asset)]),
    );
    editor.finish_pass_input(staging, finish).unwrap();
    host.finish_mutation_input(&fixture.store, finish).unwrap();
    let binding = settle(&mut host, &fixture.store, key);
    let first_before_move = occurrence(fixture.storage.clone(), &fixture.store, binding, first);
    let second_before_move = occurrence(fixture.storage.clone(), &fixture.store, binding, second);
    let third_before_move = occurrence(fixture.storage.clone(), &fixture.store, binding, third);
    assert_eq!(first_before_move.asset_id(), first_asset);
    assert_eq!(second_before_move.asset_id(), second_asset);
    assert_eq!(third_before_move.asset_id(), third_asset);
    assert_ne!(first_before_move.label(), second_before_move.label());
    assert_ne!(second_before_move.label(), third_before_move.label());

    let fourth = InlineObjectId::new(104);
    let fourth_order = InlineObjectOrder::new(4);
    let first_neighbor = InlineObjectNeighbor::new(first, first_order);
    let second_neighbor = InlineObjectNeighbor::new(second, second_order);
    let moved = ObjectTarget::new(
        SourceRange::new(
            SourcePosition::new(ByteOffset::new(0), InlineObjectGap::before(first_neighbor)),
            SourcePosition::new(
                ByteOffset::new(0),
                InlineObjectGap::between(first_neighbor, second_neighbor).unwrap(),
            ),
        )
        .unwrap(),
        first,
        first_order,
    )
    .unwrap();
    let (mut editor, begin, key) = begin_edit(binding, 6, moved.range(), 7);
    let evidence = editor.request_evidence(key).unwrap();
    assert!(matches!(
        drive_evidence(
            &mut host,
            &fixture.store,
            binding,
            &fixture.assets,
            ComposerHostMutationEvidenceRequest::Begin { begin, pass: evidence },
        ),
        ComposerHostMutationEvidenceOutcome::Started(pass) if pass == evidence
    ));
    let source = page(
        &editor,
        key,
        MutationLane::Source,
        vec![MutationPageItem::Object(ObjectChange::Move {
            target: moved,
            object: SuccessorObject::new(first, ByteOffset::new(0), first_order, 1, 1),
        })],
    );
    let acknowledgement = editor
        .submit_evidence_page(evidence, source.clone())
        .unwrap();
    assert!(matches!(
        drive_evidence(
            &mut host,
            &fixture.store,
            binding,
            &fixture.assets,
            ComposerHostMutationEvidenceRequest::Page {
                pass: evidence,
                page: source.clone(),
                metadata: Box::new([]),
            },
        ),
        ComposerHostMutationEvidenceOutcome::PageAccepted(pass) if pass == evidence
    ));
    editor.acknowledge_evidence_page(acknowledgement).unwrap();
    let proposal = page(
        &editor,
        key,
        MutationLane::Proposal,
        vec![
            MutationPageItem::Object(ObjectChange::Move {
                target: moved,
                object: SuccessorObject::new(first, ByteOffset::new(0), first_order, 1, 1),
            }),
            MutationPageItem::Object(ObjectChange::Insert {
                object: SuccessorObject::new(fourth, ByteOffset::new(0), fourth_order, 1, 1),
            }),
        ],
    );
    let acknowledgement = editor
        .submit_evidence_page(evidence, proposal.clone())
        .unwrap();
    assert!(matches!(
        drive_evidence(
            &mut host,
            &fixture.store,
            binding,
            &fixture.assets,
            ComposerHostMutationEvidenceRequest::Page {
                pass: evidence,
                page: proposal.clone(),
                metadata: Box::new([ComposerHostImageMarkerMetadata::new(fourth, fourth_asset)]),
            },
        ),
        ComposerHostMutationEvidenceOutcome::PageAccepted(pass) if pass == evidence
    ));
    editor.acknowledge_evidence_page(acknowledgement).unwrap();
    let finish = finish_input(&editor, key, after(fourth, fourth_order));
    editor.finish_evidence(evidence, finish).unwrap();
    assert!(matches!(
        drive_evidence(
            &mut host,
            &fixture.store,
            binding,
            &fixture.assets,
            ComposerHostMutationEvidenceRequest::Finish { pass: evidence, finish },
        ),
        ComposerHostMutationEvidenceOutcome::Began(actual) if actual == key
    ));
    editor.accept_preflight(key, None).unwrap();
    let staging = editor.mutation_restart(key).unwrap();
    editor.acknowledge_restart(staging).unwrap();
    stage(
        &mut host,
        &fixture.store,
        &mut editor,
        staging,
        source,
        Box::new([]),
    );
    stage(
        &mut host,
        &fixture.store,
        &mut editor,
        staging,
        proposal,
        Box::new([ComposerHostImageMarkerMetadata::new(fourth, fourth_asset)]),
    );
    editor.finish_pass_input(staging, finish).unwrap();
    host.finish_mutation_input(&fixture.store, finish).unwrap();
    let binding = settle(&mut host, &fixture.store, key);
    let first_after_move = occurrence(fixture.storage.clone(), &fixture.store, binding, first);
    let fourth_after_move = occurrence(fixture.storage.clone(), &fixture.store, binding, fourth);
    assert_eq!(first_after_move.label(), first_before_move.label());
    assert_eq!(first_after_move.asset_id(), first_asset);
    assert_eq!(fourth_after_move.asset_id(), fourth_asset);
    assert_ne!(fourth_after_move.label(), first_after_move.label());
}

#[test]
fn mismatched_evidence_pass_and_cancellation_leave_candidate_and_custody_unchanged() {
    let fixture = fixture("composer-marker-evidence-refusal", 20);
    let asset = publish_image_asset(&fixture.store, fixture.assets.clone(), b"refusal-image");
    let (mut host, binding) = activate(
        fixture.storage.clone(),
        &fixture.store,
        fixture.thread,
        21,
        22,
    );
    let root = binding.root();
    let history = binding.history();
    let (mut editor, begin, key) = begin_edit(
        binding,
        23,
        SourceRange::new(origin(), origin()).unwrap(),
        24,
    );
    let evidence = editor.request_evidence(key).unwrap();
    assert!(matches!(
        drive_evidence(
            &mut host,
            &fixture.store,
            binding,
            &fixture.assets,
            ComposerHostMutationEvidenceRequest::Begin { begin, pass: evidence },
        ),
        ComposerHostMutationEvidenceOutcome::Started(pass) if pass == evidence
    ));
    let proposal = page(
        &editor,
        key,
        MutationLane::Proposal,
        vec![MutationPageItem::Object(ObjectChange::Insert {
            object: SuccessorObject::new(
                InlineObjectId::new(501),
                ByteOffset::new(0),
                InlineObjectOrder::new(1),
                1,
                1,
            ),
        })],
    );
    let (mut substituted_editor, _, _) = begin_edit(
        binding,
        23,
        SourceRange::new(origin(), origin()).unwrap(),
        25,
    );
    let substituted = substituted_editor.request_evidence(key).unwrap();
    assert!(matches!(
        host.dispatch_mutation_evidence(
            &fixture.store,
            binding,
            &fixture.assets,
            ComposerHostMutationEvidenceRequest::Page {
                pass: substituted,
                page: proposal.clone(),
                metadata: Box::new([ComposerHostImageMarkerMetadata::new(
                    InlineObjectId::new(501),
                    asset,
                )]),
            },
            &CommandCancellation::new(),
        ),
        Err(ComposerHostError::MutationMalformed)
    ));
    let acknowledgement = editor
        .submit_evidence_page(evidence, proposal.clone())
        .unwrap();
    assert!(matches!(
        drive_evidence(
            &mut host,
            &fixture.store,
            binding,
            &fixture.assets,
            ComposerHostMutationEvidenceRequest::Page {
                pass: evidence,
                page: proposal,
                metadata: Box::new([ComposerHostImageMarkerMetadata::new(
                    InlineObjectId::new(501),
                    asset,
                )]),
            },
        ),
        ComposerHostMutationEvidenceOutcome::PageAccepted(pass) if pass == evidence
    ));
    editor.acknowledge_evidence_page(acknowledgement).unwrap();
    assert!(matches!(
        cancel_evidence(&mut host, &fixture.store, binding, &fixture.assets, key),
        ComposerHostMutationEvidenceOutcome::Refused {
            key: actual,
            failure,
        } if actual == key && matches!(failure.as_ref(), ComposerHostMutationAdmissionFailure::Cancelled)
    ));
    let live = host.binding().unwrap();
    assert_eq!(live.root(), root);
    assert_eq!(live.history(), history);

    let (mut next_editor, next_begin, next_key) = begin_edit(
        binding,
        26,
        SourceRange::new(origin(), origin()).unwrap(),
        27,
    );
    let next_evidence = next_editor.request_evidence(next_key).unwrap();
    assert!(matches!(
        drive_evidence(
            &mut host,
            &fixture.store,
            binding,
            &fixture.assets,
            ComposerHostMutationEvidenceRequest::Begin {
                begin: next_begin,
                pass: next_evidence,
            },
        ),
        ComposerHostMutationEvidenceOutcome::Started(pass) if pass == next_evidence
    ));
    assert!(matches!(
        cancel_evidence(
            &mut host,
            &fixture.store,
            binding,
            &fixture.assets,
            next_key,
        ),
        ComposerHostMutationEvidenceOutcome::Refused {
            key: actual,
            failure,
        } if actual == next_key && matches!(failure.as_ref(), ComposerHostMutationAdmissionFailure::Cancelled)
    ));
    let live = host.binding().unwrap();
    assert_eq!(live.root(), root);
    assert_eq!(live.history(), history);
}
