#[path = "../syndic_composer_history/support.rs"]
mod composer;
#[path = "../syndic_composer_publication/support.rs"]
mod publication;

use beryl_app::{
    cas_projection::{MinimumTurnCaptureReserve, ProjectionServiceConfig},
    composer_host::{ComposerHostSubmissionAdvance, ComposerHostSubmissionRequest},
};
use beryl_home_store::CommandCancellation;
use beryl_model::{SyndicDraftId, SyndicItemId, SyndicThreadId};
use syndic_storage::{
    DraftComposerMaterializationOperationIdV1, DraftPieceOperationIdV1, FirstAcceptanceKind,
    SyndicTimestamp,
};

pub(super) fn submit(fixture: &super::Fixture, thread: SyndicThreadId) {
    let live = fixture.service().live_home_command().unwrap();
    let home = live.home();
    let assets = fixture.state.assets();
    let (mut host, binding) = composer::activated(fixture.storage.clone(), home, thread, 150, 151);
    composer::commit_text(
        &mut host,
        home,
        binding,
        1,
        0,
        0,
        "continue durable work",
        21,
        1,
    );
    let seals = publication::service(home, fixture.storage.clone(), assets.clone(), 1, 1);
    let admitted_at = SyndicTimestamp::from_unix_millis(5);
    let requirement =
        ProjectionServiceConfig::try_new(1, 4, MinimumTurnCaptureReserve::try_new(1).unwrap())
            .unwrap()
            .turn_start_admission_requirement();
    let ticket = host
        .begin_submission(ComposerHostSubmissionRequest::new(
            SyndicDraftId::from_bytes([155; 16]),
            SyndicItemId::from_bytes([156; 16]),
            DraftComposerMaterializationOperationIdV1::from_bytes([157; 16]),
            DraftPieceOperationIdV1::from_bytes([158; 16]),
            admitted_at,
            requirement,
        ))
        .unwrap();
    for _ in 0..16_384 {
        match host
            .advance_submission(
                home,
                ticket,
                assets.clone(),
                &seals,
                composer::operation_id(160),
                None,
                admitted_at,
                &CommandCancellation::new(),
            )
            .unwrap()
        {
            ComposerHostSubmissionAdvance::Progress(_)
            | ComposerHostSubmissionAdvance::ReconciliationPending => {}
            ComposerHostSubmissionAdvance::ExactSuccess(FirstAcceptanceKind::Idle { .. }) => return,
            outcome => panic!("ordinary durable submission did not commit exactly: {outcome:?}"),
        }
    }
    panic!("durable submission did not converge");
}
