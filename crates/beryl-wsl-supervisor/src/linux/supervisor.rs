use super::{context, namespace, sys};
use crate::{
    DiagnosticTail, ExitStatus, FailureKind, Frame, FrameReader, MAX_PATH_LEN, Role,
    WorkloadResult, read_initial_frame,
};
use std::io;
use std::os::fd::AsRawFd;
use std::time::{Duration, Instant};

fn failure(output: &mut sys::Output, kind: FailureKind, error: &io::Error) {
    output.push(Frame::Failure {
        kind,
        errno: error
            .raw_os_error()
            .filter(|number| (1..=4095).contains(number)),
    });
}
fn drain(fd: i32, tail: &mut DiagnosticTail) -> io::Result<bool> {
    let mut scratch = [0u8; 8192];
    // A noisy workload gets a bounded share of each control-loop iteration.
    for _ in 0..8 {
        match sys::read(fd, &mut scratch) {
            Ok(0) => return Ok(true),
            Ok(count) => {
                if count >= 4096 {
                    tail.truncated |= !tail.bytes.is_empty() || count > 4096;
                    tail.bytes.clear();
                    tail.bytes.extend_from_slice(&scratch[count - 4096..count]);
                } else {
                    let excess = (tail.bytes.len() + count).saturating_sub(4096);
                    if excess != 0 {
                        tail.bytes.drain(..excess);
                        tail.truncated = true;
                    }
                    tail.bytes.extend_from_slice(&scratch[..count]);
                }
            }
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => return Ok(false),
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(error),
        }
    }
    Ok(false)
}
fn collect(fd: i32, bytes: &mut Vec<u8>, maximum: usize) -> io::Result<bool> {
    let mut scratch = [0u8; 8192];
    loop {
        match sys::read(fd, &mut scratch) {
            Ok(0) => return Ok(true),
            Ok(count) => {
                if bytes.len() + count > maximum {
                    return Err(io::Error::other("private result bound"));
                }
                bytes.extend_from_slice(&scratch[..count]);
            }
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => return Ok(false),
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(error),
        }
    }
}
fn empty_result() -> WorkloadResult {
    WorkloadResult {
        exit: ExitStatus::Exited(1),
        observation: None,
        stdout: DiagnosticTail::default(),
        stderr: DiagnosticTail::default(),
    }
}
pub fn supervise() -> io::Result<()> {
    sys::prepare()?;
    let (nonce, request) = read_initial_frame(sys::DeadlineReader {
        fd: 0,
        deadline: Instant::now() + Duration::from_secs(5),
    })
    .map_err(sys::codec)?;
    let mut output = sys::Output::new(nonce)?;
    if unsafe { libc::geteuid() } != 0 {
        failure(
            &mut output,
            FailureKind::Credentials,
            &io::Error::from_raw_os_error(libc::EPERM),
        );
        output.flush();
        return Err(io::Error::from_raw_os_error(libc::EPERM));
    }
    if !matches!(request, Frame::LaunchServer { .. } | Frame::Observe { .. }) {
        failure(
            &mut output,
            FailureKind::Protocol,
            &io::Error::other("invalid launch request"),
        );
        output.flush();
        return Err(io::Error::other("invalid launch request"));
    }
    let (context, broker_pidfd, broker_channel) = match context::acquire(&nonce) {
        Ok(context) => context,
        Err(error) => {
            failure(&mut output, FailureKind::Context, &error);
            output.flush();
            return Err(error);
        }
    };
    output.push(Frame::Ready {
        role: Role::Supervisor,
    });
    let started_at = Instant::now();
    let observation_deadline = match request {
        Frame::Observe { timeout_ms, .. } => {
            Some(started_at + Duration::from_millis(timeout_ms as u64))
        }
        _ => None,
    };
    let mut namespace = match namespace::spawn(&context, &request) {
        Ok(namespace) => Some(namespace),
        Err(error) => {
            failure(&mut output, FailureKind::Namespace, &error);
            None
        }
    };
    drop(context);
    let mut reader = FrameReader::new(nonce);
    let mut control_live = true;
    let mut stopping: Option<Instant> = if namespace.is_none() {
        Some(Instant::now())
    } else {
        None
    };
    let mut attempt_deadline = stopping.map(|time| time + Duration::from_secs(5));
    let mut kill_sent = false;
    let mut broker_close_sent = false;
    let mut namespace_closed = namespace.is_none();
    let mut result = empty_result();
    if namespace_closed {
        output.push(Frame::OwnedNamespaceClosed {
            result: result.clone(),
        });
    }
    let mut proof = Vec::new();
    let mut proof_finished = false;
    let mut started = false;
    let mut status = Vec::new();
    let mut observation = Vec::new();
    let mut stdout_closed = false;
    let mut stderr_closed = false;
    loop {
        let mut reap_blocked = false;
        let mut broker_proof_blocked = false;
        output.drain();
        let now = Instant::now();
        let mut request_stop = output.lost();
        if control_live {
            let mut scratch = [0u8; 8192];
            match sys::read(0, &mut scratch) {
                Ok(0) => {
                    control_live = false;
                    request_stop = true;
                }
                Ok(count) => match reader.feed(&scratch[..count]) {
                    Ok(frames) => {
                        for frame in frames {
                            if frame == Frame::Stop {
                                request_stop = true;
                                attempt_deadline = Some(now + Duration::from_secs(5));
                            } else {
                                failure(
                                    &mut output,
                                    FailureKind::Protocol,
                                    &io::Error::other("illegal supervisor request"),
                                );
                                control_live = false;
                                request_stop = true;
                            }
                        }
                    }
                    Err(error) => {
                        failure(&mut output, FailureKind::Protocol, &sys::codec(error));
                        control_live = false;
                        request_stop = true;
                    }
                },
                Err(error)
                    if error.kind() == io::ErrorKind::WouldBlock
                        || error.kind() == io::ErrorKind::Interrupted => {}
                Err(error) => {
                    failure(&mut output, FailureKind::Control, &error);
                    control_live = false;
                    request_stop = true;
                }
            }
        }
        if !namespace_closed {
            if let Some(deadline) = observation_deadline {
                if now >= deadline && stopping.is_none() {
                    failure(
                        &mut output,
                        FailureKind::Timeout,
                        &io::Error::from(io::ErrorKind::TimedOut),
                    );
                    request_stop = true;
                }
            }
            if now >= started_at + Duration::from_secs(5) && !proof_finished && stopping.is_none() {
                failure(
                    &mut output,
                    FailureKind::Timeout,
                    &io::Error::from(io::ErrorKind::TimedOut),
                );
                request_stop = true;
            }
            let broker_exited = sys::exited(broker_pidfd.as_raw_fd()).unwrap_or(true);
            let mut peer_poll = [libc::pollfd {
                fd: broker_channel.as_raw_fd(),
                events: libc::POLLIN,
                revents: 0,
            }];
            if sys::poll(&mut peer_poll, Duration::ZERO).is_err()
                || broker_exited
                || peer_poll[0].revents != 0
            {
                request_stop = true;
            }
        }
        if request_stop && stopping.is_none() {
            stopping = Some(now);
            attempt_deadline = Some(now + Duration::from_secs(5));
        }
        if let Some(owner) = namespace.as_ref() {
            if !stdout_closed {
                match drain(owner.stdout.as_raw_fd(), &mut result.stdout) {
                    Ok(closed) => stdout_closed = closed,
                    Err(error) => {
                        failure(&mut output, FailureKind::Control, &error);
                        stdout_closed = true;
                        stopping.get_or_insert(now);
                    }
                }
            }
            if !stderr_closed {
                match drain(owner.stderr.as_raw_fd(), &mut result.stderr) {
                    Ok(closed) => stderr_closed = closed,
                    Err(error) => {
                        failure(&mut output, FailureKind::Control, &error);
                        stderr_closed = true;
                        stopping.get_or_insert(now);
                    }
                }
            }
            if !proof_finished {
                match collect(owner.proof.as_raw_fd(), &mut proof, 5) {
                    Ok(closed) => {
                        let setup_ready = proof == [0];
                        if closed || setup_ready {
                            proof_finished = true;
                            if setup_ready {
                                if stopping.is_none() {
                                    output.push(Frame::WorkloadStarted);
                                    started = true;
                                }
                            } else if let Some((kind, errno)) = namespace::decode_failure(&proof) {
                                failure(&mut output, kind, &io::Error::from_raw_os_error(errno));
                                stopping.get_or_insert(now);
                            } else {
                                failure(
                                    &mut output,
                                    FailureKind::Protocol,
                                    &io::Error::other("invalid setup proof"),
                                );
                                stopping.get_or_insert(now);
                            }
                        }
                    }
                    Err(error) => {
                        failure(&mut output, FailureKind::Control, &error);
                        proof_finished = true;
                        stopping.get_or_insert(now);
                    }
                }
            }
            if let Err(error) = collect(owner.status.as_raw_fd(), &mut status, 5) {
                failure(&mut output, FailureKind::Control, &error);
                stopping.get_or_insert(now);
            }
            if let Err(error) = collect(owner.observation.as_raw_fd(), &mut observation, 8192) {
                failure(&mut output, FailureKind::Observation, &error);
                stopping.get_or_insert(now);
            }
            if let Some(stop) = stopping {
                if now < stop + Duration::from_secs(2) {
                    let _ = sys::write(owner.commands.as_raw_fd(), b"S");
                } else if !kill_sent {
                    match sys::signal(owner.pidfd.as_raw_fd(), libc::SIGKILL) {
                        Ok(()) => kill_sent = true,
                        Err(error) if error.raw_os_error() == Some(libc::ESRCH) => kill_sent = true,
                        Err(error) => failure(&mut output, FailureKind::System, &error),
                    }
                }
            }
            match sys::reap(owner.pidfd.as_raw_fd()) {
                Ok(Some(init_exit)) => {
                    if !drain(owner.stdout.as_raw_fd(), &mut result.stdout).unwrap_or(false) {
                        result.stdout.truncated = true;
                    }
                    if !drain(owner.stderr.as_raw_fd(), &mut result.stderr).unwrap_or(false) {
                        result.stderr.truncated = true;
                    }
                    let _ = collect(owner.status.as_raw_fd(), &mut status, 5);
                    let _ = collect(owner.observation.as_raw_fd(), &mut observation, 8192);
                    result.exit = if status.len() == 5 {
                        let code = i32::from_le_bytes(status[1..].try_into().unwrap());
                        match status[0] {
                            1 if (0..=255).contains(&code) => ExitStatus::Exited(code),
                            2 if (1..=64).contains(&code) => ExitStatus::Signaled(code),
                            _ => init_exit,
                        }
                    } else {
                        init_exit
                    };
                    if matches!(request, Frame::Observe { .. }) {
                        if result.exit == ExitStatus::Exited(0) && started {
                            let prefix = b"BRYL-OBSERVE1\0";
                            let parsed = observation
                                .strip_prefix(prefix)
                                .and_then(|bytes| bytes.strip_suffix(&[0]))
                                .and_then(|bytes| std::str::from_utf8(bytes).ok());
                            match parsed {
                                Some(path)
                                    if path.starts_with('/')
                                        && !path.as_bytes().contains(&0)
                                        && path.len() <= MAX_PATH_LEN =>
                                {
                                    result.observation = Some(path.to_owned())
                                }
                                _ => failure(
                                    &mut output,
                                    FailureKind::Observation,
                                    &io::Error::other("invalid observation result"),
                                ),
                            }
                        } else if stopping.is_none() {
                            failure(
                                &mut output,
                                FailureKind::Observation,
                                &io::Error::other("observation failed"),
                            );
                        }
                    }
                    namespace.take();
                    namespace_closed = true;
                    output.push(Frame::OwnedNamespaceClosed {
                        result: result.clone(),
                    });
                    attempt_deadline.get_or_insert(now + Duration::from_secs(5));
                }
                Ok(None) => {}
                Err(error) => {
                    failure(&mut output, FailureKind::System, &error);
                    stopping.get_or_insert(now);
                    reap_blocked = true;
                }
            }
        }
        if namespace_closed {
            if !broker_close_sent {
                match context::packet(
                    broker_channel.as_raw_fd(),
                    b"C",
                    now + Duration::from_millis(25),
                ) {
                    Ok(()) => broker_close_sent = true,
                    Err(error)
                        if matches!(
                            error.kind(),
                            io::ErrorKind::BrokenPipe | io::ErrorKind::ConnectionReset
                        ) =>
                    {
                        broker_close_sent = true
                    }
                    Err(error) => failure(&mut output, FailureKind::Control, &error),
                }
            }
            match sys::exited(broker_pidfd.as_raw_fd()) {
                Ok(true) => {
                    drop(broker_channel);
                    drop(broker_pidfd);
                    output.push(Frame::LinuxCompanionsClosed);
                    output.flush();
                    return Ok(());
                }
                Ok(false) => {}
                Err(error) => {
                    failure(&mut output, FailureKind::System, &error);
                    broker_proof_blocked = true;
                }
            }
        }
        if let Some(stop) = stopping {
            if attempt_deadline.is_none() && now < stop + Duration::from_secs(5) {
                attempt_deadline = Some(stop + Duration::from_secs(5));
            }
        }
        if attempt_deadline.is_some_and(|deadline| now >= deadline) {
            output.push(Frame::ShutdownPending);
            attempt_deadline = None;
        }
        let mut fds = [
            libc::pollfd {
                fd: if control_live { 0 } else { -1 },
                events: libc::POLLIN,
                revents: 0,
            },
            libc::pollfd {
                fd: if reap_blocked {
                    -1
                } else {
                    namespace
                        .as_ref()
                        .map_or(-1, |owner| owner.pidfd.as_raw_fd())
                },
                events: libc::POLLIN,
                revents: 0,
            },
            libc::pollfd {
                fd: if namespace_closed && !broker_proof_blocked {
                    broker_pidfd.as_raw_fd()
                } else {
                    -1
                },
                events: libc::POLLIN,
                revents: 0,
            },
        ];
        if let Err(error) = sys::poll(&mut fds, Duration::from_millis(25)) {
            failure(&mut output, FailureKind::Control, &error);
            stopping.get_or_insert(now);
        }
    }
}
