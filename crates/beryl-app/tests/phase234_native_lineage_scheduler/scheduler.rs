#![allow(dead_code)]

use std::{thread, time::Instant};

use beryl_home_store::HomeStore;
use beryl_model::{
    ExecutionBinding, PathFlavor, RootId, RuntimeId, RuntimeMode, RuntimeNativePath,
    SyndicAcceptedInputId, SyndicThreadId, SyndicTurnId,
};
use syndic_storage::{
    AcceptedRouteEffectiveState, AcceptedRouteRevision, BindingState, SyndicPointReadLimit,
    SyndicReadError, SyndicStorage,
};

#[path = "../phase37_normal_terminal/server.rs"]
mod server;

pub use server::{SUBMITTED_TEXT, TIMEOUT};

#[path = "support/execution.rs"]
mod execution;
pub use execution::{SessionPool, SessionSlot, pooled_ready_provider, ready_provider};

pub const EXECUTION_ROOT: &str = r"C:\work\beryl";

#[derive(Clone, Copy)]
pub struct NextRecordIds {
    pub thread: SyndicThreadId,
    pub accepted_input: SyndicAcceptedInputId,
    pub parent: SyndicTurnId,
}

pub fn execution_binding(runtime_id: RuntimeId) -> ExecutionBinding {
    ExecutionBinding::new(
        runtime_id,
        RootId::from_bytes([162; 16]),
        RuntimeNativePath::from_admitted(RuntimeMode::host(), PathFlavor::Windows, EXECUTION_ROOT)
            .unwrap(),
    )
}

pub fn admit_runtime_next_input(fixture: &mut crate::syndic::Fixture, seed: u8) -> NextRecordIds {
    let ids = seed_runtime_next_input_without_wake_after_direct_setup(fixture, seed, || {});
    fixture.store.notify_scheduled_ordinary_execution_ready();
    ids
}

pub fn admit_runtime_next_input_after_direct_setup(
    fixture: &mut crate::syndic::Fixture,
    seed: u8,
    after_direct_setup: impl FnOnce(),
) -> NextRecordIds {
    let ids =
        seed_runtime_next_input_without_wake_after_direct_setup(fixture, seed, after_direct_setup);
    fixture.store.notify_scheduled_ordinary_execution_ready();
    ids
}

pub fn seed_runtime_next_input_without_wake(
    fixture: &mut crate::syndic::Fixture,
    seed: u8,
) -> NextRecordIds {
    seed_runtime_next_input_without_wake_after_direct_setup(fixture, seed, || {})
}

fn seed_runtime_next_input_without_wake_after_direct_setup(
    fixture: &mut crate::syndic::Fixture,
    seed: u8,
    after_direct_setup: impl FnOnce(),
) -> NextRecordIds {
    let thread = fixture.thread;
    seed_runtime_next_input_on_without_wake_after_direct_setup(
        fixture,
        thread,
        seed,
        after_direct_setup,
    )
}

pub fn seed_runtime_next_input_on_without_wake_after_direct_setup(
    fixture: &mut crate::syndic::Fixture,
    thread: SyndicThreadId,
    seed: u8,
    after_direct_setup: impl FnOnce(),
) -> NextRecordIds {
    let predecessor = if thread == fixture.thread {
        "phase62 non-steerable predecessor".to_owned()
    } else {
        format!("phase234 non-steerable predecessor {seed}")
    };
    let active = fixture.submit_text_on(thread, &predecessor);
    let source = fixture.activate_without_terminal_on(thread, active);
    fixture.mark_active_unknown_terminal_on(thread, active, &source);
    after_direct_setup();
    let ids = admit_runtime_awaiting_terminal_input_on(fixture, thread, seed);
    fixture.advance_clock_to(62_102);
    fixture.complete_active_without_assistant_on(thread, active, &source);
    {
        let command_home = fixture.store.live_home_command().unwrap();
        command_home
            .home()
            .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
            .unwrap();
    }
    ids
}

pub fn admit_runtime_awaiting_terminal_input(
    fixture: &mut crate::syndic::Fixture,
    seed: u8,
) -> NextRecordIds {
    let thread = fixture.thread;
    admit_runtime_awaiting_terminal_input_on(fixture, thread, seed)
}

pub fn admit_runtime_awaiting_terminal_input_on(
    fixture: &mut crate::syndic::Fixture,
    thread: SyndicThreadId,
    seed: u8,
) -> NextRecordIds {
    let (thread, parent) = {
        let command_home = fixture.store.live_home_command().unwrap();
        let home = command_home.home();
        let parent = fixture
            .storage
            .thread(home, thread, point_limit())
            .unwrap()
            .and_then(|thread| thread.committed_tail())
            .expect("runtime next-turn fixture has completed parent history");
        (thread, parent)
    };
    let submitted_text = if thread == fixture.thread {
        SUBMITTED_TEXT.to_owned()
    } else {
        format!("{SUBMITTED_TEXT} {seed}")
    };
    let accepted_input = fixture.accept_text_on(thread, &submitted_text);
    fixture
        .store
        .live_home_command()
        .unwrap()
        .home()
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap();

    NextRecordIds {
        thread,
        accepted_input,
        parent,
    }
}

pub fn current_cas_thread_id(
    store: &HomeStore,
    storage: SyndicStorage,
    thread: SyndicThreadId,
) -> String {
    let binding = storage
        .current_binding(store, thread, point_limit())
        .unwrap()
        .expect("completed fixture history has a current binding");
    let BindingState::Valid(usable) = binding.binding().state() else {
        panic!("completed fixture history retains a valid CAS binding")
    };
    usable.cas_thread_id().as_str().to_owned()
}

pub fn accepted_route_state(
    store: &HomeStore,
    storage: SyndicStorage,
    ids: &NextRecordIds,
) -> AcceptedRouteEffectiveState {
    try_accepted_route_state(store, storage, ids)
        .unwrap()
        .expect("fixture accepted input remains addressable within bounded route history")
}

pub fn try_accepted_route_state(
    store: &HomeStore,
    storage: SyndicStorage,
    ids: &NextRecordIds,
) -> Result<Option<AcceptedRouteEffectiveState>, SyndicReadError> {
    let input = storage.accepted_input(store, ids.accepted_input, point_limit())?;
    let Some(input) = input else {
        return Ok(None);
    };
    for revision in 1..=8 {
        match storage.accepted_route_page(
            store,
            ids.thread,
            input.route_generation(),
            AcceptedRouteRevision::new(revision).unwrap(),
            None,
        ) {
            Ok(page) => {
                if let Some(state) = page
                    .records()
                    .iter()
                    .find(|entry| entry.input().id() == ids.accepted_input)
                    .map(|entry| entry.effective_state())
                {
                    return Ok(Some(state));
                }
            }
            Err(SyndicReadError::StaleAcceptedRoute) => {}
            Err(error) => return Err(error),
        }
    }
    Ok(None)
}

pub fn wait_until<T>(label: &str, mut observation: impl FnMut() -> Option<T>) -> T {
    let deadline = Instant::now() + TIMEOUT;
    loop {
        if let Some(value) = observation() {
            return value;
        }
        assert!(Instant::now() < deadline, "timed out waiting for {label}");
        thread::yield_now();
    }
}

pub fn point_limit() -> SyndicPointReadLimit {
    SyndicPointReadLimit::new(1_000_000).unwrap()
}
