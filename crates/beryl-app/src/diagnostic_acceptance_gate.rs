//! Fixed acceptance-only frame released after exact Windows Job ownership is established.

pub const DIAGNOSTIC_ACCEPTANCE_STARTUP_GATE_FRAME: &[u8] =
    b"beryl_diagnostic_acceptance_gate_v1\n";
pub const MAX_DIAGNOSTIC_ACCEPTANCE_STARTUP_GATE_BYTES: usize =
    DIAGNOSTIC_ACCEPTANCE_STARTUP_GATE_FRAME.len();
pub const DIAGNOSTIC_ACCEPTANCE_STARTUP_READY_FRAME: &[u8] =
    b"beryl_diagnostic_acceptance_ready_v1\n";
