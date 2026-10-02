mod windows;

use ::windows::Win32::Foundation::{HANDLE, HWND};
use std::{
    io::{BufRead, BufReader, Read},
    os::windows::io::AsRawHandle,
    path::Path,
    process::{Child, ChildStdin, ChildStdout, Command, ExitStatus, Stdio},
    thread,
    time::{Duration, Instant},
};

const DEADLINE: Duration = Duration::from_secs(15);
type Output = BufReader<std::io::Take<ChildStdout>>;

pub struct Application {
    child: Child,
    reporters: Vec<windows::Reporter>,
    output: Option<Output>,
}

impl Application {
    pub fn spawn(home: &Path, arguments: &[&str], panic_at_open: bool) -> Self {
        let mut command = Command::new(env!("CARGO_BIN_EXE_beryl"));
        command
            .env_remove("RUST_MIN_STACK")
            .env_remove("BERYL_TEST_PANIC_HOME_OPEN")
            .args(arguments)
            .arg("--beryl-home-dir")
            .arg(home)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit());
        if panic_at_open {
            command.env("BERYL_TEST_PANIC_HOME_OPEN", "1");
        }
        let mut child = command.spawn().unwrap();
        let output = Some(BufReader::new(
            child.stdout.take().unwrap().take(256 * 1024 + 1),
        ));
        Self {
            child,
            reporters: Vec::new(),
            output,
        }
    }

    pub fn input(&mut self) -> &mut ChildStdin {
        self.child.stdin.as_mut().unwrap()
    }
    pub fn close_input(&mut self) {
        self.child.stdin.take();
    }

    pub fn adopt_reporter(&mut self) {
        let deadline = Instant::now() + DEADLINE;
        loop {
            self.reporters = windows::children(
                HANDLE(self.child.as_raw_handle()),
                Path::new(env!("CARGO_BIN_EXE_beryl")),
            );
            if !self.reporters.is_empty() {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "actual application did not start reporter"
            );
            thread::sleep(Duration::from_millis(10));
        }
        assert_eq!(
            self.reporters.len(),
            1,
            "application must own exactly one reporter"
        );
        assert!(!self.reporters[0].exited());
    }

    pub fn assert_single_waiting_reporter(&mut self) {
        assert!(self.child.try_wait().unwrap().is_none());
        let observed = windows::children(
            HANDLE(self.child.as_raw_handle()),
            Path::new(env!("CARGO_BIN_EXE_beryl")),
        );
        let identities = observed
            .into_iter()
            .map(windows::Reporter::observation_identity)
            .collect::<Vec<_>>();
        assert_eq!(identities, vec![self.reporters[0].identity()]);
        assert!(!self.reporters[0].exited());
    }

    pub fn read_line(&mut self) -> String {
        let mut output = self.output.take().unwrap();
        let read = thread::spawn(move || {
            let mut line = String::new();
            output.read_line(&mut line).unwrap();
            (line, output)
        });
        let deadline = Instant::now() + DEADLINE;
        while !read.is_finished() && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(10));
        }
        if !read.is_finished() {
            let _ = self.child.kill();
            let _ = self.child.wait();
            let _ = read.join();
            panic!("actual application output deadline elapsed");
        }
        let (line, output) = read.join().unwrap();
        self.output = Some(output);
        assert!(line.len() <= 256 * 1024);
        line
    }

    pub fn wait_for_exit(&mut self) -> ExitStatus {
        let deadline = Instant::now() + DEADLINE;
        loop {
            if let Some(status) = self.child.try_wait().unwrap() {
                return status;
            }
            assert!(Instant::now() < deadline, "actual application did not exit");
            thread::sleep(Duration::from_millis(10));
        }
    }

    pub fn report_window(&self) -> Option<HWND> {
        self.reporters.first().and_then(windows::Reporter::window)
    }

    #[cfg(feature = "test-faults")]
    pub fn close_report_window_after_parent_exit(&mut self) {
        assert!(self.child.try_wait().unwrap().is_some());
        let deadline = Instant::now() + DEADLINE;
        loop {
            assert!(
                !self.reporters[0].exited(),
                "independent reporter exited before presentation"
            );
            if let Some(window) = self.report_window() {
                self.reporters[0].close(window);
                return;
            }
            assert!(
                Instant::now() < deadline,
                "actual report window did not open after parent death"
            );
            thread::sleep(Duration::from_millis(10));
        }
    }

    pub fn wait_for_reporter_exit(&self) {
        let deadline = Instant::now() + DEADLINE;
        while !self.reporters[0].exited() {
            assert!(
                Instant::now() < deadline,
                "exact reporter survived shutdown"
            );
            thread::sleep(Duration::from_millis(10));
        }
    }
}

impl Drop for Application {
    fn drop(&mut self) {
        self.close_input();
        if self.child.try_wait().ok().flatten().is_none() {
            let _ = self.child.kill();
        }
        let deadline = Instant::now() + Duration::from_secs(5);
        while self.child.try_wait().ok().flatten().is_none() {
            if Instant::now() >= deadline {
                eprintln!(
                    "exact application cleanup deadline elapsed for retained process {}",
                    self.child.id()
                );
                break;
            }
            thread::sleep(Duration::from_millis(10));
        }
    }
}
