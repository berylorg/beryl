use std::mem;

use beryl_app::composer_host::{
    ComposerHostBinding, ComposerHostError, ComposerHostImageMarkerMetadata,
    ComposerHostMutationEvidenceOutcome, ComposerHostMutationEvidenceRequest,
    ComposerHostMutationOutcome, SyndicComposerHost,
};
use beryl_home_store::{CommandCancellation, HomeStore};
use beryl_model::{AssetId, ImageLabelOrdinal, SyndicDraftId, SyndicThreadId};
use beryl_state::AssetState;
use gpui_text_input::{
    BindingId, ByteOffset, InlineObjectGap, InlineObjectId, InlineObjectNeighbor,
    InlineObjectOrder, LogicalExtent, MutationBeginRequest, MutationCommitRequest, MutationCursor,
    MutationFinishInput, MutationIdentity, MutationKey, MutationKind, MutationLane, MutationLimits,
    MutationPage, MutationPageAcceptance, MutationPageItem, MutationPageKey, MutationPageRequest,
    MutationPositions, MutationProducerIdentity, MutationProposal, MutationStreamFinish,
    MutationTotals, ObjectChange, OperationId, RangeEditCoordinator, SourcePosition, SourceRange,
    SourceRevision, SuccessorObject,
};

use super::{Fixture, LogicalInput, TEXT_PATTERN};

#[path = "mutation/stream.rs"]
mod stream;
use stream::stream_logical_input;

pub(super) fn commit_logical_input(
    host: &mut SyndicComposerHost,
    store: &HomeStore,
    mut binding: ComposerHostBinding,
    shape: LogicalInput,
    assets: &AssetState,
    marker_assets: &[AssetId],
) -> ComposerHostBinding {
    let (repetitions, marker_count) = match shape {
        LogicalInput::MarkerFree { repetitions } => (repetitions, 0),
        LogicalInput::AlternatingImages {
            marker_count,
            repetitions_per_text,
        } => (repetitions_per_text, marker_count),
    };
    binding = commit_segment(
        host,
        store,
        binding,
        InputSegment::Text {
            repetitions,
            after: None,
        },
        assets,
        marker_assets,
        1,
    );
    for ordinal in 1..=marker_count {
        let label = ImageLabelOrdinal::new(ordinal).unwrap();
        binding = commit_segment(
            host,
            store,
            binding,
            InputSegment::Marker(label),
            assets,
            marker_assets,
            ordinal * 2,
        );
        binding = commit_segment(
            host,
            store,
            binding,
            InputSegment::Text {
                repetitions,
                after: Some(label),
            },
            assets,
            marker_assets,
            ordinal * 2 + 1,
        );
    }
    binding
}

#[derive(Clone, Copy)]
enum InputSegment {
    Text {
        repetitions: u64,
        after: Option<ImageLabelOrdinal>,
    },
    Marker(ImageLabelOrdinal),
}

fn after_marker(draft: SyndicDraftId, label: ImageLabelOrdinal) -> InlineObjectGap {
    InlineObjectGap::after(InlineObjectNeighbor::new(
        InlineObjectId::new(u128::from_be_bytes(
            *Fixture::draft_marker_id(draft, label.get()).as_bytes(),
        )),
        InlineObjectOrder::new(u128::from(label.get())),
    ))
}

fn commit_segment(
    host: &mut SyndicComposerHost,
    store: &HomeStore,
    binding: ComposerHostBinding,
    segment: InputSegment,
    assets: &AssetState,
    marker_assets: &[AssetId],
    operation: u64,
) -> ComposerHostBinding {
    let key = MutationKey::new(
        BindingId::new(binding.host_generation().get()),
        SourceRevision::new(binding.candidate().candidate_generation()),
        OperationId::new(operation),
    );
    let extent = binding.range_binding().extent();
    let origin = SourcePosition::new(
        ByteOffset::new(extent.byte_len()),
        match segment {
            InputSegment::Text {
                after: Some(label), ..
            } => after_marker(binding.candidate().draft_id(), label),
            _ => InlineObjectGap::NoObjects,
        },
    );
    let begin = MutationBeginRequest::new(
        MutationProposal::new(
            key,
            MutationKind::Edit,
            MutationPositions::collapsed(origin),
            SourceRange::new(origin, origin).unwrap(),
            0,
        ),
        MutationCursor::new(0),
        MutationCursor::new(0),
    )
    .with_replayable_producer(MutationProducerIdentity::new(operation));
    let mut editor = RangeEditCoordinator::new(
        binding.range_binding(),
        MutationLimits::new(256, 65_536)
            .unwrap()
            .with_object_limits(256, 65_536, 65_536)
            .unwrap(),
    );
    editor.begin(begin).unwrap();
    let evidence = editor.request_evidence(key).unwrap();
    assert!(matches!(
        drive_evidence(
            host,
            store,
            assets,
            binding,
            ComposerHostMutationEvidenceRequest::Begin {
                begin,
                pass: evidence,
            },
        ),
        ComposerHostMutationEvidenceOutcome::Started(actual) if actual == evidence
    ));
    let evidence_stream = stream_logical_input(
        key,
        segment,
        extent,
        binding.candidate().draft_id(),
        marker_assets,
        |page, metadata| {
            let acknowledgement = editor.submit_evidence_page(evidence, page.clone()).unwrap();
            assert!(matches!(
                drive_evidence(
                    host,
                    store,
                    assets,
                    binding,
                    ComposerHostMutationEvidenceRequest::Page {
                        pass: evidence,
                        page,
                        metadata,
                    },
                ),
                ComposerHostMutationEvidenceOutcome::PageAccepted(actual) if actual == evidence
            ));
            editor.acknowledge_evidence_page(acknowledgement).unwrap();
        },
    );
    let source_finish = editor.stream_finish(key, MutationLane::Source).unwrap();
    let proposal_finish = editor.stream_finish(key, MutationLane::Proposal).unwrap();
    assert_eq!(proposal_finish, evidence_stream.finish);
    let finish = MutationFinishInput::new(
        key,
        source_finish,
        proposal_finish,
        evidence_stream.extent,
        evidence_stream.positions,
    );
    editor.finish_evidence(evidence, finish).unwrap();
    assert!(matches!(
        drive_evidence(
            host,
            store,
            assets,
            binding,
            ComposerHostMutationEvidenceRequest::Finish {
                pass: evidence,
                finish,
            },
        ),
        ComposerHostMutationEvidenceOutcome::Began(actual) if actual == key
    ));
    editor.accept_preflight(key, None).unwrap();
    let staging = editor.mutation_restart(key).unwrap();
    editor.acknowledge_restart(staging).unwrap();
    let staging_stream = stream_logical_input(
        key,
        segment,
        extent,
        binding.candidate().draft_id(),
        marker_assets,
        |page, metadata| {
            assert!(matches!(
                editor.accept_pass_page(staging, page.clone()),
                Ok(MutationPageAcceptance::Accepted { .. })
            ));
            host.stage_mutation_page(
                store,
                MutationPageRequest::new(page).with_pass(staging),
                metadata,
            )
            .unwrap();
        },
    );
    assert_eq!(staging_stream.finish, proposal_finish);
    assert_eq!(staging_stream.extent, evidence_stream.extent);
    assert_eq!(staging_stream.positions, evidence_stream.positions);
    editor.finish_pass_input(staging, finish).unwrap();
    host.finish_mutation_input(store, finish).unwrap();

    let maximum_steps = usize::try_from(proposal_finish.totals.pages)
        .unwrap()
        .saturating_mul(4)
        .saturating_add(32);
    for _ in 0..maximum_steps {
        match host.execute_mutation(
            store,
            MutationCommitRequest::new(key, MutationIdentity::ROOT),
            &CommandCancellation::new(),
        ) {
            Ok(ComposerHostMutationOutcome::Committed { binding, .. }) => return binding,
            Err(ComposerHostError::MutationWorkPending) => {}
            other => {
                let diagnostic = host.mutation_build_diagnostics().map(|diagnostic| {
                    (
                        diagnostic.state,
                        diagnostic.classification,
                        diagnostic.result,
                        diagnostic.original_failure,
                        diagnostic.later_failure,
                        diagnostic.cleanup_failure,
                        diagnostic.local_failure,
                    )
                });
                panic!(
                    "streamed fixture mutation did not commit: {other:?}; diagnostics (state, classification, result, original, later, cleanup, local): {diagnostic:?}",
                );
            }
        }
    }
    let diagnostic = host.mutation_build_diagnostics().map(|diagnostic| {
        (
            diagnostic.state,
            diagnostic.classification,
            diagnostic.result,
            diagnostic.original_failure,
            diagnostic.later_failure,
            diagnostic.cleanup_failure,
            diagnostic.local_failure,
        )
    });
    panic!(
        "streamed fixture mutation remained pending; diagnostics (state, classification, result, original, later, cleanup, local): {diagnostic:?}",
    )
}

fn drive_evidence(
    host: &mut SyndicComposerHost,
    store: &HomeStore,
    assets: &AssetState,
    binding: ComposerHostBinding,
    request: ComposerHostMutationEvidenceRequest,
) -> ComposerHostMutationEvidenceOutcome {
    let cancellation = CommandCancellation::new();
    let mut request = Some(request);
    for _ in 0..64 {
        match host
            .dispatch_mutation_evidence(
                store,
                binding,
                assets,
                request.take().unwrap(),
                &cancellation,
            )
            .unwrap()
        {
            ComposerHostMutationEvidenceOutcome::Pending(key) => {
                request = Some(ComposerHostMutationEvidenceRequest::Advance(key));
            }
            outcome => return outcome,
        }
    }
    panic!("streamed fixture mutation evidence exceeded its bounded work budget")
}
