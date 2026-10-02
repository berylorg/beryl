#![cfg(target_os = "windows")]

use std::{
    io::{BufRead, BufReader, Read, Write},
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};

struct Process(Child);

impl Drop for Process {
    fn drop(&mut self) {
        if self.0.try_wait().ok().flatten().is_none() {
            let _ = self.0.kill();
        }
        let _ = self.0.wait();
    }
}

#[test]
fn diagnostic_handshake_and_input_loss_exit_the_exact_process() {
    let home = tempfile::tempdir().unwrap();
    disconnect(home.path(), false, false);
}

#[test]
fn running_and_restored_process_exit_after_diagnostic_input_loss() {
    let home = tempfile::tempdir().unwrap();
    let first = disconnect(home.path(), true, false);
    let restored = disconnect(home.path(), true, false);
    assert_eq!(
        restored, first,
        "relaunch must restore the exact window identity"
    );
}

#[test]
fn running_process_exits_after_diagnostic_output_loss() {
    let home = tempfile::tempdir().unwrap();
    disconnect(home.path(), true, true);
}

fn disconnect(
    home: &std::path::Path,
    wait_for_running: bool,
    output_loss: bool,
) -> Vec<serde_json::Value> {
    let mut process = Process(
        Command::new(env!("CARGO_BIN_EXE_beryl"))
            .env_remove("RUST_MIN_STACK")
            .args(["--diagnostic-target-stdio", "--beryl-home-dir"])
            .arg(home)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .unwrap(),
    );
    let mut input = process.0.stdin.take().unwrap();
    writeln!(
        input,
        "{{\"id\":\"bootstrap-handshake\",\"command\":\"handshake\"}}"
    )
    .unwrap();
    input.flush().unwrap();
    let output = process.0.stdout.take().unwrap();
    let output = BufReader::new(output.take(256 * 1024 + 1));
    let (response, mut output) = read_frame(output, &mut process);
    assert_eq!(response["id"], "bootstrap-handshake");
    assert_eq!(response["ok"], true);
    assert_eq!(response["result"]["protocol"], "beryl_diagnostic_child");
    assert_eq!(response["result"]["protocolVersion"], 1);
    let mut windows = Vec::new();
    if wait_for_running {
        let deadline = Instant::now() + Duration::from_secs(20);
        loop {
            writeln!(input, "{{\"id\":\"stage\",\"command\":\"read_ui_state\"}}").unwrap();
            input.flush().unwrap();
            let (response, returned) = read_frame(output, &mut process);
            output = returned;
            assert_eq!(response["ok"], true, "{response}");
            if response["result"]["stage"] == "running" {
                windows = response["result"]["mainWindowIds"]
                    .as_array()
                    .unwrap()
                    .clone();
                assert_eq!(
                    windows.len(),
                    1,
                    "fresh or restored zero-runtime home must have one shell"
                );
                break;
            }
            assert!(
                Instant::now() < deadline,
                "fresh home never reached Running: {response}"
            );
            std::thread::sleep(Duration::from_millis(50));
        }
    }
    let _retained_input = if output_loss {
        drop(output);
        writeln!(
            input,
            "{{\"id\":\"lost-output\",\"command\":\"read_process\"}}"
        )
        .unwrap();
        input.flush().unwrap();
        Some(input)
    } else {
        drop(input);
        None
    };
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        if let Some(status) = process.0.try_wait().unwrap() {
            if wait_for_running {
                assert!(
                    status.success(),
                    "Running diagnostic input loss must exit orderly: {status}"
                );
            } else {
                assert!(
                    matches!(status.code(), Some(0 | 1)),
                    "startup cancellation must exit normally: {status}"
                );
            }
            break;
        }
        assert!(
            Instant::now() < deadline,
            "diagnostic process did not finish orderly shutdown"
        );
        std::thread::sleep(Duration::from_millis(50));
    }
    windows
}

type Output = BufReader<std::io::Take<std::process::ChildStdout>>;

fn read_frame(mut output: Output, process: &mut Process) -> (serde_json::Value, Output) {
    let read = std::thread::spawn(move || {
        let mut line = String::new();
        output.read_line(&mut line).unwrap();
        (line, output)
    });
    let deadline = Instant::now() + Duration::from_secs(5);
    while !read.is_finished() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(10));
    }
    if !read.is_finished() {
        process.0.kill().unwrap();
        process.0.wait().unwrap();
        let _ = read.join();
        panic!("diagnostic response deadline elapsed");
    }
    let (line, output) = read.join().unwrap();
    assert!(line.len() <= 256 * 1024);
    (serde_json::from_str(&line).unwrap(), output)
}
