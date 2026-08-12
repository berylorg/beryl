use std::{thread, time::Instant};

use beryl_app::input_admission::prepare_accepted_input_admission;
use beryl_home_store::{
    CommandOutcome, HomeCommand, HomeOpenOptions, HomeSchemaVersion, HomeStore,
    test_faults::FaultController,
};
use beryl_model::{
    ExecutionBinding, PathFlavor, RootId, RuntimeId, RuntimeMode, RuntimeNativePath,
    SyndicAcceptedInputId, SyndicDraftId, SyndicThreadId, SyndicTurnId,
};
use beryl_state::BerylState;
use syndic_storage::{
    AcceptedInputAdmission, AcceptedRouteEffectiveState, AcceptedRouteRevision, BindingState,
    ComposerAtom, ComposerPayload, DraftPayloadUpdate, DraftPayloadUpdateDecision, PreparedContent,
    SyndicPointReadLimit, SyndicReadError, SyndicStorage, SyndicTimestamp,
    test_faults::{FixtureBatch, FixtureRecord},
};

#[path = "../phase37_normal_terminal/server.rs"]
mod server;

pub use server::{AUTHORIZATION, NormalTerminalServer, SUBMITTED_TEXT, TIMEOUT};

#[path = "support/execution.rs"]
mod execution;
pub use execution::{CheckoutProvider, SessionSlot, UnavailableProvider, ready_provider};

mod records {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/phase58_accepted_promotion/support/records.rs"
    ));
}

pub const EXECUTION_ROOT: &str = r"C:\work\beryl";

#[derive(Clone, Copy)]
pub struct NextRecordIds {
    pub thread: SyndicThreadId,
    pub accepted_input: SyndicAcceptedInputId,
    pub parent: SyndicTurnId,
}

pub fn open_registered_home() -> (tempfile::TempDir, HomeStore, SyndicStorage, BerylState) {
    let directory = tempfile::tempdir().unwrap();
    let mut home = HomeStore::open(HomeOpenOptions::new(
        directory.path(),
        HomeSchemaVersion::CURRENT,
    ))
    .unwrap();
    let storage = SyndicStorage::register(&mut home).unwrap();
    let state = BerylState::register(&mut home).unwrap();
    (directory, home, storage, state)
}

pub fn execution_binding(runtime_id: RuntimeId) -> ExecutionBinding {
    ExecutionBinding::new(
        runtime_id,
        RootId::from_bytes([162; 16]),
        RuntimeNativePath::from_admitted(RuntimeMode::host(), PathFlavor::Windows, EXECUTION_ROOT)
            .unwrap(),
    )
}

pub fn install_next_records(
    store: &HomeStore,
    storage: SyndicStorage,
    seed: u8,
    execution: ExecutionBinding,
) -> NextRecordIds {
    let thread = SyndicThreadId::from_bytes([seed; 16]);
    let current_draft = SyndicDraftId::from_bytes([seed.wrapping_add(1); 16]);
    let source_draft = SyndicDraftId::from_bytes([seed.wrapping_add(2); 16]);
    let accepted_input = source_draft.accepted_input_id();
    let parent = SyndicTurnId::from_bytes([seed.wrapping_add(3); 16]);

    let accepted = PreparedContent::composer(
        &ComposerPayload::new(vec![ComposerAtom::text(SUBMITTED_TEXT).unwrap()]).unwrap(),
    )
    .unwrap();
    let current = PreparedContent::composer(
        &ComposerPayload::new(vec![ComposerAtom::text("retained draft").unwrap()]).unwrap(),
    )
    .unwrap();
    let empty = PreparedContent::composer(&ComposerPayload::default()).unwrap();
    let (accepted_reference, accepted_records) = records::prepared_content_records(&accepted);
    let (current_reference, current_records) = records::prepared_content_records(&current);
    let (_, empty_records) = records::prepared_content_records(&empty);
    let mut fixture_records = records::promotion_records(
        thread,
        current_draft,
        source_draft,
        parent,
        execution,
        accepted_reference,
        current_reference,
        None,
        false,
    );
    fixture_records.extend(accepted_records);
    fixture_records.extend(current_records);
    fixture_records.extend(empty_records);

    let mut batch = FixtureBatch::new();
    for record in fixture_records {
        batch.put(record).unwrap();
    }
    let mut command = HomeCommand::new(store.home_revision().unwrap());
    command
        .add(storage.fixture_contribution(storage.revision(store).unwrap(), batch))
        .unwrap();
    match store.execute(command) {
        CommandOutcome::Committed {
            later_failure: None,
            ..
        } => {}
        outcome @ CommandOutcome::NotCommitted { .. } => {
            panic!("expected committed fixture setup, got {outcome:?}")
        }
        outcome @ CommandOutcome::Committed {
            later_failure: Some(_),
            ..
        } => panic!("expected no later failure, got {outcome:?}"),
        outcome @ CommandOutcome::Indeterminate { .. } => {
            panic!("expected committed fixture setup, got {outcome:?}")
        }
    }
    store
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap();

    NextRecordIds {
        thread,
        accepted_input,
        parent,
    }
}

pub fn admit_runtime_next_input(fixture: &mut crate::syndic::Fixture, seed: u8) -> NextRecordIds {
    let ids = seed_runtime_next_input_without_wake(fixture, seed);
    fixture.store.notify_scheduled_ordinary_execution_ready();
    ids
}

pub fn seed_runtime_next_input_without_wake(
    fixture: &mut crate::syndic::Fixture,
    seed: u8,
) -> NextRecordIds {
    let active = fixture.submit_text("phase62 non-steerable predecessor");
    let source = fixture.activate_without_terminal(active);
    fixture.mark_active_unknown_terminal(active, &source);
    let ids = admit_runtime_awaiting_terminal_input(fixture, seed);
    fixture.advance_clock_to(62_102);
    fixture.complete_active_without_assistant(active, &source);
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
    let (thread, parent, accepted_input, prepared) = {
        let command_home = fixture.store.live_home_command().unwrap();
        let home = command_home.home();
        let thread = fixture.thread;
        let parent = fixture
            .storage
            .thread(home, thread, point_limit())
            .unwrap()
            .and_then(|thread| thread.committed_tail())
            .expect("runtime next-turn fixture has completed parent history");
        let content = PreparedContent::composer(
            &ComposerPayload::new(vec![ComposerAtom::text(SUBMITTED_TEXT).unwrap()]).unwrap(),
        )
        .unwrap();
        let (_, content_records) = records::prepared_content_records(&content);
        let mut content_batch = FixtureBatch::new();
        for record in content_records {
            content_batch.put(record).unwrap();
        }
        execute_syndic_contribution(home, fixture.storage, content_batch);

        let current = fixture
            .storage
            .current_draft(home, thread, point_limit())
            .unwrap()
            .unwrap();
        let DraftPayloadUpdateDecision::Update(update) =
            DraftPayloadUpdate::prepare(&current, &content, time(62_100)).unwrap()
        else {
            panic!("runtime next-turn fixture must replace the empty draft")
        };
        execute_contribution(
            home,
            fixture
                .storage
                .update_draft_payload(fixture.storage.revision(home).unwrap(), update),
        );

        let current = fixture
            .storage
            .current_draft(home, thread, point_limit())
            .unwrap()
            .unwrap();
        let gate = fixture
            .storage
            .input_gate(home, thread, point_limit())
            .unwrap()
            .unwrap();
        let next_draft = SyndicDraftId::from_bytes([seed.wrapping_add(10); 16]);
        let admission = AcceptedInputAdmission::new(
            thread,
            current.thread().revision(),
            current.draft().id(),
            current.draft().revision(),
            current.draft().content(),
            gate.revision(),
            next_draft,
            None,
            time(62_101),
        );
        let accepted_input = admission.accepted_input_id();
        let prepared = prepare_accepted_input_admission(
            home,
            fixture.storage,
            fixture.state.assets(),
            admission,
        )
        .unwrap();
        home.scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
            .unwrap();
        (thread, parent, accepted_input, prepared)
    };
    fixture
        .store
        .execute_accepted_input_admission(prepared)
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

fn execute_syndic_contribution(store: &HomeStore, storage: SyndicStorage, batch: FixtureBatch) {
    execute_contribution(
        store,
        storage.fixture_contribution(storage.revision(store).unwrap(), batch),
    );
}

fn execute_contribution(store: &HomeStore, contribution: beryl_home_store::MutationContribution) {
    let mut command = HomeCommand::new(store.home_revision().unwrap());
    command.add(contribution).unwrap();
    match store.execute(command) {
        CommandOutcome::Committed {
            later_failure: None,
            ..
        } => {}
        outcome @ CommandOutcome::NotCommitted { .. } => {
            panic!("expected committed contribution, got {outcome:?}")
        }
        outcome @ CommandOutcome::Committed {
            later_failure: Some(_),
            ..
        } => panic!("expected no later failure, got {outcome:?}"),
        outcome @ CommandOutcome::Indeterminate { .. } => {
            panic!("expected committed contribution, got {outcome:?}")
        }
    }
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

fn time(value: u64) -> SyndicTimestamp {
    SyndicTimestamp::from_unix_millis(value)
}
