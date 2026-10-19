use beryl_app::composer_host::*;
use beryl_app::composer_marker_seal::DraftMarkerSealService;
use beryl_home_store::{CommandCancellation, HomeStore};
use beryl_model::{AssetReferenceSetId, SyndicDraftId, SyndicItemId};
use beryl_state::{AssetReferenceSetStagingAuthority, AssetState};
use syndic_storage::{
    DraftComposerMaterializationOperationIdV1, DraftMarkerSealOperationIdV1,
    DraftPieceOperationIdV1, SyndicTimestamp,
};

pub fn submit(
    host: &mut SyndicComposerHost,
    store: &HomeStore,
    assets: &AssetState,
    seals: &DraftMarkerSealService,
    seed: u8,
) {
    host.test_arm_submission_after_execute_fault(|_, outcome| {
        assert!(
            matches!(outcome, beryl_home_store::CommandOutcome::Committed { .. }),
            "exact source acceptance command failed: {outcome:?}"
        );
    });
    let marker_authority =
        (host.binding().unwrap().root().summary().marker_count() != 0).then(|| {
            ComposerHostMarkerSealAuthority::new(
                DraftMarkerSealOperationIdV1::from_bytes([seed.wrapping_add(4); 16]),
                AssetReferenceSetStagingAuthority::new(
                    AssetReferenceSetId::from_bytes([seed.wrapping_add(5); 16]),
                    [seed.wrapping_add(6); 32],
                ),
            )
        });
    let ticket = host
        .begin_submission(ComposerHostSubmissionRequest::new(
            beryl_app::cas_projection::SubmissionExecutionWake::storage_only_for_test(),
            SyndicDraftId::from_bytes([seed; 16]),
            SyndicItemId::from_bytes([seed.wrapping_add(1); 16]),
            DraftComposerMaterializationOperationIdV1::from_bytes([seed.wrapping_add(2); 16]),
            DraftPieceOperationIdV1::from_bytes([seed.wrapping_add(3); 16]),
            SyndicTimestamp::from_unix_millis(50),
            beryl_app::cas_projection::ProjectionServiceConfig::try_new(
                1,
                4,
                beryl_home_store::MinimumTurnCaptureReserve::try_new(1).unwrap(),
            )
            .unwrap()
            .turn_start_admission_requirement(),
        ))
        .unwrap();
    let mut stage = None;
    for _ in 0..512 {
        match host
            .advance_submission(
                store,
                ticket,
                assets.clone(),
                seals,
                DraftPieceOperationIdV1::from_bytes((u128::from(seed) + 1000).to_be_bytes()),
                marker_authority,
                SyndicTimestamp::from_unix_millis(50),
                &CommandCancellation::new(),
            )
            .unwrap()
        {
            ComposerHostSubmissionAdvance::Progress(current) => stage = Some(current),
            ComposerHostSubmissionAdvance::ExactSuccess(_) => return,
            other => panic!(
                "exact source acceptance seed {seed} failed after {stage:?}: {other:?}; materializer outcome {:?}",
                host.test_submission_materializer_outcome()
            ),
        }
    }
    panic!("source acceptance exceeded its bounded drive")
}

pub fn recall(
    host: &mut SyndicComposerHost,
    store: &HomeStore,
    assets: &AssetState,
    ticket: ComposerHostRecallTicket,
) -> ComposerHostRecallCompletion {
    host.test_arm_recall_after_execute_fault(|_, outcome| {
        assert!(
            matches!(outcome, beryl_home_store::CommandOutcome::Committed { .. }),
            "recall command failed: {outcome:?}"
        );
    });
    for _ in 0..8192 {
        match host
            .advance_accepted_input_recall(store, ticket, assets, &CommandCancellation::new())
            .unwrap()
        {
            ComposerHostRecallAdvance::Progress => {}
            ComposerHostRecallAdvance::Ready(completion) => {
                host.acknowledge_accepted_input_recall(ticket, completion.binding())
                    .unwrap();
                return completion;
            }
            ComposerHostRecallAdvance::NotCommitted => panic!("recall was not committed"),
        }
    }
    panic!("bounded recall driver did not finish")
}
