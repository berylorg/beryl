#![cfg(feature = "test-faults")]

#[path = "discussion_creation/cases.rs"]
mod cases;
#[path = "discussion_creation/collisions.rs"]
mod collisions;
#[path = "discussion_creation/process_fence.rs"]
mod process_fence;
#[path = "../../syndic-storage/tests/support/mod.rs"]
mod support;

use beryl_app::{discussion_creation::*, process_admission::ProcessAdmissionGate};
use beryl_home_store::{
    CommandCancellation, CommandOutcome, CursorReadLimits, HomeCommand, HomeOpenCandidate,
    HomeOpenOptions, HomeSchemaVersion, HomeStore,
    test_faults::{FaultController, FaultPoint},
};
use beryl_model::{AdmittedHostPath, PathFlavor, RuntimeMode, RuntimeNativePath};
use beryl_state::{
    AvailabilitySnapshot, BerylState, CatalogClaimSummary, CatalogPointReadLimit,
    CreateRuntimeWithHomeRoot, RootRegistration, RuntimeRegistration, UnixMillis,
};
use std::num::NonZeroUsize;
use support::{draft_id, id, timestamp};
use syndic_storage::*;

struct Fixture {
    _directory: tempfile::TempDir,
    store: HomeStore,
    state: BerylState,
    syndic: SyndicStorage,
    faults: FaultController,
    operations: DiscussionCreationOperations,
    process: ProcessAdmissionGate,
}

impl Fixture {
    fn new(capacity: usize) -> Self {
        let directory = tempfile::tempdir().unwrap();
        let faults = FaultController::new();
        let mut candidate = HomeOpenCandidate::open_with_faults(
            HomeOpenOptions::new(directory.path(), HomeSchemaVersion::CURRENT),
            faults.clone(),
        )
        .unwrap();
        let state = BerylState::register(&mut candidate).unwrap();
        let syndic = SyndicStorage::register(&mut candidate).unwrap();
        let store = candidate
            .prepare_publication(
                BerylState::required_domains()
                    .unwrap()
                    .merge(SyndicStorage::required_domains().unwrap())
                    .unwrap(),
            )
            .unwrap()
            .publish()
            .unwrap();
        support::seed_populated(&store, syndic.clone());
        let binding = syndic
            .thread_catalog_summary(&store, id(30), limit())
            .unwrap()
            .unwrap()
            .execution()
            .clone();
        let executable =
            AdmittedHostPath::from_admitted(PathFlavor::Windows, r"C:\codex.exe").unwrap();
        let runtime = RuntimeRegistration::new(
            binding.runtime_id(),
            executable,
            RuntimeMode::host(),
            RuntimeNativePath::from_admitted(
                RuntimeMode::host(),
                PathFlavor::Windows,
                r"C:\codex.exe",
            )
            .unwrap(),
            UnixMillis::new(1),
            AvailabilitySnapshot::unknown(),
        )
        .unwrap();
        let root = RootRegistration::new(
            binding.root_id(),
            binding.root_path().clone(),
            AdmittedHostPath::from_admitted(PathFlavor::Windows, r"C:\populated").unwrap(),
            UnixMillis::new(1),
            AvailabilitySnapshot::unknown(),
        );
        let mut command = HomeCommand::new(store.home_revision().unwrap());
        command
            .add(state.runtime_roots().create_runtime_with_home_root(
                state.runtime_roots().revision(&store).unwrap(),
                CreateRuntimeWithHomeRoot::new(runtime, root).unwrap(),
            ))
            .unwrap();
        assert!(matches!(
            store.execute(command),
            CommandOutcome::Committed {
                later_failure: None,
                ..
            }
        ));
        let process = ProcessAdmissionGate::new();
        Self {
            _directory: directory,
            store,
            state,
            syndic,
            faults,
            operations: DiscussionCreationOperations::new(
                process.clone(),
                NonZeroUsize::new(capacity).unwrap(),
            ),
            process,
        }
    }

    fn service(&self) -> DiscussionCreationService {
        DiscussionCreationService::new(
            self.operations.clone(),
            self.store.service_reference(),
            self.state.clone(),
            self.syndic.clone(),
        )
    }

    fn prepare(
        &self,
        child: u8,
        cancellation: CommandCancellation,
    ) -> PreparedDiscussionCreationOperation {
        self.service()
            .prepare(
                source(&self.store, &self.syndic),
                request(child),
                cancellation,
            )
            .unwrap()
    }
}

fn limit() -> SyndicPointReadLimit {
    SyndicPointReadLimit::new(400_000).unwrap()
}

fn request(child: u8) -> CreateDiscussion {
    CreateDiscussion::new(
        id(child),
        draft_id(child + 1),
        timestamp(100),
        DraftEditHistoryPolicyV1::new(65_536, 1).unwrap(),
    )
}

fn source(store: &HomeStore, syndic: &SyndicStorage) -> PreparedDiscussionSource {
    let thread = syndic.thread(store, id(30), limit()).unwrap().unwrap();
    let head = syndic
        .transcript_view_head(store, id(30), limit())
        .unwrap()
        .unwrap();
    let entries = syndic
        .transcript_entries(
            store,
            id(30),
            head.generation(),
            None,
            CursorReadLimits::new(64, 1_000_000).unwrap(),
        )
        .unwrap();
    let entry = entries
        .records()
        .iter()
        .find(|entry| entry.projection_id() == support::populated::source_projection())
        .unwrap();
    syndic
        .prepare_discussion_source(
            store,
            DiscussionContextSource::new(
                id(30),
                support::populated::source_turn(),
                support::populated::source_item(),
                entry.projection_id(),
                entry.projection_revision(),
                DiscussionContextRange::new(0, 9).unwrap(),
            ),
            thread.selected_path(),
            CurrentTranscriptEntryProof::new(head.generation(), entry.position()),
            DiscussionContextText::new("assistant").unwrap(),
        )
        .unwrap()
}

fn no_child(store: &HomeStore, syndic: &SyndicStorage, state: &BerylState, child: u8) {
    assert!(syndic.thread(store, id(child), limit()).unwrap().is_none());
    assert!(
        state
            .catalog()
            .row(store, id(child), CatalogPointReadLimit::schema_maximum())
            .unwrap()
            .is_none()
    );
}
