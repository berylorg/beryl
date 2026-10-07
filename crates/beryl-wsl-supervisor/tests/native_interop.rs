#![cfg(target_os = "linux")]
#[path = "support/native_role.rs"]
mod support;
use beryl_wsl_supervisor::*;
use sha2::{Digest, Sha256};
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::os::unix::fs::MetadataExt;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};
use support::{RoleProcess, launch, retire};

fn address(cookie: &Nonce) -> (libc::sockaddr_un, libc::socklen_t) {
    let mut address: libc::sockaddr_un = unsafe { std::mem::zeroed() };
    address.sun_family = libc::AF_UNIX as u16;
    let mut name = String::from("beryl-native-interop-");
    for byte in Sha256::digest(cookie) {
        use std::fmt::Write;
        write!(name, "{byte:02x}").unwrap();
    }
    for (target, byte) in address.sun_path[1..].iter_mut().zip(name.bytes()) {
        *target = byte as libc::c_char;
    }
    (
        address,
        (std::mem::offset_of!(libc::sockaddr_un, sun_path) + 1 + name.len()) as libc::socklen_t,
    )
}
fn descriptor(fd: i32) -> OwnedFd {
    assert!(fd >= 0, "{}", std::io::Error::last_os_error());
    unsafe { OwnedFd::from_raw_fd(fd) }
}
fn ready(fd: i32, timeout: i32) -> bool {
    let mut poll = libc::pollfd {
        fd,
        events: libc::POLLIN,
        revents: 0,
    };
    let result = unsafe { libc::poll(&mut poll, 1, timeout) };
    assert!(result >= 0);
    result == 1 && poll.revents != 0
}
fn identity(fd: i32) -> i32 {
    std::fs::read_to_string(format!("/proc/self/fdinfo/{fd}"))
        .unwrap()
        .lines()
        .find_map(|line| {
            line.strip_prefix("Pid:")
                .and_then(|value| value.trim().parse().ok())
        })
        .unwrap()
}
fn cookie_argument() -> Nonce {
    let args: Vec<_> = std::env::args().collect();
    let text = args
        .windows(2)
        .find(|args| args[0] == "--skip")
        .map(|args| args[1].as_str())
        .unwrap();
    assert_eq!(text.len(), 64);
    let mut cookie = [0; 32];
    for (index, byte) in cookie.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&text[index * 2..index * 2 + 2], 16).unwrap();
    }
    cookie
}
struct Outside {
    pidfd: OwnedFd,
    peer: OwnedFd,
}
impl Outside {
    fn retire(&self) {
        if !ready(self.pidfd.as_raw_fd(), 0) {
            assert_eq!(
                unsafe {
                    libc::syscall(
                        libc::SYS_pidfd_send_signal,
                        self.pidfd.as_raw_fd(),
                        libc::SIGKILL,
                        std::ptr::null::<libc::siginfo_t>(),
                        0,
                    )
                },
                0
            );
        }
        assert!(
            ready(self.pidfd.as_raw_fd(), 5_000),
            "original outside fixture failed to exit"
        );
    }
}
impl Drop for Outside {
    fn drop(&mut self) {
        self.retire();
    }
}
#[test]
fn windows_interop_created_outside_work_survives_owned_namespace_retirement() {
    let cookie = [71; 32];
    let nonce = [72; 32];
    let listener = descriptor(unsafe {
        libc::socket(libc::AF_UNIX, libc::SOCK_SEQPACKET | libc::SOCK_CLOEXEC, 0)
    });
    let (address, length) = address(&cookie);
    assert_eq!(
        unsafe {
            libc::bind(
                listener.as_raw_fd(),
                (&address as *const libc::sockaddr_un).cast(),
                length,
            )
        },
        0
    );
    assert_eq!(unsafe { libc::listen(listener.as_raw_fd(), 1) }, 0);
    let mut text = String::new();
    for byte in cookie {
        use std::fmt::Write;
        write!(text, "{byte:02x}").unwrap();
    }
    let (broker, mut root) = launch(
        Frame::LaunchServer {
            executable: std::env::current_exe()
                .unwrap()
                .to_str()
                .unwrap()
                .to_owned(),
            execution_root: "/".into(),
            arguments: vec![
                "--ignored".into(),
                "--exact".into(),
                "interop_bridge_fixture".into(),
                "--nocapture".into(),
                "--skip".into(),
                text,
            ],
        },
        nonce,
    );
    assert_eq!(root.next(), Frame::WorkloadStarted);
    assert!(
        ready(listener.as_raw_fd(), 12_000),
        "outside fixture did not connect through Windows interoperability"
    );
    let peer = descriptor(unsafe {
        libc::accept4(
            listener.as_raw_fd(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            libc::SOCK_CLOEXEC,
        )
    });
    assert!(ready(peer.as_raw_fd(), 5_000));
    let mut payload = [0u8; 48];
    let mut ancillary = [0usize; 8];
    let mut iov = libc::iovec {
        iov_base: payload.as_mut_ptr().cast(),
        iov_len: payload.len(),
    };
    let mut message: libc::msghdr = unsafe { std::mem::zeroed() };
    message.msg_iov = &mut iov;
    message.msg_iovlen = 1;
    message.msg_control = ancillary.as_mut_ptr().cast();
    message.msg_controllen = std::mem::size_of_val(&ancillary) as _;
    assert_eq!(
        unsafe { libc::recvmsg(peer.as_raw_fd(), &mut message, libc::MSG_CMSG_CLOEXEC) },
        payload.len() as isize
    );
    let header = unsafe { libc::CMSG_FIRSTHDR(&message) };
    assert!(!header.is_null());
    assert_eq!(unsafe { (*header).cmsg_level }, libc::SOL_SOCKET);
    assert_eq!(unsafe { (*header).cmsg_type }, libc::SCM_RIGHTS);
    assert_eq!(
        unsafe { (*header).cmsg_len } as usize,
        unsafe { libc::CMSG_LEN(4) } as usize
    );
    let pidfd = descriptor(unsafe { std::ptr::read(libc::CMSG_DATA(header).cast::<i32>()) });
    assert_eq!(message.msg_flags & (libc::MSG_TRUNC | libc::MSG_CTRUNC), 0);
    assert_eq!(&payload[..32], &cookie);
    assert!(unsafe { libc::CMSG_NXTHDR(&message, header) }.is_null());
    let mut peer_pidfd = -1;
    let mut size = 4 as libc::socklen_t;
    assert_eq!(
        unsafe {
            libc::getsockopt(
                peer.as_raw_fd(),
                libc::SOL_SOCKET,
                77,
                (&mut peer_pidfd as *mut i32).cast(),
                &mut size,
            )
        },
        0
    );
    let peer_pidfd = descriptor(peer_pidfd);
    assert!(!ready(pidfd.as_raw_fd(), 0));
    assert!(!ready(peer_pidfd.as_raw_fd(), 0));
    assert_eq!(
        identity(pidfd.as_raw_fd()),
        identity(peer_pidfd.as_raw_fd())
    );
    let mut credentials: libc::ucred = unsafe { std::mem::zeroed() };
    let mut size = std::mem::size_of::<libc::ucred>() as libc::socklen_t;
    assert_eq!(
        unsafe {
            libc::getsockopt(
                peer.as_raw_fd(),
                libc::SOL_SOCKET,
                libc::SO_PEERCRED,
                (&mut credentials as *mut libc::ucred).cast(),
                &mut size,
            )
        },
        0
    );
    assert_eq!(
        u32::from_le_bytes(payload[40..44].try_into().unwrap()),
        credentials.uid
    );
    assert_eq!(
        u32::from_le_bytes(payload[44..48].try_into().unwrap()),
        credentials.gid
    );
    assert_eq!(
        u64::from_le_bytes(payload[32..40].try_into().unwrap()),
        std::fs::metadata("/proc/self/ns/pid").unwrap().ino(),
        "service-created fixture must be in the outside initial PID namespace"
    );
    assert!(!ready(pidfd.as_raw_fd(), 0));
    assert!(!ready(peer_pidfd.as_raw_fd(), 0));
    let outside = Outside { pidfd, peer };
    assert!(root.child.try_wait().unwrap().is_none());
    root.send(&nonce, &Frame::Stop);
    let (_, failures) = retire(broker, root);
    assert!(failures.is_empty(), "{failures:?}");
    assert!(
        !ready(outside.pidfd.as_raw_fd(), 0),
        "excluded service-created work must survive joined owned retirement"
    );
    outside.retire();
}
#[test]
#[ignore]
fn interop_bridge_fixture() {
    assert_eq!(
        std::env::var("BERYL_NATIVE_SENTINEL").unwrap(),
        "ordinary-context-value"
    );
    let windows = "/mnt/c/Windows/System32/wsl.exe";
    assert!(std::path::Path::new(windows).is_file());
    let distribution = std::env::var("WSL_DISTRO_NAME").unwrap();
    let binary = std::env::current_exe().unwrap();
    let args: Vec<_> = std::env::args().collect();
    let cookie = args
        .windows(2)
        .find(|args| args[0] == "--skip")
        .map(|args| args[1].clone())
        .unwrap();
    let child = RoleProcess::spawn_command(
        {
            let mut command = Command::new(windows);
            command
                .args([
                    "--distribution",
                    distribution.as_str(),
                    "--exec",
                    binary.to_str().unwrap(),
                    "--ignored",
                    "--exact",
                    "outside_producer_fixture",
                    "--skip",
                    cookie.as_str(),
                ])
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::null());
            command
        },
        [0; 32],
    );
    let mut child = child;
    child.join();
    loop {
        std::thread::sleep(Duration::from_secs(1));
    }
}
#[test]
#[ignore]
fn outside_producer_fixture() {
    let cookie = cookie_argument();
    unsafe {
        let child = libc::fork();
        assert!(child >= 0);
        if child != 0 {
            let mut status = 0;
            assert_eq!(libc::waitpid(child, &mut status, 0), child);
            return;
        }
        if libc::setsid() < 0 {
            libc::_exit(1);
        }
        let detached = libc::fork();
        if detached < 0 {
            libc::_exit(1);
        }
        if detached != 0 {
            libc::_exit(0);
        }
        if libc::syscall(libc::SYS_close_range, 0, u32::MAX, 0) != 0 {
            libc::_exit(1);
        }
        outside_child(cookie);
    }
}
fn outside_child(cookie: Nonce) -> ! {
    let socket = descriptor(unsafe {
        libc::socket(libc::AF_UNIX, libc::SOCK_SEQPACKET | libc::SOCK_CLOEXEC, 0)
    });
    let (address, length) = address(&cookie);
    if unsafe {
        libc::connect(
            socket.as_raw_fd(),
            (&address as *const libc::sockaddr_un).cast(),
            length,
        )
    } != 0
    {
        unsafe { libc::_exit(1) }
    }
    let pidfd =
        descriptor(unsafe { libc::syscall(libc::SYS_pidfd_open, libc::getpid(), 0) } as i32);
    let mut payload = [0u8; 48];
    payload[..32].copy_from_slice(&cookie);
    payload[32..40].copy_from_slice(
        &std::fs::metadata("/proc/self/ns/pid")
            .unwrap()
            .ino()
            .to_le_bytes(),
    );
    payload[40..44].copy_from_slice(&unsafe { libc::getuid() }.to_le_bytes());
    payload[44..48].copy_from_slice(&unsafe { libc::getgid() }.to_le_bytes());
    let mut ancillary = [0usize; 8];
    let mut iov = libc::iovec {
        iov_base: payload.as_mut_ptr().cast(),
        iov_len: payload.len(),
    };
    let mut message: libc::msghdr = unsafe { std::mem::zeroed() };
    message.msg_iov = &mut iov;
    message.msg_iovlen = 1;
    message.msg_control = ancillary.as_mut_ptr().cast();
    message.msg_controllen = unsafe { libc::CMSG_SPACE(4) } as _;
    unsafe {
        let header = libc::CMSG_FIRSTHDR(&message);
        (*header).cmsg_level = libc::SOL_SOCKET;
        (*header).cmsg_type = libc::SCM_RIGHTS;
        (*header).cmsg_len = libc::CMSG_LEN(4) as _;
        std::ptr::write(libc::CMSG_DATA(header).cast::<i32>(), pidfd.as_raw_fd());
    }
    if unsafe { libc::sendmsg(socket.as_raw_fd(), &message, libc::MSG_NOSIGNAL) }
        != payload.len() as isize
    {
        unsafe { libc::_exit(1) }
    }
    let deadline = Instant::now() + Duration::from_secs(20);
    while Instant::now() < deadline {
        if ready(socket.as_raw_fd(), 100) {
            let mut byte = 0u8;
            let count =
                unsafe { libc::recv(socket.as_raw_fd(), (&mut byte as *mut u8).cast(), 1, 0) };
            if count <= 0 {
                break;
            }
        }
    }
    unsafe { libc::_exit(0) }
}
