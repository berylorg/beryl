use std::{
    fs,
    io::Write,
    os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle},
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant},
};

use tempfile::TempDir;
use windows::Win32::{
    Foundation::{FILETIME, HANDLE, HWND, LPARAM, WAIT_OBJECT_0, WPARAM},
    System::Threading::{
        GetProcessTimes, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_SYNCHRONIZE,
        PROCESS_TERMINATE, TerminateProcess, WaitForSingleObject,
    },
    UI::WindowsAndMessaging::{FindWindowExW, GetWindowThreadProcessId, PostMessageW, WM_CLOSE},
};
use windows::core::w;

pub struct ReportProcess {
    child: Child,
    reporter: Option<OwnedHandle>,
    installation: String,
    directory: TempDir,
    gui: bool,
}

pub struct Outcome {
    pub success: bool,
    pub report: Option<String>,
    pub installation: String,
    pub continued: bool,
    pub gui_closed: bool,
}

impl ReportProcess {
    pub fn start(mode: &str, startup: Option<&str>) -> Self {
        let directory = tempfile::tempdir().unwrap();
        let mut command = Command::new(env!("CARGO_BIN_EXE_crash-report-fixture"));
        command
            .arg(mode)
            .env("BERYL_CRASH_REPORT_TEST_DIR", directory.path())
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(fs::File::create(directory.path().join("stderr")).unwrap());
        if let Some(startup) = startup {
            command.env("BERYL_CRASH_REPORT_TEST_STARTUP", startup);
        } else {
            command.env_remove("BERYL_CRASH_REPORT_TEST_STARTUP");
        }
        if mode == "gui" {
            command.env("BERYL_CRASH_REPORT_TEST_GUI", "1");
        } else {
            command.env_remove("BERYL_CRASH_REPORT_TEST_GUI");
        }
        let child = command.spawn().unwrap();
        let mut process = Self {
            child,
            reporter: None,
            installation: String::new(),
            directory,
            gui: mode == "gui",
        };
        let deadline = Instant::now() + Duration::from_secs(15);
        loop {
            if process.reporter.is_none() {
                process.reporter = process.open_reporter();
            }
            if let Ok(installation) =
                fs::read_to_string(process.directory.path().join("installation"))
            {
                if !installation.is_empty() {
                    process.installation = installation;
                    break;
                }
            }
            assert!(
                process.child.try_wait().unwrap().is_none(),
                "fixture exited before readiness: {}",
                process.stderr()
            );
            assert!(Instant::now() < deadline, "fixture readiness timed out");
            thread::sleep(Duration::from_millis(10));
        }
        if process.installation.contains("ready=true") {
            if process.reporter.is_none() {
                process.reporter = process.open_reporter();
            }
            assert!(
                process.reporter.is_some(),
                "ready reporter identity unavailable"
            );
        } else if startup.is_none() && mode != "job-refusal" {
            panic!(
                "reporter startup unexpectedly unavailable: {}; receiver: {}",
                process.installation,
                fs::read_to_string(process.directory.path().join("receiver-state"))
                    .unwrap_or_default()
            );
        }
        if startup == Some("timeout") {
            let handle = process
                .reporter
                .as_ref()
                .expect("timeout child was not observed");
            assert_eq!(
                unsafe { WaitForSingleObject(HANDLE(handle.as_raw_handle()), 0) },
                WAIT_OBJECT_0,
                "provisional child survived the reported startup timeout"
            );
        }
        process
    }

    pub fn stop_reporter(&mut self) {
        let handle = HANDLE(self.reporter.as_ref().unwrap().as_raw_handle());
        unsafe { TerminateProcess(handle, 3) }.unwrap();
        assert_eq!(unsafe { WaitForSingleObject(handle, 5_000) }, WAIT_OBJECT_0);
    }

    pub fn finish(mut self) -> Outcome {
        self.child.stdin.take().unwrap().write_all(b"x").unwrap();
        let deadline = Instant::now() + Duration::from_secs(15);
        let status = loop {
            if let Some(status) = self.child.try_wait().unwrap() {
                break status;
            }
            assert!(Instant::now() < deadline, "fixture exit timed out");
            thread::sleep(Duration::from_millis(10));
        };
        if self.gui {
            self.close_report_window();
        }
        if let Some(handle) = &self.reporter {
            assert_eq!(
                unsafe { WaitForSingleObject(HANDLE(handle.as_raw_handle()), 5_000) },
                WAIT_OBJECT_0,
                "reporter did not exit after its exact parent"
            );
        }
        Outcome {
            success: status.success(),
            report: fs::read_to_string(self.directory.path().join("report")).ok(),
            installation: self.installation.clone(),
            continued: self.directory.path().join("continued").exists(),
            gui_closed: self.directory.path().join("gui-closed").exists(),
        }
    }

    fn stderr(&self) -> String {
        fs::read_to_string(self.directory.path().join("stderr")).unwrap_or_default()
    }

    fn close_report_window(&self) {
        let identity = fs::read_to_string(self.directory.path().join("reporter-pid")).unwrap();
        let pid: u32 = identity.split_once(' ').unwrap().0.parse().unwrap();
        assert!(
            self.reporter.is_some(),
            "reporter identity must remain pinned"
        );
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            let mut previous = HWND::default();
            for _ in 0..1000 {
                let Ok(window) = (unsafe {
                    FindWindowExW(None, Some(previous), None, w!("Beryl — Internal error"))
                }) else {
                    break;
                };
                let mut owner = 0;
                unsafe {
                    GetWindowThreadProcessId(window, Some(&mut owner));
                }
                if owner == pid {
                    unsafe { PostMessageW(Some(window), WM_CLOSE, WPARAM(0), LPARAM(0)) }.unwrap();
                    return;
                }
                previous = window;
            }
            assert!(
                Instant::now() < deadline,
                "native report window did not open: {}",
                self.stderr()
            );
            thread::sleep(Duration::from_millis(10));
        }
    }

    fn open_reporter(&self) -> Option<OwnedHandle> {
        let identity = fs::read_to_string(self.directory.path().join("reporter-pid")).ok()?;
        let (pid, expected_created) = identity.split_once(' ')?;
        let pid = pid.parse().ok()?;
        let expected_created: u64 = expected_created.parse().ok()?;
        let handle = unsafe {
            OpenProcess(
                PROCESS_SYNCHRONIZE | PROCESS_TERMINATE | PROCESS_QUERY_LIMITED_INFORMATION,
                false,
                pid,
            )
        }
        .ok()?;
        let handle = unsafe { OwnedHandle::from_raw_handle(handle.0) };
        let mut created = FILETIME::default();
        let mut exited = FILETIME::default();
        let mut kernel = FILETIME::default();
        let mut user = FILETIME::default();
        unsafe {
            GetProcessTimes(
                HANDLE(handle.as_raw_handle()),
                &mut created,
                &mut exited,
                &mut kernel,
                &mut user,
            )
        }
        .ok()?;
        let created = (u64::from(created.dwHighDateTime) << 32) | u64::from(created.dwLowDateTime);
        (created == expected_created).then_some(handle)
    }
}

impl Drop for ReportProcess {
    fn drop(&mut self) {
        if self.child.try_wait().ok().flatten().is_none() {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
        if let Some(handle) = &self.reporter {
            let handle = HANDLE(handle.as_raw_handle());
            if unsafe { WaitForSingleObject(handle, 0) } != WAIT_OBJECT_0 {
                let _ = unsafe { TerminateProcess(handle, 1) };
                let _ = unsafe { WaitForSingleObject(handle, 5_000) };
            }
        }
    }
}
