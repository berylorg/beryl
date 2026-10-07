#![cfg(target_os = "linux")]
#[path = "support/native_role.rs"]
mod support;
use beryl_wsl_supervisor::*;
use std::io::Write;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::process::{Command, Stdio};
use std::time::Duration;
use support::{RoleProcess, launch, retire};

#[test]
fn ordinary_context_and_fixed_home_observation_join_all_roles() {
    let nonce = [21; 32];
    let (broker, mut root) = launch(
        Frame::Observe {
            kind: ObservationKind::Home,
            path: None,
            timeout_ms: 10_000,
        },
        nonce,
    );
    assert_eq!(root.next(), Frame::WorkloadStarted);
    let (result, failures) = retire(broker, root);
    assert!(failures.is_empty(), "{failures:?}");
    assert_eq!(result.exit, ExitStatus::Exited(0));
    assert_eq!(
        result.observation,
        Some(
            std::fs::canonicalize(std::env::var_os("HOME").unwrap())
                .unwrap()
                .to_str()
                .unwrap()
                .to_owned()
        )
    );
}
#[test]
fn exec_and_chdir_failures_never_claim_workload_started() {
    for (nonce, executable, execution_root, expected) in [
        (
            [22; 32],
            "/beryl-native-definitely-missing",
            "/",
            FailureKind::Exec,
        ),
        (
            [23; 32],
            "/bin/true",
            "/beryl-native-definitely-missing",
            FailureKind::WorkingDirectory,
        ),
    ] {
        let (broker, mut root) = launch(
            Frame::LaunchServer {
                executable: executable.into(),
                execution_root: execution_root.into(),
                arguments: vec![],
            },
            nonce,
        );
        let event = root.next();
        assert!(
            matches!(event,Frame::Failure{kind,..} if kind==expected),
            "{event:?}"
        );
        let (result, failures) = retire(broker, root);
        assert!(result.observation.is_none());
        assert!(!failures.contains(&FailureKind::Observation));
    }
}
#[test]
fn detached_double_fork_and_nested_descendants_close_with_the_original_namespace() {
    let mut unrelated_child = Command::new("/bin/sleep")
        .arg("60")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let unrelated_fd =
        unsafe { libc::syscall(libc::SYS_pidfd_open, unrelated_child.id(), 0) } as i32;
    assert!(unrelated_fd >= 0);
    let mut unrelated = RoleProcess {
        stdin: None,
        stdout: unrelated_child.stdout.take().unwrap(),
        reader: FrameReader::new([0; 32]),
        pidfd: unsafe { OwnedFd::from_raw_fd(unrelated_fd) },
        child: unrelated_child,
    };
    let executable = std::env::current_exe()
        .unwrap()
        .to_str()
        .unwrap()
        .to_owned();
    let nonce = [24; 32];
    let (broker, mut root) = launch(
        Frame::LaunchServer {
            executable,
            execution_root: "/".into(),
            arguments: vec![
                "--ignored".into(),
                "--exact".into(),
                "workload_descendant_fixture".into(),
                "--nocapture".into(),
            ],
        },
        nonce,
    );
    assert_eq!(root.next(), Frame::WorkloadStarted);
    // The fixture creates detached and nested descendants before retirement.
    std::thread::sleep(Duration::from_millis(250));
    root.send(&nonce, &Frame::Stop);
    root.send(&nonce, &Frame::Stop);
    let (result, failures) = retire(broker, root);
    assert!(failures.is_empty(), "{failures:?}");
    let output = String::from_utf8_lossy(&result.stdout.bytes);
    assert!(output.contains("ordinary-context-value"), "{output}");
    assert!(output.contains("descendants-ready"), "{output}");
    assert!(
        result
            .stdout
            .bytes
            .windows(8)
            .any(|bytes| bytes == b"BRYLWSL1"),
        "CLI bytes must be retained only as diagnostic output"
    );
    assert!(
        unrelated.child.try_wait().unwrap().is_none(),
        "unrelated process must survive namespace retirement"
    );
    assert_eq!(
        unsafe {
            libc::syscall(
                libc::SYS_pidfd_send_signal,
                unrelated.pidfd.as_raw_fd(),
                0,
                std::ptr::null::<libc::siginfo_t>(),
                0,
            )
        },
        0
    );
}
#[test]
fn lost_control_disposes_namespace_and_broker() {
    let nonce = [25; 32];
    let (broker, mut root) = launch(
        Frame::LaunchServer {
            executable: "/bin/sleep".into(),
            execution_root: "/".into(),
            arguments: vec!["60".into()],
        },
        nonce,
    );
    assert_eq!(root.next(), Frame::WorkloadStarted);
    root.stdin.take();
    let (_, failures) = retire(broker, root);
    assert!(failures.is_empty(), "{failures:?}");
}
#[test]
fn broker_only_close_is_exact_and_idempotent() {
    let nonce = [26; 32];
    let mut broker = RoleProcess::spawn("context-broker", nonce);
    broker.send(&nonce, &Frame::Initialize);
    assert_eq!(
        broker.next(),
        Frame::Ready {
            role: Role::ContextBroker
        }
    );
    broker.send(&nonce, &Frame::BrokerClose);
    broker.send(&nonce, &Frame::BrokerClose);
    assert_eq!(broker.next(), Frame::LinuxCompanionsClosed);
    broker.join();
}
#[test]
fn ordinary_and_abnormal_server_exit_dispose_all_roles() {
    for (nonce, executable, arguments, expected) in [
        (
            [27; 32],
            "/bin/true".to_owned(),
            vec![],
            ExitStatus::Exited(0),
        ),
        (
            [28; 32],
            std::env::current_exe()
                .unwrap()
                .to_str()
                .unwrap()
                .to_owned(),
            vec![
                "--ignored".into(),
                "--exact".into(),
                "abnormal_exit_fixture".into(),
            ],
            ExitStatus::Exited(47),
        ),
    ] {
        let (broker, mut root) = launch(
            Frame::LaunchServer {
                executable,
                execution_root: "/".into(),
                arguments,
            },
            nonce,
        );
        assert_eq!(root.next(), Frame::WorkloadStarted);
        let (result, failures) = retire(broker, root);
        assert_eq!(result.exit, expected);
        assert!(failures.is_empty());
    }
}
#[test]
#[ignore]
fn abnormal_exit_fixture() {
    std::process::exit(47);
}
#[test]
#[ignore]
fn workload_descendant_fixture() {
    use std::os::unix::ffi::OsStrExt;
    assert_eq!(
        std::env::var("BERYL_NATIVE_SENTINEL").unwrap(),
        "ordinary-context-value"
    );
    assert_eq!(
        std::env::var_os("BERYL_NATIVE_BYTES").unwrap().as_bytes(),
        &[0xfe, 0xff, b'X']
    );
    assert_eq!(unsafe { libc::getpid() }, 2);
    for entry in std::fs::read_dir("/proc/self/fd").unwrap() {
        let target = std::fs::read_link(entry.unwrap().path()).unwrap_or_default();
        let target = target.to_string_lossy();
        assert!(
            !target.contains("pidfd") && !target.starts_with("socket:"),
            "privileged descriptor inherited: {target}"
        );
    }
    std::io::stdout()
        .write_all(&encode_frame(&[24; 32], &Frame::WorkloadStarted).unwrap())
        .unwrap();
    println!(
        "ordinary-context-value uid={} gid={}",
        unsafe { libc::getuid() },
        unsafe { libc::getgid() }
    );
    unsafe {
        let child = libc::fork();
        assert!(child >= 0);
        if child == 0 {
            assert!(libc::setsid() >= 0);
            let detached = libc::fork();
            assert!(detached >= 0);
            if detached == 0 {
                libc::signal(libc::SIGTERM, libc::SIG_IGN);
                loop {
                    libc::pause();
                }
            }
            libc::_exit(0);
        }
        let mut status = 0;
        assert_eq!(libc::waitpid(child, &mut status, 0), child);
        let nested = libc::fork();
        assert!(nested >= 0);
        if nested == 0 {
            assert_eq!(libc::unshare(libc::CLONE_NEWPID), 0);
            let member = libc::fork();
            assert!(member >= 0);
            if member == 0 {
                loop {
                    libc::pause();
                }
            }
            loop {
                libc::pause();
            }
        }
    }
    println!("descendants-ready");
    std::io::stdout().flush().unwrap();
    loop {
        std::thread::sleep(Duration::from_secs(1));
    }
}
