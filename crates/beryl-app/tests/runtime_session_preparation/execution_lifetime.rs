use super::*;
use beryl_app::cas_projection::RuntimeInterestKind;
use beryl_model::SyndicTurnId;
use serde_json::Value;
use syndic_storage::{FirstAcceptanceKind, SyndicPointReadLimit, TurnLifecycle};

fn started(fixture: &Fixture, sessions: &ScheduledExecutionSessions, ordinal: u8) -> Value {
    let path = fixture
        .root(1)
        .join(format!("execution-started-{ordinal}.json"));
    let deadline = std::time::Instant::now() + TIMEOUT;
    loop {
        let evidence = fs::read(&path)
            .ok()
            .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok());
        if let Some(evidence) = evidence {
            return evidence;
        }
        if std::time::Instant::now() >= deadline {
            let live = fixture.service().live_home_command().unwrap();
            let gate = fixture
                .storage
                .input_gate(
                    live.home(),
                    thread_id(1),
                    SyndicPointReadLimit::new(1_000_000).unwrap(),
                )
                .unwrap();
            panic!(
                "managed execution did not start: sessions {:?}, scheduler {:?}, gate {:?}",
                sessions.diagnostics(),
                fixture.service().accepted_input_scheduler_diagnostics(),
                gate
            );
        }
        thread::sleep(Duration::from_millis(2));
    }
}

fn turn(fixture: &Fixture, expected: TurnLifecycle) -> SyndicTurnId {
    let mut selected = None;
    wait_until(|| {
        let live = fixture.service().live_home_command().unwrap();
        let limit = SyndicPointReadLimit::new(1_000_000).unwrap();
        let Some(record) = fixture
            .storage
            .thread(live.home(), thread_id(1), limit)
            .unwrap()
        else {
            return false;
        };
        let Some(id) = record.committed_tail() else {
            return false;
        };
        let state = fixture
            .storage
            .turn_state(live.home(), id, limit)
            .unwrap()
            .unwrap();
        if state.lifecycle() == expected
            && (expected != TurnLifecycle::Active || state.source_event_count() >= 3)
        {
            selected = Some(id);
            true
        } else {
            false
        }
    });
    selected.unwrap()
}

fn release(fixture: &Fixture, ordinal: u8) {
    fs::write(
        fixture.root(1).join(format!("execution-release-{ordinal}")),
        "complete",
    )
    .unwrap();
}

#[test]
fn managed_direct_execution_survives_last_view_detach_and_immediate_reattach() {
    let (mut fixture, sessions, _attention) = fixture(8);
    fs::write(fixture.root(1).join("fixture-mode"), "execution-lifetime").unwrap();
    let view = fixture.acquire(1, RuntimeInterestKind::View).unwrap();
    let readiness = support::ready(&view);
    submission::submit(&fixture, thread_id(1));
    let evidence = started(&fixture, &sessions, 0);
    assert_eq!(evidence["text"], "continue durable work");
    let process = ProcessWitness::open(evidence["pid"].as_u64().unwrap() as u32);
    let active = turn(&fixture, TurnLifecycle::Active);
    assert_eq!(sessions.diagnostics().checked_out, 1);
    drop(view);
    let reattached = fixture.acquire(1, RuntimeInterestKind::View).unwrap();
    assert_eq!(support::ready(&reattached), readiness);
    assert_eq!(sessions.diagnostics().checked_out, 1);
    assert!(process.running());
    assert!(!fixture.root(1).join("execution-unsubscribed-0").exists());
    assert!(
        !fixture
            .root(1)
            .join("runtime-session-evidence-2.json")
            .exists()
    );
    drop(reattached);
    release(&fixture, 0);
    assert_eq!(turn(&fixture, TurnLifecycle::Complete), active);
    process.assert_exited();
    wait_until(|| {
        sessions.diagnostics().retained == 0
            && fixture.token_count() == 0
            && fixture.service().worker_pool_diagnostics().active() == 0
    });
    assert!(!fixture.root(1).join("execution-started-1.json").exists());
    close(&mut fixture, &sessions);
}

#[test]
fn managed_accepted_successor_dispatches_and_captures_without_a_view() {
    let (mut fixture, sessions, _attention) = fixture(8);
    fs::write(fixture.root(1).join("fixture-mode"), "execution-next").unwrap();
    let view = fixture.acquire(1, RuntimeInterestKind::View).unwrap();
    support::ready(&view);
    submission::submit(&fixture, thread_id(1));
    let first = started(&fixture, &sessions, 0);
    let first_turn = turn(&fixture, TurnLifecycle::Active);
    let process = ProcessWitness::open(first["pid"].as_u64().unwrap() as u32);
    drop(view);
    assert_eq!(
        submission::submit_text(
            &fixture,
            thread_id(1),
            "queued without a view",
            180,
            SyndicTimestamp::from_unix_millis(37_003)
        ),
        FirstAcceptanceKind::Accepted
    );
    wait_until(|| fixture.root(1).join("execution-steering-denied").exists());
    release(&fixture, 0);
    let second = started(&fixture, &sessions, 1);
    let successor_process = ProcessWitness::open(second["pid"].as_u64().unwrap() as u32);
    if second["pid"] != first["pid"] {
        process.assert_exited();
    }
    assert_eq!(second["thread"], first["thread"]);
    assert_eq!(second["text"], "queued without a view");
    let second_turn = turn(&fixture, TurnLifecycle::Active);
    assert_ne!(second_turn, first_turn);
    assert_eq!(sessions.diagnostics().checked_out, 1);
    release(&fixture, 1);
    assert_eq!(turn(&fixture, TurnLifecycle::Complete), second_turn);
    process.assert_exited();
    successor_process.assert_exited();
    wait_until(|| {
        sessions.diagnostics().retained == 0
            && fixture.token_count() == 0
            && fixture.service().worker_pool_diagnostics().active() == 0
    });
    assert!(
        !fixture
            .root(1)
            .join("runtime-session-evidence-3.json")
            .exists()
    );
    close(&mut fixture, &sessions);
}
