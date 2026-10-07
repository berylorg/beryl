use crate::{Frame, MAX_PENDING_FRAMES, Nonce, encode_frame};
use std::collections::VecDeque;
use std::io::{self, Read};
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd, RawFd};
use std::time::{Duration, Instant};

pub(super) fn error() -> io::Error {
    io::Error::last_os_error()
}
pub(super) fn check(value: libc::c_int) -> io::Result<()> {
    if value == -1 { Err(error()) } else { Ok(()) }
}
pub(super) fn owned(value: libc::c_int) -> io::Result<OwnedFd> {
    if value == -1 {
        Err(error())
    } else {
        Ok(unsafe { OwnedFd::from_raw_fd(value) })
    }
}
pub(super) fn codec(error: crate::CodecError) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, error)
}
pub(super) fn nonblocking(fd: RawFd) -> io::Result<()> {
    let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
    check(flags)?;
    check(unsafe { libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) })
}
pub(super) fn pipe() -> io::Result<(OwnedFd, OwnedFd)> {
    let mut fds = [-1; 2];
    check(unsafe { libc::pipe2(fds.as_mut_ptr(), libc::O_CLOEXEC) })?;
    Ok((unsafe { OwnedFd::from_raw_fd(fds[0]) }, unsafe {
        OwnedFd::from_raw_fd(fds[1])
    }))
}
pub(super) fn pidfd_self() -> io::Result<OwnedFd> {
    owned(unsafe { libc::syscall(libc::SYS_pidfd_open, libc::getpid(), 0) } as i32)
}
pub(super) fn signal(fd: RawFd, signal: i32) -> io::Result<()> {
    check(unsafe {
        libc::syscall(
            libc::SYS_pidfd_send_signal,
            fd,
            signal,
            std::ptr::null::<libc::siginfo_t>(),
            0,
        )
    } as i32)
}
pub(super) fn exited(fd: RawFd) -> io::Result<bool> {
    let mut poll = libc::pollfd {
        fd,
        events: libc::POLLIN,
        revents: 0,
    };
    let result = unsafe { libc::poll(&mut poll, 1, 0) };
    check(result)?;
    if poll.revents & libc::POLLNVAL != 0 {
        return Err(io::Error::other("invalid pidfd"));
    }
    Ok(poll.revents & libc::POLLIN != 0)
}
pub(super) fn reap(fd: RawFd) -> io::Result<Option<crate::ExitStatus>> {
    let mut info: libc::siginfo_t = unsafe { std::mem::zeroed() };
    check(unsafe {
        libc::waitid(
            libc::P_PIDFD,
            fd as u32,
            &mut info,
            libc::WEXITED | libc::WNOHANG,
        )
    })?;
    if unsafe { info.si_pid() } == 0 {
        return Ok(None);
    }
    Ok(Some(match info.si_code {
        libc::CLD_EXITED => crate::ExitStatus::Exited(unsafe { info.si_status() }),
        _ => crate::ExitStatus::Signaled(unsafe { info.si_status() }),
    }))
}
pub(super) fn poll(fds: &mut [libc::pollfd], timeout: Duration) -> io::Result<()> {
    loop {
        let result = unsafe {
            libc::poll(
                fds.as_mut_ptr(),
                fds.len() as libc::nfds_t,
                timeout.as_millis().min(i32::MAX as u128) as i32,
            )
        };
        if result >= 0 {
            return Ok(());
        }
        let error = error();
        if error.kind() != io::ErrorKind::Interrupted {
            return Err(error);
        }
    }
}
pub(super) fn read(fd: RawFd, bytes: &mut [u8]) -> io::Result<usize> {
    let result = unsafe { libc::read(fd, bytes.as_mut_ptr().cast(), bytes.len()) };
    if result == -1 {
        Err(error())
    } else {
        Ok(result as usize)
    }
}
pub(super) fn write(fd: RawFd, bytes: &[u8]) -> io::Result<usize> {
    let result = unsafe { libc::write(fd, bytes.as_ptr().cast(), bytes.len()) };
    if result == -1 {
        Err(error())
    } else {
        Ok(result as usize)
    }
}
pub(super) fn write_all(fd: RawFd, mut bytes: &[u8]) -> io::Result<()> {
    while !bytes.is_empty() {
        match write(fd, bytes) {
            Ok(0) => return Err(io::Error::from(io::ErrorKind::WriteZero)),
            Ok(count) => bytes = &bytes[count..],
            Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
            Err(error) => return Err(error),
        }
    }
    Ok(())
}
pub(super) struct DeadlineReader {
    pub fd: RawFd,
    pub deadline: Instant,
}
impl Read for DeadlineReader {
    fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        loop {
            let remaining = self
                .deadline
                .checked_duration_since(Instant::now())
                .ok_or_else(|| io::Error::from(io::ErrorKind::TimedOut))?;
            let mut fds = [libc::pollfd {
                fd: self.fd,
                events: libc::POLLIN,
                revents: 0,
            }];
            poll(&mut fds, remaining)?;
            if fds[0].revents == 0 {
                return Err(io::Error::from(io::ErrorKind::TimedOut));
            }
            match read(self.fd, bytes) {
                Err(error)
                    if matches!(
                        error.kind(),
                        io::ErrorKind::Interrupted | io::ErrorKind::WouldBlock
                    ) =>
                {
                    continue;
                }
                result => return result,
            }
        }
    }
}
pub(super) struct Output {
    nonce: Nonce,
    frames: VecDeque<Vec<u8>>,
    offset: usize,
    lost: bool,
}
impl Output {
    pub fn new(nonce: Nonce) -> io::Result<Self> {
        nonblocking(1)?;
        Ok(Self {
            nonce,
            frames: VecDeque::new(),
            offset: 0,
            lost: false,
        })
    }
    pub fn push(&mut self, frame: Frame) {
        if self.lost {
            return;
        }
        if self.frames.len() == MAX_PENDING_FRAMES {
            self.lost = true;
            self.frames.clear();
            return;
        }
        match encode_frame(&self.nonce, &frame) {
            Ok(bytes) => self.frames.push_back(bytes),
            Err(_) => self.lost = true,
        }
        self.drain();
    }
    pub fn drain(&mut self) {
        while let Some(frame) = self.frames.front() {
            match write(1, &frame[self.offset..]) {
                Ok(0) => {
                    self.lost = true;
                    break;
                }
                Ok(count) => {
                    self.offset += count;
                    if self.offset == frame.len() {
                        self.frames.pop_front();
                        self.offset = 0;
                    }
                }
                Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => break,
                Err(_) => {
                    self.lost = true;
                    break;
                }
            }
        }
        if self.lost {
            self.frames.clear();
        }
    }
    pub fn lost(&self) -> bool {
        self.lost
    }
    pub fn flush(&mut self) {
        let deadline = Instant::now() + Duration::from_secs(5);
        while !self.frames.is_empty() && !self.lost && Instant::now() < deadline {
            let mut fds = [libc::pollfd {
                fd: 1,
                events: libc::POLLOUT,
                revents: 0,
            }];
            let _ = poll(&mut fds, Duration::from_millis(25));
            self.drain();
        }
    }
}
pub(super) fn prepare() -> io::Result<()> {
    unsafe {
        libc::signal(libc::SIGPIPE, libc::SIG_IGN);
        libc::signal(libc::SIGCHLD, libc::SIG_DFL);
    }
    nonblocking(0)?;
    if !cfg!(target_arch = "x86_64") {
        return Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "unsupported architecture",
        ));
    }
    let mut name: libc::utsname = unsafe { std::mem::zeroed() };
    check(unsafe { libc::uname(&mut name) })?;
    let release = unsafe { std::ffi::CStr::from_ptr(name.release.as_ptr()) }.to_string_lossy();
    let mut numbers = release.split('.');
    let major = numbers
        .next()
        .and_then(|value| value.parse::<u32>().ok())
        .unwrap_or(0);
    let minor = numbers
        .next()
        .and_then(|value| value.parse::<u32>().ok())
        .unwrap_or(0);
    if (major, minor) < (6, 6) || !release.to_ascii_lowercase().contains("microsoft") {
        return Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "unsupported WSL kernel",
        ));
    }
    Ok(())
}
pub(super) fn close_except(keep: &[RawFd]) -> io::Result<()> {
    let mut keep = keep.to_vec();
    keep.sort_unstable();
    keep.dedup();
    let mut first = 0u32;
    for fd in keep.into_iter().filter(|fd| *fd >= 0) {
        let fd = fd as u32;
        if first < fd {
            check(unsafe { libc::syscall(libc::SYS_close_range, first, fd - 1, 0) } as i32)?;
        }
        first = fd + 1;
    }
    check(unsafe { libc::syscall(libc::SYS_close_range, first, u32::MAX, 0) } as i32)
}
#[repr(C)]
#[derive(Default)]
struct CloneArgs {
    flags: u64,
    pidfd: u64,
    child_tid: u64,
    parent_tid: u64,
    exit_signal: u64,
    stack: u64,
    stack_size: u64,
    tls: u64,
    set_tid: u64,
    set_tid_size: u64,
    cgroup: u64,
}
pub(super) enum Spawn {
    Child,
    Parent { pidfd: OwnedFd, pid: i32 },
}
pub(super) fn clone_pidfd(namespaces: bool) -> io::Result<Spawn> {
    let mut fd: i32 = -1;
    let args = CloneArgs {
        flags: libc::CLONE_PIDFD as u64
            | if namespaces {
                (libc::CLONE_NEWPID | libc::CLONE_NEWNS) as u64
            } else {
                0
            },
        pidfd: (&mut fd as *mut i32) as u64,
        exit_signal: libc::SIGCHLD as u64,
        ..Default::default()
    };
    let result =
        unsafe { libc::syscall(libc::SYS_clone3, &args, std::mem::size_of::<CloneArgs>()) };
    if result < 0 {
        return Err(error());
    }
    if result == 0 {
        Ok(Spawn::Child)
    } else {
        Ok(Spawn::Parent {
            pidfd: unsafe { OwnedFd::from_raw_fd(fd) },
            pid: result as i32,
        })
    }
}
pub(super) fn peer(fd: RawFd) -> io::Result<libc::ucred> {
    let mut credentials: libc::ucred = unsafe { std::mem::zeroed() };
    let mut size = std::mem::size_of::<libc::ucred>() as libc::socklen_t;
    check(unsafe {
        libc::getsockopt(
            fd,
            libc::SOL_SOCKET,
            libc::SO_PEERCRED,
            (&mut credentials as *mut libc::ucred).cast(),
            &mut size,
        )
    })?;
    if size as usize != std::mem::size_of::<libc::ucred>() {
        return Err(io::Error::other("invalid peer credentials"));
    }
    Ok(credentials)
}
pub(super) fn peer_groups(fd: RawFd) -> io::Result<Vec<u32>> {
    let mut groups = vec![0u32; 256];
    let mut length = (groups.len() * 4) as libc::socklen_t;
    const SO_PEERGROUPS: i32 = 59;
    check(unsafe {
        libc::getsockopt(
            fd,
            libc::SOL_SOCKET,
            SO_PEERGROUPS,
            groups.as_mut_ptr().cast(),
            &mut length,
        )
    })?;
    if length as usize > groups.len() * 4 || length % 4 != 0 {
        return Err(io::Error::other("peer group bound"));
    }
    groups.truncate(length as usize / 4);
    Ok(groups)
}
pub(super) fn peer_pidfd(fd: RawFd) -> io::Result<OwnedFd> {
    const SO_PEERPIDFD: i32 = 77;
    let mut pidfd = -1;
    let mut length = 4 as libc::socklen_t;
    check(unsafe {
        libc::getsockopt(
            fd,
            libc::SOL_SOCKET,
            SO_PEERPIDFD,
            (&mut pidfd as *mut i32).cast(),
            &mut length,
        )
    })?;
    let descriptor = owned(pidfd)?;
    if length != 4 {
        return Err(io::Error::other("invalid peer pidfd"));
    }
    check(unsafe { libc::fcntl(descriptor.as_raw_fd(), libc::F_SETFD, libc::FD_CLOEXEC) })?;
    Ok(descriptor)
}
pub(super) fn pidfd_identity(fd: RawFd) -> io::Result<i32> {
    use std::io::Read;
    let file = std::fs::File::open(format!("/proc/self/fdinfo/{fd}"))?;
    let mut bytes = Vec::new();
    file.take(2049).read_to_end(&mut bytes)?;
    if bytes.len() > 2048 {
        return Err(io::Error::other("pidfd info bound"));
    }
    let text = std::str::from_utf8(&bytes).map_err(|_| io::Error::other("invalid pidfd info"))?;
    let pid = text
        .lines()
        .find_map(|line| {
            line.strip_prefix("Pid:")
                .and_then(|value| value.trim().parse::<i32>().ok())
        })
        .ok_or_else(|| io::Error::other("missing pidfd identity"))?;
    if pid <= 0 {
        return Err(io::Error::other("dead pidfd identity"));
    }
    Ok(pid)
}
pub(super) fn socket_address(nonce: &Nonce) -> (libc::sockaddr_un, libc::socklen_t) {
    use sha2::{Digest, Sha256};
    let mut address: libc::sockaddr_un = unsafe { std::mem::zeroed() };
    address.sun_family = libc::AF_UNIX as u16;
    let mut name = String::from("beryl-wsl-");
    for byte in Sha256::digest(nonce) {
        use std::fmt::Write;
        let _ = write!(name, "{byte:02x}");
    }
    for (target, byte) in address.sun_path[1..].iter_mut().zip(name.bytes()) {
        *target = byte as libc::c_char;
    }
    let length = std::mem::offset_of!(libc::sockaddr_un, sun_path) + 1 + name.len();
    (address, length as libc::socklen_t)
}
pub(super) fn socket() -> io::Result<OwnedFd> {
    owned(unsafe {
        libc::socket(
            libc::AF_UNIX,
            libc::SOCK_SEQPACKET | libc::SOCK_CLOEXEC | libc::SOCK_NONBLOCK,
            0,
        )
    })
}
