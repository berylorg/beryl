#![cfg(feature = "test-faults")]

#[path = "unchanged_reconciliation/support.rs"]
mod support;

use std::sync::{Mutex, atomic::Ordering};

use beryl_home_store::{
    CommandError, CommandOutcome, HomeCommand, HomeHealthState, HomeOpenCandidate, HomeOpenOptions,
    HomeSchemaVersion, ReconciliationResolution,
};
use support::*;

static SERIAL: Mutex<()> = Mutex::new(());

fn seed(home: &Home, source: u64, asset: u64, passive: u64) {
    let mut command = HomeCommand::new(home.store.home_revision().unwrap());
    command
        .add(home.source.contribution(
            home.store.domain_revision(&home.source).unwrap(),
            Edits::<Source, SourceRecord>::new(&[(1, Some(source))]),
        ))
        .unwrap();
    command
        .add(home.asset.contribution(
            home.store.domain_revision(&home.asset).unwrap(),
            Edits::<Asset, AssetRecord>::new(&[(1, Some(asset)), (13, Some(42))]),
        ))
        .unwrap();
    command
        .add(home.passive.contribution(
            home.store.domain_revision(&home.passive).unwrap(),
            Edits::<Passive, PassiveRecord>::new(&[(1, Some(passive))]),
        ))
        .unwrap();
    committed(home.store.execute(command));
}

fn ordinary_command(
    home: &Home,
    source: &[(u64, Option<u64>)],
    asset: &[(u64, Option<u64>)],
) -> HomeCommand {
    let mut command = HomeCommand::new(home.store.home_revision().unwrap());
    command
        .add(home.source.contribution(
            home.store.domain_revision(&home.source).unwrap(),
            Edits::<Source, SourceRecord>::new(source),
        ))
        .unwrap();
    command
        .add(home.asset.contribution(
            home.store.domain_revision(&home.asset).unwrap(),
            Edits::<Asset, AssetRecord>::new(asset),
        ))
        .unwrap();
    command
}

#[test]
fn unchanged_present_and_absent_constraints_compose_new_and_restore_original_receipt() {
    let _serial = SERIAL.lock().unwrap();
    for neutral in [Some(1), None] {
        let home = open("neutral-new");
        seed(&home, 1, 1, 1);
        let source_before = home.store.domain_revision(&home.source).unwrap();
        let asset_before = home.store.domain_revision(&home.asset).unwrap();
        let home_before = home.store.home_revision().unwrap();
        let generation = home.store.health().generation().unwrap();
        let neutral_key = if neutral.is_some() { 1 } else { 7 };
        let command =
            ordinary_command(&home, &[(1, Some(2)), (7, None)], &[(neutral_key, neutral)]);
        let handle = installed(&home, command);
        committed(
            home.store.execute_current(
                home.passive
                    .current_command(Edits::<Passive, PassiveRecord>::new(&[(1, Some(3))])),
            ),
        );
        let receipt = match home.store.reconcile(&handle).unwrap() {
            ReconciliationResolution::ExactNew { receipt } => receipt,
            outcome => panic!("expected exact new: {outcome:?}"),
        };
        assert_eq!(receipt.home_revision(), home_before.checked_next().unwrap());
        assert_ne!(receipt.home_revision(), home.store.home_revision().unwrap());
        assert_eq!(receipt.generation(), generation);
        assert_eq!(
            home.store
                .receipt_domain_revision(&receipt, &home.source)
                .unwrap(),
            Some(source_before.checked_next().unwrap())
        );
        assert_eq!(
            home.store
                .receipt_domain_revision(&receipt, &home.asset)
                .unwrap(),
            Some(asset_before.checked_next().unwrap())
        );
        assert_eq!(
            home.store
                .receipt_domain_revision(&receipt, &home.passive)
                .unwrap(),
            None
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
fn neutral_constraints_compose_old_despite_advanced_home_and_domain_revisions() {
    let _serial = SERIAL.lock().unwrap();
    let home = open("neutral-old");
    seed(&home, 1, 1, 1);
    let home_before = home.store.home_revision().unwrap();
    let source_before = home.store.domain_revision(&home.source).unwrap();
    let command = ordinary_command(
        &home,
        &[(1, Some(2)), (7, None)],
        &[(1, Some(1)), (7, None)],
    );
    let handle = installed(&home, command);
    committed(
        home.store.execute_current(
            home.source
                .current_command(Edits::<Source, SourceRecord>::new(&[(1, Some(1))])),
        ),
    );
    assert!(home.store.home_revision().unwrap() > home_before);
    assert!(home.store.domain_revision(&home.source).unwrap() > source_before);
    assert_eq!(
        home.store.reconcile(&handle).unwrap(),
        ReconciliationResolution::ExactOld
    );
    assert!(home.store.pending_reconciliations().is_empty());
    assert_eq!(
        home.store.reconcile(&handle).unwrap(),
        ReconciliationResolution::ExactOld
    );
    home.store.close().unwrap();
}

#[test]
fn all_unchanged_domains_collide_and_empty_contributions_are_refused_before_writer() {
    let _serial = SERIAL.lock().unwrap();
    for (source, asset) in [
        (vec![(1, Some(1))], vec![(7, None)]),
        (vec![], vec![(1, Some(2))]),
        (vec![], vec![]),
    ] {
        let home = open("neutral-or-empty");
        seed(&home, 1, 1, 1);
        let home_before = home.store.home_revision().unwrap();
        let source_before = home.store.domain_revision(&home.source).unwrap();
        let asset_before = home.store.domain_revision(&home.asset).unwrap();
        let command = ordinary_command(&home, &source, &asset);
        if source.is_empty() {
            assert!(matches!(
                home.store.execute(command),
                CommandOutcome::NotCommitted {
                    evidence: CommandError::EmptyContribution {
                        domain: "unchanged-source"
                    },
                }
            ));
            assert_eq!(home.store.home_revision().unwrap(), home_before);
            assert_eq!(
                home.store.domain_revision(&home.source).unwrap(),
                source_before
            );
            assert_eq!(
                home.store.domain_revision(&home.asset).unwrap(),
                asset_before
            );
            assert!(home.store.pending_reconciliations().is_empty());
        } else {
            let handle = installed(&home, command);
            assert!(home.store.home_revision().unwrap() > home_before);
            assert_eq!(
                home.store.reconcile(&handle).unwrap(),
                ReconciliationResolution::Collision
            );
            assert_eq!(
                home.store.reconcile(&handle).unwrap(),
                ReconciliationResolution::Collision
            );
        }
        assert_eq!(home.store.health().state(), HomeHealthState::Healthy);
        home.store.close().unwrap();
    }
}

#[test]
fn mismatched_neutral_and_incompatible_changed_observations_seal_collision() {
    let _serial = SERIAL.lock().unwrap();
    for mismatch_neutral in [true, false] {
        let home = open("neutral-mismatch");
        seed(&home, 1, 1, 1);
        let asset = if mismatch_neutral { 1 } else { 2 };
        let command = ordinary_command(&home, &[(1, Some(2)), (7, None)], &[(1, Some(asset))]);
        let handle = installed(&home, command);
        committed(
            home.store.execute_current(home.asset.current_command(
                Edits::<Asset, AssetRecord>::new(&[(
                    1,
                    Some(if mismatch_neutral { 3 } else { 1 }),
                )]),
            )),
        );
        assert_eq!(
            home.store.reconcile(&handle).unwrap(),
            ReconciliationResolution::Collision
        );
        assert_eq!(home.store.health().state(), HomeHealthState::Healthy);
        home.store.close().unwrap();
    }
}

fn successor_command(home: &Home, source: u64, asset: u64, passive: u64) -> HomeCommand {
    let mut command = HomeCommand::new(home.store.home_revision().unwrap());
    command
        .add(home.source.contribution(
            home.store.domain_revision(&home.source).unwrap(),
            Edits::<Source, SourceRecord>::promotion(source, true),
        ))
        .unwrap();
    command
        .add(home.asset.contribution(
            home.store.domain_revision(&home.asset).unwrap(),
            Edits::<Asset, AssetRecord>::promotion(asset),
        ))
        .unwrap();
    command
        .add(home.passive.contribution(
            home.store.domain_revision(&home.passive).unwrap(),
            Edits::<Passive, PassiveRecord>::new(&[(1, Some(passive))]),
        ))
        .unwrap();
    command
}

#[test]
fn unchanged_source_witness_and_passive_roles_cannot_authorize_successor() {
    let _serial = SERIAL.lock().unwrap();
    for neutral_role in 0..3 {
        let home = open("neutral-successor-role");
        seed(
            &home,
            if neutral_role == 0 { 2 } else { 1 },
            if neutral_role == 1 { 2 } else { 1 },
            1,
        );
        let command = successor_command(&home, 2, 2, if neutral_role == 2 { 1 } else { 2 });
        let handle = installed(&home, command);
        if neutral_role != 0 {
            committed(
                home.store
                    .execute_current(
                        home.source
                            .current_command(Edits::<Source, SourceRecord>::new(&[(1, Some(42))])),
                    ),
            );
        }
        if neutral_role != 1 {
            committed(
                home.store.execute_current(
                    home.asset
                        .current_command(Edits::<Asset, AssetRecord>::new(&[(1, Some(42))])),
                ),
            );
        }
        assert_eq!(
            home.store.reconcile(&handle).unwrap(),
            ReconciliationResolution::Collision
        );
        assert_eq!(SOURCE_CALLS.load(Ordering::SeqCst), 0);
        assert_eq!(ASSET_CALLS.load(Ordering::SeqCst), 0);
        home.store.close().unwrap();
    }
}

#[test]
fn ordinary_exact_composition_with_neutral_roles_precedes_successor_hooks() {
    let _serial = SERIAL.lock().unwrap();
    for old in [false, true] {
        let home = open("ordinary-before-successor");
        seed(&home, 1, 2, 1);
        let home_before = home.store.home_revision().unwrap();
        let command = successor_command(&home, 2, 2, 1);
        let handle = installed(&home, command);
        if old {
            committed(
                home.store
                    .execute_current(
                        home.source
                            .current_command(Edits::<Source, SourceRecord>::new(&[(1, Some(1))])),
                    ),
            );
        }
        match (old, home.store.reconcile(&handle).unwrap()) {
            (true, ReconciliationResolution::ExactOld) => {}
            (false, ReconciliationResolution::ExactNew { receipt }) => {
                assert_eq!(receipt.home_revision(), home_before.checked_next().unwrap());
            }
            (_, result) => panic!("ordinary exact result must precede successor work: {result:?}"),
        }
        assert_eq!(SOURCE_CALLS.load(Ordering::SeqCst), 0);
        assert_eq!(ASSET_CALLS.load(Ordering::SeqCst), 0);
        assert!(home.store.pending_reconciliations().is_empty());
        home.store.close().unwrap();
    }
}

#[test]
fn eligible_changed_successor_roles_still_authenticate_and_retain_original_receipt() {
    let _serial = SERIAL.lock().unwrap();
    let home = open("eligible-successor");
    seed(&home, 1, 1, 1);
    let home_before = home.store.home_revision().unwrap();
    let command = successor_command(&home, 2, 2, 2);
    let handle = installed(&home, command);
    committed(
        home.store.execute_current(
            home.source
                .current_command(Edits::<Source, SourceRecord>::new(&[(1, Some(42))])),
        ),
    );
    committed(
        home.store.execute_current(
            home.asset
                .current_command(Edits::<Asset, AssetRecord>::new(&[(1, Some(42))])),
        ),
    );
    let receipt = match home.store.reconcile(&handle).unwrap() {
        ReconciliationResolution::ExactSuccessor { receipt } => receipt,
        result => panic!("eligible successor must retain admission: {result:?}"),
    };
    assert_eq!(receipt.home_revision(), home_before.checked_next().unwrap());
    assert_eq!(SOURCE_CALLS.load(Ordering::SeqCst), 1);
    assert_eq!(ASSET_CALLS.load(Ordering::SeqCst), 1);
    assert!(home.store.pending_reconciliations().is_empty());
    home.store.close().unwrap();
}

#[test]
fn resolved_neutral_composition_releases_retained_home_ownership() {
    let _serial = SERIAL.lock().unwrap();
    let path = fixture("neutral-disposal");
    let faults = beryl_home_store::test_faults::FaultController::new();
    let mut candidate = HomeOpenCandidate::open_with_faults(
        HomeOpenOptions::new(&path, HomeSchemaVersion::CURRENT),
        faults.clone(),
    )
    .unwrap();
    let source = candidate.register_domain::<Source>().unwrap();
    let asset = candidate.register_domain::<Asset>().unwrap();
    let store = candidate
        .prepare_publication(
            beryl_home_store::HomeDomainRequirements::new()
                .with_domain::<Source>()
                .unwrap()
                .with_domain::<Asset>()
                .unwrap(),
        )
        .unwrap()
        .publish()
        .unwrap();
    let mut command = HomeCommand::new(store.home_revision().unwrap());
    command
        .add(source.contribution(
            store.domain_revision(&source).unwrap(),
            Edits::<Source, SourceRecord>::new(&[(1, Some(2))]),
        ))
        .unwrap();
    command
        .add(asset.contribution(
            store.domain_revision(&asset).unwrap(),
            Edits::<Asset, AssetRecord>::new(&[(1, None)]),
        ))
        .unwrap();
    faults.fail_next(beryl_home_store::test_faults::FaultPoint::AfterCommitBeforePersist);
    let handle = match store.execute(command) {
        beryl_home_store::CommandOutcome::Indeterminate { reconciliation, .. } => {
            reconciliation.install_and_handle()
        }
        result => panic!("expected indeterminate command: {result:?}"),
    };
    assert!(matches!(
        store.reconcile(&handle).unwrap(),
        ReconciliationResolution::ExactNew { .. }
    ));
    assert!(store.pending_reconciliations().is_empty());
    drop(source);
    drop(asset);
    drop(store);
    let reopened =
        HomeOpenCandidate::open(HomeOpenOptions::new(&path, HomeSchemaVersion::CURRENT)).unwrap();
    drop(reopened);
    drop(handle);
}
