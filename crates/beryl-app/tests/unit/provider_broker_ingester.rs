use std::{
    num::NonZeroUsize,
    thread,
    time::{Duration, Instant},
};

use beryl_backend::ProviderObservationAbandonReason;
use beryl_home_store::{HomeOpenOptions, HomeSchemaVersion, HomeStore};
use beryl_model::{CasProcessGeneration, RuntimeId};
use beryl_stream::{SendError, fixed_channel};
use syndic_storage::SyndicStorage;

use super::*;
use crate::cas_projection::{
    accepted_input_scheduler::AcceptedInputSchedulerSignal,
    connection::{ConnectionRegistryAuthority, EventRouter},
    context_compaction::ContextCompactionCoordinator,
    persistent_failure::MasterCommandGate,
    service_config::ProjectionWorkerPool,
    service_registry::ProjectionServiceConnectionRegistry,
    initial_start::InitialStartGate,
    stop::StopCoordinator,
};

struct BrokerBuildFixture {
    authority: Arc<ConnectionRegistryAuthority>,
    router: Arc<EventRouter>,
    stop: Arc<StopCoordinator>,
    compaction: Arc<ContextCompactionCoordinator>,
    commands: LiveCommandAuthorizer,
    failure_notification: PersistentFailureNotification,
    workers: ProjectionWorkerPool,
    home: Arc<HomeStore>,
    home_id: BerylHomeId,
    home_generation: HomeGeneration,
    _directory: tempfile::TempDir,
}

impl BrokerBuildFixture {
    fn new(seed: u8) -> Self {
        let directory = tempfile::tempdir().unwrap();
        let mut home = HomeStore::open(HomeOpenOptions::new(
            directory.path(),
            HomeSchemaVersion::CURRENT,
        ))
        .unwrap();
        let storage = SyndicStorage::register(&mut home).unwrap();
        let home_id = home.home_id();
        let home_generation = home.health().generation().unwrap();
        let home = Arc::new(home);
        let failure_notification =
            crate::cas_projection::persistent_failure::test_failure_notification(
                &home,
                home_id,
                home_generation,
            );
        let commands = MasterCommandGate::new(
            failure_notification.service_generation(),
            Some(failure_notification.clone()),
        )
        .authorizer();
        let scheduler = AcceptedInputSchedulerSignal::new();
        let registry = ProjectionServiceConnectionRegistry::new(commands.service_generation());
        let stop = Arc::new(StopCoordinator::new_for_test(
            &home,
            home_id,
            home_generation,
            storage,
        ));
        let compaction = ContextCompactionCoordinator::new(
            Arc::clone(&home),
            home_id,
            home_generation,
            storage,
            registry,
            Arc::clone(&stop),
            commands.clone(),
            scheduler.clone(),
        )
        .unwrap();
        let runtime_id = RuntimeId::from_bytes([seed; 16]);
        let process_generation = CasProcessGeneration::new(82_000 + u64::from(seed)).unwrap();
        let authority =
            Arc::new(ConnectionRegistryAuthority::new(runtime_id, process_generation).unwrap());
        let router = Arc::new(
            EventRouter::new_with_scheduler(
                runtime_id,
                process_generation,
                authority.generation_for_test().get(),
                scheduler,
                commands.clone(),
                None,
            )
            .unwrap(),
        );
        Self {
            authority,
            router,
            stop,
            compaction,
            commands,
            failure_notification,
            workers: ProjectionWorkerPool::new(NonZeroUsize::new(3).unwrap()),
            home,
            home_id,
            home_generation,
            _directory: directory,
        }
    }

    fn build_error(&self, fault: ProviderBrokerBuildFault) -> ProviderBrokerBuildError {
        let mut workers = self.workers.try_acquire_pair().unwrap();
        let ingester_worker = workers.take_ingester();
        drop(workers.take_driver());
        match ProviderBroker::prepare_with_initial_start_inner(
            Arc::clone(&self.home),
            self.home_id,
            self.home_generation,
            Arc::clone(&self.authority),
            Arc::clone(&self.router),
            Arc::clone(&self.stop),
            Arc::clone(&self.compaction),
            self.commands.clone(),
            self.failure_notification.clone(),
            ingester_worker,
            InitialStartGate::ready(),
            fault,
        ) {
            Err(error) => error,
            Ok(_) => panic!("injected provider broker construction failure did not fire"),
        }
    }

    fn prepare_start_blocked(&self) -> PreparedProviderBroker {
        let mut workers = self.workers.try_acquire_pair().unwrap();
        let ingester_worker = workers.take_ingester();
        drop(workers.take_driver());
        ProviderBroker::prepare_with_initial_start(
            Arc::clone(&self.home),
            self.home_id,
            self.home_generation,
            Arc::clone(&self.authority),
            Arc::clone(&self.router),
            Arc::clone(&self.stop),
            Arc::clone(&self.compaction),
            self.commands.clone(),
            self.failure_notification.clone(),
            ingester_worker,
            InitialStartGate::ready(),
        )
        .unwrap()
    }
}

impl Drop for BrokerBuildFixture {
    fn drop(&mut self) {
        self.compaction.shutdown().unwrap();
    }
}

fn worker_disposition_owner(fixture: &BrokerBuildFixture) -> Arc<ProviderBrokerWorkerOwner> {
    let mut workers = fixture.workers.try_acquire_pair().unwrap();
    let ingester = workers.take_ingester();
    drop(workers.take_driver());
    Arc::new(ProviderBrokerWorkerOwner::new(ingester))
}

#[test]
fn idle_timeout_then_cancellation_cannot_strand_a_later_enqueue() {
    let (sender, receiver) = fixed_channel(NonZeroUsize::MIN).unwrap();
    let receiver_observer = receiver.observer();
    let cancelled = Arc::new(AtomicBool::new(false));
    let ack = AckSlot::new();

    assert!(!cancelled.load(Ordering::Acquire));
    assert!(ack.prepare());
    let receiver_cancelled = Arc::clone(&cancelled);
    let receiver_thread = thread::spawn(move || receive_next(&receiver, &receiver_cancelled));
    let deadline = Instant::now() + Duration::from_secs(1);
    while receiver_observer
        .diagnostics()
        .expect("receiver remains alive before cancellation")
        .receive_timeouts
        == 0
    {
        assert!(
            Instant::now() < deadline,
            "receiver did not observe an idle timeout"
        );
        thread::yield_now();
    }

    cancelled.store(true, Ordering::Release);
    assert!(receiver_thread.join().unwrap().is_none());

    let operation = BrokerOperation::new(OrderedTurnStreamOperation::ProviderAbandon(
        ProviderObservationAbandonReason::Cancelled,
    ));
    match sender.send_timeout(operation, Duration::from_secs(1)) {
        Err(SendError::Closed(operation)) => assert!(matches!(
            operation.into_operation(),
            OrderedTurnStreamOperation::ProviderAbandon(
                ProviderObservationAbandonReason::Cancelled
            )
        )),
        Ok(()) => panic!("late enqueue reached a cancelled receiver"),
        Err(SendError::Full(_)) => panic!("empty cancelled receiver reported full"),
        Err(SendError::Timeout(_)) => panic!("cancelled receiver did not close the channel"),
    }
}

#[test]
fn whole_connection_failure_is_sticky_across_every_command_observation() {
    let failure = StickyRoutingFailure::default();
    failure.record(WholeConnectionRoutingFailure::Router);

    assert!(matches!(
        failure.get(),
        Some(WholeConnectionRoutingFailure::Router)
    ));
    assert!(matches!(
        failure.get(),
        Some(WholeConnectionRoutingFailure::Router)
    ));
    failure.record(WholeConnectionRoutingFailure::Backend);
    assert!(matches!(
        failure.get(),
        Some(WholeConnectionRoutingFailure::Router)
    ));
}

#[test]
fn phase82_page_pool_failure_retains_the_acquired_worker() {
    let fixture = BrokerBuildFixture::new(191);
    let error = fixture.build_error(ProviderBrokerBuildFault::PagePool);

    assert_eq!(
        error.resource_snapshot(),
        ProviderBrokerBuildResourceSnapshot {
            worker: true,
            pages: false,
            channel: false,
            sink: false,
            control: false,
            ingester: false,
            start_gate: false,
            initial_start: false,
        }
    );
    assert_eq!(fixture.workers.diagnostics().active(), 1);
    drop(error);
    assert_eq!(fixture.workers.diagnostics().active(), 0);
}

#[test]
fn phase82_channel_failure_retains_the_worker_and_fixed_page_pool() {
    let fixture = BrokerBuildFixture::new(192);
    let error = fixture.build_error(ProviderBrokerBuildFault::Channel);

    assert_eq!(
        error.resource_snapshot(),
        ProviderBrokerBuildResourceSnapshot {
            worker: true,
            pages: true,
            channel: false,
            sink: false,
            control: false,
            ingester: false,
            start_gate: false,
            initial_start: false,
        }
    );
    assert_eq!(fixture.workers.diagnostics().active(), 1);
    drop(error);
    assert_eq!(fixture.workers.diagnostics().active(), 0);
}

#[test]
fn phase82_spawn_failure_retains_the_complete_unstarted_broker() {
    let fixture = BrokerBuildFixture::new(193);
    let error = fixture.build_error(ProviderBrokerBuildFault::Spawn);

    assert_eq!(
        error.resource_snapshot(),
        ProviderBrokerBuildResourceSnapshot {
            worker: true,
            pages: true,
            channel: true,
            sink: true,
            control: true,
            ingester: true,
            start_gate: true,
            initial_start: true,
        }
    );
    assert_eq!(fixture.workers.diagnostics().active(), 1);
    drop(error);
    assert_eq!(fixture.workers.diagnostics().active(), 0);
}

#[test]
fn start_blocked_join_preserves_its_unarmed_worker_admission() {
    let fixture = BrokerBuildFixture::new(200);
    let PreparedProviderBroker {
        sink,
        control,
        ingester,
        start,
    } = fixture.prepare_start_blocked();
    assert_eq!(fixture.workers.diagnostics().active(), 1);

    let stopped = ingester.cancel_and_join(start);
    assert!(stopped.receipt().is_exact(
        fixture.commands.service_generation(),
        fixture.home_generation,
    ));
    let worker = stopped
        .into_worker()
        .expect("an unarmed start-blocked join preserves its worker admission");
    assert_eq!(fixture.workers.diagnostics().active(), 1);

    drop(worker);
    drop(sink);
    drop(control);
    assert_eq!(fixture.workers.diagnostics().active(), 0);
}

#[test]
fn provider_worker_ordinary_disposition_before_terminal_releases_at_terminal() {
    let fixture = BrokerBuildFixture::new(194);
    let worker = worker_disposition_owner(&fixture);
    assert_eq!(fixture.workers.diagnostics().active(), 1);

    worker.arm_ordinary_release().unwrap();
    assert_eq!(fixture.workers.diagnostics().active(), 1);

    worker.mark_terminal();
    assert_eq!(fixture.workers.diagnostics().active(), 0);
    assert!(!worker.retains_worker());
    assert!(worker.take_joined_worker().worker.is_none());
}

#[test]
fn provider_worker_terminal_before_ordinary_disposition_releases_without_join() {
    let fixture = BrokerBuildFixture::new(195);
    let worker = worker_disposition_owner(&fixture);
    worker.mark_terminal();
    assert_eq!(fixture.workers.diagnostics().active(), 1);

    worker.arm_ordinary_release().unwrap();
    assert_eq!(fixture.workers.diagnostics().active(), 0);
    assert!(!worker.retains_worker());
}

#[test]
fn provider_worker_terminal_guard_marks_terminal_during_unwind() {
    let fixture = BrokerBuildFixture::new(199);
    let worker = worker_disposition_owner(&fixture);
    worker.arm_ordinary_release().unwrap();
    let thread_worker = Arc::clone(&worker);

    let unwind = catch_unwind(AssertUnwindSafe(move || {
        let _terminal = ProviderBrokerWorkerTerminalGuard::new(thread_worker);
        panic!("injected provider worker unwind");
    }));

    assert!(unwind.is_err());
    assert_eq!(fixture.workers.diagnostics().active(), 0);
    assert!(!worker.retains_worker());
}

#[test]
fn provider_worker_poison_retains_without_claiming_a_disposition() {
    let fixture = BrokerBuildFixture::new(201);
    let worker = worker_disposition_owner(&fixture);
    let poison_worker = Arc::clone(&worker);
    let unwind = catch_unwind(AssertUnwindSafe(move || {
        let _state = poison_worker.state.lock().unwrap();
        panic!("injected provider worker disposition poison");
    }));
    assert!(unwind.is_err());

    assert_eq!(
        worker.arm_ordinary_release(),
        Err(ProviderBrokerWorkerDispositionArmError::Poisoned)
    );
    worker.mark_terminal();
    assert_eq!(fixture.workers.diagnostics().active(), 1);
    let joined = worker.take_joined_worker();
    assert_eq!(joined.disposition, None);
    drop(
        joined
            .worker
            .expect("poison must preserve the exact worker for consuming disposition"),
    );
    assert_eq!(fixture.workers.diagnostics().active(), 0);
}
