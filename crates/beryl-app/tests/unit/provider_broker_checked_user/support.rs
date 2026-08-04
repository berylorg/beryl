//! Durable checked-user broker test fixture and assertions.

#[path = "support/frame.rs"]
mod frame;
#[path = "support/storage.rs"]
mod storage;
#[path = "support/terminal.rs"]
mod terminal;

use std::{num::NonZeroUsize, sync::Arc, time::Duration};

use crate::{
    cas_projection::{
        connection::{
            provider_broker::RunningProviderBrokerIngester,
            registry::LoadedThreadKey,
            router::{LiveEventTargetCloseReason, TargetRegistration},
            ConnectionRegistryAuthority, EventRouter, ProviderBroker, ProviderBrokerControl,
            TargetTurnRegistration,
        },
        service_config::ProjectionWorkerPool,
        service_registry::ProjectionServiceConnectionRegistry,
        test_faults::{
            install_checked_user_publication_barrier_for_key,
            CheckedUserPublicationBarrierController, ProviderBrokerSnapshot,
        },
        PendingTurnActivation,
    },
    conversation_tools::ConversationToolRegistry,
    input_admission::idle_submission_command,
};
use beryl_backend::{
    lifecycle_test_support::checked_user_message, CheckedUserMessage, OrderedTurnStreamCompletion,
    OrderedTurnStreamOperation, OrderedTurnStreamSink, UserMessageEchoLifecycle,
};
use beryl_home_store::{
    test_faults::FaultController, CursorReadLimits, HomeCommand, HomeOpenOptions,
    HomeSchemaVersion, HomeStore,
};
use beryl_model::{
    CasItemId, CasNativeTurnCount, CasProcessGeneration, CasThreadId, CasTurnId, ExecutionBinding,
    PathFlavor, RootId, RuntimeId, RuntimeMode, RuntimeNativePath, SyndicDraftId,
    SyndicExecutionSnapshotId, SyndicItemId, SyndicThreadId, SyndicTurnId,
};
use beryl_state::BerylState;
use syndic_storage::{
    empty_selected_path_digest, ActivateBinding, BindingState, CanonicalItemKind, CasLineageProof,
    CasRepresentedPrefixProof, ComposerAtom, ComposerPayload, ContentAppend, ContentBuild,
    ContentReference, CreateThread, DraftPayloadUpdate, DraftPayloadUpdateDecision, IdleSubmission,
    NativeCasLineage, PreparedContent, ProviderFrameOrdinalV1, ProviderItemFrameV1,
    ProviderItemLifecycle, ProviderItemObservationV1, ProviderItemV1,
    ProviderLifecycleTimestampMsV1, PublishValidBinding, SelectedPathProof, SourceEventRecord,
    SourceEventSequence, SyndicPointReadLimit, SyndicStorage, SyndicTimestamp, TurnItemOrdinal,
};

pub(super) use frame::{assert_user_message_frame, read_provider_frame};
pub(super) use storage::point_limit;
use storage::{execute, execution_binding, selected_path, stage_prepared_content};

const POINT_READ_BYTES: usize = 1_000_000;
const EXECUTION_ROOT: &str = r"C:\work\beryl-checked-user-test";

pub(super) struct CheckedUserFixture {
    directory: tempfile::TempDir,
    pub(super) home: Arc<HomeStore>,
    pub(super) storage: SyndicStorage,
    worker_pool: ProjectionWorkerPool,
    sink: Option<Box<dyn OrderedTurnStreamSink>>,
    pub(super) broker: Option<Arc<ProviderBrokerControl>>,
    ingester: Option<RunningProviderBrokerIngester>,
    pub(super) commands: crate::cas_projection::persistent_failure::LiveCommandAuthorizer,
    authority: Arc<ConnectionRegistryAuthority>,
    router: Arc<EventRouter>,
    pub(super) registration: TargetRegistration,
    pub(super) thread_id: SyndicThreadId,
    pub(super) turn_id: SyndicTurnId,
    pub(super) item_id: SyndicItemId,
    pub(super) submitted_content: ContentReference,
    pub(super) snapshot_id: SyndicExecutionSnapshotId,
    pub(super) cas_thread_id: CasThreadId,
    pub(super) cas_turn_id: CasTurnId,
}

impl CheckedUserFixture {
    pub(super) fn new(seed: u8) -> Self {
        Self::new_with_start_authority(seed, true, None)
    }

    pub(super) fn before_turn_start(seed: u8) -> Self {
        Self::new_with_start_authority(seed, false, None)
    }

    pub(super) fn with_faults(seed: u8, faults: FaultController) -> Self {
        Self::new_with_start_authority(seed, true, Some(faults))
    }

    fn new_with_start_authority(
        seed: u8,
        authorize_turn_start: bool,
        faults: Option<FaultController>,
    ) -> Self {
        let directory = tempfile::tempdir().unwrap();
        let options = HomeOpenOptions::new(directory.path(), HomeSchemaVersion::CURRENT);
        let mut home = match faults {
            Some(faults) => HomeStore::open_with_faults(options, faults).unwrap(),
            None => HomeStore::open(options).unwrap(),
        };
        let storage = SyndicStorage::register(&mut home).unwrap();
        let state = BerylState::register(&mut home).unwrap();
        let thread_id = SyndicThreadId::from_bytes([seed; 16]);
        let initial_draft_id = SyndicDraftId::from_bytes([seed.wrapping_add(1); 16]);
        let runtime_id = RuntimeId::from_bytes([seed.wrapping_add(4); 16]);
        execute(
            &home,
            storage.create_thread(
                storage.revision(&home).unwrap(),
                CreateThread::ordinary(
                    thread_id,
                    initial_draft_id,
                    execution_binding(runtime_id, seed),
                    SyndicTimestamp::from_unix_millis(1),
                ),
            ),
        );

        let prepared = PreparedContent::composer(
            &ComposerPayload::new(vec![ComposerAtom::text("checked submitted user").unwrap()])
                .unwrap(),
        )
        .unwrap();
        stage_prepared_content(&home, storage, &prepared);
        let current = storage
            .current_draft(&home, thread_id, point_limit())
            .unwrap()
            .unwrap();
        let DraftPayloadUpdateDecision::Update(update) =
            DraftPayloadUpdate::prepare(&current, &prepared, SyndicTimestamp::from_unix_millis(2))
                .unwrap()
        else {
            panic!("checked-user fixture must replace the initial empty draft")
        };
        execute(
            &home,
            storage.update_draft_payload(storage.revision(&home).unwrap(), update),
        );
        let current = storage
            .current_draft(&home, thread_id, point_limit())
            .unwrap()
            .unwrap();
        let gate = storage
            .input_gate(&home, thread_id, point_limit())
            .unwrap()
            .unwrap();
        let item_id = SyndicItemId::from_bytes([seed.wrapping_add(2); 16]);
        let submission = IdleSubmission::new(
            thread_id,
            current.thread().revision(),
            current.draft().id(),
            current.draft().revision(),
            current.draft().content(),
            gate.revision(),
            SyndicDraftId::from_bytes([seed.wrapping_add(3); 16]),
            item_id,
            None,
            SyndicTimestamp::from_unix_millis(3),
        );
        let turn_id = submission.submitted_turn_id();
        home.execute(idle_submission_command(&home, storage, state.assets(), submission).unwrap())
            .unwrap();

        let item = storage
            .canonical_item(&home, item_id, point_limit())
            .unwrap()
            .unwrap();
        let submitted_content = item.presentation_content().unwrap();
        assert_eq!(item.kind(), CanonicalItemKind::UserInput);
        assert_eq!(item.ordinal(), TurnItemOrdinal::FIRST);
        assert_eq!(
            item.provider_lifecycle(),
            ProviderItemLifecycle::AwaitingCorrelation
        );

        let selected = selected_path(&home, storage, thread_id);
        let process_generation = CasProcessGeneration::new(36_000 + u64::from(seed)).unwrap();
        let cas_thread_id = CasThreadId::new(format!("checked-user-thread-{seed}")).unwrap();
        let cas_turn_id = CasTurnId::new(format!("checked-user-turn-{seed}")).unwrap();
        let execution = execution_binding(runtime_id, seed);
        let represented = CasRepresentedPrefixProof::new(
            None,
            selected.thread_revision(),
            empty_selected_path_digest(),
        );
        let lineage =
            CasLineageProof::native(NativeCasLineage::Fresh, represented.clone()).unwrap();
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
                    execution,
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
        let (loaded_generation, _lease_token) = authority
            .register_new_for_test(key.clone(), thread_id)
            .unwrap()
            .unwrap();
        let binding = storage
            .current_binding(&home, thread_id, point_limit())
            .unwrap()
            .unwrap();
        let gate = storage
            .input_gate(&home, thread_id, point_limit())
            .unwrap()
            .unwrap();
        let snapshot_id = SyndicExecutionSnapshotId::from_bytes([seed.wrapping_add(5); 16]);
        let observed_at = SyndicTimestamp::from_unix_millis(4);
        execute(
            &home,
            storage.activate_binding(
                storage.revision(&home).unwrap(),
                ActivateBinding::new(
                    thread_id,
                    binding.binding().revision(),
                    gate.revision(),
                    selected,
                    snapshot_id,
                    turn_id,
                    loaded_generation,
                    observed_at,
                ),
            ),
        );
        let binding = storage
            .current_binding(&home, thread_id, point_limit())
            .unwrap()
            .unwrap();
        assert!(matches!(binding.binding().state(), BindingState::Active(_)));
        let gate = storage
            .input_gate(&home, thread_id, point_limit())
            .unwrap()
            .unwrap();
        let turn_state = storage
            .turn_state(&home, turn_id, point_limit())
            .unwrap()
            .unwrap();
        let pending = PendingTurnActivation::new(
            thread_id,
            turn_id,
            binding.binding().revision(),
            gate.revision(),
            turn_state.revision(),
            snapshot_id,
            observed_at,
        );

        let home_generation = home.health().generation().unwrap();
        let home_id = home.home_id();
        let home = Arc::new(home);
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
        let scheduler_signal =
            crate::cas_projection::accepted_input_scheduler::AcceptedInputSchedulerSignal::new();
        let router = Arc::new(
            EventRouter::new_with_scheduler(
                runtime_id,
                process_generation,
                authority.generation_for_test().get(),
                scheduler_signal.clone(),
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
                TargetTurnRegistration::Pending(pending),
            )
            .unwrap();
        drop(router_command);
        if authorize_turn_start {
            router.authorize_turn_start(&registration.proof()).unwrap();
        }

        let worker_pool =
            ProjectionWorkerPool::new(NonZeroUsize::new(3).expect("worker pool is nonzero"));
        let mut worker_pair = worker_pool.try_acquire_pair().unwrap();
        let ingester_worker = worker_pair.take_ingester();
        drop(worker_pair.take_driver());
        let stop_coordinator =
            Arc::new(crate::cas_projection::stop::StopCoordinator::new_for_test(
                &home,
                home_id,
                home_generation,
                storage,
            ));
        let context_compaction =
            crate::cas_projection::context_compaction::ContextCompactionCoordinator::new(
                Arc::clone(&home),
                home_id,
                home_generation,
                storage,
                ProjectionServiceConnectionRegistry::new(commands.service_generation()),
                Arc::clone(&stop_coordinator),
                commands.clone(),
                scheduler_signal,
            )
            .unwrap();
        let (sink, broker, ingester) = ProviderBroker::start(
            Arc::clone(&home),
            home_id,
            home_generation,
            Arc::clone(&authority),
            Arc::clone(&router),
            stop_coordinator,
            context_compaction,
            commands.clone(),
            failure_notification,
            ingester_worker,
        )
        .unwrap();

        Self {
            directory,
            home,
            storage,
            worker_pool,
            sink: Some(sink),
            broker: Some(broker),
            ingester: Some(ingester),
            commands,
            authority,
            router,
            registration,
            thread_id,
            turn_id,
            item_id,
            submitted_content,
            snapshot_id,
            cas_thread_id,
            cas_turn_id,
        }
    }

    pub(super) fn submit_checked(
        &mut self,
        lifecycle: UserMessageEchoLifecycle,
        item_id: CasItemId,
    ) {
        let message = self.checked_message(lifecycle, item_id);
        assert!(matches!(
            self.sink
                .as_mut()
                .unwrap()
                .submit(OrderedTurnStreamOperation::CheckedUserMessage(message)),
            Ok(OrderedTurnStreamCompletion::Applied)
        ));
    }

    pub(super) fn submit_checked_while_publication_paused(
        &mut self,
        lifecycle: UserMessageEchoLifecycle,
        item_id: CasItemId,
        inspect: impl FnOnce(&Self),
    ) {
        let barrier = self.checked_publication_barrier(lifecycle);
        let message = self.checked_message(lifecycle, item_id);
        let mut sink = self.sink.take().expect("checked-user sink remains open");
        let (returned_sink, result) = std::thread::scope(|scope| {
            let worker = scope.spawn(move || {
                let result = sink.submit(OrderedTurnStreamOperation::CheckedUserMessage(message));
                (sink, result)
            });
            assert!(barrier.wait_until_paused(Duration::from_secs(1)));
            inspect(self);
            barrier.release();
            worker.join().unwrap()
        });
        self.sink = Some(returned_sink);
        assert!(matches!(result, Ok(OrderedTurnStreamCompletion::Applied)));
    }

    pub(super) fn broker_snapshot(&self) -> ProviderBrokerSnapshot {
        self.broker
            .as_ref()
            .expect("checked-user broker remains open")
            .test_snapshot()
    }

    fn checked_publication_barrier(
        &self,
        lifecycle: UserMessageEchoLifecycle,
    ) -> CheckedUserPublicationBarrierController {
        install_checked_user_publication_barrier_for_key(
            self.broker
                .as_ref()
                .expect("checked-user broker remains open")
                .test_key(),
            lifecycle,
        )
    }

    fn checked_message(
        &self,
        lifecycle: UserMessageEchoLifecycle,
        item_id: CasItemId,
    ) -> CheckedUserMessage {
        checked_user_message(
            lifecycle,
            self.cas_thread_id.clone(),
            self.cas_turn_id.clone(),
            item_id,
            match lifecycle {
                UserMessageEchoLifecycle::Started => 10,
                UserMessageEchoLifecycle::Completed => 11,
            },
            1,
        )
    }

    pub(super) fn canonical_item(&self) -> syndic_storage::CanonicalItemRecord {
        self.storage
            .canonical_item(&self.home, self.item_id, point_limit())
            .unwrap()
            .unwrap()
    }

    pub(super) fn source_event(&self, sequence: u64) -> SourceEventRecord {
        self.storage
            .source_event(
                &self.home,
                self.turn_id,
                SourceEventSequence::new(sequence).unwrap(),
                point_limit(),
            )
            .unwrap()
            .unwrap()
    }

    pub(super) fn close(self) {
        let Self {
            directory,
            home,
            storage: _,
            worker_pool,
            sink,
            broker,
            ingester,
            commands: _,
            authority,
            router,
            registration,
            thread_id: _,
            turn_id: _,
            item_id: _,
            submitted_content: _,
            snapshot_id: _,
            cas_thread_id: _,
            cas_turn_id: _,
        } = self;
        drop(sink);
        let broker_control = broker
            .as_ref()
            .expect("checked-user broker control remains owned");
        let stopped = ingester
            .expect("checked-user ingester remains owned")
            .stop_and_join();
        assert!(stopped.receipt().is_exact(
            broker_control.commands.service_generation(),
            broker_control.home_generation,
        ));
        drop(stopped);
        drop(broker);
        drop(registration);
        authority.retire().unwrap();
        router.retire(LiveEventTargetCloseReason::WorkerStopped);
        drop(router);
        drop(authority);
        let home =
            Arc::try_unwrap(home).unwrap_or_else(|_| panic!("broker retained the test home"));
        home.close().unwrap();
        drop(directory);
        assert_eq!(worker_pool.diagnostics().active(), 0);
    }
}
