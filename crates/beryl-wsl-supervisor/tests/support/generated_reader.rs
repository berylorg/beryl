#![allow(dead_code)]
use super::support::{RoleProcess, launch};
use beryl_wsl_supervisor::*;
use std::fs::File;
use std::io::Write;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

static NEXT_FIXTURE: AtomicU64 = AtomicU64::new(1);

pub struct Fixture {
    pub root: PathBuf,
}
impl Fixture {
    pub fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "beryl-generated-reader-{}-{}",
            std::process::id(),
            NEXT_FIXTURE.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&root).unwrap();
        std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o755)).unwrap();
        Self { root }
    }
    pub fn file(&self, name: &str, bytes: &[u8]) -> PathBuf {
        let path = self.root.join(name);
        let mut file = File::create_new(&path).unwrap();
        file.write_all(bytes).unwrap();
        path
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        for entry in std::fs::read_dir(&self.root).unwrap() {
            std::fs::remove_file(entry.unwrap().path()).unwrap();
        }
        std::fs::remove_dir(&self.root).unwrap();
    }
}

pub fn server(nonce: Nonce, limit: u32) -> (RoleProcess, RoleProcess) {
    let (broker, mut root) = launch(request(), nonce);
    assert_eq!(root.next(), Frame::WorkloadStarted);
    root.send(&nonce, &Frame::ConfigureReaders { limit });
    (broker, root)
}

pub fn request() -> Frame {
    Frame::LaunchServer {
        executable: "/bin/sleep".into(),
        execution_root: "/".into(),
        arguments: vec!["60".into()],
    }
}

pub fn credential_server(nonce: Nonce) -> (RoleProcess, RoleProcess) {
    let mut command = RoleProcess::companion_command("context-broker");
    unsafe {
        command.pre_exec(|| {
            let groups = [62003u32];
            if libc::setgroups(groups.len(), groups.as_ptr()) != 0
                || libc::setgid(62002) != 0
                || libc::setuid(62001) != 0
            {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
    let mut broker = RoleProcess::spawn_command(command, nonce);
    broker.send(&nonce, &Frame::Initialize);
    assert_eq!(
        broker.next(),
        Frame::Ready {
            role: Role::ContextBroker
        }
    );
    let mut root = RoleProcess::spawn("supervise", nonce);
    root.send(&nonce, &request());
    assert_eq!(
        root.next(),
        Frame::Ready {
            role: Role::Supervisor
        }
    );
    assert_eq!(root.next(), Frame::WorkloadStarted);
    root.send(&nonce, &Frame::ConfigureReaders { limit: 1 });
    (broker, root)
}

pub fn id(number: u128) -> ReaderId {
    number.to_be_bytes()
}

pub fn open(root: &mut RoleProcess, nonce: Nonce, number: u128, path: &Path, length: u64) {
    root.send(
        &nonce,
        &Frame::ReaderOpen {
            id: id(number),
            path: path.to_str().unwrap().to_owned(),
        },
    );
    assert_eq!(
        root.next(),
        Frame::ReaderOpened {
            id: id(number),
            length
        }
    );
}

pub fn close(root: &mut RoleProcess, nonce: Nonce, number: u128, sequence: u64, cancelled: bool) {
    let frame = if cancelled {
        Frame::ReaderCancel {
            id: id(number),
            sequence,
        }
    } else {
        Frame::ReaderClose {
            id: id(number),
            sequence,
        }
    };
    root.send(&nonce, &frame);
    assert_eq!(
        root.next(),
        Frame::ReaderClosed {
            id: id(number),
            sequence,
            cancelled
        }
    );
}

pub fn failed(root: &mut RoleProcess, number: u128, sequence: u64, kind: ReaderFailure) {
    assert_eq!(
        root.next(),
        Frame::ReaderFailed {
            id: id(number),
            sequence,
            kind
        }
    );
    assert_eq!(
        root.next(),
        Frame::ReaderClosed {
            id: id(number),
            sequence,
            cancelled: true
        }
    );
}

pub fn children(pid: u32) -> Vec<u32> {
    std::fs::read_to_string(format!("/proc/{pid}/task/{pid}/children"))
        .unwrap()
        .split_whitespace()
        .map(|pid| pid.parse().unwrap())
        .collect()
}

pub fn pidfd(pid: u32) -> OwnedFd {
    let fd = unsafe { libc::syscall(libc::SYS_pidfd_open, pid, 0) } as i32;
    assert!(fd >= 0, "{}", std::io::Error::last_os_error());
    unsafe { OwnedFd::from_raw_fd(fd) }
}

pub fn reader_process(root: &RoleProcess) -> (u32, OwnedFd, u32) {
    let original_init = children(root.child.id());
    assert_eq!(original_init.len(), 1);
    let init = original_init[0];
    let reader = children(init)
        .into_iter()
        .find(|pid| {
            std::fs::read_link(format!("/proc/{pid}/exe"))
                .unwrap()
                .file_name()
                .unwrap()
                == "beryl-wsl-supervisor"
        })
        .expect("fixed reader child in original namespace");
    (reader, pidfd(reader), init)
}

pub fn signal(fd: &OwnedFd, signal: i32) {
    assert_eq!(
        unsafe {
            libc::syscall(
                libc::SYS_pidfd_send_signal,
                fd.as_raw_fd(),
                signal,
                std::ptr::null::<libc::siginfo_t>(),
                0,
            )
        },
        0
    );
}

pub fn exited(fd: &OwnedFd, timeout: Duration) -> bool {
    let mut poll = libc::pollfd {
        fd: fd.as_raw_fd(),
        events: libc::POLLIN,
        revents: 0,
    };
    let result = unsafe { libc::poll(&mut poll, 1, timeout.as_millis() as i32) };
    assert!(result >= 0);
    result == 1 && poll.revents & libc::POLLIN != 0
}
