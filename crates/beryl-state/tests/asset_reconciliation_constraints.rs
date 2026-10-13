#![cfg(feature = "test-faults")]

#[path = "asset_reconciliation_constraints/support.rs"]
mod support;

use std::sync::Mutex;

use beryl_home_store::{
    CommandOutcome, HomeCommand, HomeHealthState, HomeOpenCandidate, HomeOpenOptions,
    HomeSchemaVersion, ReconciliationHandle, ReconciliationResolution, test_faults::FaultPoint,
};
use beryl_model::{SyndicDraftId, SyndicItemId};
use beryl_state::{
    AssetOwner, AssetOwnerHeadAssertion, AssetOwnerHeadUpdate, UpdateAssetOwnerHeads,
    ValidateAssetOwnerHeads,
};
use support::*;

static SERIAL: Mutex<()> = Mutex::new(());

fn owners() -> [AssetOwner; 2] {
    [
        AssetOwner::SubmittedTurnItem(SyndicItemId::from_bytes([1; 16])),
        AssetOwner::CurrentDraft(SyndicDraftId::from_bytes([2; 16])),
    ]
}

fn seed_image_head(home: &Home) {
    let proof = image_set(home);
    let assets = home.state.assets();
    execute(
        &home.store,
        assets.update_owner_heads(
            assets.revision(&home.store).unwrap(),
            UpdateAssetOwnerHeads::new(Box::from([AssetOwnerHeadUpdate::replace(
                owners()[0],
                None,
                Some(proof),
            )]))
            .unwrap(),
        ),
    );
}

fn command(home: &Home, probe: Option<u8>) -> HomeCommand {
    let assets = home.state.assets();
    let assertions = owners().map(|owner| {
        AssetOwnerHeadAssertion::new(
            owner,
            assets
                .owner_head(&home.store, owner)
                .unwrap()
                .map(|head| head.expectation()),
        )
    });
    let mut command = HomeCommand::new(home.store.home_revision().unwrap());
    command
        .add(assets.unchanged_owner_heads_for_test(
            assets.revision(&home.store).unwrap(),
            ValidateAssetOwnerHeads::new(Box::from(assertions)).unwrap(),
        ))
        .unwrap();
    if let Some(probe) = probe {
        command
            .add(home.probe.contribution(
                home.store.domain_revision(&home.probe).unwrap(),
                PutProbe(probe),
            ))
            .unwrap();
    }
    command
}

fn installed(home: &Home, command: HomeCommand) -> ReconciliationHandle {
    home.faults.fail_next(FaultPoint::AfterCommitBeforePersist);
    match home.store.execute(command) {
        CommandOutcome::Indeterminate { reconciliation, .. } => reconciliation.install_and_handle(),
        outcome => panic!("expected indeterminate command: {outcome:?}"),
    }
}

#[test]
fn concrete_image_and_marker_free_asset_constraints_compose_new_with_original_receipt() {
    let _serial = SERIAL.lock().unwrap();
    for image in [false, true] {
        let home = open("exact-new");
        if image {
            seed_image_head(&home);
        }
        let assets = home.state.assets();
        let heads_before = owners().map(|owner| assets.owner_head(&home.store, owner).unwrap());
        let home_before = home.store.home_revision().unwrap();
        let assets_before = assets.revision(&home.store).unwrap();
        let probe_before = home.store.domain_revision(&home.probe).unwrap();
        let generation = home.store.health().generation().unwrap();
        let handle = installed(&home, command(&home, Some(2)));
        execute(
            &home.store,
            home.probe.contribution(
                home.store.domain_revision(&home.probe).unwrap(),
                PutProbe(2),
            ),
        );
        let receipt = match home.store.reconcile(&handle).unwrap() {
            ReconciliationResolution::ExactNew { receipt } => receipt,
            outcome => panic!("exact Asset constraints must support new side: {outcome:?}"),
        };
        assert_eq!(receipt.home_revision(), home_before.checked_next().unwrap());
        assert_ne!(receipt.home_revision(), home.store.home_revision().unwrap());
        assert_eq!(receipt.generation(), generation);
        assert_eq!(
            assets.committed_revision(&home.store, &receipt).unwrap(),
            Some(assets_before.checked_next().unwrap())
        );
        assert_eq!(
            home.store
                .receipt_domain_revision(&receipt, &home.probe)
                .unwrap(),
            Some(probe_before.checked_next().unwrap())
        );
        assert_eq!(
            owners().map(|owner| assets.owner_head(&home.store, owner).unwrap()),
            heads_before
        );
        assert!(home.store.pending_reconciliations().is_empty());
        assert!(matches!(
            home.store.reconcile(&handle).unwrap(),
            ReconciliationResolution::ExactNew { .. }
        ));
        assert_eq!(home.store.health().state(), HomeHealthState::Healthy);
        home.store.close().unwrap();
    }
}

#[test]
fn concrete_asset_constraints_compose_old_without_using_advanced_revisions() {
    let _serial = SERIAL.lock().unwrap();
    for image in [false, true] {
        let home = open("exact-old");
        if image {
            seed_image_head(&home);
        }
        let assets = home.state.assets();
        let heads_before = owners().map(|owner| assets.owner_head(&home.store, owner).unwrap());
        let assets_before = assets.revision(&home.store).unwrap();
        let probe_before = home.store.domain_revision(&home.probe).unwrap();
        let handle = installed(&home, command(&home, Some(2)));
        execute(
            &home.store,
            home.probe.contribution(
                home.store.domain_revision(&home.probe).unwrap(),
                PutProbe(1),
            ),
        );
        assert!(assets.revision(&home.store).unwrap() > assets_before);
        assert!(home.store.domain_revision(&home.probe).unwrap() > probe_before);
        assert_eq!(
            home.store.reconcile(&handle).unwrap(),
            ReconciliationResolution::ExactOld
        );
        assert_eq!(
            owners().map(|owner| assets.owner_head(&home.store, owner).unwrap()),
            heads_before
        );
        assert!(home.store.pending_reconciliations().is_empty());
        home.store.close().unwrap();
    }
}

#[test]
fn concrete_asset_only_and_all_neutral_facts_cannot_classify_commit_from_revisions() {
    let _serial = SERIAL.lock().unwrap();
    for image in [false, true] {
        for probe in [None, Some(1)] {
            let home = open("all-neutral");
            if image {
                seed_image_head(&home);
            }
            let assets = home.state.assets();
            let assets_before = assets.revision(&home.store).unwrap();
            let handle = installed(&home, command(&home, probe));
            assert!(assets.revision(&home.store).unwrap() > assets_before);
            assert_eq!(
                home.store.reconcile(&handle).unwrap(),
                ReconciliationResolution::Collision
            );
            assert_eq!(
                home.store.reconcile(&handle).unwrap(),
                ReconciliationResolution::Collision
            );
            assert_eq!(home.store.health().state(), HomeHealthState::Healthy);
            home.store.close().unwrap();
        }
    }
}

#[test]
fn concrete_asset_constraint_mismatch_closes_only_the_affected_operation() {
    let _serial = SERIAL.lock().unwrap();
    for mismatch_present in [false, true] {
        let home = open("mismatch");
        seed_image_head(&home);
        let assets = home.state.assets();
        let present = assets
            .owner_head(&home.store, owners()[0])
            .unwrap()
            .unwrap();
        let handle = installed(&home, command(&home, Some(2)));
        let update = if mismatch_present {
            AssetOwnerHeadUpdate::replace(owners()[0], Some(present.expectation()), None)
        } else {
            AssetOwnerHeadUpdate::replace(owners()[1], None, Some(present.set()))
        };
        execute(
            &home.store,
            assets.update_owner_heads(
                assets.revision(&home.store).unwrap(),
                UpdateAssetOwnerHeads::new(Box::from([update])).unwrap(),
            ),
        );
        assert_eq!(
            home.store.reconcile(&handle).unwrap(),
            ReconciliationResolution::Collision
        );
        execute(
            &home.store,
            home.probe.contribution(
                home.store.domain_revision(&home.probe).unwrap(),
                PutProbe(3),
            ),
        );
        assert_eq!(
            home.store.reconcile(&handle).unwrap(),
            ReconciliationResolution::Collision
        );
        assert_eq!(home.store.health().state(), HomeHealthState::Healthy);
        let path = home.path.clone();
        home.store.close().unwrap();
        drop(assets);
        drop(home.state);
        drop(home.probe);
        drop(handle);
        drop(
            HomeOpenCandidate::open(HomeOpenOptions::new(path, HomeSchemaVersion::CURRENT))
                .unwrap(),
        );
    }
}

#[test]
fn resolved_concrete_asset_constraints_release_retained_home_ownership() {
    let _serial = SERIAL.lock().unwrap();
    let home = open("disposal");
    seed_image_head(&home);
    let handle = installed(&home, command(&home, Some(2)));
    assert!(matches!(
        home.store.reconcile(&handle).unwrap(),
        ReconciliationResolution::ExactNew { .. }
    ));
    assert!(home.store.pending_reconciliations().is_empty());
    let path = home.path.clone();
    drop(home.probe);
    drop(home.state);
    drop(home.store);
    drop(HomeOpenCandidate::open(HomeOpenOptions::new(path, HomeSchemaVersion::CURRENT)).unwrap());
    drop(handle);
}
