use std::sync::{Arc, Condvar, Mutex};

use beryl_app::cas_projection::{
    ScheduledOrdinaryAdmission, ScheduledOrdinaryAdmissionError, ScheduledOrdinaryAdmissionResult,
    ScheduledOrdinaryExecutionProvider, ScheduledOrdinaryExecutionUnavailable,
};
use beryl_home_store::{HomeCommand, HomeStore};
use beryl_model::{RuntimeId, SyndicThreadId};
use syndic_storage::{
    SyndicStorage,
    test_faults::{FixtureBatch, FixtureRecord},
};

use crate::{
    app_support::{
        close_seeded, point_limit, promote_installed_next, restart_service, seeded_home,
    },
    phase62_support::{NextRecordIds, execution_binding, install_next_records, wait_until},
};

#[derive(Default)]
struct AttemptState {
    threads: Vec<SyndicThreadId>,
    release_first: bool,
}

#[derive(Clone, Default)]
struct AttemptControl {
    shared: Arc<(Mutex<AttemptState>, Condvar)>,
}

impl AttemptControl {
    fn record_and_wait_first(&self, thread_id: SyndicThreadId) {
        let (state, changed) = &*self.shared;
        let mut state = state.lock().unwrap();
        state.threads.push(thread_id);
        changed.notify_all();
        while state.threads.len() == 1 && !state.release_first {
            state = changed.wait(state).unwrap();
        }
    }

    fn attempts(&self) -> Vec<SyndicThreadId> {
        self.shared.0.lock().unwrap().threads.clone()
    }

    fn release_first(&self) {
        let (state, changed) = &*self.shared;
        state.lock().unwrap().release_first = true;
        changed.notify_all();
    }
}

struct BlockingUnavailableProvider {
    attempts: AttemptControl,
}

impl ScheduledOrdinaryExecutionProvider for BlockingUnavailableProvider {
    fn try_issue(
        &mut self,
        admission: ScheduledOrdinaryAdmission,
    ) -> Result<ScheduledOrdinaryAdmissionResult, ScheduledOrdinaryAdmissionError> {
        self.attempts.record_and_wait_first(admission.thread_id());
        Ok(admission.decline(ScheduledOrdinaryExecutionUnavailable::RuntimeNotReady))
    }

    fn shutdown(&mut self) {}
}

fn recovered_pair(
    first_seed: u8,
    second_seed: u8,
) -> (tempfile::TempDir, NextRecordIds, NextRecordIds) {
    let home = seeded_home();
    let first = install_next_records(
        &home.store,
        home.storage,
        first_seed,
        execution_binding(RuntimeId::from_bytes([first_seed; 16])),
    );
    promote_installed_next(
        &home.store,
        home.storage,
        &home.state,
        first,
        first_seed.wrapping_add(30),
    );
    let second = install_next_records(
        &home.store,
        home.storage,
        second_seed,
        execution_binding(RuntimeId::from_bytes([second_seed; 16])),
    );
    promote_installed_next(
        &home.store,
        home.storage,
        &home.state,
        second,
        second_seed.wrapping_add(27),
    );
    (close_seeded(home), first, second)
}

fn rewrite_current_gate(store: &HomeStore, storage: SyndicStorage, thread_id: SyndicThreadId) {
    let gate = storage
        .input_gate(store, thread_id, point_limit())
        .unwrap()
        .unwrap();
    let prior_revision = storage.revision(store).unwrap();
    let mut batch = FixtureBatch::new();
    batch.put(FixtureRecord::InputGate(gate)).unwrap();
    let mut command = HomeCommand::new(store.home_revision().unwrap());
    command
        .add(storage.fixture_contribution(prior_revision, batch))
        .unwrap();
    store.execute(command).unwrap();
    assert_ne!(storage.revision(store).unwrap(), prior_revision);
}

#[test]
fn work_neutral_revision_drift_preserves_the_pass_floor_without_self_retry() {
    let (directory, first, second) = recovered_pair(186, 190);
    let attempts = AttemptControl::default();
    let (service, storage, _) = restart_service(
        directory.path(),
        Box::new(BlockingUnavailableProvider {
            attempts: attempts.clone(),
        }),
    );
    wait_until("first recovered provider attempt", || {
        (attempts.attempts() == vec![first.thread]).then_some(())
    });

    {
        let command_home = service.live_home_command().unwrap();
        rewrite_current_gate(command_home.home(), storage, first.thread);
    }
    attempts.release_first();
    wait_until(
        "work-neutral drift advances from the physical floor",
        || {
            let diagnostics = service.accepted_input_scheduler_diagnostics();
            (attempts.attempts().len() >= 2
                && diagnostics.recovered_pending_stale_scans() >= 1
                && !diagnostics.recovered_pending_retained_source_cursor())
            .then_some(())
        },
    );

    assert_eq!(attempts.attempts(), vec![first.thread, second.thread]);
    service.close().unwrap();
}

#[test]
fn execution_ready_drift_restarts_the_complete_recovered_scan() {
    let (directory, first, second) = recovered_pair(187, 191);
    let attempts = AttemptControl::default();
    let (service, storage, _) = restart_service(
        directory.path(),
        Box::new(BlockingUnavailableProvider {
            attempts: attempts.clone(),
        }),
    );
    wait_until("first recovered provider attempt", || {
        (attempts.attempts() == vec![first.thread]).then_some(())
    });

    {
        let command_home = service.live_home_command().unwrap();
        rewrite_current_gate(command_home.home(), storage, first.thread);
    }
    service.notify_scheduled_ordinary_execution_ready();
    attempts.release_first();
    wait_until(
        "execution readiness reopens discovery from the beginning",
        || {
            let diagnostics = service.accepted_input_scheduler_diagnostics();
            (attempts.attempts().len() >= 3
                && diagnostics.recovered_pending_execution_unavailable() >= 3
                && !diagnostics.recovered_pending_retained_source_cursor())
            .then_some(())
        },
    );

    assert_eq!(
        attempts.attempts(),
        vec![first.thread, first.thread, second.thread]
    );
    service.close().unwrap();
}
