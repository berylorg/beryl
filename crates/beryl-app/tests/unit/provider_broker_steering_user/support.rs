use std::{
    num::{NonZeroU64, NonZeroUsize},
    sync::Arc,
    time::Duration,
};

use beryl_backend::{
    lifecycle_test_support::decode_provider_json_for_test, ClientUserMessageId,
    ManagedBackendError, OrderedTurnStreamProgress, OrderedTurnStreamSink,
    UserMessageEchoLifecycle,
};
use beryl_home_store::{
    CommandOutcome, HomeCommand, HomeOpenOptions, HomeSchemaVersion, HomeStore, SidecarByteLimit,
    SidecarNamespace,
};
use beryl_model::{
    AssetId, CasItemId, CasNativeTurnCount, CasProcessGeneration, CasThreadId, CasTurnId,
    ExecutionBinding, ImageLabelOrdinal, PathFlavor, RootId, RuntimeId, RuntimeMode,
    RuntimeNativePath, SyndicAcceptedInputId, SyndicDraftId, SyndicExecutionSnapshotId,
    SyndicItemId, SyndicThreadId, SyndicTurnId,
};
use beryl_state::{AssetMediaType, BerylState, PublishAssetMetadata};
use syndic_storage::{
    empty_selected_path_digest, AcceptedInputLifecycle, AcceptedRouteEffectiveState,
    AcceptedRouteLeafState, ActivateBinding, BeginAcceptedInputDelivery, BindingState,
    CasLineageProof, CasRepresentedPrefixProof, CreateThread, DraftEditHistoryPolicyV1,
    FirstAcceptanceKind, NativeCasLineage, NextTurnReason, PublishActiveCasTurn,
    PublishValidBinding, RetryAcceptedInputDelivery, SelectedPathProof,
    SyndicDeliveringSteeringInput, SyndicPointReadLimit, SyndicStorage, SyndicTimestamp,
    TurnIncompleteReason,
};

use crate::{
    cas_projection::{
        connection::{
            provider_broker::{CheckedSteeringLifecycleOwner, RunningProviderBrokerIngester},
            registry::LoadedThreadKey,
            router::{ActiveSteeringAttemptPermit, LiveEventTargetCloseReason, TargetRegistration},
            ConnectionRegistryAuthority, EventRouter, ProviderBroker, ProviderBrokerControl,
            TargetTurnRegistration,
        },
        input_replay::encode_accepted_input_steering_correlation,
        service_config::ProjectionWorkerPool,
        service_registry::ProjectionServiceConnectionRegistry,
        test_faults::install_steering_selection_barrier_for_key,
        PendingTurnActivation,
    },
    conversation_tools::ConversationToolRegistry,
};
use super::submission_fixture::{submit_atoms, Atom};

const POINT_READ_BYTES: usize = 1_000_000;
const EXECUTION_ROOT: &str = r"C:\work\beryl-steering-user-test";
pub(super) const STEERING_TEXT: &str = "delayed steering text";
pub(super) const IMAGE_STEERING_TEXT: &str = "image steering Image A:";

#[derive(Clone, Copy)]
enum FixtureInput {
    Text,
    Image,
}

pub(super) struct SteeringFixture {
    directory: tempfile::TempDir,
    pub(super) home: Arc<HomeStore>,
    pub(super) storage: SyndicStorage,
    worker_pool: ProjectionWorkerPool,
    sink: Option<Box<dyn OrderedTurnStreamSink>>,
    pub(super) broker: Option<Arc<ProviderBrokerControl>>,
    ingester: Option<RunningProviderBrokerIngester>,
    lifecycle_owner: Option<CheckedSteeringLifecycleOwner>,
    authority: Arc<ConnectionRegistryAuthority>,
    router: Arc<EventRouter>,
    pub(super) registration: TargetRegistration,
    pub(super) thread_id: SyndicThreadId,
    pub(super) turn_id: SyndicTurnId,
    pub(super) accepted_input_id: SyndicAcceptedInputId,
    pub(super) cas_thread_id: CasThreadId,
    pub(super) cas_turn_id: CasTurnId,
    pub(super) cas_item_id: CasItemId,
    image_path: Option<Box<str>>,
}

impl SteeringFixture {
    pub(super) fn new(seed: u8) -> Self {
        Self::build(seed, None, FixtureInput::Text)
    }

    pub(super) fn image(seed: u8) -> Self {
        Self::build(seed, None, FixtureInput::Image)
    }

    pub(super) fn with_router_turn_mismatch(seed: u8) -> Self {
        Self::build(
            seed,
            Some(CasTurnId::new(format!("wrong-steering-turn-{seed}")).unwrap()),
            FixtureInput::Text,
        )
    }

    fn build(seed: u8, router_turn: Option<CasTurnId>, fixture_input: FixtureInput) -> Self {
        let directory = tempfile::tempdir().unwrap();
        let mut home = HomeStore::open(HomeOpenOptions::new(
            directory.path(),
            HomeSchemaVersion::CURRENT,
        ))
        .unwrap();
        let storage = SyndicStorage::register(&mut home).unwrap();
        let state = BerylState::register(&mut home).unwrap();
        let thread_id = SyndicThreadId::from_bytes([seed; 16]);
        let runtime_id = RuntimeId::from_bytes([seed.wrapping_add(4); 16]);
        execute(
            &home,
            storage.create_thread(
                storage.revision(&home).unwrap(),
                CreateThread::ordinary(
                    thread_id,
                    SyndicDraftId::from_bytes([seed.wrapping_add(1); 16]),
                    execution_binding(runtime_id, seed),
                    timestamp(1),
                    DraftEditHistoryPolicyV1::new(65_536, 1).unwrap(),
                ),
            ),
        );

        let initial_item = SyndicItemId::from_bytes([seed.wrapping_add(3); 16]);
        let (kind, source_draft) = submit_atoms(
            &home,
            storage,
            state.assets(),
            thread_id,
            SyndicDraftId::from_bytes([seed.wrapping_add(2); 16]),
            initial_item,
            &[Atom::Text("initial turn")],
            seed.wrapping_add(20),
            timestamp(3),
        );
        assert!(matches!(kind, FirstAcceptanceKind::Idle { user_item_id } if user_item_id == initial_item));
        let turn_id = source_draft.submitted_turn_id();

        let selected = selected_path(&home, storage, thread_id);
        let process_generation = CasProcessGeneration::new(52_000 + u64::from(seed)).unwrap();
        let cas_thread_id = CasThreadId::new(format!("steering-thread-{seed}")).unwrap();
        let cas_turn_id = CasTurnId::new(format!("steering-turn-{seed}")).unwrap();
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
        let observed_at = timestamp(4);
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
        let pending_activation = PendingTurnActivation::new(
            thread_id,
            turn_id,
            binding.binding().revision(),
            gate.revision(),
            turn_state.revision(),
            snapshot_id,
            observed_at,
        );
        execute(
            &home,
            storage.publish_active_cas_turn(
                storage.revision(&home).unwrap(),
                PublishActiveCasTurn::new(
                    thread_id,
                    binding.binding().revision(),
                    gate.revision(),
                    snapshot_id,
                    cas_thread_id.clone(),
                    cas_turn_id.clone(),
                    timestamp(5),
                ),
            ),
        );

        let (atoms, image_path) = match fixture_input {
            FixtureInput::Text => (vec![Atom::Text(STEERING_TEXT)], None),
            FixtureInput::Image => {
                let asset = publish_image_asset(&home, &state);
                let verified = state.assets().verify_sidecar(&home, asset).unwrap();
                let path = verified
                    .path()
                    .to_str()
                    .expect("test sidecar path is Unicode")
                    .into();
                drop(verified);
                (
                    vec![
                        Atom::Text("image steering "),
                        Atom::Image(ImageLabelOrdinal::FIRST, asset),
                    ],
                    Some(path),
                )
            }
        };
        let (kind, source_draft) = submit_atoms(
            &home,
            storage,
            state.assets(),
            thread_id,
            SyndicDraftId::from_bytes([seed.wrapping_add(6); 16]),
            SyndicItemId::from_bytes([seed.wrapping_add(7); 16]),
            &atoms,
            seed.wrapping_add(40),
            timestamp(7),
        );
        assert_eq!(kind, FirstAcceptanceKind::Accepted);
        let accepted_input_id = source_draft.accepted_input_id();

        let gate = storage
            .input_gate(&home, thread_id, point_limit())
            .unwrap()
            .unwrap();
        let route = gate.selected_route().unwrap();
        let page = storage
            .accepted_route_page(&home, thread_id, route.generation(), route.revision(), None)
            .unwrap();
        let entry = page
            .records()
            .iter()
            .find(|entry| entry.input().id() == accepted_input_id)
            .unwrap();
        assert_eq!(entry.leaf().state(), AcceptedRouteLeafState::Routed);
        assert_eq!(entry.leaf().lifecycle(), AcceptedInputLifecycle::Admitted);
        assert_eq!(entry.effective_state(), AcceptedRouteEffectiveState::Ready);
        let ready = storage
            .ready_steering_input(&home, accepted_input_id, point_limit())
            .unwrap()
            .expect("fixture accepted input has an exact steering target");
        execute(
            &home,
            storage.begin_accepted_input_delivery(
                storage.revision(&home).unwrap(),
                BeginAcceptedInputDelivery::new(
                    thread_id,
                    accepted_input_id,
                    entry.leaf().revision(),
                    ready.target().clone(),
                ),
            ),
        );
        let delivering = storage
            .delivering_steering_input(&home, accepted_input_id, point_limit())
            .unwrap()
            .expect("fixture accepted input is Routed + Delivering");
        assert_eq!(delivering.input().id(), accepted_input_id);

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
                TargetTurnRegistration::Pending(pending_activation),
            )
            .unwrap();
        drop(router_command);
        router.authorize_turn_start(&registration.proof()).unwrap();
        router
            .acquire_source_publication(
                &cas_thread_id,
                &router_turn.unwrap_or_else(|| cas_turn_id.clone()),
            )
            .unwrap()
            .finish()
            .unwrap();

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
            commands,
            failure_notification,
            ingester_worker,
        )
        .unwrap();
        let lifecycle_owner = broker
            .arm_checked_steering_lifecycle_for_test(
                &delivering,
                home_generation,
                &encode_accepted_input_steering_correlation(accepted_input_id),
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
            lifecycle_owner: Some(lifecycle_owner),
            authority,
            router,
            registration,
            thread_id,
            turn_id,
            accepted_input_id,
            cas_thread_id,
            cas_turn_id,
            cas_item_id: CasItemId::new(format!("steering-item-{seed}")).unwrap(),
            image_path,
        }
    }

    pub(super) fn correlation(&self) -> ClientUserMessageId {
        encode_accepted_input_steering_correlation(self.accepted_input_id)
    }

    pub(super) fn active_attempt(&self) -> ActiveSteeringAttemptPermit {
        let route = self.delivering_route();
        self.router
            .acquire_active_steering_attempt(
                &self.registration.proof(),
                route.target(),
                route.loaded_generation(),
            )
            .unwrap()
    }

    pub(super) fn take_lifecycle_owner(&mut self) -> CheckedSteeringLifecycleOwner {
        self.lifecycle_owner
            .take()
            .expect("steering fixture retains its armed lifecycle owner")
    }

    pub(super) fn lifecycle_json(
        &self,
        lifecycle: UserMessageEchoLifecycle,
        correlation: &ClientUserMessageId,
    ) -> String {
        let (method, timestamp_name, timestamp_value) = match lifecycle {
            UserMessageEchoLifecycle::Started => ("item/started", "startedAtMs", 123),
            UserMessageEchoLifecycle::Completed => ("item/completed", "completedAtMs", 124),
        };
        let content = self.image_path.as_ref().map_or_else(
            || {
                format!(
                    r#"[{{"type":"text","text":"{STEERING_TEXT}","text_elements":[]}}]"#
                )
            },
            |path| {
                let path = serde_json::to_string(path.as_ref()).unwrap();
                format!(
                    r#"[{{"type":"text","text":"{IMAGE_STEERING_TEXT}","text_elements":[]}},{{"type":"localImage","detail":null,"path":{path}}}]"#
                )
            },
        );
        format!(
            r#"{{"method":"{method}","params":{{"item":{{"type":"userMessage","id":"{}","clientId":"{}","content":{content}}},"threadId":"{}","turnId":"{}","{timestamp_name}":{timestamp_value}}}}}"#,
            self.cas_item_id.as_str(),
            correlation.as_str(),
            self.cas_thread_id.as_str(),
            self.cas_turn_id.as_str(),
        )
    }

    pub(super) fn decode_lifecycle(
        &mut self,
        lifecycle: UserMessageEchoLifecycle,
        correlation: &ClientUserMessageId,
    ) -> Result<OrderedTurnStreamProgress, ManagedBackendError> {
        let json = self.lifecycle_json(lifecycle, correlation);
        self.decode_json(&json)
    }

    pub(super) fn decode_lifecycle_while_selection_paused(
        &mut self,
        lifecycle: UserMessageEchoLifecycle,
        correlation: &ClientUserMessageId,
        inspect: impl FnOnce(&Self),
    ) -> Result<OrderedTurnStreamProgress, ManagedBackendError> {
        let barrier = install_steering_selection_barrier_for_key(
            self.broker
                .as_ref()
                .expect("steering fixture broker remains open")
                .test_key(),
            lifecycle,
        );
        let json = self.lifecycle_json(lifecycle, correlation);
        let sink = self
            .sink
            .take()
            .expect("steering fixture sink remains open");
        let (sink, result) = std::thread::scope(|scope| {
            let worker = scope.spawn(move || {
                let mut sink = sink;
                let result = decode_provider_json_for_test(json.as_bytes(), 3, sink.as_mut());
                (sink, result)
            });
            assert!(barrier.wait_until_paused(Duration::from_secs(1)));
            inspect(self);
            barrier.release();
            worker.join().unwrap()
        });
        self.sink = Some(sink);
        result
    }

    pub(super) fn decode_json(
        &mut self,
        json: &str,
    ) -> Result<OrderedTurnStreamProgress, ManagedBackendError> {
        decode_provider_json_for_test(
            json.as_bytes(),
            3,
            self.sink
                .as_deref_mut()
                .expect("steering fixture sink remains open"),
        )
    }

    pub(super) fn retry_delivering_route(&self) {
        let gate = self
            .storage
            .input_gate(&self.home, self.thread_id, point_limit())
            .unwrap()
            .unwrap();
        let route = gate.selected_route().unwrap();
        let page = self
            .storage
            .accepted_route_page(
                &self.home,
                self.thread_id,
                route.generation(),
                route.revision(),
                None,
            )
            .unwrap();
        let entry = page
            .records()
            .iter()
            .find(|entry| entry.input().id() == self.accepted_input_id)
            .unwrap();
        let delivering = self
            .storage
            .delivering_steering_input(&self.home, self.accepted_input_id, point_limit())
            .unwrap()
            .expect("fixture accepted input retains its exact steering target");
        execute(
            &self.home,
            self.storage.retry_accepted_input_delivery(
                self.storage.revision(&self.home).unwrap(),
                RetryAcceptedInputDelivery::new(
                    self.thread_id,
                    self.accepted_input_id,
                    entry.leaf().revision(),
                    delivering.target().clone(),
                ),
            ),
        );
        assert!(self
            .storage
            .delivering_steering_input(&self.home, self.accepted_input_id, point_limit())
            .unwrap()
            .is_none());
    }

    pub(super) fn delivering_route(&self) -> SyndicDeliveringSteeringInput {
        self.storage
            .delivering_steering_input(&self.home, self.accepted_input_id, point_limit())
            .unwrap()
            .expect("checked lifecycle must not change delivery disposition")
    }

    pub(super) fn converge_and_assert_projection_loss(&self) {
        self.converge_and_assert_projection_loss_state(
            AcceptedRouteEffectiveState::DeliveryUnknown,
        );
    }

    pub(super) fn converge_and_assert_retryable_projection_loss(&self) {
        self.converge_and_assert_projection_loss_state(AcceptedRouteEffectiveState::NextTurn(
            NextTurnReason::ProjectionLost,
        ));
    }

    fn converge_and_assert_projection_loss_state(
        &self,
        expected_state: AcceptedRouteEffectiveState,
    ) {
        self.broker
            .as_ref()
            .unwrap()
            .converge_target_loss(&self.registration.proof(), TurnIncompleteReason::StreamLost)
            .unwrap();
        assert!(
            self.storage
                .delivering_steering_input(&self.home, self.accepted_input_id, point_limit())
                .unwrap()
                .is_none(),
            "projection loss must remove delivering eligibility"
        );
        let gate = self
            .storage
            .input_gate(&self.home, self.thread_id, point_limit())
            .unwrap()
            .unwrap();
        let route = gate
            .selected_route()
            .expect("projection loss retains the selected generation proof");
        let page = self
            .storage
            .accepted_route_page(
                &self.home,
                self.thread_id,
                route.generation(),
                route.revision(),
                None,
            )
            .unwrap();
        let entry = page
            .records()
            .iter()
            .find(|entry| entry.input().id() == self.accepted_input_id)
            .expect("projection loss retains permanent accepted-input history");
        assert_eq!(entry.effective_state(), expected_state);
        let binding = self
            .storage
            .current_binding(&self.home, self.thread_id, point_limit())
            .unwrap()
            .unwrap();
        assert!(matches!(binding.binding().state(), BindingState::Stale(_)));
    }

    pub(super) fn expected_input_items(&self) -> u64 {
        if self.image_path.is_some() {
            2
        } else {
            1
        }
    }

    pub(super) fn image_path(&self) -> Option<&str> {
        self.image_path.as_deref()
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
            lifecycle_owner,
            authority,
            router,
            registration,
            thread_id: _,
            turn_id: _,
            accepted_input_id: _,
            cas_thread_id: _,
            cas_turn_id: _,
            cas_item_id: _,
            image_path: _,
        } = self;
        drop(lifecycle_owner);
        drop(sink);
        let broker_control = broker
            .as_ref()
            .expect("steering broker control remains owned");
        let stopped = ingester
            .expect("steering ingester remains owned")
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

fn publish_image_asset(home: &HomeStore, state: &BerylState) -> AssetId {
    let sidecar = home
        .admit_sidecar(
            SidecarNamespace::new("images").unwrap(),
            b"\x89PNG\r\n\x1a\nphase52-steering-image",
            SidecarByteLimit::new(NonZeroU64::new(1024 * 1024).unwrap()),
        )
        .unwrap();
    let asset = AssetId::sha256_v1(
        sidecar.address().digest().as_bytes(),
        NonZeroU64::new(sidecar.address().length()).unwrap(),
    );
    let assets = state.assets();
    let revision = assets.revision(home).unwrap();
    let metadata = assets
        .publish_metadata(
            revision,
            sidecar,
            PublishAssetMetadata::new(
                asset,
                AssetMediaType::new("image/png").unwrap(),
                None,
                revision.checked_next().unwrap(),
            ),
        )
        .unwrap();
    let mut command = HomeCommand::new(home.home_revision().unwrap());
    metadata.add_to(&mut command).unwrap();
    match home.execute(command) {
        CommandOutcome::Committed { later_failure: None, .. } => {}
        outcome @ CommandOutcome::Committed { later_failure: Some(_), .. } => panic!("steering-user image metadata command committed with later failure: {outcome:?}"),
        CommandOutcome::NotCommitted { evidence } => panic!("steering-user image metadata command was not committed: {evidence:?}"),
        outcome @ CommandOutcome::Indeterminate { .. } => panic!("steering-user image metadata command was indeterminate: {outcome:?}"),
    }
    asset
}

fn selected_path(
    home: &HomeStore,
    storage: SyndicStorage,
    thread_id: SyndicThreadId,
) -> SelectedPathProof {
    let thread = storage
        .thread(home, thread_id, point_limit())
        .unwrap()
        .unwrap();
    SelectedPathProof::new(
        thread.committed_tail(),
        thread.revision(),
        thread.selected_path_digest(),
    )
}

fn execution_binding(runtime_id: RuntimeId, seed: u8) -> ExecutionBinding {
    ExecutionBinding::new(
        runtime_id,
        RootId::from_bytes([seed.wrapping_add(7); 16]),
        RuntimeNativePath::from_admitted(RuntimeMode::host(), PathFlavor::Windows, EXECUTION_ROOT)
            .unwrap(),
    )
}

fn execute(home: &HomeStore, contribution: beryl_home_store::MutationContribution) {
    let mut command = HomeCommand::new(home.home_revision().unwrap());
    command.add(contribution).unwrap();
    match home.execute(command) {
        CommandOutcome::Committed { later_failure: None, .. } => {}
        outcome @ CommandOutcome::Committed { later_failure: Some(_), .. } => panic!("steering-user fixture contribution command committed with later failure: {outcome:?}"),
        CommandOutcome::NotCommitted { evidence } => panic!("steering-user fixture contribution command was not committed: {evidence:?}"),
        outcome @ CommandOutcome::Indeterminate { .. } => panic!("steering-user fixture contribution command was indeterminate: {outcome:?}"),
    }
}

pub(super) fn point_limit() -> SyndicPointReadLimit {
    SyndicPointReadLimit::new(POINT_READ_BYTES).unwrap()
}

fn timestamp(value: u64) -> SyndicTimestamp {
    SyndicTimestamp::from_unix_millis(value)
}
