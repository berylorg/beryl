use std::{
    num::NonZeroU64,
    path::Path,
    sync::{Arc, Barrier},
    thread,
};

use beryl_home_store::{
    test_faults::FaultController, CommandError, CommandOutcome, HomeCommand, HomeOpenOptions,
    HomeSchemaVersion, HomeStore, SidecarByteLimit,
};
use beryl_model::{
    AdmittedHostPath, Availability, CasThreadId, CasTurnId, ClaimRevision, DynamicToolCallId,
    PathFlavor, ProjectionRevision, ResolutionIntentId, RootId, RuntimeId, SyndicDraftId,
    SyndicThreadId, SyndicTurnId, WindowBounds, WindowDisplayState, WindowId, WindowPlacement,
};
use beryl_state::{
    AssetOwner, BranchHandoffJobAdmission, CatalogArchiveSummary, CatalogAvailabilitySummary,
    CatalogClaimSummary, CatalogExecutionSummary, CatalogFacts, CatalogLineageSummary,
    CatalogResolvedTitle, CatalogSourceRevisions, DiscussionContextDigest,
    DiscussionContextOwnerId, RecordRevision, RememberedTarget, ResolutionAttemptOrdinal,
    ResolutionRequestIdentity, ResolutionText, UnixMillis,
};

pub fn open_with_faults(
    path: &Path,
    faults: FaultController,
) -> (HomeStore, beryl_state::BerylState) {
    let mut store = HomeStore::open_with_faults(
        HomeOpenOptions::new(path, HomeSchemaVersion::CURRENT),
        faults,
    )
    .unwrap();
    let state = beryl_state::BerylState::register(&mut store).unwrap();
    (store, state)
}

pub fn command(
    store: &HomeStore,
    contribution: beryl_home_store::MutationContribution,
) -> HomeCommand {
    let mut command = HomeCommand::new(store.home_revision().unwrap());
    command.add(contribution).unwrap();
    command
}

pub fn race_commands(
    store: &HomeStore,
    first: HomeCommand,
    second: HomeCommand,
) -> [CommandOutcome; 2] {
    let barrier = Arc::new(Barrier::new(3));
    thread::scope(|scope| {
        let first_barrier = Arc::clone(&barrier);
        let first_worker = scope.spawn(move || {
            first_barrier.wait();
            store.execute(first)
        });
        let second_barrier = Arc::clone(&barrier);
        let second_worker = scope.spawn(move || {
            second_barrier.wait();
            store.execute(second)
        });
        barrier.wait();
        [first_worker.join().unwrap(), second_worker.join().unwrap()]
    })
}

pub fn assert_one_success_one_conflict(results: &[CommandOutcome; 2]) {
    for outcome in results {
        match outcome {
            CommandOutcome::Committed {
                later_failure: None,
                ..
            }
            | CommandOutcome::NotCommitted {
                evidence: CommandError::Conflict { .. },
            } => {}
            outcome => panic!("expected clean commit or exact conflict, got {outcome:?}"),
        }
    }
    assert_eq!(
        results
            .iter()
            .filter(|outcome| {
                matches!(
                    outcome,
                    CommandOutcome::Committed {
                        later_failure: None,
                        ..
                    }
                )
            })
            .count(),
        1
    );
    assert_eq!(
        results
            .iter()
            .filter(|outcome| matches!(
                outcome,
                CommandOutcome::NotCommitted {
                    evidence: CommandError::Conflict { .. }
                }
            ))
            .count(),
        1
    );
}

pub fn placement(seed: i32) -> WindowPlacement {
    WindowPlacement::new(
        WindowBounds::new(seed, seed + 1, 900, 700).unwrap(),
        WindowDisplayState::Normal,
        None,
        None,
    )
}

pub fn target(runtime: u8, root: u8) -> RememberedTarget {
    RememberedTarget::new(
        RuntimeId::from_bytes([runtime; 16]),
        RootId::from_bytes([root; 16]),
    )
}

pub fn admission(seed: u8) -> BranchHandoffJobAdmission {
    BranchHandoffJobAdmission::new(
        ResolutionIntentId::from_bytes([seed; 16]),
        ResolutionAttemptOrdinal::new(1).unwrap(),
        SyndicThreadId::from_bytes([seed; 16]),
        SyndicThreadId::from_bytes([seed.wrapping_add(1); 16]),
        DiscussionContextOwnerId::Draft(SyndicDraftId::from_bytes([seed.wrapping_add(2); 16])),
        DiscussionContextDigest::from_bytes([seed.wrapping_add(3); 32]),
        SyndicTurnId::from_bytes([seed.wrapping_add(4); 16]),
        ResolutionRequestIdentity::new(
            CasThreadId::new(format!("child-thread-{seed}")).unwrap(),
            CasTurnId::new(format!("child-turn-{seed}")).unwrap(),
            DynamicToolCallId::new(format!("tool-call-{seed}")).unwrap(),
        ),
        beryl_state::ParentQueueOrdinal::new(u64::from(seed)),
        ResolutionText::new(format!("resolution-{seed}")).unwrap(),
    )
}

pub fn catalog_sources(revision: u64) -> CatalogSourceRevisions {
    CatalogSourceRevisions::new(
        ProjectionRevision::new(revision).unwrap(),
        RecordRevision::new(revision).unwrap(),
        RecordRevision::new(revision).unwrap(),
        None::<ClaimRevision>,
    )
}

pub fn catalog_facts(seed: u8, revision: u64, activity: u64) -> CatalogFacts {
    let title = format!("Thread {seed} revision {revision}");
    let resolved_title = CatalogResolvedTitle::history_derived(&title).unwrap();
    let execution = CatalogExecutionSummary::new(
        RuntimeId::from_bytes([1; 16]),
        RootId::from_bytes([2; 16]),
        "Host",
        AdmittedHostPath::from_admitted(PathFlavor::Windows, r"C:\Codex\codex.exe").unwrap(),
        AdmittedHostPath::from_admitted(PathFlavor::Windows, r"C:\Work\beryl").unwrap(),
        CatalogAvailabilitySummary::new(Availability::Available, Availability::Available),
    )
    .unwrap();
    CatalogFacts::new(
        resolved_title,
        execution,
        CatalogArchiveSummary::Ordinary,
        UnixMillis::new(activity),
        true,
        CatalogClaimSummary::Unclaimed,
        CatalogLineageSummary::TopLevel,
    )
    .unwrap()
}

pub fn sidecar_limit() -> SidecarByteLimit {
    SidecarByteLimit::new(NonZeroU64::new(1024 * 1024).unwrap())
}

pub fn asset_owner(seed: u8) -> AssetOwner {
    AssetOwner::AcceptedInput(beryl_model::SyndicAcceptedInputId::from_bytes([seed; 16]))
}

pub fn thread(seed: u8) -> SyndicThreadId {
    SyndicThreadId::from_bytes([seed; 16])
}

pub fn window(seed: u8) -> WindowId {
    WindowId::from_bytes([seed; 16])
}
