use super::*;
#[path = "../historical_replacement_host/support.rs"]
mod history;
#[path = "../syndic_composer_host/support.rs"]
mod host_support;
use beryl_app::{
    cas_projection::{
        AdmittedProjectionSession, CasProjectionRequest, LoadedCasProjection,
        OrdinaryTurnCaptureLoss,
    },
    composer_host::{
        ComposerHostHistoricalTarget, ComposerHostSubmissionAdvance, ComposerHostSubmissionRequest,
    },
    composer_marker_seal::{DraftMarkerSealService, DraftMarkerSealServiceLimits},
};
use beryl_home_store::{CommandCancellation, MinimumTurnCaptureReserve};
use beryl_model::{SyndicDraftId, SyndicItemId};
use syndic_storage::{
    CurrentTranscriptEntryProof, DraftComposerMaterializationOperationIdV1, DraftSubmissionIntent,
    FirstAcceptanceKind, TurnLifecycle,
};

#[test]
fn accepted_replacement_remains_selected_after_raw_execution_connection_loss() {
    let _guard = TEST_LOCK.lock().unwrap();
    let mut fixture = Fixture::new(157);
    let (target, mut original_session, original_projection, original_server) =
        completed_source(&mut fixture);
    let assets = fixture.state.assets();
    let mut host = history::activate(&fixture.home(), fixture.storage.clone(), fixture.thread);
    let ticket = host
        .begin_historical_replacement(
            &fixture.home(),
            host.binding().unwrap(),
            target,
            history::operation(211),
        )
        .unwrap();
    let completion = history::finish(&mut host, &fixture.home(), &assets, ticket);
    host.acknowledge_historical_replacement(ticket, completion.binding())
        .unwrap();
    let submitted_turn = host
        .binding()
        .unwrap()
        .candidate()
        .draft_id()
        .submitted_turn_id();
    let seals = DraftMarkerSealService::test_new(
        &fixture.home(),
        fixture.home().health().generation().unwrap(),
        fixture.storage.clone(),
        assets.clone(),
        DraftMarkerSealServiceLimits::new(std::num::NonZeroUsize::MIN, std::num::NonZeroUsize::MIN)
            .unwrap(),
    )
    .unwrap();
    let current = fixture
        .storage
        .current_draft(&fixture.home(), fixture.thread, syndic::point_limit())
        .unwrap()
        .unwrap();
    let summary = fixture
        .storage
        .history_summary(&fixture.home(), fixture.thread, syndic::point_limit())
        .unwrap()
        .unwrap();
    let admitted_at = SyndicTimestamp::from_unix_millis(
        current
            .draft()
            .updated_at()
            .unix_millis()
            .max(summary.last_activity_at().unix_millis())
            .checked_add(1)
            .unwrap(),
    );
    let ticket = host
        .begin_submission(ComposerHostSubmissionRequest::new(
            fixture.store.submission_execution_wake(),
            SyndicDraftId::from_bytes([221; 16]),
            SyndicItemId::from_bytes([222; 16]),
            DraftComposerMaterializationOperationIdV1::from_bytes([223; 16]),
            history::operation(224),
            admitted_at,
            beryl_app::cas_projection::ProjectionServiceConfig::try_new(
                1,
                4,
                MinimumTurnCaptureReserve::try_new(1).unwrap(),
            )
            .unwrap()
            .turn_start_admission_requirement(),
        ))
        .unwrap();
    let mut accepted = false;
    for _ in 0..1_024 {
        match host
            .advance_submission(
                &fixture.home(),
                ticket,
                assets.clone(),
                &seals,
                history::operation(225),
                None,
                admitted_at,
                &CommandCancellation::new(),
            )
            .unwrap()
        {
            ComposerHostSubmissionAdvance::Progress(_) => {}
            ComposerHostSubmissionAdvance::ExactSuccess(FirstAcceptanceKind::Idle {
                user_item_id,
            }) => {
                assert_eq!(user_item_id, SyndicItemId::from_bytes([222; 16]));
                accepted = true;
                break;
            }
            other => panic!("replacement did not accept directly: {other:?}"),
        }
    }
    assert!(accepted);
    let selected = fixture.selected_path(fixture.thread);
    assert_eq!(
        fixture
            .storage
            .thread(&fixture.home(), fixture.thread, syndic::point_limit())
            .unwrap()
            .unwrap()
            .committed_tail(),
        Some(submitted_turn)
    );
    assert_ne!(submitted_turn, target.turn_id);
    drop((host, seals, assets));
    original_session.invalidate_connection();
    drop((original_projection, original_session));
    original_server.join();

    let server = NormalTerminalServer::spawn_connection_loss_on_thread("replacement-cas-thread");
    let connector =
        ManagedBackendClientConnector::for_lifecycle_test(server.endpoint(), AUTHORIZATION);
    let mut session = fixture
        .store
        .admit_runtime_lifecycle_test_candidate(
            &connector,
            execution_binding(),
            CasProcessGeneration::new(37_157).unwrap(),
            Path::new(EXECUTION_ROOT),
            TIMEOUT,
        )
        .unwrap();
    let coordinator = CasProjectionCoordinator::for_healthy_home(&fixture.home()).unwrap();
    let projection = coordinator
        .obtain_projection(
            &fixture.home(),
            &fixture.storage,
            &mut session,
            &CasProjectionRequest::new(
                fixture.thread,
                selected,
                execution_binding(),
                ThreadStartOptions::persistent(),
                Some(2_000_000),
                admitted_at,
                TIMEOUT,
            ),
            &fixture.cancellation,
        )
        .unwrap();
    server.wait_for_projection();
    let mut lifecycle = NoopLifecycle::default();
    let mut branch = NoopBranch::default();
    let outcome = coordinator
        .execute_ordinary_turn(
            &fixture.home(),
            &fixture.storage,
            &fixture.state.assets(),
            None,
            projection,
            &fixture.cancellation,
            &OrdinaryTurnExecutionRequest::new(TurnStartOptions::default(), TIMEOUT),
            OrdinaryDynamicToolHandlers::new(&mut lifecycle, &mut branch),
        )
        .unwrap();
    assert!(matches!(
        outcome,
        OrdinaryTurnExecutionOutcome::Incomplete {
            reason: OrdinaryTurnCaptureLoss::TargetClosed(_)
        }
    ));
    assert_eq!(
        fixture
            .storage
            .thread(&fixture.home(), fixture.thread, syndic::point_limit())
            .unwrap()
            .unwrap()
            .committed_tail(),
        Some(submitted_turn)
    );
    assert_eq!(
        fixture
            .storage
            .turn_state(&fixture.home(), submitted_turn, syndic::point_limit())
            .unwrap()
            .unwrap()
            .lifecycle(),
        TurnLifecycle::Incomplete
    );
    assert!(
        fixture
            .storage
            .canonical_item(&fixture.home(), target.item_id, syndic::point_limit())
            .unwrap()
            .is_some()
    );
    let current = fixture
        .storage
        .current_draft(&fixture.home(), fixture.thread, syndic::point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(current.draft().id(), SyndicDraftId::from_bytes([221; 16]));
    assert_eq!(
        current.draft().submission_intent(),
        DraftSubmissionIntent::Ordinary
    );
    assert_eq!(lifecycle.calls, 0);
    assert_eq!(branch.calls, 0);
    session.invalidate_connection();
    drop(session);
    server.join();
    let (directory, service) = fixture.into_service();
    service.close().unwrap();
    drop(directory);
}

fn completed_source(
    fixture: &mut Fixture,
) -> (
    ComposerHostHistoricalTarget,
    AdmittedProjectionSession,
    Box<LoadedCasProjection>,
    NormalTerminalServer,
) {
    let submitted = fixture.submit_text(SUBMITTED_TEXT);
    let server = NormalTerminalServer::spawn();
    let (session, projection) = ordinary_custody::prepare(fixture, &server, false);
    let outcome = ordinary_custody::execute(fixture, projection, &fixture.cancellation).unwrap();
    let OrdinaryTurnExecutionOutcome::Terminal { projection, status } = outcome else {
        panic!("historical source did not complete: {outcome:?}");
    };
    assert_eq!(status, syndic_storage::TurnEndStatus::complete());
    let head = fixture
        .storage
        .transcript_view_head(&fixture.home(), fixture.thread, syndic::point_limit())
        .unwrap()
        .unwrap();
    let entries = fixture
        .storage
        .transcript_entries(
            &fixture.home(),
            fixture.thread,
            head.generation(),
            None,
            beryl_home_store::CursorReadLimits::new(8, 65_536).unwrap(),
        )
        .unwrap();
    assert!(!entries.has_more());
    let entry = entries
        .records()
        .iter()
        .find(|entry| entry.item_id() == submitted.user_item)
        .unwrap();
    let target = ComposerHostHistoricalTarget {
        thread_id: fixture.thread,
        turn_id: submitted.turn,
        item_id: entry.item_id(),
        transcript: CurrentTranscriptEntryProof::new(head.generation(), entry.position()),
    };
    assert_eq!(
        fixture
            .store
            .exact_stop_worker()
            .selected_submission_availability(fixture.thread),
        Ok(())
    );
    (target, session, projection, server)
}
