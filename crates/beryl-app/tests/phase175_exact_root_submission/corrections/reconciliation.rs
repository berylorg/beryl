use super::*;

use beryl_home_store::HomeCommand;
use syndic_storage::{
    AcceptedRouteLeafRecord, DraftImageLabelProtectionHeadV1, ImageLabelFrontier,
    test_faults::{
        FixtureBatch, FixtureRecord, accepted_route_generation, reset_syndic_point_read_count,
        syndic_point_read_count,
    },
};

#[test]
fn promoted_image_descendant_reconciles_after_home_restart_without_re_admission() {
    let (_home, mut store, storage, thread, faults) =
        base::fault_fixture("phase175-promoted-descendant-restart", 211);
    let state = BerylState::register(&mut store).unwrap();
    let assets = state.assets();
    let seals = service(&store, storage.clone(), assets.clone(), 1, 1);
    let (mut first_host, empty) = activated(storage.clone(), &store, thread, 212, 213);
    let first = commit_text(&mut first_host, &store, empty, 1, 0, 0, "first", 5, 1);
    let parent = first.candidate().draft_id().submitted_turn_id();
    let parent_item = SyndicItemId::from_bytes([214; 16]);
    let first_ticket = first_host
        .begin_submission(ComposerHostSubmissionRequest::new(
            SyndicDraftId::from_bytes([215; 16]),
            parent_item,
            DraftComposerMaterializationOperationIdV1::from_bytes([216; 16]),
            DraftPieceOperationIdV1::from_bytes([217; 16]),
            SyndicTimestamp::from_unix_millis(218),
            admission_requirement(),
        ))
        .unwrap();
    assert!(matches!(
        drive_submission(
            &mut first_host,
            &store,
            assets.clone(),
            &seals,
            first_ticket,
            operation_id(219),
        ),
        ComposerHostSubmissionAdvance::ExactSuccess(FirstAcceptanceKind::Idle { .. })
    ));

    let (mut queued_host, empty) = activated(storage.clone(), &store, thread, 220, 221);
    let protection = storage
        .draft_image_label_protection_head(&store, thread, point_limit())
        .unwrap()
        .unwrap();
    let mut protection_batch = FixtureBatch::new();
    protection_batch
        .put(FixtureRecord::DraftImageLabelProtectionHead(
            DraftImageLabelProtectionHeadV1::new(
                thread,
                protection.revision() + 1,
                ImageLabelFrontier::from_raw(1),
            )
            .unwrap(),
        ))
        .unwrap();
    base::committed(base::execute(
        &store,
        storage.fixture_contribution(storage.revision(&store).unwrap(), protection_batch),
    ));
    let image =
        publication::publish_image_asset(&store, assets.clone(), b"phase175 accepted image");
    let (binding, _, _) = publication::insert_published_marker_with_readiness(
        &mut queued_host,
        &store,
        &storage,
        empty,
        1,
        image,
    );
    let queued =
        publication::insert_text_after_published_marker(&mut queued_host, &store, binding, 2);
    let accepted_input = queued.candidate().draft_id().accepted_input_id();
    let queued_ticket = queued_host.begin_submission(request(222)).unwrap();
    let authority = Some(publication::authority(223));
    advance_until_stage(
        &mut queued_host,
        &store,
        assets.clone(),
        &seals,
        queued_ticket,
        ComposerHostSubmissionStage::Accepting,
        224,
        authority,
    );
    let injected = faults.clone();
    queued_host.test_arm_submission_before_execute_fault(move |_, _| {
        injected.fail_next(FaultPoint::AfterCommitBeforePersist);
    });
    assert_eq!(
        advance_with_authority(
            &mut queued_host,
            &store,
            assets.clone(),
            &seals,
            queued_ticket,
            224,
            authority,
        )
        .unwrap(),
        ComposerHostSubmissionAdvance::ReconciliationPending
    );
    assert!(queued_host.submission_diagnostics().command_attempted());
    promotion_support::terminalize_parent_fixture(
        &store,
        storage.clone(),
        thread,
        parent,
        parent_item,
        SyndicTimestamp::from_unix_millis(1_230),
    );
    let (mut current_host, empty) = activated(storage.clone(), &store, thread, 228, 229);
    commit_text(&mut current_host, &store, empty, 1, 0, 0, "retained", 8, 1);
    promotion_support::publish_current_draft(
        &mut current_host,
        &store,
        storage.clone(),
        assets.clone(),
        230,
        None,
        SyndicTimestamp::from_unix_millis(1_250),
    );
    let limits = CursorReadLimits::new(256, ACCEPTED_NEXT_PAGE_MAX_BYTES).unwrap();
    let revision = storage.revision(&store).unwrap();
    let source = storage
        .accepted_next_source_page(&store, revision, None, limits)
        .unwrap()
        .records()[0];
    let promotion_candidate = storage
        .accepted_next_candidate_page(&store, source, None, limits)
        .unwrap()
        .into_candidate()
        .unwrap();
    let successor_item = SyndicItemId::from_bytes([225; 16]);
    let promotion = PromoteAcceptedInput::new(
        promotion_candidate,
        SyndicTurnId::from_bytes([226; 16]),
        successor_item,
        SyndicTimestamp::from_unix_millis(1_300),
    );
    let accepted_proof = assets
        .owner_head(&store, AssetOwner::AcceptedInput(accepted_input))
        .unwrap()
        .unwrap()
        .set();
    let command = accepted_input_promotion_command(&store, &storage, &assets, promotion).unwrap();
    assert!(matches!(
        store.execute(command),
        CommandOutcome::Committed {
            later_failure: None,
            ..
        }
    ));
    assert!(
        assets
            .owner_head(&store, AssetOwner::AcceptedInput(accepted_input))
            .unwrap()
            .is_none()
    );
    assert_eq!(
        assets
            .owner_head(&store, AssetOwner::SubmittedTurnItem(successor_item))
            .unwrap()
            .unwrap()
            .set(),
        accepted_proof
    );
    drop(seals);
    faults.fail_next(FaultPoint::BeforeReadConfirmation);
    assert!(store.home_revision().is_err());
    let candidate = store.recover_same_home().unwrap();
    let recovered_state = BerylState::reacquire_candidate(&candidate).unwrap();
    let recovered_storage = SyndicStorage::reacquire_candidate(&candidate).unwrap();
    let store = candidate.publish();
    let assets = recovered_state.assets();
    let seals = service(&store, recovered_storage.clone(), assets.clone(), 1, 1);
    let revision_before_restart_reconciliation = store.home_revision().unwrap();
    assert_eq!(
        advance(&mut queued_host, &store, assets, &seals, queued_ticket, 224,).unwrap(),
        ComposerHostSubmissionAdvance::ExactSuccess(FirstAcceptanceKind::Accepted)
    );
    assert_eq!(
        store.home_revision().unwrap(),
        revision_before_restart_reconciliation
    );
    store
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap();
}

#[test]
fn corrupted_permanent_promoted_route_leaf_is_terminal_collision_without_replay() {
    let (_home, mut store, storage, thread, faults) =
        base::fault_fixture("phase175-corrupt-promoted-leaf", 231);
    let state = BerylState::register(&mut store).unwrap();
    let assets = state.assets();
    let seals = service(&store, storage.clone(), assets.clone(), 1, 1);
    let (mut first_host, empty) = activated(storage.clone(), &store, thread, 232, 233);
    let first = commit_text(&mut first_host, &store, empty, 1, 0, 0, "first", 5, 1);
    let parent = first.candidate().draft_id().submitted_turn_id();
    let parent_item = SyndicItemId::from_bytes([234; 16]);
    let first_ticket = first_host
        .begin_submission(ComposerHostSubmissionRequest::new(
            SyndicDraftId::from_bytes([235; 16]),
            parent_item,
            DraftComposerMaterializationOperationIdV1::from_bytes([236; 16]),
            DraftPieceOperationIdV1::from_bytes([237; 16]),
            SyndicTimestamp::from_unix_millis(238),
            admission_requirement(),
        ))
        .unwrap();
    assert!(matches!(
        drive_submission(
            &mut first_host,
            &store,
            assets.clone(),
            &seals,
            first_ticket,
            operation_id(239),
        ),
        ComposerHostSubmissionAdvance::ExactSuccess(FirstAcceptanceKind::Idle { .. })
    ));

    let (mut queued_host, empty) = activated(storage.clone(), &store, thread, 240, 241);
    let queued = commit_text(&mut queued_host, &store, empty, 1, 0, 0, "queued", 6, 1);
    let accepted_input = queued.candidate().draft_id().accepted_input_id();
    let queued_request = request(242);
    let queued_ticket = queued_host.begin_submission(queued_request).unwrap();
    advance_until_stage(
        &mut queued_host,
        &store,
        assets.clone(),
        &seals,
        queued_ticket,
        ComposerHostSubmissionStage::Accepting,
        244,
        None,
    );
    let queued_at_attempt = queued_host.binding().unwrap();
    let injected = faults.clone();
    queued_host.test_arm_submission_before_execute_fault(move |_, _| {
        injected.fail_next(FaultPoint::AfterCommitBeforePersist);
    });
    assert_eq!(
        advance_with_authority(
            &mut queued_host,
            &store,
            assets.clone(),
            &seals,
            queued_ticket,
            244,
            None,
        )
        .unwrap(),
        ComposerHostSubmissionAdvance::ReconciliationPending
    );
    assert_eq!(store.pending_reconciliations().len(), 1);

    promotion_support::terminalize_parent_fixture(
        &store,
        storage.clone(),
        thread,
        parent,
        parent_item,
        SyndicTimestamp::from_unix_millis(1_330),
    );
    let (mut current_host, empty) = activated(storage.clone(), &store, thread, 245, 246);
    commit_text(&mut current_host, &store, empty, 1, 0, 0, "retained", 8, 1);
    promotion_support::publish_current_draft(
        &mut current_host,
        &store,
        storage.clone(),
        assets.clone(),
        247,
        None,
        SyndicTimestamp::from_unix_millis(1_350),
    );
    let limits = CursorReadLimits::new(256, ACCEPTED_NEXT_PAGE_MAX_BYTES).unwrap();
    let source = storage
        .accepted_next_source_page(&store, storage.revision(&store).unwrap(), None, limits)
        .unwrap()
        .records()[0];
    let promotion_candidate = storage
        .accepted_next_candidate_page(&store, source, None, limits)
        .unwrap()
        .into_candidate()
        .unwrap();
    let successor_item = SyndicItemId::from_bytes([248; 16]);
    let promotion = PromoteAcceptedInput::new(
        promotion_candidate,
        SyndicTurnId::from_bytes([249; 16]),
        successor_item,
        SyndicTimestamp::from_unix_millis(1_400),
    );
    let command = accepted_input_promotion_command(&store, &storage, &assets, promotion).unwrap();
    assert!(matches!(
        store.execute(command),
        CommandOutcome::Committed {
            later_failure: None,
            ..
        }
    ));

    let route =
        accepted_route_generation(&store, storage.clone(), thread, source.generation()).unwrap();
    let promoted = storage
        .accepted_route_page(&store, thread, source.generation(), route.revision(), None)
        .unwrap()
        .records()
        .iter()
        .find(|entry| entry.input().id() == accepted_input)
        .unwrap()
        .leaf()
        .clone();
    assert!(promoted.promotion().is_some());
    let corrupt = AcceptedRouteLeafRecord::new(
        promoted.input_id(),
        promoted.thread_id(),
        promoted.generation(),
        promoted.ordinal(),
        promoted.revision(),
        promoted.state(),
        promoted.lifecycle(),
    );
    let mut batch = FixtureBatch::new();
    batch
        .put(FixtureRecord::AcceptedRouteLeaf(corrupt))
        .unwrap();
    let mut corrupt_command = HomeCommand::new(store.home_revision().unwrap());
    corrupt_command
        .add(storage.fixture_contribution(storage.revision(&store).unwrap(), batch))
        .unwrap();
    assert!(matches!(
        store.execute(corrupt_command),
        CommandOutcome::Committed {
            later_failure: None,
            ..
        }
    ));

    let revision_before_reconciliation = store.home_revision().unwrap();
    reset_syndic_point_read_count();
    assert_eq!(
        advance(
            &mut queued_host,
            &store,
            assets.clone(),
            &seals,
            queued_ticket,
            244,
        )
        .unwrap(),
        ComposerHostSubmissionAdvance::Collision
    );
    assert_eq!(syndic_point_read_count(), 0);
    assert_eq!(
        store.home_revision().unwrap(),
        revision_before_reconciliation
    );
    assert_eq!(store.pending_reconciliations().len(), 1);
    assert_eq!(queued_host.binding(), Some(queued_at_attempt));
    assert!(queued_host.is_unavailable());
    assert!(!queued_host.submission_diagnostics().pending());
    assert_eq!(
        advance(&mut queued_host, &store, assets, &seals, queued_ticket, 244,).unwrap(),
        ComposerHostSubmissionAdvance::Stale
    );
    assert_eq!(
        store.home_revision().unwrap(),
        revision_before_reconciliation
    );
    assert!(queued_host.begin_submission(request(250)).is_err());
    assert_eq!(
        store.home_revision().unwrap(),
        revision_before_reconciliation
    );
}
