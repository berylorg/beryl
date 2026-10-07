#![cfg(target_os = "linux")]
#[path = "support/native_role.rs"]
mod support;
use beryl_wsl_supervisor::*;
use std::io;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::os::unix::process::CommandExt;
use std::time::{Duration, Instant};
use support::{RoleProcess, launch, retire};

fn server() -> Frame {
    Frame::LaunchServer {
        executable: "/bin/sleep".into(),
        execution_root: "/".into(),
        arguments: vec!["60".into()],
    }
}
fn read_root_closure(root: &mut RoleProcess) -> WorkloadResult {
    let mut result = None;
    loop {
        match root.next() {
            Frame::Failure { .. } | Frame::ShutdownPending => {}
            Frame::OwnedNamespaceClosed { result: closed } => {
                assert!(result.is_none());
                result = Some(closed);
            }
            Frame::LinuxCompanionsClosed => return result.unwrap(),
            event => panic!("unexpected lifecycle event {event:?}"),
        }
    }
}
fn namespace_pidfd(root: &RoleProcess) -> OwnedFd {
    let deadline = Instant::now() + Duration::from_secs(2);
    let children = format!(
        "/proc/{}/task/{}/children",
        root.child.id(),
        root.child.id()
    );
    loop {
        let mut root_poll = libc::pollfd {
            fd: root.pidfd.as_raw_fd(),
            events: libc::POLLIN,
            revents: 0,
        };
        assert_eq!(
            unsafe { libc::poll(&mut root_poll, 1, 0) },
            0,
            "original supervisor died before namespace capability capture"
        );
        let ids = std::fs::read_to_string(&children).unwrap_or_default();
        if let Some(pid) = ids.split_whitespace().next() {
            let pid: i32 = pid.parse().unwrap();
            let descriptor = unsafe { libc::syscall(libc::SYS_pidfd_open, pid, 0) } as i32;
            if descriptor >= 0 {
                let descriptor = unsafe { OwnedFd::from_raw_fd(descriptor) };
                let still_owned = std::fs::read_to_string(&children)
                    .unwrap_or_default()
                    .split_whitespace()
                    .any(|value| value == pid.to_string());
                let mut init_poll = libc::pollfd {
                    fd: descriptor.as_raw_fd(),
                    events: libc::POLLIN,
                    revents: 0,
                };
                if still_owned
                    && unsafe { libc::poll(&mut root_poll, 1, 0) } == 0
                    && unsafe { libc::poll(&mut init_poll, 1, 0) } == 0
                {
                    return descriptor;
                }
            }
        }
        assert!(
            Instant::now() < deadline,
            "original namespace init was not visible"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
}
fn signal(fd: &OwnedFd, value: i32) {
    assert_eq!(
        unsafe {
            libc::syscall(
                libc::SYS_pidfd_send_signal,
                fd.as_raw_fd(),
                value,
                std::ptr::null::<libc::siginfo_t>(),
                0,
            )
        },
        0
    );
}
fn wait_pidfd(fd: &OwnedFd) {
    let mut poll = libc::pollfd {
        fd: fd.as_raw_fd(),
        events: libc::POLLIN,
        revents: 0,
    };
    assert_eq!(unsafe { libc::poll(&mut poll, 1, 6_000) }, 1);
    assert_ne!(poll.revents & libc::POLLIN, 0);
}
#[test]
fn broker_death_and_broker_channel_loss_retire_the_owned_namespace() {
    for (nonce, killed) in [([31; 32], true), ([32; 32], false)] {
        let (mut broker, mut root) = launch(server(), nonce);
        assert_eq!(root.next(), Frame::WorkloadStarted);
        if killed {
            broker.signal(libc::SIGKILL);
        } else {
            broker.stdin.take();
        }
        read_root_closure(&mut root);
        root.join();
        if killed {
            broker.join_failed();
        } else {
            assert_eq!(broker.next(), Frame::LinuxCompanionsClosed);
            broker.join();
        }
    }
}
#[test]
fn init_death_has_positive_namespace_reaping_evidence() {
    let (broker, mut root) = launch(server(), [33; 32]);
    assert_eq!(root.next(), Frame::WorkloadStarted);
    let init = namespace_pidfd(&root);
    signal(&init, libc::SIGKILL);
    let (result, _) = retire(broker, root);
    assert_eq!(result.exit, ExitStatus::Signaled(libc::SIGKILL));
    wait_pidfd(&init);
}
#[test]
fn supervisor_death_uses_stable_parent_death_handling() {
    let (mut broker, mut root) = launch(server(), [34; 32]);
    assert_eq!(root.next(), Frame::WorkloadStarted);
    let init = namespace_pidfd(&root);
    root.signal(libc::SIGKILL);
    root.join_failed();
    wait_pidfd(&init);
    assert_eq!(broker.next(), Frame::LinuxCompanionsClosed);
    broker.join();
    assert_eq!(root.reader.read_frame(&mut root.stdout).unwrap(), None);
}
#[test]
fn deadline_expiry_retains_broker_and_retry_uses_the_original_owner() {
    let nonce = [35; 32];
    let (mut broker, mut root) = launch(server(), nonce);
    assert_eq!(root.next(), Frame::WorkloadStarted);
    broker.signal(libc::SIGSTOP);
    root.send(&nonce, &Frame::Stop);
    let result = match root.next() {
        Frame::OwnedNamespaceClosed { result } => result,
        event => panic!("{event:?}"),
    };
    assert_eq!(root.next(), Frame::ShutdownPending);
    assert!(root.child.try_wait().unwrap().is_none());
    root.send(&nonce, &Frame::Stop);
    assert_eq!(root.next(), Frame::ShutdownPending);
    assert!(root.child.try_wait().unwrap().is_none());
    broker.signal(libc::SIGCONT);
    assert_eq!(root.next(), Frame::LinuxCompanionsClosed);
    assert_eq!(broker.next(), Frame::LinuxCompanionsClosed);
    root.join();
    broker.join();
    assert!(result.observation.is_none());
}
#[test]
fn stale_nonce_and_illegal_requests_refuse_without_another_workload() {
    for (nonce, foreign) in [([36; 32], true), ([37; 32], false)] {
        let (broker, mut root) = launch(server(), nonce);
        assert_eq!(root.next(), Frame::WorkloadStarted);
        root.send(
            &if foreign { [99; 32] } else { nonce },
            &if foreign {
                Frame::Stop
            } else {
                Frame::Initialize
            },
        );
        assert!(matches!(
            root.next(),
            Frame::Failure {
                kind: FailureKind::Protocol,
                ..
            }
        ));
        let (_, failures) = retire(broker, root);
        assert!(failures.is_empty());
    }
}
fn filtered_supervisor(nonce: Nonce, syscall: i64, verdict: u32) -> RoleProcess {
    let mut command = RoleProcess::companion_command("supervise");
    unsafe {
        command.pre_exec(move || {
            let mut filter = [
                libc::sock_filter {
                    code: (libc::BPF_LD | libc::BPF_W | libc::BPF_ABS) as u16,
                    jt: 0,
                    jf: 0,
                    k: 0,
                },
                libc::sock_filter {
                    code: (libc::BPF_JMP | libc::BPF_JEQ | libc::BPF_K) as u16,
                    jt: 0,
                    jf: 1,
                    k: syscall as u32,
                },
                libc::sock_filter {
                    code: (libc::BPF_RET | libc::BPF_K) as u16,
                    jt: 0,
                    jf: 0,
                    k: verdict,
                },
                libc::sock_filter {
                    code: (libc::BPF_RET | libc::BPF_K) as u16,
                    jt: 0,
                    jf: 0,
                    k: libc::SECCOMP_RET_ALLOW,
                },
            ];
            let program = libc::sock_fprog {
                len: filter.len() as u16,
                filter: filter.as_mut_ptr(),
            };
            if libc::prctl(libc::PR_SET_NO_NEW_PRIVS, 1, 0, 0, 0) != 0
                || libc::prctl(libc::PR_SET_SECCOMP, libc::SECCOMP_MODE_FILTER, &program) != 0
            {
                return Err(io::Error::last_os_error());
            }
            Ok(())
        });
    }
    RoleProcess::spawn_command(command, nonce)
}
#[test]
fn credential_failure_and_preexec_death_never_claim_exec_success() {
    for (nonce, syscall, verdict, expected) in [
        (
            [38; 32],
            libc::SYS_setgroups,
            libc::SECCOMP_RET_ERRNO | libc::EPERM as u32,
            FailureKind::Credentials,
        ),
        (
            [39; 32],
            libc::SYS_setresuid,
            libc::SECCOMP_RET_KILL_PROCESS,
            FailureKind::Protocol,
        ),
    ] {
        let mut broker = RoleProcess::spawn("context-broker", nonce);
        broker.send(&nonce, &Frame::Initialize);
        assert_eq!(
            broker.next(),
            Frame::Ready {
                role: Role::ContextBroker
            }
        );
        let mut root = filtered_supervisor(nonce, syscall, verdict);
        root.send(&nonce, &server());
        assert_eq!(
            root.next(),
            Frame::Ready {
                role: Role::Supervisor
            }
        );
        assert!(matches!(root.next(),Frame::Failure{kind,..} if kind==expected));
        let (_, failures) = retire(broker, root);
        assert!(failures.is_empty(), "{failures:?}");
    }
}
#[test]
fn namespace_construction_failure_closes_the_unreleased_boundary() {
    let nonce = [40; 32];
    let mut broker = RoleProcess::spawn("context-broker", nonce);
    broker.send(&nonce, &Frame::Initialize);
    assert_eq!(
        broker.next(),
        Frame::Ready {
            role: Role::ContextBroker
        }
    );
    let mut root = filtered_supervisor(
        nonce,
        libc::SYS_clone3,
        libc::SECCOMP_RET_ERRNO | libc::EPERM as u32,
    );
    root.send(&nonce, &server());
    assert_eq!(
        root.next(),
        Frame::Ready {
            role: Role::Supervisor
        }
    );
    assert!(matches!(
        root.next(),
        Frame::Failure {
            kind: FailureKind::Namespace,
            errno: Some(libc::EPERM)
        }
    ));
    let (result, failures) = retire(broker, root);
    assert_eq!(result.exit, ExitStatus::Exited(1));
    assert!(failures.is_empty());
}
