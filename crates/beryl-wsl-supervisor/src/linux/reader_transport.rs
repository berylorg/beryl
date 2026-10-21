use super::sys;
use crate::{Frame, HEADER_LEN, MAX_READER_PAGE, decode_frame, encode_frame};
use std::io;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd, RawFd};

pub(super) fn pair() -> io::Result<(OwnedFd, OwnedFd)> {
    let mut fds = [-1; 2];
    sys::check(unsafe {
        libc::socketpair(
            libc::AF_UNIX,
            libc::SOCK_SEQPACKET | libc::SOCK_CLOEXEC,
            0,
            fds.as_mut_ptr(),
        )
    })?;
    Ok((unsafe { OwnedFd::from_raw_fd(fds[0]) }, unsafe {
        OwnedFd::from_raw_fd(fds[1])
    }))
}

pub(super) fn send(fd: RawFd, frame: &Frame) -> io::Result<bool> {
    let bytes = encode_frame(&[0; 32], frame).map_err(sys::codec)?;
    let count = unsafe { libc::send(fd, bytes.as_ptr().cast(), bytes.len(), libc::MSG_NOSIGNAL) };
    if count < 0 {
        let error = sys::error();
        if matches!(
            error.kind(),
            io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted
        ) {
            return Ok(false);
        }
        return Err(error);
    }
    if count as usize != bytes.len() {
        return Err(io::Error::other("partial private packet"));
    }
    Ok(true)
}

pub(super) enum Received {
    Pending,
    Closed,
    Frame(Frame),
}

pub(super) fn receive(fd: RawFd) -> io::Result<Received> {
    // Receive directly into the single charged frame; decoding adds at most one page.
    let mut bytes = vec![0u8; HEADER_LEN + 37 + MAX_READER_PAGE];
    let count = unsafe { libc::recv(fd, bytes.as_mut_ptr().cast(), bytes.len(), libc::MSG_TRUNC) };
    if count < 0 {
        let error = sys::error();
        return if matches!(
            error.kind(),
            io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted
        ) {
            Ok(Received::Pending)
        } else {
            Err(error)
        };
    }
    if count == 0 {
        return Ok(Received::Closed);
    }
    if count as usize > bytes.len() {
        return Err(io::Error::other("oversized private packet"));
    }
    bytes.truncate(count as usize);
    Ok(Received::Frame(
        decode_frame(&[0; 32], &bytes).map_err(sys::codec)?,
    ))
}

pub(super) fn nonblocking_pair() -> io::Result<(OwnedFd, OwnedFd)> {
    let pair = pair()?;
    sys::nonblocking(pair.0.as_raw_fd())?;
    sys::nonblocking(pair.1.as_raw_fd())?;
    Ok(pair)
}
