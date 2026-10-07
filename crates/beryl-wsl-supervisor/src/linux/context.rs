use super::sys;
use crate::{Frame, FrameReader, MAX_PAYLOAD_LEN, Nonce, Role, read_initial_frame};
use std::io;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::time::{Duration, Instant};

pub(super) use super::context_data::Context;
use super::context_data::SecretBytes;

pub(super) fn packet(fd: i32, bytes: &[u8], deadline: Instant) -> io::Result<()> {
    loop {
        let result =
            unsafe { libc::send(fd, bytes.as_ptr().cast(), bytes.len(), libc::MSG_NOSIGNAL) };
        if result == bytes.len() as isize {
            return Ok(());
        }
        if result >= 0 {
            return Err(io::Error::other("partial packet"));
        }
        let error = sys::error();
        if error.kind() != io::ErrorKind::WouldBlock && error.kind() != io::ErrorKind::Interrupted {
            return Err(error);
        }
        let remaining = deadline
            .checked_duration_since(Instant::now())
            .ok_or_else(|| io::Error::from(io::ErrorKind::TimedOut))?;
        sys::poll(
            &mut [libc::pollfd {
                fd,
                events: libc::POLLOUT,
                revents: 0,
            }],
            remaining,
        )?;
    }
}
fn wait_read(fd: i32, deadline: Instant) -> io::Result<()> {
    let remaining = deadline
        .checked_duration_since(Instant::now())
        .ok_or_else(|| io::Error::from(io::ErrorKind::TimedOut))?;
    let mut fds = [libc::pollfd {
        fd,
        events: libc::POLLIN,
        revents: 0,
    }];
    sys::poll(&mut fds, remaining)?;
    if fds[0].revents == 0 {
        return Err(io::Error::from(io::ErrorKind::TimedOut));
    }
    Ok(())
}
fn send_context(
    fd: i32,
    context: &Context,
    nonce: &Nonce,
    pidfd: i32,
    deadline: Instant,
) -> io::Result<()> {
    let mut bytes = context.encode(nonce)?;
    let mut control = [0usize; 8];
    let mut iov = libc::iovec {
        iov_base: bytes.as_mut_ptr().cast(),
        iov_len: bytes.len(),
    };
    let mut message: libc::msghdr = unsafe { std::mem::zeroed() };
    message.msg_iov = &mut iov;
    message.msg_iovlen = 1;
    message.msg_control = control.as_mut_ptr().cast();
    message.msg_controllen = unsafe { libc::CMSG_SPACE(std::mem::size_of::<i32>() as u32) } as _;
    unsafe {
        let header = libc::CMSG_FIRSTHDR(&message);
        (*header).cmsg_level = libc::SOL_SOCKET;
        (*header).cmsg_type = libc::SCM_RIGHTS;
        (*header).cmsg_len = libc::CMSG_LEN(4) as _;
        std::ptr::write(libc::CMSG_DATA(header).cast::<i32>(), pidfd);
    }
    let result = loop {
        let result = unsafe { libc::sendmsg(fd, &message, libc::MSG_NOSIGNAL) };
        if result == bytes.len() as isize {
            break Ok(());
        }
        if result >= 0 {
            break Err(io::Error::other("partial context packet"));
        }
        let error = sys::error();
        if error.kind() != io::ErrorKind::WouldBlock && error.kind() != io::ErrorKind::Interrupted {
            break Err(error);
        }
        let Some(remaining) = deadline.checked_duration_since(Instant::now()) else {
            break Err(io::Error::from(io::ErrorKind::TimedOut));
        };
        if let Err(error) = sys::poll(
            &mut [libc::pollfd {
                fd,
                events: libc::POLLOUT,
                revents: 0,
            }],
            remaining,
        ) {
            break Err(error);
        }
    };
    result
}
pub(super) fn acquire(nonce: &Nonce) -> io::Result<(Context, OwnedFd, OwnedFd)> {
    let deadline = Instant::now() + Duration::from_secs(5);
    let socket = sys::socket()?;
    let (address, length) = sys::socket_address(nonce);
    sys::check(unsafe {
        libc::connect(
            socket.as_raw_fd(),
            (&address as *const libc::sockaddr_un).cast(),
            length,
        )
    })?;
    wait_read(socket.as_raw_fd(), deadline)?;
    let mut bytes = SecretBytes(vec![0u8; MAX_PAYLOAD_LEN]);
    let mut control = [0usize; 64];
    let mut iov = libc::iovec {
        iov_base: bytes.as_mut_ptr().cast(),
        iov_len: bytes.len(),
    };
    let mut message: libc::msghdr = unsafe { std::mem::zeroed() };
    message.msg_iov = &mut iov;
    message.msg_iovlen = 1;
    message.msg_control = control.as_mut_ptr().cast();
    message.msg_controllen = std::mem::size_of_val(&control) as _;
    let count = unsafe { libc::recvmsg(socket.as_raw_fd(), &mut message, libc::MSG_CMSG_CLOEXEC) };
    if count < 0 {
        return Err(sys::error());
    }
    let mut descriptors = Vec::new();
    let mut invalid = false;
    unsafe {
        let mut header = libc::CMSG_FIRSTHDR(&message);
        while !header.is_null() {
            let base = libc::CMSG_LEN(0) as usize;
            let length = (*header).cmsg_len as usize;
            if length < base {
                invalid = true;
                break;
            }
            if (*header).cmsg_level != libc::SOL_SOCKET || (*header).cmsg_type != libc::SCM_RIGHTS {
                invalid = true;
            } else {
                let data_length = length - base;
                if data_length % 4 != 0 {
                    invalid = true;
                }
                for index in 0..data_length / 4 {
                    descriptors.push(OwnedFd::from_raw_fd(std::ptr::read(
                        libc::CMSG_DATA(header).cast::<i32>().add(index),
                    )));
                }
            }
            header = libc::CMSG_NXTHDR(&message, header);
        }
    }
    if invalid
        || message.msg_flags & (libc::MSG_TRUNC | libc::MSG_CTRUNC) != 0
        || descriptors.len() != 1
    {
        return Err(io::Error::other("invalid context ancillary data"));
    }
    bytes.truncate(count as usize);
    let context = Context::decode(nonce, &bytes)?;
    let peer = sys::peer(socket.as_raw_fd())?;
    if peer.uid != context.uid
        || peer.gid != context.gid
        || sys::peer_groups(socket.as_raw_fd())? != context.groups
    {
        return Err(io::Error::other("context credentials mismatch"));
    }
    let pidfd = descriptors.pop().unwrap();
    sys::signal(pidfd.as_raw_fd(), 0)?;
    let peer_pidfd = sys::peer_pidfd(socket.as_raw_fd())?;
    if sys::exited(pidfd.as_raw_fd())?
        || sys::exited(peer_pidfd.as_raw_fd())?
        || sys::pidfd_identity(pidfd.as_raw_fd())? != sys::pidfd_identity(peer_pidfd.as_raw_fd())?
        || sys::exited(pidfd.as_raw_fd())?
        || sys::exited(peer_pidfd.as_raw_fd())?
    {
        return Err(io::Error::other("broker pidfd identity mismatch"));
    }
    packet(socket.as_raw_fd(), nonce, deadline)?;
    Ok((context, pidfd, socket))
}
pub fn context_broker() -> io::Result<()> {
    sys::prepare()?;
    let deadline = Instant::now() + Duration::from_secs(5);
    let (nonce, initial) =
        read_initial_frame(sys::DeadlineReader { fd: 0, deadline }).map_err(sys::codec)?;
    let mut output = sys::Output::new(nonce)?;
    if initial != Frame::Initialize {
        return Err(io::Error::other("invalid broker initialization"));
    }
    let context = Context::capture()?;
    let self_pidfd = sys::pidfd_self()?;
    let listener = sys::socket()?;
    let (address, length) = sys::socket_address(&nonce);
    sys::check(unsafe {
        libc::bind(
            listener.as_raw_fd(),
            (&address as *const libc::sockaddr_un).cast(),
            length,
        )
    })?;
    sys::check(unsafe { libc::listen(listener.as_raw_fd(), 1) })?;
    output.push(Frame::Ready {
        role: Role::ContextBroker,
    });
    let mut reader = FrameReader::new(nonce);
    let mut peer: Option<OwnedFd> = None;
    let mut closing = false;
    while !closing {
        if peer.is_none() && Instant::now() >= deadline {
            break;
        }
        let mut fds = [
            libc::pollfd {
                fd: 0,
                events: libc::POLLIN,
                revents: 0,
            },
            libc::pollfd {
                fd: peer
                    .as_ref()
                    .map_or(listener.as_raw_fd(), AsRawFd::as_raw_fd),
                events: libc::POLLIN,
                revents: 0,
            },
        ];
        sys::poll(&mut fds, Duration::from_millis(25))?;
        output.drain();
        if output.lost() {
            break;
        }
        if fds[0].revents != 0 {
            let mut scratch = [0u8; 8192];
            match sys::read(0, &mut scratch) {
                Ok(0) => closing = true,
                Ok(count) => match reader.feed(&scratch[..count]) {
                    Ok(frames) => {
                        for frame in frames {
                            match frame {
                                Frame::BrokerClose | Frame::Stop => closing = true,
                                _ => closing = true,
                            }
                        }
                    }
                    Err(_) => closing = true,
                },
                Err(error)
                    if matches!(
                        error.kind(),
                        io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted
                    ) => {}
                Err(_) => closing = true,
            }
        }
        if closing {
            break;
        }
        if fds[1].revents != 0 {
            if let Some(socket) = &peer {
                let mut bytes = [0u8; 2];
                let count = unsafe {
                    libc::recv(
                        socket.as_raw_fd(),
                        bytes.as_mut_ptr().cast(),
                        bytes.len(),
                        libc::MSG_TRUNC,
                    )
                };
                if count == 0 || count == 1 && bytes[0] == b'C' {
                    closing = true;
                } else if count >= 0 {
                    closing = true;
                } else {
                    let error = sys::error();
                    if error.kind() != io::ErrorKind::WouldBlock
                        && error.kind() != io::ErrorKind::Interrupted
                    {
                        closing = true;
                    }
                }
            } else {
                let socket = sys::owned(unsafe {
                    libc::accept4(
                        listener.as_raw_fd(),
                        std::ptr::null_mut(),
                        std::ptr::null_mut(),
                        libc::SOCK_CLOEXEC | libc::SOCK_NONBLOCK,
                    )
                })?;
                if sys::peer(socket.as_raw_fd())?.uid != 0 {
                    break;
                }
                send_context(
                    socket.as_raw_fd(),
                    &context,
                    &nonce,
                    self_pidfd.as_raw_fd(),
                    deadline,
                )?;
                wait_read(socket.as_raw_fd(), deadline)?;
                let mut received = [0u8; 32];
                let count = unsafe {
                    libc::recv(
                        socket.as_raw_fd(),
                        received.as_mut_ptr().cast(),
                        received.len(),
                        libc::MSG_TRUNC,
                    )
                };
                if count != 32 || received != nonce {
                    break;
                }
                peer = Some(socket);
            }
        }
    }
    drop(peer);
    drop(listener);
    drop(context);
    drop(self_pidfd);
    output.push(Frame::LinuxCompanionsClosed);
    output.flush();
    Ok(())
}
