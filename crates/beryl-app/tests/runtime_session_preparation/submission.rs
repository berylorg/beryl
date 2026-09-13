#[path = "../syndic_composer_history/support.rs"]
mod composer;
#[path = "../syndic_composer_publication/support.rs"]
mod publication;

use beryl_app::{
    cas_projection::{MinimumTurnCaptureReserve, ProjectionServiceConfig, SubmissionExecutionWake},
    composer_host::{ComposerHostSubmissionAdvance, ComposerHostSubmissionRequest},
};
use beryl_home_store::CommandCancellation;
use beryl_model::{SyndicDraftId, SyndicItemId, SyndicThreadId};
use syndic_storage::{
    DraftComposerMaterializationOperationIdV1, DraftPieceOperationIdV1, FirstAcceptanceKind,
    SyndicTimestamp,
};

pub(super) fn submit(fixture: &super::Fixture, thread: SyndicThreadId) {
    assert!(matches!(
        submit_text(
            fixture,
            thread,
            "continue durable work",
            150,
            SyndicTimestamp::from_unix_millis(5)
        ),
        FirstAcceptanceKind::Idle { .. }
    ));
}

pub(super) fn submit_text(
    fixture: &super::Fixture,
    thread: SyndicThreadId,
    text: &str,
    seed: u8,
    admitted_at: SyndicTimestamp,
) -> FirstAcceptanceKind {
    submit_text_with_wake(
        fixture,
        thread,
        text,
        seed,
        admitted_at,
        fixture.service().submission_execution_wake(),
    )
}

pub(super) fn seed_pending(fixture: &super::Fixture, thread: SyndicThreadId) {
    assert!(matches!(
        submit_text_with_wake(
            fixture,
            thread,
            "continue durable work",
            150,
            SyndicTimestamp::from_unix_millis(5),
            SubmissionExecutionWake::storage_only_for_test()
        ),
        FirstAcceptanceKind::Idle { .. }
    ));
}

fn submit_text_with_wake(
    fixture: &super::Fixture,
    thread: SyndicThreadId,
    text: &str,
    seed: u8,
    admitted_at: SyndicTimestamp,
    execution_wake: SubmissionExecutionWake,
) -> FirstAcceptanceKind {
    let live = fixture.service().live_home_command().unwrap();
    let home = live.home();
    let assets = fixture.state.assets();
    let (mut host, binding) =
        composer::activated(fixture.storage.clone(), home, thread, seed, seed + 1);
    composer::commit_text(
        &mut host,
        home,
        binding,
        1,
        0,
        0,
        text,
        text.len() as u64,
        1 + text.bytes().filter(|byte| *byte == b'\n').count() as u64,
    );
    let seals = publication::service(home, fixture.storage.clone(), assets.clone(), 1, 1);
    let limit = syndic_storage::SyndicPointReadLimit::new(1_000_000).unwrap();
    let admitted_at = admitted_at
        .max(
            fixture
                .storage
                .history_summary(home, thread, limit)
                .unwrap()
                .unwrap()
                .last_activity_at(),
        )
        .max(
            fixture
                .storage
                .current_draft(home, thread, limit)
                .unwrap()
                .unwrap()
                .draft()
                .updated_at(),
        );
    let requirement =
        ProjectionServiceConfig::try_new(1, 4, MinimumTurnCaptureReserve::try_new(1).unwrap())
            .unwrap()
            .turn_start_admission_requirement();
    let request = ComposerHostSubmissionRequest::new(
        execution_wake,
        SyndicDraftId::from_bytes([seed + 5; 16]),
        SyndicItemId::from_bytes([seed + 6; 16]),
        DraftComposerMaterializationOperationIdV1::from_bytes([seed + 7; 16]),
        DraftPieceOperationIdV1::from_bytes([seed + 8; 16]),
        admitted_at,
        requirement,
    );
    let mut ticket = host.begin_submission(request.clone()).unwrap();
    for _ in 0..16_384 {
        match host
            .advance_submission(
                home,
                ticket,
                assets.clone(),
                &seals,
                composer::operation_id(u64::from(seed) + 10),
                None,
                admitted_at,
                &CommandCancellation::new(),
            )
            .unwrap()
        {
            ComposerHostSubmissionAdvance::Progress(_)
            | ComposerHostSubmissionAdvance::ReconciliationPending => {}
            ComposerHostSubmissionAdvance::ExactSuccess(kind) => return kind,
            ComposerHostSubmissionAdvance::NotCommitted => {
                ticket = host.begin_submission(request.clone()).unwrap();
            }
            outcome => panic!("ordinary durable submission did not commit exactly: {outcome:?}"),
        }
    }
    panic!("durable submission did not converge");
}
