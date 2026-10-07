#![cfg(target_os = "linux")]
#[path = "support/native_role.rs"]
mod support;
use beryl_wsl_supervisor::*;
use sha2::{Digest, Sha256};
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use support::RoleProcess;

fn socket_address(nonce: &Nonce) -> (libc::sockaddr_un, libc::socklen_t) {
    let mut address: libc::sockaddr_un = unsafe { std::mem::zeroed() };
    address.sun_family = libc::AF_UNIX as u16;
    let mut name = String::from("beryl-wsl-");
    for byte in Sha256::digest(nonce) {
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
fn context(nonce: &Nonce, forge_uid: bool, forge_groups: bool) -> Vec<u8> {
    let count = unsafe { libc::getgroups(0, std::ptr::null_mut()) };
    assert!(count >= 0);
    let mut groups = vec![0u32; count as usize];
    assert_eq!(
        unsafe { libc::getgroups(count, groups.as_mut_ptr()) },
        count
    );
    if forge_groups {
        groups.push(u32::MAX);
    }
    let uid = unsafe { libc::getuid() };
    let gid = unsafe { libc::getgid() };
    let mut bytes = nonce.to_vec();
    for value in [
        if forge_uid { uid.wrapping_add(1) } else { uid },
        gid,
        groups.len() as u32,
    ] {
        bytes.extend_from_slice(&value.to_le_bytes());
    }
    for group in groups {
        bytes.extend_from_slice(&group.to_le_bytes());
    }
    bytes.extend_from_slice(&0u32.to_le_bytes());
    bytes
}
fn forged_peer(
    nonce: Nonce,
    mut bytes: Vec<u8>,
    descriptors: Vec<OwnedFd>,
) -> std::thread::JoinHandle<()> {
    let listener_fd =
        unsafe { libc::socket(libc::AF_UNIX, libc::SOCK_SEQPACKET | libc::SOCK_CLOEXEC, 0) };
    assert!(listener_fd >= 0);
    let listener = unsafe { OwnedFd::from_raw_fd(listener_fd) };
    let (address, length) = socket_address(&nonce);
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
    std::thread::spawn(move || {
        let mut poll = libc::pollfd {
            fd: listener.as_raw_fd(),
            events: libc::POLLIN,
            revents: 0,
        };
        assert_eq!(unsafe { libc::poll(&mut poll, 1, 5_000) }, 1);
        let peer_fd = unsafe {
            libc::accept4(
                listener.as_raw_fd(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                libc::SOCK_CLOEXEC,
            )
        };
        assert!(peer_fd >= 0);
        let peer = unsafe { OwnedFd::from_raw_fd(peer_fd) };
        let mut ancillary = vec![0usize; 80];
        let mut iov = libc::iovec {
            iov_base: bytes.as_mut_ptr().cast(),
            iov_len: bytes.len(),
        };
        let mut message: libc::msghdr = unsafe { std::mem::zeroed() };
        message.msg_iov = &mut iov;
        message.msg_iovlen = 1;
        if !descriptors.is_empty() {
            message.msg_control = ancillary.as_mut_ptr().cast();
            message.msg_controllen =
                unsafe { libc::CMSG_SPACE((descriptors.len() * 4) as u32) } as _;
            unsafe {
                let header = libc::CMSG_FIRSTHDR(&message);
                (*header).cmsg_level = libc::SOL_SOCKET;
                (*header).cmsg_type = libc::SCM_RIGHTS;
                (*header).cmsg_len = libc::CMSG_LEN((descriptors.len() * 4) as u32) as _;
                for (index, descriptor) in descriptors.iter().enumerate() {
                    std::ptr::write(
                        libc::CMSG_DATA(header).cast::<i32>().add(index),
                        descriptor.as_raw_fd(),
                    );
                }
            }
        }
        assert_eq!(
            unsafe { libc::sendmsg(peer.as_raw_fd(), &message, libc::MSG_NOSIGNAL) },
            bytes.len() as isize
        );
        // A rejected peer receives no original nonce acknowledgement.
        let mut poll = libc::pollfd {
            fd: peer.as_raw_fd(),
            events: libc::POLLIN,
            revents: 0,
        };
        assert_eq!(unsafe { libc::poll(&mut poll, 1, 5_000) }, 1);
        let mut received = [0u8; 32];
        assert_eq!(
            unsafe {
                libc::recv(
                    peer.as_raw_fd(),
                    received.as_mut_ptr().cast(),
                    received.len(),
                    0,
                )
            },
            0
        );
    })
}
fn self_pidfd() -> OwnedFd {
    let fd = unsafe { libc::syscall(libc::SYS_pidfd_open, libc::getpid(), 0) } as i32;
    assert!(fd >= 0);
    unsafe { OwnedFd::from_raw_fd(fd) }
}
fn refusal(nonce: Nonce, bytes: Vec<u8>, descriptors: Vec<OwnedFd>) {
    let peer = forged_peer(nonce, bytes, descriptors);
    let mut root = RoleProcess::spawn("supervise", nonce);
    root.send(
        &nonce,
        &Frame::LaunchServer {
            executable: "/bin/sleep".into(),
            execution_root: "/".into(),
            arguments: vec!["60".into()],
        },
    );
    assert!(matches!(
        root.next(),
        Frame::Failure {
            kind: FailureKind::Context,
            ..
        }
    ));
    root.join_failed();
    assert_eq!(root.reader.read_frame(&mut root.stdout).unwrap(), None);
    peer.join().unwrap();
}
#[test]
fn claimed_credentials_groups_and_original_nonce_do_not_authorize_root_execution() {
    let nonce = [51; 32];
    refusal(nonce, context(&nonce, true, false), vec![self_pidfd()]);
    let nonce = [52; 32];
    refusal(nonce, context(&nonce, false, true), vec![self_pidfd()]);
    let nonce = [53; 32];
    refusal(nonce, context(&[99; 32], false, false), vec![self_pidfd()]);
}
#[test]
fn missing_duplicate_truncated_and_non_pidfd_ancillary_are_refused() {
    let nonce = [54; 32];
    refusal(nonce, context(&nonce, false, false), vec![]);
    let nonce = [55; 32];
    refusal(
        nonce,
        context(&nonce, false, false),
        vec![self_pidfd(), self_pidfd()],
    );
    let nonce = [56; 32];
    refusal(
        nonce,
        context(&nonce, false, false),
        (0..128).map(|_| self_pidfd()).collect(),
    );
    let nonce = [57; 32];
    refusal(
        nonce,
        context(&nonce, false, false),
        vec![std::fs::File::open("/dev/null").unwrap().into()],
    );
    let nonce = [58; 32];
    let mut oversized = context(&nonce, false, false);
    oversized.resize(MAX_PAYLOAD_LEN + 1, 0);
    refusal(nonce, oversized, vec![self_pidfd()]);
}
#[test]
fn another_live_process_pidfd_cannot_substitute_for_the_original_peer() {
    let nonce = [59; 32];
    let mut unrelated = RoleProcess::spawn_command(
        {
            let mut command = std::process::Command::new("/bin/sleep");
            command
                .arg("60")
                .stdin(std::process::Stdio::piped())
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::null());
            command
        },
        nonce,
    );
    let duplicated = unsafe { libc::fcntl(unrelated.pidfd.as_raw_fd(), libc::F_DUPFD_CLOEXEC, 3) };
    assert!(duplicated >= 0);
    refusal(
        nonce,
        context(&nonce, false, false),
        vec![unsafe { OwnedFd::from_raw_fd(duplicated) }],
    );
    assert!(unrelated.child.try_wait().unwrap().is_none());
    unrelated.signal(0);
}
