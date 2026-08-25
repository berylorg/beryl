use beryl_app::{
    composer_host::{
        ComposerHostSubmissionAdvance, ComposerHostSubmissionRequest, ComposerHostSubmissionStage,
    },
    composer_marker_seal::DraftMarkerSealService,
};
use beryl_home_store::CommandCancellation;
use beryl_model::{SyndicDraftId, SyndicItemId};
use beryl_state::{AssetState, BerylState};
use syndic_storage::{
    DraftComposerMaterializationOperationIdV1, DraftPieceOperationIdV1, FirstAcceptanceKind,
    SyndicPointReadLimit, SyndicTimestamp,
};

#[path = "phase141_syndic_composer_host/support.rs"]
mod base;
#[path = "phase166_syndic_composer_history/support.rs"]
mod composer;
#[path = "phase172_syndic_composer_publication/support.rs"]
mod publication;
#[cfg(feature = "test-faults")]
use composer as composer_support;
#[cfg(feature = "test-faults")]
use publication as publication_support;
#[cfg(feature = "test-faults")]
#[path = "phase175_exact_root_submission/corrections.rs"]
mod corrections;
#[cfg(feature = "test-faults")]
#[path = "phase58_accepted_promotion/support.rs"]
mod promotion_support;

use base::fixture;
use composer::{activated, commit_text, history_intent, operation_id};
use publication::service;

#[test]
fn exact_published_root_streams_to_idle_acceptance_and_releases_all_custody() {
    let (_home, mut store, storage, thread) = fixture("phase175-idle", 1);
    let assets = BerylState::register(&mut store).unwrap().assets();
    let seals = service(&store, storage, assets, 1, 1);
    let (mut host, empty) = activated(storage, &store, thread, 2, 3);
    let edited = commit_text(&mut host, &store, empty, 1, 0, 0, "submitted text", 14, 1);
    let next_draft = SyndicDraftId::from_bytes([20; 16]);
    let item = SyndicItemId::from_bytes([21; 16]);
    let ticket = host
        .begin_submission(ComposerHostSubmissionRequest::new(
            next_draft,
            item,
            DraftComposerMaterializationOperationIdV1::from_bytes([22; 16]),
            DraftPieceOperationIdV1::from_bytes([23; 16]),
            SyndicTimestamp::from_unix_millis(50),
            admission_requirement(),
        ))
        .unwrap();

    let outcome = drive_submission(&mut host, &store, assets, &seals, ticket, operation_id(25));
    assert_eq!(
        outcome,
        ComposerHostSubmissionAdvance::ExactSuccess(FirstAcceptanceKind::Idle {
            user_item_id: item,
        })
    );
    assert!(host.binding().is_none());
    assert!(!host.submission_diagnostics().pending());
    assert_eq!(host.submission_diagnostics().retained_roots(), 0);
    assert_eq!(host.submission_diagnostics().retained_materializations(), 0);
    store
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap();

    let current = storage
        .current_draft(&store, thread, point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(current.draft().id(), next_draft);
    let submitted = storage
        .canonical_item(&store, item, point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(
        submitted
            .presentation_content()
            .unwrap()
            .summary()
            .logical_utf8_bytes(),
        14
    );
    assert!(
        storage
            .accepted_input(
                &store,
                edited.candidate().draft_id().accepted_input_id(),
                point_limit(),
            )
            .unwrap()
            .is_none()
    );
}

#[test]
fn captured_submission_blocks_later_edits_and_retains_only_bounded_authority() {
    let (_home, mut store, storage, thread) = fixture("phase175-capture-barrier", 31);
    let assets = BerylState::register(&mut store).unwrap().assets();
    let seals = service(&store, storage, assets, 1, 1);
    let (mut host, empty) = activated(storage, &store, thread, 32, 33);
    let edited = commit_text(&mut host, &store, empty, 1, 0, 0, "root", 4, 1);
    let ticket = host
        .begin_submission(ComposerHostSubmissionRequest::new(
            SyndicDraftId::from_bytes([34; 16]),
            SyndicItemId::from_bytes([35; 16]),
            DraftComposerMaterializationOperationIdV1::from_bytes([36; 16]),
            DraftPieceOperationIdV1::from_bytes([37; 16]),
            SyndicTimestamp::from_unix_millis(50),
            admission_requirement(),
        ))
        .unwrap();

    loop {
        let outcome = host
            .advance_submission(
                &store,
                ticket,
                assets,
                &seals,
                operation_id(39),
                None,
                SyndicTimestamp::from_unix_millis(39),
                &CommandCancellation::new(),
            )
            .unwrap();
        if host.submission_diagnostics().stage() == Some(ComposerHostSubmissionStage::Materializing)
        {
            break;
        }
        assert!(matches!(
            outcome,
            ComposerHostSubmissionAdvance::Progress(_)
        ));
    }
    let diagnostics = host.submission_diagnostics();
    assert_eq!(diagnostics.retained_roots(), 1);
    assert_eq!(diagnostics.retained_materializations(), 0);
    assert!(
        host.begin_history_selection(
            &store,
            edited,
            history_intent(
                edited,
                2,
                gpui_text_input::MutationKind::Undo,
                gpui_text_input::SourcePosition::new(
                    gpui_text_input::ByteOffset::new(4),
                    gpui_text_input::InlineObjectGap::NoObjects,
                ),
            ),
        )
        .is_err()
    );
}

#[test]
fn busy_thread_uses_the_same_exact_root_boundary_for_accepted_next() {
    let (_home, mut store, storage, thread) = fixture("phase175-accepted-next", 61);
    let assets = BerylState::register(&mut store).unwrap().assets();
    let seals = service(&store, storage, assets, 1, 1);
    let (mut host, empty) = activated(storage, &store, thread, 62, 63);
    let first = commit_text(&mut host, &store, empty, 1, 0, 0, "first", 5, 1);
    let first_ticket = host
        .begin_submission(ComposerHostSubmissionRequest::new(
            SyndicDraftId::from_bytes([64; 16]),
            SyndicItemId::from_bytes([65; 16]),
            DraftComposerMaterializationOperationIdV1::from_bytes([66; 16]),
            DraftPieceOperationIdV1::from_bytes([67; 16]),
            SyndicTimestamp::from_unix_millis(70),
            admission_requirement(),
        ))
        .unwrap();
    assert!(matches!(
        drive_submission(
            &mut host,
            &store,
            assets,
            &seals,
            first_ticket,
            operation_id(68),
        ),
        ComposerHostSubmissionAdvance::ExactSuccess(FirstAcceptanceKind::Idle { .. })
    ));

    let (mut host, empty) = activated(storage, &store, thread, 71, 72);
    let second = commit_text(&mut host, &store, empty, 1, 0, 0, "queued", 6, 1);
    let accepted_id = second.candidate().draft_id().accepted_input_id();
    let second_ticket = host
        .begin_submission(ComposerHostSubmissionRequest::new(
            SyndicDraftId::from_bytes([73; 16]),
            SyndicItemId::from_bytes([74; 16]),
            DraftComposerMaterializationOperationIdV1::from_bytes([75; 16]),
            DraftPieceOperationIdV1::from_bytes([76; 16]),
            SyndicTimestamp::from_unix_millis(100),
            admission_requirement(),
        ))
        .unwrap();
    assert_eq!(
        drive_submission_at(
            &mut host,
            &store,
            assets,
            &seals,
            second_ticket,
            operation_id(77),
            SyndicTimestamp::from_unix_millis(90),
        ),
        ComposerHostSubmissionAdvance::ExactSuccess(FirstAcceptanceKind::Accepted)
    );
    let accepted = storage
        .accepted_input(&store, accepted_id, point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(accepted.content().summary().logical_utf8_bytes(), 6);
    assert_eq!(accepted.ordinal().get(), 1);
    assert_eq!(
        storage
            .current_draft(&store, thread, point_limit())
            .unwrap()
            .unwrap()
            .draft()
            .id(),
        SyndicDraftId::from_bytes([73; 16])
    );
    assert_eq!(
        first.candidate().draft_id().submitted_turn_id(),
        storage
            .input_gate(&store, thread, point_limit())
            .unwrap()
            .unwrap()
            .state()
            .blocking_turn_id()
            .unwrap()
    );
}

#[test]
fn empty_rejection_preserves_the_exact_draft_and_starts_no_model_work() {
    let (_home, mut store, storage, thread) = fixture("phase175-empty", 101);
    let assets = BerylState::register(&mut store).unwrap().assets();
    let seals = service(&store, storage, assets, 1, 1);
    let (mut host, empty) = activated(storage, &store, thread, 102, 103);
    let ticket = host
        .begin_submission(ComposerHostSubmissionRequest::new(
            SyndicDraftId::from_bytes([104; 16]),
            SyndicItemId::from_bytes([105; 16]),
            DraftComposerMaterializationOperationIdV1::from_bytes([106; 16]),
            DraftPieceOperationIdV1::from_bytes([107; 16]),
            SyndicTimestamp::from_unix_millis(110),
            admission_requirement(),
        ))
        .unwrap();
    loop {
        match host.advance_submission(
            &store,
            ticket,
            assets,
            &seals,
            operation_id(108),
            None,
            SyndicTimestamp::from_unix_millis(109),
            &CommandCancellation::new(),
        ) {
            Ok(ComposerHostSubmissionAdvance::Progress(_))
            | Ok(ComposerHostSubmissionAdvance::ReconciliationPending) => {}
            Ok(ComposerHostSubmissionAdvance::NotCommitted)
            | Err(beryl_app::composer_host::ComposerHostSubmissionError::Empty) => break,
            other => panic!("empty submission reached an unexpected outcome: {other:?}"),
        }
    }
    assert_eq!(host.binding().unwrap().candidate(), empty.candidate());
    assert!(!host.submission_diagnostics().pending());
    let current = storage
        .current_draft(&store, thread, point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(current.draft().id(), empty.candidate().draft_id());
    assert!(matches!(
        storage
            .input_gate(&store, thread, point_limit())
            .unwrap()
            .unwrap()
            .state(),
        syndic_storage::InputGateState::Idle
    ));
}

#[cfg(feature = "test-faults")]
#[path = "phase175_exact_root_submission/legacy_reconciliation.rs"]
mod legacy_reconciliation;
fn advance_to_accepting(
    host: &mut beryl_app::composer_host::SyndicComposerHost,
    store: &beryl_home_store::HomeStore,
    assets: AssetState,
    seals: &DraftMarkerSealService,
    ticket: beryl_app::composer_host::ComposerHostSubmissionTicket,
    publication_operation: DraftPieceOperationIdV1,
    published_at: SyndicTimestamp,
) {
    for _ in 0..128 {
        let outcome = host
            .advance_submission(
                store,
                ticket,
                assets,
                seals,
                publication_operation,
                None,
                published_at,
                &CommandCancellation::new(),
            )
            .unwrap();
        if outcome
            == ComposerHostSubmissionAdvance::Progress(ComposerHostSubmissionStage::Accepting)
        {
            return;
        }
    }
    panic!("submission did not reach acceptance within the bounded test drive")
}

fn drive_submission(
    host: &mut beryl_app::composer_host::SyndicComposerHost,
    store: &beryl_home_store::HomeStore,
    assets: AssetState,
    seals: &DraftMarkerSealService,
    ticket: beryl_app::composer_host::ComposerHostSubmissionTicket,
    publication_operation: DraftPieceOperationIdV1,
) -> ComposerHostSubmissionAdvance {
    drive_submission_at(
        host,
        store,
        assets,
        seals,
        ticket,
        publication_operation,
        SyndicTimestamp::from_unix_millis(40),
    )
}

fn drive_submission_at(
    host: &mut beryl_app::composer_host::SyndicComposerHost,
    store: &beryl_home_store::HomeStore,
    assets: AssetState,
    seals: &DraftMarkerSealService,
    ticket: beryl_app::composer_host::ComposerHostSubmissionTicket,
    publication_operation: DraftPieceOperationIdV1,
    published_at: SyndicTimestamp,
) -> ComposerHostSubmissionAdvance {
    for _ in 0..128 {
        let outcome = host
            .advance_submission(
                store,
                ticket,
                assets,
                seals,
                publication_operation,
                None,
                published_at,
                &CommandCancellation::new(),
            )
            .unwrap();
        if !matches!(outcome, ComposerHostSubmissionAdvance::Progress(_)) {
            return outcome;
        }
    }
    panic!("submission did not settle within the bounded test drive")
}

fn point_limit() -> SyndicPointReadLimit {
    SyndicPointReadLimit::new(64 * 1024).unwrap()
}

fn admission_requirement() -> beryl_home_store::TurnStartAdmissionRequirement {
    beryl_app::cas_projection::ProjectionServiceConfig::try_new(
        1,
        4,
        beryl_home_store::MinimumTurnCaptureReserve::try_new(1).unwrap(),
    )
    .unwrap()
    .turn_start_admission_requirement()
}
