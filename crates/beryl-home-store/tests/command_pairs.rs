#[path = "current_home_commands/domains.rs"]
mod domains;
#[path = "command_pairs/fixture.rs"]
mod fixture;
mod support;

#[cfg(feature = "test-faults")]
#[path = "command_pairs/faults.rs"]
mod faults;

use std::{marker::PhantomData, sync::atomic::Ordering};

use beryl_home_store::{
    CommandBuildError, CommandCancellation, CommandError, CommandPairRequirement,
    CommandParticipantKind::{Mutation, Validation},
    CurrentHomeCommand, HomeCommand, StorageDomain,
};
use tempfile::tempdir;

use domains::{AlphaDomain, BetaDomain};
use fixture::{Fixture, PairValidator, pair, requirements};
use support::{PutBytes, committed, not_committed};

#[test]
fn reciprocal_roles_commit_only_mutating_domains_in_either_order() {
    for kinds in [
        [Mutation, Mutation],
        [Mutation, Validation],
        [Validation, Mutation],
    ] {
        for reverse in [false, true] {
            let directory = tempdir().unwrap();
            let fixture = Fixture::new(directory.path());
            let before = fixture.store.home_revision().unwrap();
            let receipt = committed(fixture.store.execute(fixture.command(
                kinds,
                requirements(kinds),
                reverse,
            )));
            assert_eq!(receipt.home_revision().get(), before.get() + 1);
            assert_eq!(fixture.calls.sealed.load(Ordering::SeqCst), 2);
            assert_eq!(fixture.calls.prepared.load(Ordering::SeqCst), 2);
            for (kind, affected, value) in [
                (
                    kinds[0],
                    fixture
                        .store
                        .receipt_domain_revision(&receipt, &fixture.alpha)
                        .unwrap(),
                    fixture.value(&fixture.alpha),
                ),
                (
                    kinds[1],
                    fixture
                        .store
                        .receipt_domain_revision(&receipt, &fixture.beta)
                        .unwrap(),
                    fixture.value(&fixture.beta),
                ),
            ] {
                assert_eq!(affected.is_some(), kind == Mutation);
                assert_eq!(value.is_some(), kind == Mutation);
            }
            assert!(
                fixture
                    .store
                    .receipt_domain_revision(&receipt, &fixture.third)
                    .unwrap()
                    .is_none()
            );
        }
    }
}

#[test]
fn mismatched_or_unbound_roles_refuse_before_all_callbacks_and_writes() {
    static LONG_NAME: [u8; 256] = [b'a'; 256];
    let normal = requirements([Mutation, Mutation]);
    let cases = [
        [normal[0], None],
        [None, normal[1]],
        [normal[0], pair("other", Mutation)],
        [pair("", Mutation), normal[1]],
        [pair("UPPER", Mutation), normal[1]],
        [pair("space name", Mutation), normal[1]],
        [
            pair(std::str::from_utf8(&LONG_NAME).unwrap(), Mutation),
            normal[1],
        ],
        [pair(AlphaDomain::NAME, Mutation), normal[1]],
        [normal[0], pair(AlphaDomain::NAME, Validation)],
        [
            Some(CommandPairRequirement::new::<0, 1>(
                BetaDomain::NAME,
                Mutation,
                [7; 32],
            )),
            normal[1],
        ],
        [
            Some(CommandPairRequirement::new::<1, 0>(
                BetaDomain::NAME,
                Mutation,
                [7; 32],
            )),
            normal[1],
        ],
        [
            Some(CommandPairRequirement::new::<2, 1>(
                BetaDomain::NAME,
                Mutation,
                [7; 32],
            )),
            normal[1],
        ],
        [
            Some(CommandPairRequirement::new::<1, 2>(
                BetaDomain::NAME,
                Mutation,
                [7; 32],
            )),
            normal[1],
        ],
        [
            Some(CommandPairRequirement::new::<1, 1>(
                BetaDomain::NAME,
                Mutation,
                [8; 32],
            )),
            normal[1],
        ],
    ];
    for requirements in cases {
        let directory = tempdir().unwrap();
        let fixture = Fixture::new(directory.path());
        let before = fixture.store.home_revision().unwrap();
        let health = fixture.store.health();
        let command = fixture
            .command([Mutation, Mutation], requirements, false)
            .with_local_admission(|| panic!("invalid pair reached local admission"));
        assert!(matches!(
            not_committed(fixture.store.execute(command)),
            CommandError::InvalidCommandPair
        ));
        assert_eq!(fixture.calls.sealed.load(Ordering::SeqCst), 2);
        assert_eq!(fixture.calls.reserved.load(Ordering::SeqCst), 0);
        assert_eq!(fixture.calls.prepared.load(Ordering::SeqCst), 0);
        assert_eq!(fixture.calls.assembled.load(Ordering::SeqCst), 0);
        assert_eq!(fixture.store.home_revision().unwrap(), before);
        assert_eq!(fixture.store.health().state(), health.state());
        assert_eq!(fixture.store.health().generation(), health.generation());
        assert!(fixture.value(&fixture.alpha).is_none());
        assert!(fixture.value(&fixture.beta).is_none());
        assert!(fixture.store.pending_reconciliations().is_empty());
    }
}

#[test]
fn actual_participant_kinds_cannot_be_replaced_by_declared_peer_kinds() {
    for kinds in [
        [Mutation, Validation],
        [Validation, Mutation],
        [Validation, Validation],
    ] {
        let directory = tempdir().unwrap();
        let fixture = Fixture::new(directory.path());
        let command = fixture.command(kinds, requirements([Mutation, Mutation]), false);
        assert!(matches!(
            not_committed(fixture.store.execute(command)),
            CommandError::InvalidCommandPair
        ));
        assert_eq!(fixture.calls.reserved.load(Ordering::SeqCst), 0);
        assert_eq!(fixture.calls.prepared.load(Ordering::SeqCst), 0);
        assert!(fixture.value(&fixture.alpha).is_none());
        assert!(fixture.value(&fixture.beta).is_none());
    }
}

#[test]
fn missing_and_extra_participants_cannot_commit_a_bound_mutation() {
    for extra in [false, true] {
        let directory = tempdir().unwrap();
        let fixture = Fixture::new(directory.path());
        let mut command = HomeCommand::new(fixture.store.home_revision().unwrap());
        fixture.add(
            &mut command,
            &fixture.alpha,
            Mutation,
            pair(BetaDomain::NAME, Mutation),
        );
        if extra {
            fixture.add(
                &mut command,
                &fixture.beta,
                Mutation,
                pair(AlphaDomain::NAME, Mutation),
            );
            fixture.add(&mut command, &fixture.third, Mutation, None);
        }
        assert!(matches!(
            not_committed(fixture.store.execute(command)),
            CommandError::InvalidCommandPair
        ));
        assert_eq!(fixture.calls.reserved.load(Ordering::SeqCst), 0);
        assert_eq!(fixture.calls.prepared.load(Ordering::SeqCst), 0);
        assert!(fixture.value(&fixture.alpha).is_none());
        assert!(fixture.value(&fixture.beta).is_none());
        assert!(fixture.value(&fixture.third).is_none());
    }
}

#[test]
fn duplicate_domain_keeps_its_build_error() {
    let directory = tempdir().unwrap();
    let fixture = Fixture::new(directory.path());
    let mut command = HomeCommand::new(fixture.store.home_revision().unwrap());
    fixture.add(
        &mut command,
        &fixture.alpha,
        Mutation,
        pair(BetaDomain::NAME, Validation),
    );
    let error = command
        .add_validation(fixture.alpha.validation(
            fixture.store.domain_revision(&fixture.alpha).unwrap(),
            PairValidator::<AlphaDomain> {
                requirement: pair(BetaDomain::NAME, Mutation),
                calls: fixture.calls.clone(),
                reject: false,
                typed: PhantomData,
            },
        ))
        .err()
        .unwrap();
    assert!(matches!(error, CommandBuildError::DuplicateDomain { .. }));
    assert_eq!(fixture.calls.reserved.load(Ordering::SeqCst), 0);
}

#[test]
fn semantic_peer_rejection_prevents_local_admission_and_assembly() {
    let directory = tempdir().unwrap();
    let fixture = Fixture::new(directory.path());
    let mut command = HomeCommand::new(fixture.store.home_revision().unwrap())
        .with_local_admission(|| panic!("rejected peer reached local admission"));
    fixture.add(
        &mut command,
        &fixture.alpha,
        Mutation,
        pair(BetaDomain::NAME, Validation),
    );
    command
        .add_validation(fixture.beta.validation(
            fixture.store.domain_revision(&fixture.beta).unwrap(),
            PairValidator::<BetaDomain> {
                requirement: pair(AlphaDomain::NAME, Mutation),
                calls: fixture.calls.clone(),
                reject: true,
                typed: PhantomData,
            },
        ))
        .unwrap();
    assert!(matches!(
        not_committed(fixture.store.execute(command)),
        CommandError::ContributorValidation { .. }
    ));
    assert_eq!(fixture.calls.prepared.load(Ordering::SeqCst), 2);
    assert_eq!(fixture.calls.assembled.load(Ordering::SeqCst), 0);
    assert!(fixture.value(&fixture.alpha).is_none());
}

#[test]
fn stale_and_foreign_plans_fail_before_preparation() {
    let directory = tempdir().unwrap();
    let fixture = Fixture::new(directory.path());
    let command = fixture
        .command(
            [Mutation, Mutation],
            requirements([Mutation, Mutation]),
            false,
        )
        .with_local_admission(|| panic!("stale pair reached local admission"));
    committed(
        fixture
            .store
            .execute_current(
                fixture
                    .third
                    .current_command(PutBytes::<support::AlphaDomain>::new(
                        3,
                        b"unrelated".to_vec(),
                    )),
            ),
    );
    assert!(matches!(
        not_committed(fixture.store.execute(command)),
        CommandError::Conflict { .. }
    ));
    assert_eq!(fixture.calls.prepared.load(Ordering::SeqCst), 0);
    let other_directory = tempdir().unwrap();
    let other = Fixture::new(other_directory.path());
    let mut foreign = HomeCommand::new(fixture.store.home_revision().unwrap());
    fixture.add(
        &mut foreign,
        &fixture.alpha,
        Mutation,
        pair(BetaDomain::NAME, Mutation),
    );
    other.add(
        &mut foreign,
        &other.beta,
        Mutation,
        pair(AlphaDomain::NAME, Mutation),
    );
    assert!(matches!(
        not_committed(fixture.store.execute(foreign)),
        CommandError::ForeignDomain { .. }
    ));
    assert_eq!(fixture.calls.prepared.load(Ordering::SeqCst), 0);
    assert_eq!(other.calls.prepared.load(Ordering::SeqCst), 0);
    assert!(fixture.value(&fixture.alpha).is_none());
    assert!(fixture.value(&fixture.beta).is_none());
}

#[test]
fn current_commands_preserve_requirements_and_admit_only_a_complete_mutation_pair() {
    let directory = tempdir().unwrap();
    let fixture = Fixture::new(directory.path());
    assert!(matches!(
        not_committed(
            fixture.store.execute_current(fixture.alpha.current_command(
                fixture.mutation::<AlphaDomain>(pair(BetaDomain::NAME, Mutation))
            ),)
        ),
        CommandError::InvalidCommandPair
    ));
    let mut invalid = CurrentHomeCommand::new(
        fixture
            .alpha
            .current_command(fixture.mutation::<AlphaDomain>(pair(BetaDomain::NAME, Mutation))),
    );
    invalid
        .add(
            fixture
                .beta
                .current_command(fixture.mutation::<BetaDomain>(None)),
        )
        .unwrap();
    assert!(matches!(
        not_committed(fixture.store.execute_current_home(invalid)),
        CommandError::InvalidCommandPair
    ));
    assert_eq!(fixture.calls.reserved.load(Ordering::SeqCst), 0);
    let mut valid = CurrentHomeCommand::new(
        fixture
            .alpha
            .current_command(fixture.mutation::<AlphaDomain>(pair(BetaDomain::NAME, Mutation))),
    );
    valid
        .add(
            fixture
                .beta
                .current_command(fixture.mutation::<BetaDomain>(pair(AlphaDomain::NAME, Mutation))),
        )
        .unwrap();
    let receipt = committed(fixture.store.execute_current_home(valid));
    assert!(
        fixture
            .store
            .receipt_domain_revision(&receipt, &fixture.alpha)
            .unwrap()
            .is_some()
    );
    assert!(
        fixture
            .store
            .receipt_domain_revision(&receipt, &fixture.beta)
            .unwrap()
            .is_some()
    );
}

#[test]
fn cancellation_precedes_invalid_pair_but_cannot_hide_an_admitted_result() {
    let directory = tempdir().unwrap();
    let fixture = Fixture::new(directory.path());
    let cancellation = CommandCancellation::new();
    cancellation.cancel();
    let mut command =
        HomeCommand::new(fixture.store.home_revision().unwrap()).with_cancellation(cancellation);
    fixture.add(
        &mut command,
        &fixture.alpha,
        Mutation,
        pair(BetaDomain::NAME, Mutation),
    );
    assert!(matches!(
        not_committed(fixture.store.execute(command)),
        CommandError::CancelledBeforeAdmission
    ));
    assert_eq!(fixture.calls.reserved.load(Ordering::SeqCst), 0);
    let cancellation = CommandCancellation::new();
    let mut command = HomeCommand::new(fixture.store.home_revision().unwrap())
        .with_cancellation(cancellation.clone());
    let mut mutation = fixture.mutation::<AlphaDomain>(pair(BetaDomain::NAME, Mutation));
    mutation.cancellation = Some(cancellation.clone());
    command
        .add(fixture.alpha.contribution(
            fixture.store.domain_revision(&fixture.alpha).unwrap(),
            mutation,
        ))
        .unwrap();
    fixture.add(
        &mut command,
        &fixture.beta,
        Mutation,
        pair(AlphaDomain::NAME, Mutation),
    );
    committed(fixture.store.execute(command));
    assert!(cancellation.is_cancelled());
    assert!(fixture.value(&fixture.alpha).is_some());
    assert!(fixture.value(&fixture.beta).is_some());
}

#[test]
fn local_admission_follows_both_preparations_and_refusal_keeps_records_unchanged() {
    let directory = tempdir().unwrap();
    let fixture = Fixture::new(directory.path());
    let calls = fixture.calls.clone();
    let command = fixture
        .command(
            [Mutation, Mutation],
            requirements([Mutation, Mutation]),
            false,
        )
        .with_local_admission(move || {
            assert_eq!(calls.prepared.load(Ordering::SeqCst), 2);
            false
        });
    assert!(matches!(
        not_committed(fixture.store.execute(command)),
        CommandError::LocalAdmissionRejected
    ));
    assert_eq!(fixture.calls.assembled.load(Ordering::SeqCst), 0);
    assert!(fixture.value(&fixture.alpha).is_none());
    assert!(fixture.value(&fixture.beta).is_none());
}
