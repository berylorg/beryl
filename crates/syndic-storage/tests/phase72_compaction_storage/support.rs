use std::{
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

use beryl_home_store::{
    CommandOutcome, HomeCommand, HomeOpenOptions, HomeSchemaVersion, HomeStore,
};
use beryl_model::{
    CasConversationToolProfile, CasLoadedSessionGeneration, CasLoadedThreadGeneration,
    CasNativeTurnCount, CasProcessGeneration, CasThreadId, CasTurnId, ExecutionBinding, PathFlavor,
    RootId, RuntimeId, RuntimeMode, RuntimeNativePath, SyndicItemId, SyndicThreadId,
};
#[cfg(feature = "test-faults")]
use beryl_model::{DraftRevision, SyndicAcceptedInputId};
use syndic_storage::{
    BindingState, CasLineageProof, CasRepresentedPrefixProof, ClaimCompactionDispatch,
    CompactionAdmissionRead, CompactionAttemptNonce, CompactionMarkerLifecycle,
    CompactionOperationId, CompactionOperationNonce, CompactionOperationRecord,
    CompactionProviderEvent, CompactionProviderSequence, CompactionRequestDisposition,
    CompactionThreadStatus, ContentAppend, ContentBuild, ContentManifestRecord, CreateThread,
    DraftEditHistoryPolicyV1, InputGateRecord, NativeCasLineage, PreparedContent,
    PublishCompactionProviderEvent, PublishCompactionRequestDisposition, PublishValidBinding,
    SealLifecycleContinuationContent, SyndicPointReadLimit, SyndicStorage, SyndicTimestamp,
    TurnEndStatus, TurnTerminalOutcome, empty_selected_path_digest,
    prepare_lifecycle_continuation_content,
};

static NEXT_HOME: AtomicU64 = AtomicU64::new(1);

pub struct TestHome {
    path: PathBuf,
}

impl TestHome {
    fn new(name: &str) -> Self {
        loop {
            let sequence = NEXT_HOME.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "beryl-syndic-phase72-{name}-{}-{sequence}",
                std::process::id()
            ));
            match std::fs::create_dir(&path) {
                Ok(()) => return Self { path },
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(error) => panic!("failed to create isolated compaction test home: {error}"),
            }
        }
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TestHome {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

fn open(path: &Path) -> HomeStore {
    HomeStore::open(HomeOpenOptions::new(path, HomeSchemaVersion::CURRENT)).unwrap()
}

pub fn timestamp(value: u64) -> SyndicTimestamp {
    SyndicTimestamp::from_unix_millis(value)
}

fn execution_binding() -> ExecutionBinding {
    ExecutionBinding::new(
        RuntimeId::from_bytes([246; 16]),
        RootId::from_bytes([247; 16]),
        RuntimeNativePath::from_admitted(
            RuntimeMode::host(),
            PathFlavor::Windows,
            "C:\\syndic-phase72-compaction",
        )
        .unwrap(),
    )
}

fn tool_profile() -> CasConversationToolProfile {
    CasConversationToolProfile::v1([248; 32])
}

pub fn point_limit() -> SyndicPointReadLimit {
    SyndicPointReadLimit::new(1_000_000).unwrap()
}

pub fn execute(store: &HomeStore, contribution: beryl_home_store::MutationContribution) {
    let mut command = HomeCommand::new(store.home_revision().unwrap());
    command.add(contribution).unwrap();
    match store.execute(command) {
        CommandOutcome::Committed {
            later_failure: None,
            ..
        } => {}
        outcome => panic!("expected clean compaction fixture command, got {outcome:?}"),
    }
}

fn stage_prepared_content(
    store: &HomeStore,
    storage: SyndicStorage,
    prepared: &PreparedContent,
) -> ContentManifestRecord {
    let mut manifest = prepared.building_manifest();
    execute(
        store,
        storage.begin_content(
            storage.revision(store).unwrap(),
            ContentBuild::from_prepared(prepared),
        ),
    );
    loop {
        let Some(append) = ContentAppend::prepare(&manifest, prepared).unwrap() else {
            break;
        };
        manifest = append.next_manifest().clone();
        execute(
            store,
            storage.append_content(storage.revision(store).unwrap(), append),
        );
    }
    manifest
}

pub fn loaded_generation() -> CasLoadedSessionGeneration {
    CasLoadedSessionGeneration::new(
        CasProcessGeneration::new(72).unwrap(),
        CasLoadedThreadGeneration::new(1).unwrap(),
    )
}

pub struct CompactionFixture {
    pub home: TestHome,
    pub store: HomeStore,
    pub storage: SyndicStorage,
    pub thread: SyndicThreadId,
}

impl CompactionFixture {
    pub fn new(name: &str, id_byte: u8) -> Self {
        let home = TestHome::new(name);
        let mut store = open(home.path());
        let storage = SyndicStorage::register(&mut store).unwrap();
        let thread = SyndicThreadId::from_bytes([id_byte; 16]);
        execute(
            &store,
            storage.create_thread(
                storage.revision(&store).unwrap(),
                CreateThread::ordinary(
                    thread,
                    beryl_model::SyndicDraftId::from_bytes([id_byte.wrapping_add(1); 16]),
                    execution_binding(),
                    timestamp(1),
                    DraftEditHistoryPolicyV1::new(65_536, 1).unwrap(),
                ),
            ),
        );
        let current = storage
            .current_binding(&store, thread, point_limit())
            .unwrap()
            .unwrap();
        let selected_path = current.binding().selected_path();
        let represented_prefix = CasRepresentedPrefixProof::new(
            None,
            selected_path.thread_revision(),
            empty_selected_path_digest(),
        );
        let lineage = CasLineageProof::native(NativeCasLineage::Fresh, represented_prefix).unwrap();
        execute(
            &store,
            storage.publish_valid_binding(
                storage.revision(&store).unwrap(),
                PublishValidBinding::new(
                    thread,
                    current.binding().revision(),
                    selected_path,
                    execution_binding(),
                    CasThreadId::new(format!("phase72-compaction-{id_byte}")).unwrap(),
                    represented_prefix,
                    CasNativeTurnCount::ZERO,
                    tool_profile(),
                    lineage,
                ),
            ),
        );
        let fixture = Self {
            home,
            store,
            storage,
            thread,
        };
        assert_eq!(
            fixture.gate().state(),
            &syndic_storage::InputGateState::Idle
        );
        assert!(matches!(fixture.binding_state(), BindingState::Valid(_)));
        fixture
            .store
            .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
            .unwrap();
        fixture
    }

    pub fn gate(&self) -> InputGateRecord {
        self.storage
            .input_gate(&self.store, self.thread, point_limit())
            .unwrap()
            .unwrap()
    }

    pub fn binding_state(&self) -> BindingState {
        self.storage
            .current_binding(&self.store, self.thread, point_limit())
            .unwrap()
            .unwrap()
            .binding()
            .state()
            .clone()
    }

    pub fn operation(&self, id: CompactionOperationId) -> CompactionOperationRecord {
        self.storage
            .compaction_operation(&self.store, id, point_limit())
            .unwrap()
            .unwrap()
    }

    pub fn admit(&self, nonce_byte: u8, at: u64) -> CompactionOperationId {
        let CompactionAdmissionRead::Admissible(candidate) = self
            .storage
            .compaction_admission_read(&self.store, self.thread, point_limit())
            .unwrap()
        else {
            panic!("idle valid-binding fixture must admit compaction");
        };
        let admission = candidate.admission(
            CompactionOperationNonce::from_bytes([nonce_byte; 16]),
            CompactionAttemptNonce::from_bytes([nonce_byte.wrapping_add(1); 16]),
            loaded_generation(),
            timestamp(at),
        );
        let id = admission.operation_id();
        assert_eq!(admission.home_id(), self.store.home_id());
        match self
            .store
            .execute_current(self.storage.current_admit_compaction_operation(admission))
        {
            CommandOutcome::Committed {
                later_failure: None,
                ..
            } => {}
            outcome => panic!("expected clean compaction admission, got {outcome:?}"),
        }
        assert_eq!(self.operation(id).home_id(), self.store.home_id());
        id
    }

    pub fn claim(&self, id: CompactionOperationId) {
        let operation = self.operation(id);
        match self
            .store
            .execute_current(self.storage.current_claim_compaction_dispatch(
                ClaimCompactionDispatch::new(id, operation.revision(), operation.attempt()),
            )) {
            CommandOutcome::Committed {
                later_failure: None,
                ..
            } => {}
            outcome => panic!("expected clean compaction claim, got {outcome:?}"),
        }
    }

    pub fn publish_request(
        &self,
        id: CompactionOperationId,
        disposition: CompactionRequestDisposition,
    ) {
        let operation = self.operation(id);
        match self.store.execute_current(
            self.storage.current_publish_compaction_request_disposition(
                PublishCompactionRequestDisposition::new(
                    id,
                    operation.revision(),
                    operation.attempt(),
                    disposition,
                ),
            ),
        ) {
            CommandOutcome::Committed {
                later_failure: None,
                ..
            } => {}
            outcome => panic!("expected clean compaction request publication, got {outcome:?}"),
        }
    }

    pub fn publish_provider(
        &self,
        id: CompactionOperationId,
        event: CompactionProviderEvent,
        at: u64,
    ) {
        let operation = self.operation(id);
        let sequence = operation
            .provider_frontier()
            .map_or(CompactionProviderSequence::FIRST, |frontier| {
                frontier.checked_next().unwrap()
            });
        match self
            .store
            .execute_current(self.storage.current_publish_compaction_provider_event(
                PublishCompactionProviderEvent::new(
                    id,
                    operation.revision(),
                    sequence,
                    event,
                    timestamp(at),
                ),
            )) {
            CommandOutcome::Committed {
                later_failure: None,
                ..
            } => {}
            outcome => panic!("expected clean compaction provider publication, got {outcome:?}"),
        }
    }

    pub fn reopen(self) -> Self {
        let Self {
            home,
            store,
            storage: _,
            thread,
        } = self;
        drop(store);
        let mut store = open(home.path());
        let storage = SyndicStorage::register(&mut store).unwrap();
        Self {
            home,
            store,
            storage,
            thread,
        }
    }

    pub fn publish_success(&self, id: CompactionOperationId, at: u64) {
        self.publish_provider(
            id,
            CompactionProviderEvent::ThreadStatus(CompactionThreadStatus::Active),
            at,
        );
        self.publish_provider(
            id,
            CompactionProviderEvent::TurnStarted(
                CasTurnId::new(format!("compaction-{id:?}")).unwrap(),
            ),
            at + 1,
        );
        let marker = SyndicItemId::from_bytes([id.nonce().as_bytes()[0].wrapping_add(70); 16]);
        self.publish_provider(
            id,
            CompactionProviderEvent::Marker {
                item_id: marker,
                lifecycle: CompactionMarkerLifecycle::Started,
            },
            at + 2,
        );
        self.publish_provider(
            id,
            CompactionProviderEvent::Marker {
                item_id: marker,
                lifecycle: CompactionMarkerLifecycle::Completed,
            },
            at + 3,
        );
        self.publish_provider(
            id,
            CompactionProviderEvent::ThreadStatus(CompactionThreadStatus::Idle),
            at + 4,
        );
        self.publish_provider(
            id,
            CompactionProviderEvent::Terminal(
                TurnEndStatus::new(TurnTerminalOutcome::Complete, None).unwrap(),
            ),
            at + 5,
        );
    }

    pub fn prepare_lifecycle_content(&self) -> syndic_storage::ContentReference {
        let prepared = prepare_lifecycle_continuation_content().unwrap();
        let manifest = stage_prepared_content(&self.store, self.storage, &prepared);
        match self
            .store
            .execute_current(self.storage.current_seal_lifecycle_continuation_content(
                SealLifecycleContinuationContent::new(manifest),
            )) {
            CommandOutcome::Committed {
                later_failure: None,
                ..
            } => {}
            outcome => panic!("expected clean lifecycle content seal, got {outcome:?}"),
        }
        self.storage
            .content_manifest(&self.store, prepared.id(), point_limit())
            .unwrap()
            .unwrap()
            .sealed_reference()
            .unwrap()
    }

    #[cfg(feature = "test-faults")]
    pub fn inject_deferred_accepted_next(
        &self,
        next_draft_byte: u8,
        at: u64,
    ) -> SyndicAcceptedInputId {
        use syndic_storage::{
            AcceptedInputAdmissionProof, AcceptedInputLifecycle, AcceptedInputOrdinal,
            AcceptedInputRecord, AcceptedNextSourceRecord, AcceptedOrderIndexRecord,
            AcceptedRouteGeneration, AcceptedRouteGenerationHeadRecord,
            AcceptedRouteGenerationRecord, AcceptedRouteHeadProof, AcceptedRouteLeafRecord,
            AcceptedRouteLeafState, AcceptedRouteRevision, AcceptedRouteTarget, ComposerPayload,
            DraftByThreadRecord, DraftRecord, HistorySummaryRecord, NextTurnReason, ThreadRecord,
            test_faults::{FixtureBatch, FixtureDelete, FixtureRecord},
        };

        let current = self
            .storage
            .current_draft(&self.store, self.thread, point_limit())
            .unwrap()
            .unwrap();
        let source_draft = current.draft().clone();
        let source_thread = current.thread().clone();
        let source_gate = self.gate();
        let summary = self
            .storage
            .history_summary(&self.store, self.thread, point_limit())
            .unwrap()
            .unwrap();
        let replacement = beryl_model::SyndicDraftId::from_bytes([next_draft_byte; 16]);
        let replacement_roots = crate::support::seed_detached_canonical_draft_backing(
            &self.store,
            self.storage,
            SyndicThreadId::from_bytes([next_draft_byte.wrapping_add(1); 16]),
            replacement,
        );
        let (content, content_records) =
            crate::support::composer_content_records(&ComposerPayload::default());
        let next_thread_revision = source_thread.revision().checked_next().unwrap();
        let next_gate_revision = source_gate.revision().checked_next().unwrap();
        let generation = AcceptedRouteGeneration::FIRST;
        let route_revision = AcceptedRouteRevision::FIRST;
        let ordinal = AcceptedInputOrdinal::FIRST;
        let input_id = source_draft.id().accepted_input_id();
        let admitted_at = timestamp(at);
        let admission = AcceptedInputAdmissionProof::new(
            source_thread.revision(),
            source_draft.id(),
            source_draft.revision(),
            source_gate.revision(),
            replacement,
        )
        .unwrap();
        let input = AcceptedInputRecord::new(
            input_id,
            self.thread,
            ordinal,
            admission,
            generation,
            content,
            None,
            admitted_at,
        )
        .unwrap();
        let mut batch = FixtureBatch::new();
        for record in content_records {
            batch.put(record).unwrap();
        }
        for record in [
            FixtureRecord::Thread(ThreadRecord::new(
                self.thread,
                syndic_storage::SelectedPathProof::new(
                    source_thread.committed_tail(),
                    next_thread_revision,
                    source_thread.selected_path_digest(),
                ),
                replacement,
                source_thread.lineage(),
                source_thread.context_owner_id(),
            )),
            FixtureRecord::Draft(DraftRecord::new(
                replacement,
                self.thread,
                DraftRevision::new(1).unwrap(),
                source_draft.submission_intent(),
                replacement_roots,
                admitted_at,
                admitted_at,
            )),
            FixtureRecord::DraftByThread(DraftByThreadRecord::new(
                self.thread,
                replacement,
                DraftRevision::new(1).unwrap(),
                next_thread_revision,
            )),
            FixtureRecord::HistorySummary(HistorySummaryRecord::new(
                self.thread,
                summary.revision().checked_next().unwrap(),
                next_thread_revision,
                summary.committed_tail(),
                summary.selected_path_digest(),
                summary.complete(),
                admitted_at,
            )),
            FixtureRecord::InputGate(
                InputGateRecord::new(
                    self.thread,
                    next_gate_revision,
                    source_gate.state().clone(),
                    ordinal.get(),
                    Some(generation),
                    source_gate.selected_route(),
                    source_gate.live_steering_count(),
                    1,
                    content.summary().logical_utf8_bytes(),
                )
                .unwrap(),
            ),
            FixtureRecord::AcceptedInput(input),
            FixtureRecord::AcceptedOrder(AcceptedOrderIndexRecord::new(
                self.thread,
                ordinal,
                input_id,
                generation,
            )),
            FixtureRecord::AcceptedRouteGenerationHead(AcceptedRouteGenerationHeadRecord::new(
                self.thread,
                AcceptedRouteHeadProof::new(generation, route_revision),
            )),
            FixtureRecord::AcceptedRouteGeneration(
                AcceptedRouteGenerationRecord::new(
                    self.thread,
                    generation,
                    route_revision,
                    AcceptedRouteTarget::NextTurn(NextTurnReason::Compaction),
                    Some(ordinal),
                    Some(ordinal),
                    1,
                    0,
                    0,
                    1,
                    0,
                    content.summary().logical_utf8_bytes(),
                    0,
                )
                .unwrap(),
            ),
            FixtureRecord::AcceptedRouteLeaf(AcceptedRouteLeafRecord::new(
                input_id,
                self.thread,
                generation,
                ordinal,
                beryl_model::AcceptedInputRevision::new(1).unwrap(),
                AcceptedRouteLeafState::NextTurn(NextTurnReason::Compaction),
                AcceptedInputLifecycle::Admitted,
            )),
            FixtureRecord::AcceptedNextSource(AcceptedNextSourceRecord::new(
                self.thread,
                generation,
                route_revision,
                ordinal,
                ordinal,
            )),
        ] {
            batch.put(record).unwrap();
        }
        batch
            .delete(FixtureDelete::Draft(source_draft.id()))
            .unwrap();
        crate::support::commit(&self.store, self.storage, batch);
        input_id
    }
}
