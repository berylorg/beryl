use beryl_app::composer_host::{
    ComposerHostBinding, ComposerHostError, ComposerHostMutationOutcome,
};
use beryl_home_store::CommandCancellation;
use gpui_text_input::{
    BindingId, LogicalExtent, MutationCommitRequest, MutationCursor, MutationFinishInput,
    MutationIdentity, MutationKey, MutationLane, MutationPage, MutationPageItem, MutationPageKey,
    MutationPageRequest, MutationPositions, MutationStreamFinish, MutationTotals, OperationId,
    SourceRevision,
};

use super::{composer, support::Host};

pub fn complete_admitted_append(
    fixture: &mut Host,
    binding: ComposerHostBinding,
    operation: u64,
    text: &str,
) -> ComposerHostBinding {
    let key = MutationKey::new(
        BindingId::new(binding.host_generation().get()),
        SourceRevision::new(binding.candidate().candidate_generation()),
        OperationId::new(operation),
    );
    let page = MutationPage::new(
        MutationPageKey::new(
            key,
            MutationLane::Proposal,
            MutationCursor::new(0),
            0,
            MutationIdentity::ROOT,
        ),
        MutationCursor::new(1),
        vec![MutationPageItem::Utf8 {
            inserted_offset: 0,
            text: text.into(),
        }],
    )
    .unwrap();
    let proposal_finish = MutationStreamFinish {
        next_cursor: page.next_cursor(),
        next_ordinal: 1,
        cumulative_identity: page.cumulative_identity(),
        totals: page.totals(),
    };
    fixture
        .host
        .stage_mutation_page(&fixture.store, MutationPageRequest::new(page), Box::new([]))
        .unwrap();
    let end = binding.logical_extent().logical_utf8_bytes() + text.len() as u64;
    fixture
        .host
        .finish_mutation_input(
            &fixture.store,
            MutationFinishInput::new(
                key,
                MutationStreamFinish {
                    next_cursor: MutationCursor::new(0),
                    next_ordinal: 0,
                    cumulative_identity: MutationIdentity::ROOT,
                    totals: MutationTotals::default(),
                },
                proposal_finish,
                LogicalExtent::new(end, 1),
                MutationPositions::collapsed(composer::position(end)),
            ),
        )
        .unwrap();
    for _ in 0..16 {
        match fixture.host.execute_mutation(
            &fixture.store,
            MutationCommitRequest::new(key, MutationIdentity::ROOT),
            &CommandCancellation::new(),
        ) {
            Ok(ComposerHostMutationOutcome::Committed { binding, .. }) => return binding,
            Err(ComposerHostError::MutationWorkPending) => {}
            other => panic!("admitted edit failed during close: {other:?}"),
        }
    }
    panic!("admitted edit did not settle within 16 steps");
}
