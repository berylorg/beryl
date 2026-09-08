#[path = "../composer_marker_evidence/support.rs"]
mod evidence_support;

use beryl_app::composer_host::{
    ComposerHostBinding, ComposerHostError, ComposerHostImageMarkerMetadata,
    ComposerHostMutationEvidenceOutcome, ComposerHostMutationEvidenceRequest,
    ComposerHostMutationOutcome, SyndicComposerHost,
};
use beryl_home_store::CommandCancellation;
use gpui_text_input::{
    BindingId, ByteOffset, InlineObjectGap, InlineObjectId, InlineObjectNeighbor,
    InlineObjectOrder, LogicalExtent, MutationBeginRequest, MutationCommitRequest, MutationCursor,
    MutationFinishInput, MutationIdentity, MutationKey, MutationKind, MutationLane, MutationLimits,
    MutationPage, MutationPageAcceptance, MutationPageItem, MutationPageKey, MutationPageRequest,
    MutationPositions, MutationProducerIdentity, MutationProposal, OperationId,
    RangeEditCoordinator, SourcePosition, SourceRange, SourceRevision, SuccessorObject,
};

pub use evidence_support::{Fixture, activate, fixture, publish_image_asset};

pub fn marker_id() -> InlineObjectId {
    InlineObjectId::new(0x8001_0203_0405_0607_0809_0a0b_0c0d_0eff)
}

pub fn marker_order() -> InlineObjectOrder {
    InlineObjectOrder::new(1)
}

pub fn admit_fresh_marker(
    fixture: &Fixture,
    host: &mut SyndicComposerHost,
    binding: ComposerHostBinding,
    operation: u64,
    asset: beryl_model::AssetId,
    after_selection: bool,
) -> (ComposerHostBinding, SourcePosition, SourcePosition) {
    let key = MutationKey::new(
        BindingId::new(binding.host_generation().get()),
        SourceRevision::new(binding.candidate().candidate_generation()),
        OperationId::new(operation),
    );
    let origin = SourcePosition::new(ByteOffset::new(0), InlineObjectGap::NoObjects);
    let marker_id = marker_id();
    let marker_order = marker_order();
    let neighbor = InlineObjectNeighbor::new(marker_id, marker_order);
    let before = SourcePosition::new(ByteOffset::new(0), InlineObjectGap::before(neighbor));
    let after = SourcePosition::new(ByteOffset::new(0), InlineObjectGap::after(neighbor));
    let intended = if after_selection { after } else { before };
    let proposal = MutationProposal::new(
        key,
        MutationKind::Edit,
        MutationPositions::collapsed(origin),
        SourceRange::new(origin, origin).unwrap(),
        0,
    );
    let begin = MutationBeginRequest::new(proposal, MutationCursor::new(0), MutationCursor::new(0))
        .with_replayable_producer(MutationProducerIdentity::new(operation + 1_000));
    let mut editor = RangeEditCoordinator::new(
        binding.range_binding(),
        MutationLimits::new(16, 65_536)
            .unwrap()
            .with_object_limits(16, 65_536, 65_536)
            .unwrap(),
    );
    editor.begin(begin).unwrap();

    let evidence = editor.request_evidence(key).unwrap();
    assert!(matches!(
        drive_evidence(
            host,
            fixture,
            binding,
            ComposerHostMutationEvidenceRequest::Begin { begin, pass: evidence },
        ),
        ComposerHostMutationEvidenceOutcome::Started(actual) if actual == evidence
    ));

    let frontier = editor.stream_finish(key, MutationLane::Proposal).unwrap();
    let page = MutationPage::new(
        MutationPageKey::new(
            key,
            MutationLane::Proposal,
            frontier.next_cursor,
            frontier.next_ordinal,
            frontier.cumulative_identity,
        ),
        MutationCursor::new(frontier.next_cursor.get() + 1),
        vec![MutationPageItem::Object(
            gpui_text_input::ObjectChange::Insert {
                object: SuccessorObject::new(marker_id, ByteOffset::new(0), marker_order, 17, 5),
            },
        )],
    )
    .unwrap();
    let acknowledgement = editor.submit_evidence_page(evidence, page.clone()).unwrap();
    assert!(matches!(
        drive_evidence(
            host,
            fixture,
            binding,
            ComposerHostMutationEvidenceRequest::Page {
                pass: evidence,
                page: page.clone(),
                metadata: Box::new([ComposerHostImageMarkerMetadata::new(marker_id, asset)]),
            },
        ),
        ComposerHostMutationEvidenceOutcome::PageAccepted(actual) if actual == evidence
    ));
    editor.acknowledge_evidence_page(acknowledgement).unwrap();

    let finish = MutationFinishInput::new(
        key,
        editor.stream_finish(key, MutationLane::Source).unwrap(),
        editor.stream_finish(key, MutationLane::Proposal).unwrap(),
        LogicalExtent::new(0, 0),
        MutationPositions::collapsed(intended),
    );
    editor.finish_evidence(evidence, finish).unwrap();
    assert!(matches!(
        drive_evidence(
            host,
            fixture,
            binding,
            ComposerHostMutationEvidenceRequest::Finish { pass: evidence, finish },
        ),
        ComposerHostMutationEvidenceOutcome::Began(actual) if actual == key
    ));
    editor.accept_preflight(key, None).unwrap();
    let staging = editor.mutation_restart(key).unwrap();
    editor.acknowledge_restart(staging).unwrap();
    assert!(matches!(
        editor.accept_pass_page(staging, page.clone()),
        Ok(MutationPageAcceptance::Accepted { .. })
    ));
    host.stage_mutation_page(
        &fixture.store,
        MutationPageRequest::new(page).with_pass(staging),
        Box::new([ComposerHostImageMarkerMetadata::new(marker_id, asset)]),
    )
    .unwrap();
    editor.finish_pass_input(staging, finish).unwrap();
    host.finish_mutation_input(&fixture.store, finish).unwrap();
    for _ in 0..64 {
        match host.execute_mutation(
            &fixture.store,
            MutationCommitRequest::new(key, MutationIdentity::ROOT),
            &CommandCancellation::new(),
        ) {
            Ok(ComposerHostMutationOutcome::Committed { binding, .. }) => {
                return (binding, before, after);
            }
            Err(ComposerHostError::MutationWorkPending) => {}
            other => panic!("fresh marker admission did not settle: {other:?}"),
        }
    }
    panic!("fresh marker admission remained pending within its bounded drive budget")
}

fn drive_evidence(
    host: &mut SyndicComposerHost,
    fixture: &Fixture,
    binding: ComposerHostBinding,
    request: ComposerHostMutationEvidenceRequest,
) -> ComposerHostMutationEvidenceOutcome {
    let cancellation = CommandCancellation::new();
    let mut request = Some(request);
    for _ in 0..64 {
        let outcome = host
            .dispatch_mutation_evidence(
                &fixture.store,
                binding,
                &fixture.assets,
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
    panic!("fresh marker evidence did not settle within its bounded drive budget")
}
