use super::*;
use beryl_home_store::{HomeCommand, HomeOpenOptions, HomeSchemaVersion};
use beryl_model::{
    ExecutionBinding, PathFlavor, RootId, RuntimeId, RuntimeMode, RuntimeNativePath, SyndicDraftId,
};
use beryl_state::BerylState;
use syndic_storage::{CreateThread, DraftEditHistoryPolicyV1};

mod submission_fixture {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/unit/submission_fixture.rs"
    ));
}

struct PendingWitness(SyndicThreadId);

impl PendingOrdinaryExecutionWitness for PendingWitness {
    fn expected_syndic_thread_id(&self) -> SyndicThreadId {
        self.0
    }
    fn expected_binding_revision(&self) -> BindingRevision {
        panic!("drift precedes projection checks")
    }
    fn expected_execution_binding(&self) -> &ExecutionBinding {
        panic!("drift precedes projection checks")
    }
    fn expected_cas_thread_id(&self) -> &beryl_model::CasThreadId {
        panic!("drift precedes projection checks")
    }
    fn expected_lineage_proof(&self) -> syndic_storage::CasLineageProof {
        panic!("drift precedes projection checks")
    }
}

fn execute(store: &HomeStore, contribution: beryl_home_store::MutationContribution) {
    let mut command = HomeCommand::new(store.home_revision().unwrap());
    command.add(contribution).unwrap();
    assert!(matches!(
        store.execute(command),
        beryl_home_store::CommandOutcome::Committed {
            later_failure: None,
            ..
        }
    ));
}

#[test]
fn canonical_content_removed_during_preflight_is_concurrent_change() {
    let directory = tempfile::tempdir().unwrap();
    let mut store = HomeStore::open(HomeOpenOptions::new(
        directory.path(),
        HomeSchemaVersion::CURRENT,
    ))
    .unwrap();
    let storage = SyndicStorage::register(&mut store).unwrap();
    let assets = BerylState::register(&mut store).unwrap().assets();
    let thread = SyndicThreadId::from_bytes([61; 16]);
    let binding = ExecutionBinding::new(
        RuntimeId::from_bytes([61; 16]),
        RootId::from_bytes([62; 16]),
        RuntimeNativePath::from_admitted(
            RuntimeMode::host(),
            PathFlavor::Windows,
            r"C:\work\beryl",
        )
        .unwrap(),
    );
    execute(
        &store,
        storage.create_thread(
            storage.revision(&store).unwrap(),
            CreateThread::ordinary(
                thread,
                SyndicDraftId::from_bytes([62; 16]),
                binding,
                SyndicTimestamp::from_unix_millis(1),
                DraftEditHistoryPolicyV1::new(65_536, 1).unwrap(),
            ),
        ),
    );
    submission_fixture::submit_atoms(
        &store,
        storage.clone(),
        assets.clone(),
        thread,
        SyndicDraftId::from_bytes([63; 16]),
        SyndicItemId::from_bytes([64; 16]),
        &[submission_fixture::Atom::Text("preserved pending input")],
        65,
        SyndicTimestamp::from_unix_millis(3),
    );
    let limit = SyndicPointReadLimit::new(1_000_000).unwrap();
    let proof = storage
        .pending_dispatch_evidence(&store, thread, limit)
        .unwrap()
        .unwrap();
    let result = PendingOrdinaryExecution::read_with_confirmation_hook(
        &store,
        &storage,
        &assets,
        &PendingWitness(thread),
        limit,
        || {
            let mut changes = syndic_storage::test_faults::FixtureBatch::new();
            changes
                .delete(syndic_storage::test_faults::FixtureDelete::ContentManifest(
                    proof.input().id(),
                ))
                .unwrap();
            execute(
                &store,
                storage.fixture_contribution(storage.revision(&store).unwrap(), changes),
            );
        },
    );
    assert!(
        matches!(result, Err(OrdinaryTurnExecutionError::ConcurrentChange { thread_id }) if thread_id == thread)
    );
}
