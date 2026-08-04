use std::{
    path::Path,
    sync::Arc,
    sync::atomic::{AtomicUsize, Ordering},
    thread,
    time::{Duration, Instant},
};

use beryl_app::cas_projection::{
    ProjectionConnectionService, ProjectionServiceConfig, ScheduledOrdinaryAdmission,
    ScheduledOrdinaryAdmissionError, ScheduledOrdinaryAdmissionResult,
    ScheduledOrdinaryExecutionProvider, ScheduledOrdinaryExecutionUnavailable,
};
use beryl_app::input_admission::prepare_accepted_input_admission;
use beryl_backend::ManagedBackendClientConnector;
use beryl_model::{CasProcessGeneration, RuntimeId, SyndicDraftId, SyndicThreadId};
use beryl_state::BerylState;
use syndic_storage::{AcceptedInputAdmission, BindingState, SyndicStorage};

use crate::{
    app_support::{
        close_seeded, point_limit, promote_installed_next, restart_service,
        restart_service_with_config, seeded_home, time,
    },
    phase62_support::{
        AUTHORIZATION, CheckoutProvider, NormalTerminalServer, SessionSlot, TIMEOUT,
        UnavailableProvider, execution_binding, install_next_records, ready_provider, wait_until,
    },
    records::{activate_promoted_turn, cancel_activation},
};

struct RecoveredThenNextProvider {
    recovered_thread: SyndicThreadId,
    checkout: CheckoutProvider,
    recovered_attempts: Arc<AtomicUsize>,
    next_attempts: Arc<AtomicUsize>,
}

impl ScheduledOrdinaryExecutionProvider for RecoveredThenNextProvider {
    fn try_issue(
        &mut self,
        admission: ScheduledOrdinaryAdmission,
    ) -> Result<ScheduledOrdinaryAdmissionResult, ScheduledOrdinaryAdmissionError> {
        if admission.thread_id() == self.recovered_thread {
            self.recovered_attempts.fetch_add(1, Ordering::SeqCst);
            self.checkout.try_issue(admission)
        } else {
            self.next_attempts.fetch_add(1, Ordering::SeqCst);
            Ok(admission.decline(ScheduledOrdinaryExecutionUnavailable::RuntimeNotReady))
        }
    }

    fn shutdown(&mut self) {
        self.checkout.shutdown();
    }
}

struct SameThreadSequencedProvider {
    checkout: CheckoutProvider,
    attempts: Arc<AtomicUsize>,
    issued_once: bool,
}

impl ScheduledOrdinaryExecutionProvider for SameThreadSequencedProvider {
    fn try_issue(
        &mut self,
        admission: ScheduledOrdinaryAdmission,
    ) -> Result<ScheduledOrdinaryAdmissionResult, ScheduledOrdinaryAdmissionError> {
        self.attempts.fetch_add(1, Ordering::SeqCst);
        if self.issued_once {
            return Ok(admission.decline(ScheduledOrdinaryExecutionUnavailable::RuntimeNotReady));
        }
        let result = self.checkout.try_issue(admission)?;
        if matches!(&result, ScheduledOrdinaryAdmissionResult::Issued(_)) {
            self.issued_once = true;
        }
        Ok(result)
    }

    fn shutdown(&mut self) {
        self.checkout.shutdown();
    }
}

#[test]
fn recovered_nondispatch_hands_capacity_to_waiting_next_work_without_self_retry() {
    let home = seeded_home();
    let recovered = install_next_records(
        &home.store,
        home.storage,
        188,
        execution_binding(RuntimeId::from_bytes([248; 16])),
    );
    let promoted = promote_installed_next(&home.store, home.storage, &home.state, recovered, 218);
    let active = activate_promoted_turn(
        &home.store,
        home.storage,
        recovered.thread,
        promoted.turn,
        248,
        false,
    );
    cancel_activation(
        &home.store,
        home.storage,
        recovered.thread,
        promoted.turn,
        active.snapshot,
    );
    let current = home
        .storage
        .current_binding(&home.store, recovered.thread, point_limit())
        .unwrap()
        .unwrap();
    let BindingState::Valid(usable) = current.binding().state() else {
        panic!("recovered pending fixture must retain valid projection authority");
    };
    let runtime_id = usable.execution().runtime_id();
    let next = install_next_records(
        &home.store,
        home.storage,
        192,
        execution_binding(runtime_id),
    );
    let directory = close_seeded(home);

    let slot = SessionSlot::default();
    let provider_slot = slot.clone();
    let recovered_attempts = Arc::new(AtomicUsize::new(0));
    let provider_recovered_attempts = Arc::clone(&recovered_attempts);
    let next_attempts = Arc::new(AtomicUsize::new(0));
    let provider_next_attempts = Arc::clone(&next_attempts);
    let (service, _, _) = restart_service_with_config(
        directory.path(),
        ProjectionServiceConfig::try_new(128, 4).unwrap(),
        |state| {
            Box::new(RecoveredThenNextProvider {
                recovered_thread: recovered.thread,
                checkout: ready_provider(provider_slot, state.assets()),
                recovered_attempts: provider_recovered_attempts,
                next_attempts: provider_next_attempts,
            })
        },
    );
    wait_until("initial unavailable recovered and next attempts", || {
        (recovered_attempts.load(Ordering::SeqCst) == 1
            && next_attempts.load(Ordering::SeqCst) == 1)
            .then_some(())
    });
    let initial_capacity_waits = service
        .accepted_input_scheduler_diagnostics()
        .next_capacity_waits();

    let server = NormalTerminalServer::spawn_resume_delayed_rejection(active.cas_thread.as_str());
    let connector =
        ManagedBackendClientConnector::for_lifecycle_test(server.endpoint(), AUTHORIZATION);
    let session = service
        .admit(
            &connector,
            runtime_id,
            CasProcessGeneration::new(63_188).unwrap(),
            Path::new(crate::EXECUTION_ROOT),
            TIMEOUT,
        )
        .unwrap();
    slot.replace(session);
    service.notify_scheduled_ordinary_execution_ready();
    server.wait_for_projection();
    server.wait_for_turn_start();
    wait_until("accepted-next waits behind recovered execution", || {
        (service
            .accepted_input_scheduler_diagnostics()
            .next_capacity_waits()
            > initial_capacity_waits)
            .then_some(())
    });
    let waiting_next_passes = service
        .accepted_input_scheduler_diagnostics()
        .next_pass_count();
    let waiting_next_attempts = next_attempts.load(Ordering::SeqCst);

    server.release_turn_start_rejection();
    let handoff = wait_until(
        "recovered completion hands the permit to accepted-next",
        || {
            let diagnostics = service.accepted_input_scheduler_diagnostics();
            let observed_next_attempts = next_attempts.load(Ordering::SeqCst);
            if observed_next_attempts > waiting_next_attempts
                && observed_next_attempts <= waiting_next_attempts + 2
                && diagnostics.workers_active() == 0
            {
                Some(Ok((observed_next_attempts, diagnostics)))
            } else if diagnostics.fatal() || observed_next_attempts > waiting_next_attempts + 2 {
                Some(Err((
                    recovered_attempts.load(Ordering::SeqCst),
                    observed_next_attempts,
                    diagnostics,
                )))
            } else {
                None
            }
        },
    );
    assert!(handoff.is_ok(), "cross-lane handoff failed: {handoff:?}");
    handoff.unwrap();
    let settled_next_attempts = settle_bounded_attempts(
        &next_attempts,
        waiting_next_attempts + 2,
        "accepted-next causal wake settlement",
    );
    thread::sleep(Duration::from_millis(100));

    assert_eq!(recovered_attempts.load(Ordering::SeqCst), 2);
    assert_eq!(
        next_attempts.load(Ordering::SeqCst),
        settled_next_attempts,
        "accepted-next must not self-retry after the capacity and durable-loss wakes settle"
    );
    assert!(
        service
            .accepted_input_scheduler_diagnostics()
            .next_pass_count()
            > waiting_next_passes
    );
    service.close().unwrap();
    server.join();
}

fn settle_bounded_attempts(attempts: &AtomicUsize, maximum: usize, label: &str) -> usize {
    let deadline = Instant::now() + TIMEOUT;
    let mut observed = attempts.load(Ordering::SeqCst);
    let mut unchanged_since = Instant::now();
    loop {
        let current = attempts.load(Ordering::SeqCst);
        assert!(
            current <= maximum,
            "{label} exceeded its causally bounded attempts: {current} > {maximum}"
        );
        if current != observed {
            observed = current;
            unchanged_since = Instant::now();
        }
        if unchanged_since.elapsed() >= Duration::from_millis(100) {
            return observed;
        }
        assert!(Instant::now() < deadline, "timed out waiting for {label}");
        thread::yield_now();
    }
}

#[test]
fn recovered_completion_reopens_same_thread_next_work_waiting_on_its_worker() {
    let home = seeded_home();
    let recovered = install_next_records(
        &home.store,
        home.storage,
        196,
        execution_binding(RuntimeId::from_bytes([250; 16])),
    );
    let promoted = promote_installed_next(&home.store, home.storage, &home.state, recovered, 226);
    let active = activate_promoted_turn(
        &home.store,
        home.storage,
        recovered.thread,
        promoted.turn,
        250,
        false,
    );
    cancel_activation(
        &home.store,
        home.storage,
        recovered.thread,
        promoted.turn,
        active.snapshot,
    );
    let current = home
        .storage
        .current_binding(&home.store, recovered.thread, point_limit())
        .unwrap()
        .unwrap();
    let BindingState::Valid(usable) = current.binding().state() else {
        panic!("same-thread handoff fixture must retain valid projection authority");
    };
    let runtime_id = usable.execution().runtime_id();
    let directory = close_seeded(home);
    let (seeding_service, seeding_storage, seeding_state) =
        restart_service(directory.path(), Box::new(UnavailableProvider));
    admit_same_thread_next(
        &seeding_service,
        seeding_storage,
        &seeding_state,
        recovered.thread,
        230,
    );
    seeding_service.close().unwrap();

    let slot = SessionSlot::default();
    let provider_slot = slot.clone();
    let attempts = Arc::new(AtomicUsize::new(0));
    let provider_attempts = Arc::clone(&attempts);
    let (service, _, _) = restart_service_with_config(
        directory.path(),
        ProjectionServiceConfig::try_new(128, 8).unwrap(),
        |state| {
            Box::new(SameThreadSequencedProvider {
                checkout: ready_provider(provider_slot, state.assets()),
                attempts: provider_attempts,
                issued_once: false,
            })
        },
    );
    wait_until("initial same-thread recovered provider attempt", || {
        (attempts.load(Ordering::SeqCst) == 1).then_some(())
    });
    let initial = service.accepted_input_scheduler_diagnostics();

    let server = NormalTerminalServer::spawn_resume_delayed_rejection(active.cas_thread.as_str());
    let connector =
        ManagedBackendClientConnector::for_lifecycle_test(server.endpoint(), AUTHORIZATION);
    let session = service
        .admit(
            &connector,
            runtime_id,
            CasProcessGeneration::new(63_196).unwrap(),
            Path::new(crate::EXECUTION_ROOT),
            TIMEOUT,
        )
        .unwrap();
    slot.replace(session);
    service.notify_scheduled_ordinary_execution_ready();
    server.wait_for_projection();
    server.wait_for_turn_start();
    wait_until("same-thread next scan waits on recovered worker", || {
        let diagnostics = service.accepted_input_scheduler_diagnostics();
        (diagnostics.next_retained_source_cursor()
            && diagnostics.next_capacity_waits() == initial.next_capacity_waits())
        .then_some(())
    });
    let waiting_next_passes = service
        .accepted_input_scheduler_diagnostics()
        .next_pass_count();

    server.release_turn_start_rejection();
    wait_until(
        "recovered worker completion reopens retained same-thread next",
        || {
            let diagnostics = service.accepted_input_scheduler_diagnostics();
            (attempts.load(Ordering::SeqCst) == 3
                && diagnostics.next_pass_count() > waiting_next_passes
                && diagnostics.next_execution_unavailable()
                    == initial.next_execution_unavailable() + 1
                && !diagnostics.next_retained_source_cursor()
                && diagnostics.workers_active() == 0)
                .then_some(())
        },
    );
    thread::sleep(Duration::from_millis(100));

    assert_eq!(attempts.load(Ordering::SeqCst), 3);
    service.close().unwrap();
    server.join();
}

fn admit_same_thread_next(
    store: &ProjectionConnectionService,
    storage: SyndicStorage,
    state: &BerylState,
    thread_id: SyndicThreadId,
    seed: u8,
) {
    let current = storage
        .current_draft(store, thread_id, point_limit())
        .unwrap()
        .unwrap();
    let gate = storage
        .input_gate(store, thread_id, point_limit())
        .unwrap()
        .unwrap();
    let admission = AcceptedInputAdmission::new(
        thread_id,
        current.thread().revision(),
        current.draft().id(),
        current.draft().revision(),
        current.draft().content(),
        gate.revision(),
        SyndicDraftId::from_bytes([seed; 16]),
        None,
        time(63_196),
    );
    let prepared =
        prepare_accepted_input_admission(store, storage, state.assets(), admission).unwrap();
    store.execute_accepted_input_admission(prepared).unwrap();
}
