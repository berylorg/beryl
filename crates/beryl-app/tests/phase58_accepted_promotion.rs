#![cfg(feature = "test-faults")]

use std::time::Duration;

use beryl_app::input_admission::{
    accepted_input_promotion_command, accepted_input_promotion_status,
};
use beryl_home_store::{
    HomeCommand, HomeHealthState,
    test_faults::{FaultController, FaultPoint},
};
use beryl_model::{SyndicDraftId, SyndicItemId};
use beryl_state::AssetOwner;
use syndic_storage::{AcceptedInputAdmission, AcceptedInputPromotionStatus, SyndicTimestamp};

#[path = "phase58_accepted_promotion/support.rs"]
mod support;

use support::{Fixture, FixtureAssets, point_limit};

#[test]
fn marker_free_promotion_reconciles_prior_exact_and_collisions() {
    let fixture = Fixture::new(10, FixtureAssets::MarkerFree);
    let promotion = fixture.promotion(80);
    let draft_before = fixture
        .syndic
        .draft(&fixture.store, fixture.current_draft, point_limit())
        .unwrap()
        .unwrap();
    let home_before = fixture.store.home_revision().unwrap().get();
    assert_eq!(
        accepted_input_promotion_status(
            &fixture.store,
            fixture.syndic,
            fixture.state.assets(),
            &promotion,
            point_limit(),
        )
        .unwrap(),
        AcceptedInputPromotionStatus::Prior
    );

    let command = accepted_input_promotion_command(
        &fixture.store,
        fixture.syndic,
        fixture.state.assets(),
        promotion.clone(),
    )
    .unwrap();
    fixture.store.execute(command).unwrap();

    assert_eq!(
        fixture.store.home_revision().unwrap().get(),
        home_before + 1
    );
    assert_eq!(
        accepted_input_promotion_status(
            &fixture.store,
            fixture.syndic,
            fixture.state.assets(),
            &promotion,
            point_limit(),
        )
        .unwrap(),
        AcceptedInputPromotionStatus::Exact
    );
    assert!(
        fixture
            .state
            .assets()
            .owner_head(
                &fixture.store,
                AssetOwner::AcceptedInput(fixture.accepted_input),
            )
            .unwrap()
            .is_none()
    );
    assert!(
        fixture
            .state
            .assets()
            .owner_head(
                &fixture.store,
                AssetOwner::SubmittedTurnItem(promotion.successor_item_id()),
            )
            .unwrap()
            .is_none()
    );
    assert_eq!(
        fixture
            .syndic
            .draft(&fixture.store, fixture.current_draft, point_limit())
            .unwrap()
            .unwrap(),
        draft_before
    );

    let collision_fixture = Fixture::new(20, FixtureAssets::MarkerFree);
    let colliding = collision_fixture
        .promotion_with_ids(collision_fixture.parent, SyndicItemId::from_bytes([91; 16]));
    assert_eq!(
        accepted_input_promotion_status(
            &collision_fixture.store,
            collision_fixture.syndic,
            collision_fixture.state.assets(),
            &colliding,
            point_limit(),
        )
        .unwrap(),
        AcceptedInputPromotionStatus::Collision
    );
    let command = accepted_input_promotion_command(
        &collision_fixture.store,
        collision_fixture.syndic,
        collision_fixture.state.assets(),
        colliding,
    )
    .unwrap();
    assert!(collision_fixture.store.execute(command).is_err());

    let valid = collision_fixture.promotion(92);
    assert_eq!(
        accepted_input_promotion_status(
            &collision_fixture.store,
            collision_fixture.syndic,
            collision_fixture.state.assets(),
            &valid,
            point_limit(),
        )
        .unwrap(),
        AcceptedInputPromotionStatus::Prior
    );
    collision_fixture.install_stray_owner(
        AssetOwner::AcceptedInput(collision_fixture.accepted_input),
        94,
    );
    assert_eq!(
        accepted_input_promotion_status(
            &collision_fixture.store,
            collision_fixture.syndic,
            collision_fixture.state.assets(),
            &valid,
            point_limit(),
        )
        .unwrap(),
        AcceptedInputPromotionStatus::Collision
    );
    let command = accepted_input_promotion_command(
        &collision_fixture.store,
        collision_fixture.syndic,
        collision_fixture.state.assets(),
        valid.clone(),
    )
    .unwrap();
    assert!(collision_fixture.store.execute(command).is_err());
    assert_eq!(
        collision_fixture
            .syndic
            .accepted_input_promotion_status(&collision_fixture.store, &valid, point_limit(),)
            .unwrap(),
        AcceptedInputPromotionStatus::Prior
    );
    collision_fixture
        .store
        .validate_registered_domains()
        .unwrap();
}

#[test]
fn cross_domain_promotion_status_survives_later_pending_admission() {
    let fixture = Fixture::new(25, FixtureAssets::MarkerFree);
    let promotion = fixture.promotion(95);
    let command = accepted_input_promotion_command(
        &fixture.store,
        fixture.syndic,
        fixture.state.assets(),
        promotion.clone(),
    )
    .unwrap();
    fixture.store.execute(command).unwrap();

    let current = fixture
        .syndic
        .current_draft(&fixture.store, fixture.thread, point_limit())
        .unwrap()
        .unwrap();
    let gate = fixture
        .syndic
        .input_gate(&fixture.store, fixture.thread, point_limit())
        .unwrap()
        .unwrap();
    let admission = AcceptedInputAdmission::new(
        fixture.thread,
        current.thread().revision(),
        current.draft().id(),
        current.draft().revision(),
        current.draft().content(),
        gate.revision(),
        SyndicDraftId::from_bytes([96; 16]),
        None,
        SyndicTimestamp::from_unix_millis(21),
    );
    let mut command = HomeCommand::new(fixture.store.home_revision().unwrap());
    command
        .add(
            fixture
                .syndic
                .admit_accepted_input(fixture.syndic.revision(&fixture.store).unwrap(), admission),
        )
        .unwrap();
    fixture.store.execute(command).unwrap();

    assert_eq!(
        accepted_input_promotion_status(
            &fixture.store,
            fixture.syndic,
            fixture.state.assets(),
            &promotion,
            point_limit(),
        )
        .unwrap(),
        AcceptedInputPromotionStatus::Exact
    );
    fixture.store.validate_registered_domains().unwrap();
}

#[test]
fn cross_domain_promotion_status_survives_an_inflight_unrelated_home_commit() {
    let faults = FaultController::new();
    let fixture = Fixture::with_faults(26, FixtureAssets::MarkerFree, faults.clone());
    let promotion = fixture.promotion(97);
    let command = accepted_input_promotion_command(
        &fixture.store,
        fixture.syndic,
        fixture.state.assets(),
        promotion.clone(),
    )
    .unwrap();
    fixture.store.execute(command).unwrap();

    let blocked = faults.block_next(FaultPoint::BeforeReadConfirmation);
    let status = std::thread::scope(|scope| {
        let worker = scope.spawn(|| {
            accepted_input_promotion_status(
                &fixture.store,
                fixture.syndic,
                fixture.state.assets(),
                &promotion,
                point_limit(),
            )
        });
        if !blocked.wait_until_reached(Duration::from_secs(10)) {
            blocked.release();
            panic!("promotion reconciliation did not reach the deterministic read cut");
        }
        fixture.install_stray_owner(
            AssetOwner::CurrentDraft(SyndicDraftId::from_bytes([250; 16])),
            245,
        );
        blocked.release();
        worker.join().unwrap()
    });

    assert_eq!(status.unwrap(), AcceptedInputPromotionStatus::Exact);
    fixture.store.validate_registered_domains().unwrap();
}

#[test]
fn image_promotion_moves_only_the_accepted_owner_in_one_home_command() {
    let fixture = Fixture::new(30, FixtureAssets::ImageBearing);
    let promotion = fixture.promotion(100);
    let accepted_proof = fixture.accepted_proof.unwrap();
    let draft_before = fixture
        .syndic
        .draft(&fixture.store, fixture.current_draft, point_limit())
        .unwrap()
        .unwrap();
    let draft_owner = AssetOwner::CurrentDraft(fixture.current_draft);
    let draft_head_before = fixture
        .state
        .assets()
        .owner_head(&fixture.store, draft_owner)
        .unwrap();
    assert_eq!(
        draft_head_before.as_ref().unwrap().set(),
        fixture.draft_proof.unwrap()
    );
    let home_before = fixture.store.home_revision().unwrap().get();
    assert_eq!(
        accepted_input_promotion_status(
            &fixture.store,
            fixture.syndic,
            fixture.state.assets(),
            &promotion,
            point_limit(),
        )
        .unwrap(),
        AcceptedInputPromotionStatus::Prior
    );

    let command = accepted_input_promotion_command(
        &fixture.store,
        fixture.syndic,
        fixture.state.assets(),
        promotion.clone(),
    )
    .unwrap();
    fixture.store.execute(command).unwrap();

    assert_eq!(
        fixture.store.home_revision().unwrap().get(),
        home_before + 1
    );
    assert_eq!(
        accepted_input_promotion_status(
            &fixture.store,
            fixture.syndic,
            fixture.state.assets(),
            &promotion,
            point_limit(),
        )
        .unwrap(),
        AcceptedInputPromotionStatus::Exact
    );
    assert!(
        fixture
            .state
            .assets()
            .owner_head(
                &fixture.store,
                AssetOwner::AcceptedInput(fixture.accepted_input),
            )
            .unwrap()
            .is_none()
    );
    let submitted_head = fixture
        .state
        .assets()
        .owner_head(
            &fixture.store,
            AssetOwner::SubmittedTurnItem(promotion.successor_item_id()),
        )
        .unwrap()
        .unwrap();
    assert_eq!(submitted_head.set(), accepted_proof);
    assert_eq!(
        fixture
            .state
            .assets()
            .owner_head(&fixture.store, draft_owner)
            .unwrap(),
        draft_head_before
    );
    assert_eq!(
        fixture
            .syndic
            .draft(&fixture.store, fixture.current_draft, point_limit())
            .unwrap()
            .unwrap(),
        draft_before
    );
    let item = fixture
        .syndic
        .canonical_item(&fixture.store, promotion.successor_item_id(), point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(
        item.presentation().asset_reference_set(),
        Some(accepted_proof)
    );
    assert_eq!(
        fixture
            .syndic
            .accepted_input(&fixture.store, fixture.accepted_input, point_limit())
            .unwrap()
            .unwrap()
            .asset_reference_set(),
        Some(accepted_proof)
    );
    fixture.store.validate_registered_domains().unwrap();
}

#[test]
fn promotion_fault_cuts_reopen_to_one_cross_domain_side() {
    for (seed, point, expected_status) in [
        (
            40,
            FaultPoint::BeforeCommit,
            AcceptedInputPromotionStatus::Prior,
        ),
        (
            50,
            FaultPoint::AfterCommitBeforePersist,
            AcceptedInputPromotionStatus::Exact,
        ),
        (
            60,
            FaultPoint::AfterPersist,
            AcceptedInputPromotionStatus::Exact,
        ),
    ] {
        let faults = FaultController::new();
        let fixture = Fixture::with_faults(seed, FixtureAssets::ImageBearing, faults.clone());
        let promotion = fixture.promotion(seed.wrapping_add(100));
        let accepted_proof = fixture.accepted_proof.unwrap();
        let draft_owner = AssetOwner::CurrentDraft(fixture.current_draft);
        let draft_head_before = fixture
            .state
            .assets()
            .owner_head(&fixture.store, draft_owner)
            .unwrap();
        let draft_before = fixture
            .syndic
            .draft(&fixture.store, fixture.current_draft, point_limit())
            .unwrap()
            .unwrap();
        let home_before = fixture.store.home_revision().unwrap().get();
        let command = accepted_input_promotion_command(
            &fixture.store,
            fixture.syndic,
            fixture.state.assets(),
            promotion.clone(),
        )
        .unwrap();

        faults.fail_next(point);
        assert!(fixture.store.execute(command).is_err());
        assert_eq!(fixture.store.health().state(), HomeHealthState::Verifying);
        fixture.store.verify_health().unwrap();
        let fixture = fixture.reopen();

        assert_eq!(
            accepted_input_promotion_status(
                &fixture.store,
                fixture.syndic,
                fixture.state.assets(),
                &promotion,
                point_limit(),
            )
            .unwrap(),
            expected_status
        );
        let exact = expected_status == AcceptedInputPromotionStatus::Exact;
        assert_eq!(
            fixture.store.home_revision().unwrap().get(),
            home_before + u64::from(exact)
        );
        let accepted_head = fixture
            .state
            .assets()
            .owner_head(
                &fixture.store,
                AssetOwner::AcceptedInput(fixture.accepted_input),
            )
            .unwrap();
        let submitted_head = fixture
            .state
            .assets()
            .owner_head(
                &fixture.store,
                AssetOwner::SubmittedTurnItem(promotion.successor_item_id()),
            )
            .unwrap();
        if exact {
            assert!(accepted_head.is_none());
            assert_eq!(submitted_head.unwrap().set(), accepted_proof);
        } else {
            assert_eq!(accepted_head.unwrap().set(), accepted_proof);
            assert!(submitted_head.is_none());
        }
        assert_eq!(
            fixture
                .state
                .assets()
                .owner_head(&fixture.store, draft_owner)
                .unwrap(),
            draft_head_before
        );
        assert_eq!(
            fixture
                .syndic
                .draft(&fixture.store, fixture.current_draft, point_limit())
                .unwrap()
                .unwrap(),
            draft_before
        );
        fixture.store.validate_registered_domains().unwrap();
    }
}
