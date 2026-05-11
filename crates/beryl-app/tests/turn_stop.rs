#[allow(dead_code)]
#[path = "../src/shell/status_line.rs"]
mod status_line;

#[allow(dead_code)]
#[path = "../src/shell/turn_stop.rs"]
mod turn_stop;

use std::time::Duration;

use status_line::CancellableActiveTurn;
use turn_stop::{TurnStopBackend, request_turn_stop};

#[test]
fn request_turn_stop_interrupts_exact_target() {
    let mut backend = FakeTurnStopBackend::default();
    let target = CancellableActiveTurn::context_compaction("thread_1", "turn_7");
    let timeout = Duration::from_secs(9);

    request_turn_stop(&mut backend, &target, timeout).unwrap();

    assert_eq!(
        backend.interrupted_turns,
        vec![("thread_1".to_string(), "turn_7".to_string(), timeout)]
    );
}

#[test]
fn request_turn_stop_interrupts_ordinary_turn_exactly() {
    let mut backend = FakeTurnStopBackend::default();
    let target = CancellableActiveTurn::ordinary("thread_parent", "turn_parent");
    let timeout = Duration::from_secs(5);

    request_turn_stop(&mut backend, &target, timeout).unwrap();

    assert_eq!(
        backend.interrupted_turns,
        vec![(
            "thread_parent".to_string(),
            "turn_parent".to_string(),
            timeout
        )]
    );
}

#[test]
fn request_turn_stop_reports_backend_failure() {
    let mut backend = FakeTurnStopBackend {
        fail_with: Some("backend rejected interrupt".to_string()),
        ..FakeTurnStopBackend::default()
    };
    let target = CancellableActiveTurn::ordinary("thread_1", "turn_7");

    let error = request_turn_stop(&mut backend, &target, Duration::from_secs(3)).unwrap_err();

    assert_eq!(
        error,
        "Beryl could not stop the active turn: backend rejected interrupt"
    );
}

#[derive(Default)]
struct FakeTurnStopBackend {
    interrupted_turns: Vec<(String, String, Duration)>,
    fail_with: Option<String>,
}

impl TurnStopBackend for FakeTurnStopBackend {
    type Error = String;

    fn interrupt_turn(
        &mut self,
        thread_id: &str,
        turn_id: &str,
        timeout: Duration,
    ) -> Result<(), Self::Error> {
        if let Some(error) = self.fail_with.clone() {
            return Err(error);
        }

        self.interrupted_turns
            .push((thread_id.to_string(), turn_id.to_string(), timeout));
        Ok(())
    }
}
