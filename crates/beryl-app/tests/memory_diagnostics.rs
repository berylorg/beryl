#[path = "../src/memory_diagnostics.rs"]
mod memory_diagnostics;

use std::cell::Cell;

use memory_diagnostics::{MemoryMilestone, RetainedStateSnapshot};

#[test]
fn retained_state_snapshot_is_built_only_when_milestones_are_enabled() {
    memory_diagnostics::configure(false);
    assert!(!memory_diagnostics::enabled());

    let disabled_called = Cell::new(false);
    let _ = MemoryMilestone::new("disabled").retained_state_if_enabled(|| {
        disabled_called.set(true);
        RetainedStateSnapshot {
            loaded_transcript_turns: Some(1),
            ..RetainedStateSnapshot::default()
        }
    });
    assert!(!disabled_called.get());

    memory_diagnostics::configure(true);
    assert!(memory_diagnostics::enabled());

    let enabled_called = Cell::new(false);
    let _ = MemoryMilestone::new("enabled").retained_state_if_enabled(|| {
        enabled_called.set(true);
        RetainedStateSnapshot {
            loaded_transcript_turns: Some(1),
            ..RetainedStateSnapshot::default()
        }
    });
    assert!(enabled_called.get());

    memory_diagnostics::configure(false);
}
