use super::{context::Context, reader_transport as transport, sys};
use crate::{Frame, MAX_READER_PAGE, ReaderFailure, ReaderId};
use std::fs::File;
use std::io;
use std::os::fd::RawFd;
use std::os::unix::fs::{FileExt, OpenOptionsExt};

fn open(context: &Context, path: &str, channel: RawFd) -> Result<(File, u64), ReaderFailure> {
    let credentials = || -> io::Result<()> {
        sys::close_except(&[channel])?;
        sys::check(unsafe {
            libc::syscall(
                libc::SYS_setgroups,
                context.groups.len(),
                context.groups.as_ptr(),
            )
        } as i32)?;
        sys::check(unsafe {
            libc::syscall(libc::SYS_setresgid, context.gid, context.gid, context.gid)
        } as i32)?;
        sys::check(unsafe {
            libc::syscall(libc::SYS_setresuid, context.uid, context.uid, context.uid)
        } as i32)
    };
    credentials().map_err(|_| ReaderFailure::Unreadable)?;
    let file = File::options()
        .read(true)
        .custom_flags(libc::O_NONBLOCK | libc::O_NOCTTY)
        .open(path)
        .map_err(|error| {
            if error.kind() == io::ErrorKind::NotFound {
                ReaderFailure::Missing
            } else {
                ReaderFailure::Unreadable
            }
        })?;
    let metadata = file.metadata().map_err(|_| ReaderFailure::Unreadable)?;
    if !metadata.is_file() || metadata.len() == 0 {
        return Err(ReaderFailure::Unreadable);
    }
    Ok((file, metadata.len()))
}

fn page(
    file: &File,
    length: u64,
    id: ReaderId,
    sequence: u64,
    offset: u64,
    allowance: u32,
) -> Result<Frame, ReaderFailure> {
    if offset > length
        || offset > i64::MAX as u64
        || allowance == 0
        || allowance as usize > MAX_READER_PAGE
        || offset.checked_add(u64::from(allowance)).is_none()
    {
        return Err(ReaderFailure::Protocol);
    }
    if file
        .metadata()
        .map_err(|_| ReaderFailure::Unreadable)?
        .len()
        != length
    {
        return Err(ReaderFailure::Changed);
    }
    let mut bytes = vec![0; allowance as usize];
    let count = loop {
        match file.read_at(&mut bytes, offset) {
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            result => break result.map_err(|_| ReaderFailure::Unreadable)?,
        }
    };
    let end = offset
        .checked_add(count as u64)
        .ok_or(ReaderFailure::Protocol)?;
    if end > length
        || (count == 0 && offset != length)
        || file
            .metadata()
            .map_err(|_| ReaderFailure::Unreadable)?
            .len()
            != length
    {
        return Err(ReaderFailure::Changed);
    }
    bytes.truncate(count);
    Ok(Frame::ReaderPage {
        id,
        sequence,
        offset,
        bytes,
        eof: count == 0,
    })
}

pub(super) fn run(context: &Context, id: ReaderId, path: &str, channel: RawFd) -> ! {
    let mut gate = [0];
    if !matches!(sys::read(channel, &mut gate), Ok(1)) || gate != *b"G" {
        unsafe { libc::_exit(1) }
    }
    let (file, length) = match open(context, path, channel) {
        Ok(opened) => opened,
        Err(kind) => {
            let _ = transport::send(
                channel,
                &Frame::ReaderFailed {
                    id,
                    sequence: 0,
                    kind,
                },
            );
            unsafe { libc::_exit(1) }
        }
    };
    if !matches!(
        transport::send(channel, &Frame::ReaderOpened { id, length }),
        Ok(true)
    ) {
        unsafe { libc::_exit(1) }
    }
    let mut sequence = 0u64;
    loop {
        let frame = match transport::receive(channel) {
            Ok(transport::Received::Frame(frame)) => frame,
            _ => unsafe { libc::_exit(1) },
        };
        let (next, offset, allowance) = match frame {
            Frame::ReaderRead {
                id: actual,
                sequence: next,
                offset,
                allowance,
            } if actual == id && sequence.checked_add(1) == Some(next) => (next, offset, allowance),
            _ => unsafe { libc::_exit(1) },
        };
        sequence = next;
        let response = match page(&file, length, id, sequence, offset, allowance) {
            Ok(response) => response,
            Err(kind) => {
                let _ = transport::send(channel, &Frame::ReaderFailed { id, sequence, kind });
                unsafe { libc::_exit(1) }
            }
        };
        if !matches!(transport::send(channel, &response), Ok(true)) {
            unsafe { libc::_exit(1) }
        }
    }
}
