//! Acceptance-only startup gate for the diagnostic target bootstrap.

use std::io::{self, BufRead};

pub use beryl_app::{
    DIAGNOSTIC_ACCEPTANCE_STARTUP_GATE_FRAME, DIAGNOSTIC_ACCEPTANCE_STARTUP_READY_FRAME,
    MAX_DIAGNOSTIC_ACCEPTANCE_STARTUP_GATE_BYTES,
};

pub fn read_diagnostic_acceptance_startup_gate(input: impl BufRead) -> Result<(), io::Error> {
    let mut frame = Vec::with_capacity(MAX_DIAGNOSTIC_ACCEPTANCE_STARTUP_GATE_BYTES + 1);
    let mut bounded_input = input.take((MAX_DIAGNOSTIC_ACCEPTANCE_STARTUP_GATE_BYTES + 1) as u64);
    let read = bounded_input.read_until(b'\n', &mut frame)?;
    if read == 0 || !frame.ends_with(b"\n") {
        return Err(io::Error::new(
            io::ErrorKind::UnexpectedEof,
            "diagnostic acceptance startup gate ended before one complete frame",
        ));
    }
    if frame.len() > MAX_DIAGNOSTIC_ACCEPTANCE_STARTUP_GATE_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "diagnostic acceptance startup gate frame exceeded its byte limit",
        ));
    }
    if frame != DIAGNOSTIC_ACCEPTANCE_STARTUP_GATE_FRAME {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "diagnostic acceptance startup gate frame was invalid",
        ));
    }
    Ok(())
}

pub fn write_diagnostic_acceptance_startup_ready(
    mut output: impl io::Write,
) -> Result<(), io::Error> {
    output.write_all(DIAGNOSTIC_ACCEPTANCE_STARTUP_READY_FRAME)?;
    output.flush()
}

pub fn enforce_diagnostic_acceptance_startup_gate(
    input: impl BufRead,
    output: impl io::Write,
) -> Result<(), io::Error> {
    read_diagnostic_acceptance_startup_gate(input)?;
    write_diagnostic_acceptance_startup_ready(output)
}
