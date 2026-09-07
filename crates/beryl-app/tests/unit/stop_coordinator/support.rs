use std::{
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
        mpsc,
    },
    time::Duration,
};

use beryl_home_store::{
    CommandOutcome, HomeCommand, HomeOpenOptions, HomeSchemaVersion, HomeStore,
};
use beryl_model::{
    BindingRevision, CasThreadId, CasTurnId, InputGateRevision, SyndicDraftId,
    SyndicExecutionSnapshotId, SyndicItemId, SyndicThreadId, SyndicTurnId,
};
use syndic_storage::{
    ContentAppend, ContentBuild, CreateThread, DraftEditHistoryPolicyV1, PreparedContent,
    SourceEventPayload, StopAdmissionIneligibility, StopAdmissionRead, StopCause,
    StopOperationTarget, SyndicPointReadLimit, SyndicStorage, SyndicTimestamp, TurnStateRevision,
};

use super::*;
use crate::{
    LifecycleYieldOutcome,
    cas_projection::{
        PendingTurnActivation,
        connection::{TargetTurnRegistration, registry::LoadedThreadKey},
    },
};

#[allow(dead_code)]
mod exact_cas_support {
    use beryl_home_store::{CommandOutcome, HomeCommand, HomeStore};
    use beryl_model::{
        BindingRevision, ContentRevision, SyndicContentId, SyndicDraftId, SyndicThreadId,
    };
    use syndic_storage::test_faults::{FixtureBatch, FixtureDelete, FixtureRecord};
    use syndic_storage::{
        ContentByteSpanRecord, ContentReference, CreateThread, DraftEditHistoryPolicyV1,
        PreparedContent, SyndicStorage, TranscriptGeneration,
    };

    fn commit(store: &HomeStore, storage: SyndicStorage, batch: FixtureBatch) {
        let mut command = HomeCommand::new(store.home_revision().unwrap());
        command
            .add(
                storage
                    .clone()
                    .fixture_contribution(storage.clone().revision(store).unwrap(), batch),
            )
            .unwrap();
        match store.execute(command) {
            CommandOutcome::Committed {
                later_failure: None,
                ..
            } => {}
            CommandOutcome::Committed {
                later_failure: Some(failure),
                ..
            } => panic!("fixture-batch command committed with a later failure: {failure:?}"),
            CommandOutcome::NotCommitted { evidence } => {
                panic!("fixture-batch command did not commit: {evidence:?}")
            }
            CommandOutcome::Indeterminate {
                failure,
                reconciliation,
            } => {
                reconciliation.install();
                panic!("fixture-batch command was indeterminate: {failure:?}")
            }
        }
    }

    fn seed_detached_canonical_draft_backing(
        store: &HomeStore,
        storage: SyndicStorage,
        staging_thread: SyndicThreadId,
        draft_id: SyndicDraftId,
    ) -> syndic_storage::DraftRootHistoryPairV1 {
        let request = CreateThread::ordinary(
            staging_thread,
            draft_id,
            exact_cas::execution_binding(),
            syndic_storage::SyndicTimestamp::from_unix_millis(1),
            DraftEditHistoryPolicyV1::new(65_536, 1).unwrap(),
        );
        let mut command = HomeCommand::new(store.home_revision().unwrap());
        command
            .add(
                storage
                    .clone()
                    .create_thread(storage.clone().revision(store).unwrap(), request),
            )
            .unwrap();
        match store.execute(command) {
            CommandOutcome::Committed {
                later_failure: None,
                ..
            } => {}
            outcome => panic!("expected clean canonical thread seed, got {outcome:?}"),
        }
        let pair = storage
            .current_draft(
                store,
                staging_thread,
                syndic_storage::SyndicPointReadLimit::new(65_536).unwrap(),
            )
            .unwrap()
            .unwrap()
            .draft()
            .root_history();
        let mut cleanup = FixtureBatch::new();
        for delete in [
            FixtureDelete::Thread(staging_thread),
            FixtureDelete::ImageLabelAuthorityHead(staging_thread),
            FixtureDelete::DraftImageLabelProtectionHead(staging_thread),
            FixtureDelete::ThreadExecution(staging_thread),
            FixtureDelete::ThreadAttributes(staging_thread),
            FixtureDelete::ThreadUsage(staging_thread),
            FixtureDelete::ThreadCatalogSummary(staging_thread),
            FixtureDelete::Draft(draft_id),
            FixtureDelete::InputGate(staging_thread),
            FixtureDelete::ActivityQueryHead(staging_thread),
            FixtureDelete::TranscriptViewHead(staging_thread),
            FixtureDelete::TranscriptBuild {
                thread: staging_thread,
                generation: TranscriptGeneration::FIRST,
            },
            FixtureDelete::HistorySummary(staging_thread),
            FixtureDelete::Binding {
                thread: staging_thread,
                revision: BindingRevision::new(1).unwrap(),
            },
            FixtureDelete::DraftByThread(staging_thread),
            FixtureDelete::BindingHead(staging_thread),
        ] {
            cleanup.delete(delete).unwrap();
        }
        commit(store, storage, cleanup);
        pair
    }

    fn prepared_content_records(
        content: &PreparedContent,
    ) -> (ContentReference, Vec<FixtureRecord>) {
        let revision = ContentRevision::new(1).unwrap();
        let manifest = content.sealed_manifest(revision);
        let mut records = Vec::with_capacity(
            1 + content.chunks().len() * 2 + content.text_spans().len() + content.pieces().len(),
        );
        records.push(FixtureRecord::ContentManifest(manifest));
        let mut encoded_start = 0;
        for chunk in content.chunks() {
            records.push(FixtureRecord::ContentChunk(chunk.clone()));
            let span = ContentByteSpanRecord::for_chunk(chunk, encoded_start).unwrap();
            encoded_start = span.end();
            records.push(FixtureRecord::ContentByteSpan(span));
        }
        records.extend(
            content
                .text_spans()
                .iter()
                .copied()
                .map(FixtureRecord::ContentTextSpan),
        );
        records.extend(
            content
                .pieces()
                .iter()
                .copied()
                .map(FixtureRecord::ContentPiece),
        );
        (content.reference(revision), records)
    }

    fn batch(records: impl IntoIterator<Item = FixtureRecord>) -> FixtureBatch {
        let mut batch = FixtureBatch::new();
        let mut manifests = std::collections::HashMap::<SyndicContentId, _>::new();
        let mut chunks = std::collections::HashMap::new();
        let mut byte_spans = std::collections::HashMap::new();
        let mut text_spans = std::collections::HashMap::new();
        let mut pieces = std::collections::HashMap::new();
        for record in records {
            match &record {
                FixtureRecord::ContentManifest(manifest) => {
                    if let Some(existing) = manifests.insert(manifest.id(), manifest.clone()) {
                        assert_eq!(existing, *manifest, "conflicting fixture content manifests");
                        continue;
                    }
                }
                FixtureRecord::ContentChunk(chunk) => {
                    let key = (chunk.content_id(), chunk.ordinal());
                    if let Some(existing) = chunks.insert(key, chunk.clone()) {
                        assert_eq!(existing, *chunk, "conflicting fixture content chunks");
                        continue;
                    }
                }
                FixtureRecord::ContentByteSpan(span) => {
                    let key = (span.content_id(), span.start());
                    if let Some(existing) = byte_spans.insert(key, *span) {
                        assert_eq!(existing, *span, "conflicting fixture content byte spans");
                        continue;
                    }
                }
                FixtureRecord::ContentTextSpan(span) => {
                    let key = (span.content_id(), span.logical_start());
                    if let Some(existing) = text_spans.insert(key, *span) {
                        assert_eq!(existing, *span, "conflicting fixture content text spans");
                        continue;
                    }
                }
                FixtureRecord::ContentPiece(piece) => {
                    let key = (piece.content_id(), piece.ordinal());
                    if let Some(existing) = pieces.insert(key, *piece) {
                        assert_eq!(existing, *piece, "conflicting fixture content pieces");
                        continue;
                    }
                }
                _ => {}
            }
            batch.put(record).unwrap();
        }
        batch
    }

    #[path = "../../../../../syndic-storage/tests/support/exact_cas.rs"]
    pub mod exact_cas;
}

use exact_cas_support::exact_cas;

static NEXT_CONNECTION: AtomicU64 = AtomicU64::new(1);

fn timestamp(value: u64) -> SyndicTimestamp {
    SyndicTimestamp::from_unix_millis(value)
}

fn point_limit() -> SyndicPointReadLimit {
    SyndicPointReadLimit::new(1_000_000).unwrap()
}

fn execute(home: &HomeStore, contribution: beryl_home_store::MutationContribution) {
    let mut command = HomeCommand::new(home.home_revision().unwrap());
    command.add(contribution).unwrap();
    match home.execute(command) {
        CommandOutcome::Committed {
            later_failure: None,
            ..
        } => {}
        outcome @ CommandOutcome::Committed {
            later_failure: Some(_),
            ..
        } => panic!("stop fixture command committed with later failure: {outcome:?}"),
        CommandOutcome::NotCommitted { evidence } => {
            panic!("stop fixture command was not committed: {evidence:?}")
        }
        outcome @ CommandOutcome::Indeterminate { .. } => {
            panic!("stop fixture command was indeterminate: {outcome:?}")
        }
    }
}

fn stage_prepared_content(home: &HomeStore, storage: SyndicStorage, content: &PreparedContent) {
    execute(
        home,
        storage.begin_content(
            storage.revision(home).unwrap(),
            ContentBuild::from_prepared(content),
        ),
    );
    let mut manifest = content.building_manifest();
    while let Some(append) = ContentAppend::prepare(&manifest, content).unwrap() {
        let next = append.next_manifest().clone();
        execute(
            home,
            storage.append_content(storage.revision(home).unwrap(), append),
        );
        manifest = next;
    }
}

struct StopFixture {
    _directory: tempfile::TempDir,
    home: Arc<HomeStore>,
    storage: SyndicStorage,
    coordinator: Arc<StopCoordinator>,
    command_gate: crate::cas_projection::persistent_failure::MasterCommandGate,
    router: Arc<EventRouter>,
    thread: SyndicThreadId,
    turn: SyndicTurnId,
    target: StopOperationTarget,
    proof: StopTargetProof,
}

impl StopFixture {
    fn new(seed: u8) -> Self {
        let directory = tempfile::tempdir().unwrap();
        let mut home = HomeStore::open(HomeOpenOptions::new(
            directory.path(),
            HomeSchemaVersion::CURRENT,
        ))
        .unwrap();
        let storage = SyndicStorage::register(&mut home).unwrap();
        let thread = SyndicThreadId::from_bytes([seed; 16]);
        execute(
            &home,
            storage.create_thread(
                storage.revision(&home).unwrap(),
                CreateThread::ordinary(
                    thread,
                    SyndicDraftId::from_bytes([seed.wrapping_add(1); 16]),
                    exact_cas::execution_binding(),
                    timestamp(1),
                    DraftEditHistoryPolicyV1::new(65_536, 1).unwrap(),
                ),
            ),
        );
        let turn = exact_cas::submit_current_draft(
            &home,
            storage.clone(),
            thread,
            SyndicDraftId::from_bytes([seed.wrapping_add(2); 16]),
            SyndicItemId::from_bytes([seed.wrapping_add(3); 16]),
            "coordinator stop target",
            timestamp(2),
        );
        let source = exact_cas::establish_turn(&home, storage.clone(), thread, turn, timestamp(3));
        exact_cas::admit_event(
            &home,
            storage.clone(),
            thread,
            turn,
            &source,
            SourceEventPayload::TurnActivated,
            timestamp(4),
        );
        let target = match storage
            .stop_admission_read(&home, thread, point_limit())
            .unwrap()
        {
            StopAdmissionRead::Admissible(candidate) => candidate.target().clone(),
            other => panic!("active fixture must admit a stop, observed {other:?}"),
        };
        let home_id = home.home_id();
        let home_generation = home.health().generation().unwrap();
        let home = Arc::new(home);
        let command_gate = crate::cas_projection::persistent_failure::MasterCommandGate::new(
            crate::cas_projection::persistent_failure::ProjectionServiceGeneration::allocate()
                .unwrap(),
            None,
        );
        let coordinator = Arc::new(StopCoordinator::new(
            &home,
            home_id,
            home_generation,
            storage.clone(),
            command_gate.authorizer(),
        ));
        let router = Arc::new(
            EventRouter::new_with_scheduler(
                target.runtime_id(),
                target.loaded_generation().process(),
                NEXT_CONNECTION.fetch_add(1, Ordering::Relaxed),
                crate::cas_projection::accepted_input_scheduler::AcceptedInputSchedulerSignal::new(
                ),
                command_gate.authorizer(),
                None,
            )
            .unwrap(),
        );
        let router_command = router.authorize_command_for_test().unwrap();
        router
            .register(
                &router_command,
                LoadedThreadKey {
                    runtime_id: target.runtime_id(),
                    process_generation: target.loaded_generation().process(),
                    cas_thread_id: target.cas_thread_id().clone(),
                },
                thread,
                target.loaded_generation(),
                home_generation.get(),
                Duration::from_secs(1),
                TargetTurnRegistration::Pending(PendingTurnActivation::new(
                    thread,
                    turn,
                    BindingRevision::new(1).unwrap(),
                    InputGateRevision::new(1).unwrap(),
                    TurnStateRevision::FIRST,
                    SyndicExecutionSnapshotId::from_bytes([seed.wrapping_add(4); 16]),
                    timestamp(4),
                )),
            )
            .unwrap();
        drop(router_command);
        router.activate_stop_target_for_test(target.cas_thread_id(), target.cas_turn_id().clone());
        let proof = router
            .stop_target(thread, target.cas_thread_id(), target.cas_turn_id())
            .unwrap();
        Self {
            _directory: directory,
            home,
            storage,
            coordinator,
            command_gate,
            router,
            thread,
            turn,
            target,
            proof,
        }
    }

    fn wrong_storage_target_proof(&self, seed: u8) -> StopTargetProof {
        let cas_thread = CasThreadId::new(format!("wrong-stop-thread-{seed}")).unwrap();
        let cas_turn = CasTurnId::new(format!("wrong-stop-turn-{seed}")).unwrap();
        let router_command = self.router.authorize_command_for_test().unwrap();
        self.router
            .register(
                &router_command,
                LoadedThreadKey {
                    runtime_id: self.target.runtime_id(),
                    process_generation: self.target.loaded_generation().process(),
                    cas_thread_id: cas_thread.clone(),
                },
                self.thread,
                self.target.loaded_generation(),
                self.home.health().generation().unwrap().get(),
                Duration::from_secs(1),
                TargetTurnRegistration::Pending(PendingTurnActivation::new(
                    self.thread,
                    self.turn,
                    BindingRevision::new(1).unwrap(),
                    InputGateRevision::new(1).unwrap(),
                    TurnStateRevision::FIRST,
                    SyndicExecutionSnapshotId::from_bytes([seed.wrapping_add(1); 16]),
                    timestamp(5),
                )),
            )
            .unwrap();
        drop(router_command);
        self.router
            .activate_stop_target_for_test(&cas_thread, cas_turn.clone());
        self.router
            .stop_target(self.thread, &cas_thread, &cas_turn)
            .unwrap()
    }

    fn live_stop(&self) -> syndic_storage::SyndicLiveStopOperation {
        match self
            .storage
            .stop_admission_read(&self.home, self.thread, point_limit())
            .unwrap()
        {
            StopAdmissionRead::Stopping(live) => *live,
            other => panic!("fixture must retain a live stop, observed {other:?}"),
        }
    }
}

fn failure_identity(
    fixture: &StopFixture,
) -> crate::cas_projection::persistent_failure::PersistentFailureCutIdentity {
    crate::cas_projection::persistent_failure::PersistentFailureCutIdentity::new(
        fixture.home.home_id(),
        fixture.home.health().generation().unwrap(),
        fixture.command_gate.service_generation(),
        crate::cas_projection::persistent_failure::PersistentFailureGeneration::FIRST,
    )
}
