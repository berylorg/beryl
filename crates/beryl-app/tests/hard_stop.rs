#[allow(dead_code)]
#[path = "../src/shell/status_line.rs"]
mod status_line;

#[allow(dead_code)]
#[path = "../src/shell/hard_stop.rs"]
mod hard_stop;

use std::time::Duration;

use beryl_backend::{HardStopTarget, HardStopTargetOutcome};
use hard_stop::{HardStopBackend, request_hard_stop};
use status_line::{CancellableActiveTurn, SelectedTurnHardStopTargets};

#[test]
fn request_hard_stop_interrupts_selected_turn_before_other_exact_targets() {
    let mut backend = FakeHardStopBackend::default();
    let selected_targets = SelectedTurnHardStopTargets::new(
        CancellableActiveTurn::ordinary("thread_parent", "turn_parent"),
        vec![
            HardStopTarget::command_execution("proc_1"),
            HardStopTarget::turn("thread_parent", "turn_parent"),
            HardStopTarget::turn("thread_child", "turn_child"),
            HardStopTarget::command_execution("proc_1"),
            HardStopTarget::background_terminals("thread_parent"),
        ],
        Vec::new(),
    );
    let timeout = Duration::from_secs(7);

    let outcomes = request_hard_stop(&mut backend, &selected_targets, timeout).unwrap();

    assert_eq!(
        backend.requests,
        vec![
            (
                HardStopTarget::turn("thread_parent", "turn_parent"),
                timeout
            ),
            (HardStopTarget::command_execution("proc_1"), timeout),
            (HardStopTarget::turn("thread_child", "turn_child"), timeout),
            (
                HardStopTarget::background_terminals("thread_parent"),
                timeout
            ),
        ]
    );
    assert!(outcomes.iter().all(HardStopTargetOutcome::is_success));
}

#[test]
fn request_hard_stop_preserves_per_target_failures_and_continues() {
    let mut backend = FakeHardStopBackend {
        failing_target: Some(HardStopTarget::command_execution("proc_1")),
        ..FakeHardStopBackend::default()
    };
    let selected_targets = SelectedTurnHardStopTargets::new(
        CancellableActiveTurn::ordinary("thread_parent", "turn_parent"),
        vec![
            HardStopTarget::turn("thread_parent", "turn_parent"),
            HardStopTarget::command_execution("proc_1"),
            HardStopTarget::background_terminals("thread_parent"),
        ],
        Vec::new(),
    );

    let outcomes =
        request_hard_stop(&mut backend, &selected_targets, Duration::from_secs(7)).unwrap();

    assert_eq!(outcomes.len(), 3);
    assert!(outcomes[0].is_success());
    assert!(matches!(
        &outcomes[1],
        HardStopTargetOutcome::Failed {
            target,
            method: "command/exec/terminate",
            message,
        } if *target == HardStopTarget::command_execution("proc_1")
            && message == "backend rejected target"
    ));
    assert!(outcomes[2].is_success());
}

#[derive(Default)]
struct FakeHardStopBackend {
    requests: Vec<(HardStopTarget, Duration)>,
    failing_target: Option<HardStopTarget>,
}

impl HardStopBackend for FakeHardStopBackend {
    type Error = String;

    fn request_hard_stop_target(
        &mut self,
        target: &HardStopTarget,
        timeout: Duration,
    ) -> Result<HardStopTargetOutcome, Self::Error> {
        self.requests.push((target.clone(), timeout));
        if self.failing_target.as_ref() == Some(target) {
            return Ok(HardStopTargetOutcome::Failed {
                target: target.clone(),
                method: target.method(),
                message: "backend rejected target".to_string(),
            });
        }

        Ok(HardStopTargetOutcome::Succeeded {
            target: target.clone(),
        })
    }
}
