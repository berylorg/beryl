#![allow(dead_code)]
use beryl_wsl_supervisor::*;
use std::io::{self, Read, Write};
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::os::unix::ffi::OsStringExt;
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::time::{Duration, Instant};

struct Timed<'a> {
    pipe: &'a mut ChildStdout,
    deadline: Instant,
}
impl Read for Timed<'_> {
    fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        let remaining = self
            .deadline
            .checked_duration_since(Instant::now())
            .ok_or_else(|| io::Error::from(io::ErrorKind::TimedOut))?;
        let mut poll = libc::pollfd {
            fd: self.pipe.as_raw_fd(),
            events: libc::POLLIN,
            revents: 0,
        };
        let result = unsafe {
            libc::poll(
                &mut poll,
                1,
                remaining.as_millis().min(i32::MAX as u128) as i32,
            )
        };
        if result == 0 {
            return Err(io::Error::from(io::ErrorKind::TimedOut));
        }
        if result < 0 {
            return Err(io::Error::last_os_error());
        }
        self.pipe.read(bytes)
    }
}
pub struct RoleProcess {
    pub child: Child,
    pub stdin: Option<ChildStdin>,
    pub stdout: ChildStdout,
    pub reader: FrameReader,
    pub pidfd: OwnedFd,
}
impl RoleProcess {
    pub fn spawn(mode: &str, nonce: Nonce) -> Self {
        let companion = std::env::current_exe()
            .unwrap()
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .join("beryl-wsl-supervisor");
        assert!(
            companion.is_file(),
            "Cargo-built Linux companion must accompany integration tests: {} (build descriptor {})",
            companion.display(),
            env!("CARGO_BIN_EXE_beryl-wsl-supervisor")
        );
        let mut command = Command::new(companion);
        command
            .arg(mode)
            .env("BERYL_NATIVE_SENTINEL", "ordinary-context-value")
            .env(
                "BERYL_NATIVE_BYTES",
                std::ffi::OsString::from_vec(vec![0xfe, 0xff, b'X']),
            )
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        if mode == "supervise" {
            command
                .env("BERYL_NATIVE_SENTINEL", "root-bootstrap-value")
                .env(
                    "BERYL_NATIVE_BYTES",
                    std::ffi::OsString::from_vec(vec![b'R']),
                )
                .env("HOME", "/beryl-root-bootstrap-not-a-user-home")
                .env(
                    "WSL_INTEROP",
                    "/beryl-root-bootstrap-not-an-interop-endpoint",
                );
        }
        Self::spawn_command(command, nonce)
    }
    pub fn spawn_command(mut command: Command, nonce: Nonce) -> Self {
        let mut child = command.spawn().unwrap();
        let fd = unsafe { libc::syscall(libc::SYS_pidfd_open, child.id(), 0) } as i32;
        assert!(fd >= 0);
        Self {
            stdin: child.stdin.take(),
            stdout: child.stdout.take().unwrap(),
            reader: FrameReader::new(nonce),
            pidfd: unsafe { OwnedFd::from_raw_fd(fd) },
            child,
        }
    }
    pub fn send(&mut self, nonce: &Nonce, frame: &Frame) {
        self.stdin
            .as_mut()
            .unwrap()
            .write_all(&encode_frame(nonce, frame).unwrap())
            .unwrap();
    }
    pub fn next(&mut self) -> Frame {
        self.reader
            .read_frame(Timed {
                pipe: &mut self.stdout,
                deadline: Instant::now() + Duration::from_secs(12),
            })
            .unwrap()
            .expect("exact role closure record required")
    }
    pub fn join(&mut self) {
        let deadline = Instant::now() + Duration::from_secs(6);
        loop {
            if let Some(status) = self.child.try_wait().unwrap() {
                assert!(status.success());
                return;
            }
            assert!(
                Instant::now() < deadline,
                "companion must join after Linux closure"
            );
            std::thread::sleep(Duration::from_millis(10));
        }
    }
}
impl Drop for RoleProcess {
    fn drop(&mut self) {
        if self.child.try_wait().ok().flatten().is_none() {
            unsafe {
                libc::syscall(
                    libc::SYS_pidfd_send_signal,
                    self.pidfd.as_raw_fd(),
                    libc::SIGKILL,
                    std::ptr::null::<libc::siginfo_t>(),
                    0,
                );
            }
        }
        let _ = self.child.wait();
    }
}
pub fn launch(request: Frame, nonce: Nonce) -> (RoleProcess, RoleProcess) {
    assert_eq!(
        unsafe { libc::geteuid() },
        0,
        "native namespace tests require exact WSL root runner"
    );
    let mut broker = RoleProcess::spawn("context-broker", nonce);
    broker.send(&nonce, &Frame::Initialize);
    assert_eq!(
        broker.next(),
        Frame::Ready {
            role: Role::ContextBroker
        }
    );
    let mut root = RoleProcess::spawn("supervise", nonce);
    root.send(&nonce, &request);
    assert_eq!(
        root.next(),
        Frame::Ready {
            role: Role::Supervisor
        }
    );
    (broker, root)
}
pub fn retire(
    mut broker: RoleProcess,
    mut root: RoleProcess,
) -> (WorkloadResult, Vec<FailureKind>) {
    let mut result = None;
    let mut failures = Vec::new();
    loop {
        match root.next() {
            Frame::ShutdownPending => {}
            Frame::Failure { kind, .. } => failures.push(kind),
            Frame::OwnedNamespaceClosed { result: closed } => {
                assert!(result.is_none());
                result = Some(closed);
            }
            Frame::LinuxCompanionsClosed => break,
            event => panic!("unexpected event {event:?}"),
        }
    }
    assert_eq!(broker.next(), Frame::LinuxCompanionsClosed);
    root.join();
    broker.join();
    (result.unwrap(), failures)
}

impl RoleProcess {
    pub fn signal(&self, signal: i32) {
        assert_eq!(
            unsafe {
                libc::syscall(
                    libc::SYS_pidfd_send_signal,
                    self.pidfd.as_raw_fd(),
                    signal,
                    std::ptr::null::<libc::siginfo_t>(),
                    0,
                )
            },
            0
        );
    }
    pub fn join_failed(&mut self) {
        let deadline = Instant::now() + Duration::from_secs(6);
        loop {
            if let Some(status) = self.child.try_wait().unwrap() {
                assert!(!status.success());
                return;
            }
            assert!(Instant::now() < deadline, "failed role did not join");
            std::thread::sleep(Duration::from_millis(10));
        }
    }
    pub fn companion_command(mode: &str) -> Command {
        let path = std::env::current_exe()
            .unwrap()
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .join("beryl-wsl-supervisor");
        assert!(path.is_file());
        let mut command = Command::new(path);
        command
            .arg(mode)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        command
    }
}
