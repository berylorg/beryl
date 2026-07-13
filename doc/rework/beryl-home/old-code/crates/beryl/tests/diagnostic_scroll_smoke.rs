#![cfg(target_os = "windows")]

use std::{
    io::Write,
    process::{Child, ChildStdin, Command, Stdio},
    sync::mpsc::{self, Receiver},
    thread,
    time::{Duration, Instant},
};

use serde_json::{Value, json};

const REQUEST_TIMEOUT: Duration = Duration::from_secs(8);
const SHELL_READY_TIMEOUT: Duration = Duration::from_secs(30);
const FRAME_READY_TIMEOUT: Duration = Duration::from_secs(30);
const SCROLL_RESULT_TIMEOUT: Duration = Duration::from_secs(10);

#[test]
fn diagnostic_child_direct_protocol_scroll_smoke_preserves_wheel_continuity() {
    let home = tempfile::Builder::new()
        .prefix("beryl-diagnostic-scroll-smoke-")
        .tempdir()
        .expect("diagnostic child home should be created");
    let mut child = DirectDiagnosticChild::spawn(home.path().display().to_string());

    let handshake = child.request("handshake", json!({}), REQUEST_TIMEOUT);
    assert_eq!(handshake["protocol"], "beryl_diagnostic_child");

    let seed = child.retry_request(
        "seed_scroll_smoke_transcript",
        json!({}),
        SHELL_READY_TIMEOUT,
    );
    assert_eq!(seed["fixture"], "scroll_smoke_transcript");
    assert_eq!(seed["selectedThreadId"], "diagnostic-scroll-smoke-thread");
    assert!(seed["presentationRows"].as_u64().unwrap_or_default() > 24);
    assert_eq!(seed["published"], true);

    let ready = wait_for_rendered_frame(&mut child);
    let after_sequence = ready["scrollInputs"]["nextSequence"]
        .as_u64()
        .unwrap_or(1)
        .saturating_sub(1);

    let wheel_up = child.request(
        "scroll_transcript",
        json!({
            "command": "wheel",
            "deltaY": 96.0,
            "repeat": 8,
            "precise": false
        }),
        SCROLL_RESULT_TIMEOUT,
    );
    assert_eq!(wheel_up["status"], "applied");

    let wheel_down = child.request(
        "scroll_transcript",
        json!({
            "command": "wheel",
            "deltaY": -48.0,
            "repeat": 4,
            "precise": true
        }),
        SCROLL_RESULT_TIMEOUT,
    );
    assert_eq!(wheel_down["status"], "applied");

    let metrics = wait_for_scroll_inputs(&mut child, after_sequence, 12);
    let scroll_inputs = &metrics["scrollInputs"];
    let events = scroll_inputs["events"]
        .as_array()
        .expect("scroll input events should be an array");
    assert!(
        events.len() >= 12,
        "expected all repeated wheel events to be diagnosed, got {events:?}"
    );
    assert_eq!(scroll_inputs["continuityViolationCount"], 0);
    assert_eq!(scroll_inputs["largestContinuityErrorPx"], 0.0);

    for event in events {
        assert_eq!(event["consumed"], true);
        assert!(event["beforeAnchor"].is_object());
        assert!(event["afterAnchor"].is_object());
        assert_ne!(event["direction"], Value::Null);
        assert!(event["inputKind"] == "wheel" || event["inputKind"] == "touchpad");
    }
}

struct DirectDiagnosticChild {
    child: Child,
    stdin: ChildStdin,
    responses: Receiver<String>,
    next_id: u64,
}

impl DirectDiagnosticChild {
    fn spawn(home_dir: String) -> Self {
        let executable = option_env!("CARGO_BIN_EXE_beryl")
            .expect("Cargo should expose the beryl binary path to integration tests");
        let mut child = Command::new(executable)
            .arg("--diagnostic-target-stdio")
            .arg("--beryl-home-dir")
            .arg(home_dir)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("diagnostic child should start");
        let stdin = child
            .stdin
            .take()
            .expect("diagnostic child stdin should be piped");
        let stdout = child
            .stdout
            .take()
            .expect("diagnostic child stdout should be piped");
        let (sender, responses) = mpsc::channel();
        thread::spawn(move || {
            let reader = std::io::BufReader::new(stdout);
            for line in std::io::BufRead::lines(reader) {
                let Ok(line) = line else {
                    break;
                };
                if sender.send(line).is_err() {
                    break;
                }
            }
        });
        Self {
            child,
            stdin,
            responses,
            next_id: 1,
        }
    }

    fn retry_request(&mut self, command: &str, params: Value, timeout: Duration) -> Value {
        let started = Instant::now();
        let mut last_error = None;
        while started.elapsed() < timeout {
            match self.try_request(command, params.clone(), REQUEST_TIMEOUT) {
                Ok(value) => return value,
                Err(error) => {
                    last_error = Some(error);
                    thread::sleep(Duration::from_millis(100));
                }
            }
        }
        panic!(
            "diagnostic child command {command:?} did not succeed before timeout; last error: {:?}",
            last_error
        );
    }

    fn request(&mut self, command: &str, params: Value, timeout: Duration) -> Value {
        self.try_request(command, params, timeout)
            .unwrap_or_else(|error| panic!("diagnostic child command {command:?} failed: {error}"))
    }

    fn try_request(
        &mut self,
        command: &str,
        params: Value,
        timeout: Duration,
    ) -> Result<Value, String> {
        let id = format!("diagnostic-scroll-smoke-{}", self.next_id);
        self.next_id = self.next_id.saturating_add(1);
        let frame = json!({
            "id": id,
            "command": command,
            "params": params,
        });
        writeln!(
            self.stdin,
            "{}",
            serde_json::to_string(&frame).map_err(|error| error.to_string())?
        )
        .map_err(|error| error.to_string())?;
        self.stdin.flush().map_err(|error| error.to_string())?;

        let deadline = Instant::now() + timeout;
        loop {
            let now = Instant::now();
            if now >= deadline {
                return Err(format!("timed out waiting for response id {id}"));
            }
            let line = self
                .responses
                .recv_timeout(deadline.saturating_duration_since(now))
                .map_err(|error| error.to_string())?;
            let response: Value = serde_json::from_str(&line).map_err(|error| error.to_string())?;
            if response["id"].as_str() != Some(id.as_str()) {
                continue;
            }
            if response["ok"] == true {
                return Ok(response["result"].clone());
            }
            return Err(response["error"].to_string());
        }
    }
}

impl Drop for DirectDiagnosticChild {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn wait_for_rendered_frame(child: &mut DirectDiagnosticChild) -> Value {
    let started = Instant::now();
    while started.elapsed() < FRAME_READY_TIMEOUT {
        let metrics = child.request(
            "read_transcript_frame_metrics",
            json!({ "limit": 8 }),
            REQUEST_TIMEOUT,
        );
        if metrics["frames"].as_array().is_some_and(|frames| {
            frames.iter().any(|frame| {
                frame["renderedFrame"]["totalSegmentCount"]
                    .as_u64()
                    .unwrap_or_default()
                    > 0
            })
        }) {
            return metrics;
        }
        thread::sleep(Duration::from_millis(100));
    }
    panic!("diagnostic child did not report a rendered transcript frame before timeout");
}

fn wait_for_scroll_inputs(
    child: &mut DirectDiagnosticChild,
    after_sequence: u64,
    expected_events: usize,
) -> Value {
    let started = Instant::now();
    while started.elapsed() < SCROLL_RESULT_TIMEOUT {
        let metrics = child.request(
            "read_transcript_frame_metrics",
            json!({
                "afterSequence": after_sequence,
                "limit": 32
            }),
            REQUEST_TIMEOUT,
        );
        if metrics["scrollInputs"]["events"]
            .as_array()
            .is_some_and(|events| events.len() >= expected_events)
        {
            return metrics;
        }
        thread::sleep(Duration::from_millis(100));
    }
    panic!("diagnostic child did not report expected scroll input events before timeout");
}
