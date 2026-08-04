use std::{num::NonZeroUsize, sync::Arc, time::Duration};

use beryl_backend::{
    OrderedTurnStreamCompletion, OrderedTurnStreamOperation, OrderedTurnStreamSink, ProviderField,
    ProviderItemKind, ProviderItemLifecycle, ProviderObservationBegin, ProviderObservationControl,
    ProviderObservationRoute, ProviderScalar, ProviderValueContext,
    lifecycle_test_support::provider_observation_fragment,
};
use beryl_home_store::{HomeCommand, HomeOpenOptions, HomeSchemaVersion, HomeStore};
use beryl_model::{
    CasNativeTurnCount, CasProcessGeneration, CasThreadId, CasTurnId, ExecutionBinding, PathFlavor,
    RootId, RuntimeId, RuntimeMode, RuntimeNativePath, SyndicDraftId, SyndicThreadId,
};
use syndic_storage::{
    CasLineageProof, CasRepresentedPrefixProof, ClaimCompactionDispatch, CompactionAdmissionRead,
    CompactionAttemptNonce, CompactionMarkerLifecycle, CompactionOperationId,
    CompactionOperationNonce, CompactionProviderEvent, CompactionProviderSequence,
    CompactionThreadStatus, CreateThread, NativeCasLineage, PublishValidBinding, SelectedPathProof,
    SyndicPointReadLimit, SyndicStorage, SyndicTimestamp, empty_selected_path_digest,
};

use crate::{
    cas_projection::{
        accepted_input_scheduler::AcceptedInputSchedulerSignal,
        connection::{
            ConnectionRegistryAuthority, EventRouter, ProviderBroker, ProviderBrokerControl,
            TargetTurnRegistration,
            provider_broker::RunningProviderBrokerIngester,
            registry::LoadedThreadKey,
            router::{LiveEventTargetCloseReason, TargetRegistration},
        },
        context_compaction::{ContextCompactionCoordinator, ContextCompactionTargetAuthority},
        service_config::ProjectionWorkerPool,
        service_registry::ProjectionServiceConnectionRegistry,
        stop::StopCoordinator,
    },
    conversation_tools::ConversationToolRegistry,
};

const POINT_READ_BYTES: usize = 1_000_000;

struct Fixture {
    directory: tempfile::TempDir,
    home: Arc<HomeStore>,
    storage: SyndicStorage,
    coordinator: Arc<ContextCompactionCoordinator>,
    authority: Arc<ConnectionRegistryAuthority>,
    router: Arc<EventRouter>,
    registration: TargetRegistration,
    sink: Option<Box<dyn OrderedTurnStreamSink>>,
    broker: Option<Arc<ProviderBrokerControl>>,
    ingester: Option<RunningProviderBrokerIngester>,
    operation_id: CompactionOperationId,
    cas_thread_id: CasThreadId,
    cas_turn_id: CasTurnId,
}

impl Fixture {
    fn new(seed: u8) -> Self {
        let directory = tempfile::tempdir().unwrap();
        let mut home = HomeStore::open(HomeOpenOptions::new(
            directory.path(),
            HomeSchemaVersion::CURRENT,
        ))
        .unwrap();
        let storage = SyndicStorage::register(&mut home).unwrap();
        let thread_id = SyndicThreadId::from_bytes([seed; 16]);
        let runtime_id = RuntimeId::from_bytes([seed.wrapping_add(2); 16]);
        execute(
            &home,
            storage.create_thread(
                storage.revision(&home).unwrap(),
                CreateThread::ordinary(
                    thread_id,
                    SyndicDraftId::from_bytes([seed.wrapping_add(1); 16]),
                    execution_binding(runtime_id, seed),
                    SyndicTimestamp::from_unix_millis(1),
                ),
            ),
        );

        let thread = storage
            .thread(&home, thread_id, point_limit())
            .unwrap()
            .unwrap();
        let selected = SelectedPathProof::new(
            thread.committed_tail(),
            thread.revision(),
            thread.selected_path_digest(),
        );
        let process_generation = CasProcessGeneration::new(72_000 + u64::from(seed)).unwrap();
        let cas_thread_id = CasThreadId::new(format!("marker-thread-{seed}")).unwrap();
        let represented = CasRepresentedPrefixProof::new(
            None,
            selected.thread_revision(),
            empty_selected_path_digest(),
        );
        let lineage = CasLineageProof::native(NativeCasLineage::Fresh, represented).unwrap();
        let binding = storage
            .current_binding(&home, thread_id, point_limit())
            .unwrap()
            .unwrap();
        execute(
            &home,
            storage.publish_valid_binding(
                storage.revision(&home).unwrap(),
                PublishValidBinding::new(
                    thread_id,
                    binding.binding().revision(),
                    selected,
                    execution_binding(runtime_id, seed),
                    cas_thread_id.clone(),
                    represented,
                    CasNativeTurnCount::ZERO,
                    ConversationToolRegistry::canonical().profile(),
                    lineage,
                ),
            ),
        );

        let authority =
            Arc::new(ConnectionRegistryAuthority::new(runtime_id, process_generation).unwrap());
        let key = LoadedThreadKey {
            runtime_id,
            process_generation,
            cas_thread_id: cas_thread_id.clone(),
        };
        let (loaded_generation, _lease) = authority
            .register_new_for_test(key.clone(), thread_id)
            .unwrap()
            .unwrap();
        let candidate = match storage
            .compaction_admission_read(&home, thread_id, point_limit())
            .unwrap()
        {
            CompactionAdmissionRead::Admissible(candidate) => candidate,
            other => panic!("empty valid thread was not compaction-admissible: {other:?}"),
        };
        let attempt = CompactionAttemptNonce::from_bytes([seed.wrapping_add(3); 16]);
        let admission = candidate.admission(
            CompactionOperationNonce::from_bytes([seed.wrapping_add(4); 16]),
            attempt,
            loaded_generation,
            SyndicTimestamp::from_unix_millis(2),
        );
        let operation_id = admission.operation_id();
        home.execute_current(storage.current_admit_compaction_operation(admission))
            .unwrap();
        let admitted = operation(&home, storage, operation_id);
        home.execute_current(storage.current_claim_compaction_dispatch(
            ClaimCompactionDispatch::new(operation_id, admitted.revision(), attempt),
        ))
        .unwrap();

        let home_generation = home.health().generation().unwrap();
        let home_id = home.home_id();
        let home = Arc::new(home);
        let stop_coordinator = Arc::new(StopCoordinator::new_for_test(
            &home,
            home_id,
            home_generation,
            storage,
        ));
        let failure_notification =
            crate::cas_projection::persistent_failure::test_failure_notification(
                &home,
                home_id,
                home_generation,
            );
        let commands = crate::cas_projection::persistent_failure::MasterCommandGate::new(
            failure_notification.service_generation(),
            Some(failure_notification.clone()),
        )
        .authorizer();
        let scheduler_signal = AcceptedInputSchedulerSignal::new();
        let coordinator = ContextCompactionCoordinator::new(
            Arc::clone(&home),
            home_id,
            home_generation,
            storage,
            ProjectionServiceConnectionRegistry::new(commands.service_generation()),
            Arc::clone(&stop_coordinator),
            commands.clone(),
            scheduler_signal.clone(),
        )
        .unwrap();
        #[cfg(feature = "test-faults")]
        coordinator
            .lifecycle_test_harness()
            .mount_lifecycle_operation(
                operation_id,
                attempt,
                operation_id.provider_turn_id(),
                Duration::from_secs(30),
            )
            .unwrap();
        let router = Arc::new(
            EventRouter::new_with_scheduler(
                runtime_id,
                process_generation,
                authority.generation_for_test().get(),
                scheduler_signal,
                commands.clone(),
                None,
            )
            .unwrap(),
        );
        let router_command = commands.authorize().unwrap();
        let registration = router
            .register(
                &router_command,
                key,
                thread_id,
                loaded_generation,
                home_generation.get(),
                Duration::from_secs(1),
                TargetTurnRegistration::ContextCompaction(ContextCompactionTargetAuthority::new(
                    operation_id,
                    operation_id.provider_turn_id(),
                )),
            )
            .unwrap();
        drop(router_command);
        router
            .authorize_context_compaction_command(&registration.proof())
            .unwrap();
        let cas_turn_id = CasTurnId::new(format!("marker-turn-{seed}")).unwrap();
        coordinator
            .publish_provider_event(
                registration.compaction().unwrap(),
                CompactionProviderEvent::ThreadStatus(CompactionThreadStatus::Active),
                SyndicTimestamp::from_unix_millis(3),
            )
            .unwrap();
        router
            .acquire_compaction_thread_status(&cas_thread_id)
            .unwrap()
            .finish()
            .unwrap();
        coordinator
            .publish_provider_event(
                registration.compaction().unwrap(),
                CompactionProviderEvent::TurnStarted(cas_turn_id.clone()),
                SyndicTimestamp::from_unix_millis(4),
            )
            .unwrap();
        router
            .acquire_compaction_turn_started(&cas_thread_id, &cas_turn_id)
            .unwrap()
            .finish()
            .unwrap();
        let worker_pool =
            ProjectionWorkerPool::new(NonZeroUsize::new(3).expect("worker pool is nonzero"));
        let mut workers = worker_pool.try_acquire_pair().unwrap();
        let ingester_worker = workers.take_ingester();
        drop(workers.take_driver());
        let (sink, broker, ingester) = ProviderBroker::start(
            Arc::clone(&home),
            home_id,
            home_generation,
            Arc::clone(&authority),
            Arc::clone(&router),
            stop_coordinator,
            Arc::clone(&coordinator),
            commands,
            failure_notification,
            ingester_worker,
        )
        .unwrap();

        Self {
            directory,
            home,
            storage,
            coordinator,
            authority,
            router,
            registration,
            sink: Some(sink),
            broker: Some(broker),
            ingester: Some(ingester),
            operation_id,
            cas_thread_id,
            cas_turn_id,
        }
    }

    fn publish_marker(
        &mut self,
        observed_at: u64,
        lifecycle: ProviderItemLifecycle,
        kind: ProviderItemKind,
    ) {
        self.submit_applied(OrderedTurnStreamOperation::ProviderBegin(
            ProviderObservationBegin::Item { lifecycle, kind },
        ));
        self.submit_applied(OrderedTurnStreamOperation::ProviderControl(
            ProviderObservationControl::Scalar {
                context: ProviderValueContext::Field(ProviderField::LifecycleObservedAt),
                value: ProviderScalar::Unsigned(observed_at),
            },
        ));
        let item_context = ProviderValueContext::Field(ProviderField::ItemId);
        self.submit_applied(OrderedTurnStreamOperation::ProviderControl(
            ProviderObservationControl::BeginField(item_context),
        ));
        self.submit_fragment(item_context, b"compaction-marker");
        self.submit_applied(OrderedTurnStreamOperation::ProviderControl(
            ProviderObservationControl::EndField(item_context),
        ));
        if kind == ProviderItemKind::EnteredReviewMode {
            let review_context = ProviderValueContext::Field(ProviderField::EnteredReview);
            self.submit_applied(OrderedTurnStreamOperation::ProviderControl(
                ProviderObservationControl::BeginField(review_context),
            ));
            self.submit_fragment(review_context, b"review");
            self.submit_applied(OrderedTurnStreamOperation::ProviderControl(
                ProviderObservationControl::EndField(review_context),
            ));
        }
        self.submit_applied(OrderedTurnStreamOperation::ProviderSeal(
            ProviderObservationRoute::new(self.cas_thread_id.clone(), self.cas_turn_id.clone()),
        ));
    }

    fn submit_fragment(&mut self, context: ProviderValueContext, bytes: &[u8]) {
        let mut page = match self
            .sink
            .as_mut()
            .unwrap()
            .submit(OrderedTurnStreamOperation::ProviderAcquirePage)
            .unwrap()
        {
            OrderedTurnStreamCompletion::PageLease(page) => page,
            completion => panic!("unexpected acquire completion: {completion:?}"),
        };
        page.buffer_mut()[..bytes.len()].copy_from_slice(bytes);
        page.set_len(bytes.len()).unwrap();
        let fragment = provider_observation_fragment(context, page);
        assert!(matches!(
            self.sink
                .as_mut()
                .unwrap()
                .submit(OrderedTurnStreamOperation::ProviderFragment(fragment)),
            Ok(OrderedTurnStreamCompletion::PageLease(_))
        ));
    }

    fn submit_applied(&mut self, operation: OrderedTurnStreamOperation) {
        assert!(matches!(
            self.sink.as_mut().unwrap().submit(operation),
            Ok(OrderedTurnStreamCompletion::Applied)
        ));
    }

    fn operation(&self) -> syndic_storage::CompactionOperationRecord {
        operation(&self.home, self.storage, self.operation_id)
    }
}

#[cfg(feature = "test-faults")]
#[test]
fn mounted_compaction_markers_advance_only_the_dedicated_frontier() {
    let mut fixture = Fixture::new(181);

    fixture.publish_marker(
        191,
        ProviderItemLifecycle::Started,
        ProviderItemKind::ContextCompaction,
    );
    fixture.publish_marker(
        192,
        ProviderItemLifecycle::Completed,
        ProviderItemKind::ContextCompaction,
    );

    let operation = fixture.operation();
    assert_eq!(
        operation.provider_frontier(),
        Some(CompactionProviderSequence::new(4).unwrap())
    );
    assert_eq!(
        operation.marker().unwrap().lifecycle(),
        CompactionMarkerLifecycle::Completed
    );
    let turn = fixture
        .storage
        .turn_state(
            &fixture.home,
            fixture.operation_id.provider_turn_id(),
            point_limit(),
        )
        .unwrap()
        .unwrap();
    assert_eq!(turn.source_event_count(), 0);
    #[cfg(feature = "test-faults")]
    {
        let staging = fixture.broker.as_ref().unwrap().test_snapshot();
        assert_eq!(staging.provider_staging_batches(), 0);
        assert_eq!(staging.staged_fragment_batches(), 0);
    }
}

#[cfg(feature = "test-faults")]
#[test]
fn compaction_target_rejects_non_compaction_item_before_publication() {
    let mut fixture = Fixture::new(182);

    fixture.publish_marker(
        193,
        ProviderItemLifecycle::Started,
        ProviderItemKind::EnteredReviewMode,
    );
    assert_eq!(
        fixture.operation().provider_frontier(),
        Some(CompactionProviderSequence::new(2).unwrap())
    );
    let turn = fixture
        .storage
        .turn_state(
            &fixture.home,
            fixture.operation_id.provider_turn_id(),
            point_limit(),
        )
        .unwrap()
        .unwrap();
    assert_eq!(turn.source_event_count(), 0);
}

#[cfg(feature = "test-faults")]
mod failure_cases {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/unit/provider_broker_compaction_marker_failures.rs"
    ));
}

fn execute(home: &HomeStore, contribution: beryl_home_store::MutationContribution) {
    let mut command = HomeCommand::new(home.home_revision().unwrap());
    command.add(contribution).unwrap();
    home.execute(command).unwrap();
}

fn execution_binding(runtime_id: RuntimeId, seed: u8) -> ExecutionBinding {
    ExecutionBinding::new(
        runtime_id,
        RootId::from_bytes([seed.wrapping_add(5); 16]),
        RuntimeNativePath::from_admitted(
            RuntimeMode::host(),
            PathFlavor::Windows,
            r"C:\work\beryl-compaction-marker",
        )
        .unwrap(),
    )
}

fn operation(
    home: &HomeStore,
    storage: SyndicStorage,
    operation_id: CompactionOperationId,
) -> syndic_storage::CompactionOperationRecord {
    storage
        .compaction_operation(home, operation_id, point_limit())
        .unwrap()
        .unwrap()
}

fn point_limit() -> SyndicPointReadLimit {
    SyndicPointReadLimit::new(POINT_READ_BYTES).unwrap()
}

impl Drop for Fixture {
    fn drop(&mut self) {
        drop(self.sink.take());
        if let Some(ingester) = self.ingester.take() {
            let broker = self.broker.as_ref().unwrap();
            let stopped = ingester.stop_and_join();
            assert!(
                stopped
                    .receipt()
                    .is_exact(broker.commands.service_generation(), broker.home_generation)
            );
            drop(stopped);
        }
        self.coordinator.request_shutdown();
        let _ = self.authority.retire();
        self.router
            .retire(LiveEventTargetCloseReason::WorkerStopped);
        let _ = &self.registration;
        let _ = &self.directory;
    }
}
