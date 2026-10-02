use super::*;
use std::io::Cursor;

fn sender(
    capacity: usize,
) -> (
    DiagnosticTargetShellRequestSender,
    Receiver<DiagnosticTargetShellRequest>,
) {
    let (sender, receiver) = mpsc::sync_channel(capacity);
    (
        DiagnosticTargetShellRequestSender {
            sender,
            response_timeout: Duration::from_millis(10),
            terminal: Arc::new(AtomicBool::new(false)),
            wake: Arc::new(Mutex::new(None)),
        },
        receiver,
    )
}

#[test]
fn exact_handshake_is_required_before_shell_dispatch() {
    let (sender, receiver) = sender(1);
    let mut output = Vec::new();
    run_diagnostic_target_stdio_loop(sender, Cursor::new(b"{\"id\":\"before\",\"command\":\"read_process\"}\n{\"id\":\"hello\",\"command\":\"handshake\"}\n"), &mut output);
    let lines: Vec<serde_json::Value> = output
        .split(|byte| *byte == b'\n')
        .filter(|line| !line.is_empty())
        .map(|line| serde_json::from_slice(line).unwrap())
        .collect();
    assert_eq!(lines.len(), 2);
    assert_eq!(lines[0]["error"]["kind"], "handshake_required");
    assert_eq!(
        lines[1]["result"]["protocol"],
        DIAGNOSTIC_CHILD_PROTOCOL_NAME
    );
    assert_eq!(
        lines[1]["result"]["protocolVersion"],
        DIAGNOSTIC_CHILD_PROTOCOL_VERSION
    );
    assert!(matches!(
        receiver.try_recv(),
        Ok(DiagnosticTargetShellRequest::Shutdown)
    ));
}

#[test]
fn eof_shutdown_survives_full_shell_queue() {
    let (sender, receiver) = sender(1);
    assert!(
        sender
            .sender
            .try_send(DiagnosticTargetShellRequest::Shutdown)
            .is_ok()
    );
    let terminal = sender.terminal.clone();
    run_diagnostic_target_stdio_loop(sender, Cursor::new(Vec::<u8>::new()), Vec::new());
    assert!(terminal.load(Ordering::Acquire));
    assert!(matches!(
        receiver.try_recv(),
        Ok(DiagnosticTargetShellRequest::Shutdown)
    ));
}

#[test]
fn output_loss_ends_endpoint_and_requests_orderly_shutdown() {
    struct LostOutput;
    impl Write for LostOutput {
        fn write(&mut self, _: &[u8]) -> io::Result<usize> {
            Err(io::Error::new(io::ErrorKind::BrokenPipe, "closed"))
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    let (sender, receiver) = sender(1);
    let terminal = sender.terminal.clone();
    run_diagnostic_target_stdio_loop(
        sender,
        Cursor::new(b"{\"id\":\"hello\",\"command\":\"handshake\"}\n"),
        LostOutput,
    );
    assert!(terminal.load(Ordering::Acquire));
    assert!(matches!(
        receiver.try_recv(),
        Ok(DiagnosticTargetShellRequest::Shutdown)
    ));
}

#[test]
fn expired_request_cannot_be_claimed_for_late_shell_execution() {
    let control = DiagnosticTargetRequestControl::new(Duration::ZERO);
    assert!(!control.try_claim());
    assert_eq!(
        control.state.load(Ordering::Acquire),
        DIAGNOSTIC_TARGET_REQUEST_CANCELLED
    );
}
