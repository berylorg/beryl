use super::{context::Context, sys};
use crate::{FailureKind, Frame, MAX_PATH_LEN, ObservationKind};
use std::ffi::{CString, OsString};
use std::io;
use std::os::fd::{AsRawFd, OwnedFd};
use std::os::unix::ffi::OsStringExt;
use std::time::Duration;

pub(super) struct Channels {
    pub gate: i32,
    pub parent: i32,
    pub commands: i32,
    pub status: i32,
    pub proof: i32,
    pub observation: i32,
    pub stdout: i32,
    pub stderr: i32,
}
fn fail(fd: i32, kind: FailureKind, error: io::Error) -> ! {
    let tag = match kind {
        FailureKind::Namespace => 1,
        FailureKind::Credentials => 2,
        FailureKind::WorkingDirectory => 3,
        FailureKind::Exec => 4,
        FailureKind::Observation => 5,
        _ => 6,
    };
    let mut bytes = [0u8; 5];
    bytes[0] = tag;
    bytes[1..].copy_from_slice(&error.raw_os_error().unwrap_or(libc::EIO).to_le_bytes());
    let _ = sys::write_all(fd, &bytes);
    unsafe { libc::_exit(1) }
}
pub(super) fn decode_failure(bytes: &[u8]) -> Option<(FailureKind, i32)> {
    if bytes.len() != 5 {
        return None;
    }
    let kind = match bytes[0] {
        1 => FailureKind::Namespace,
        2 => FailureKind::Credentials,
        3 => FailureKind::WorkingDirectory,
        4 => FailureKind::Exec,
        5 => FailureKind::Observation,
        _ => FailureKind::System,
    };
    Some((kind, i32::from_le_bytes(bytes[1..].try_into().unwrap())))
}
pub(super) fn init(context: &Context, request: &Frame, channels: Channels) -> ! {
    let setup = || -> io::Result<()> {
        sys::close_except(&[
            channels.gate,
            channels.parent,
            channels.commands,
            channels.status,
            channels.proof,
            channels.observation,
            channels.stdout,
            channels.stderr,
        ])?;
        sys::check(unsafe { libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGKILL) })?;
        if sys::exited(channels.parent)? {
            return Err(io::Error::other("original parent gone"));
        }
        let mut byte = [0u8; 1];
        if sys::read(channels.gate, &mut byte)? != 1 || byte[0] != b'G' {
            return Err(io::Error::other("workload gate closed"));
        }
        if sys::exited(channels.parent)? {
            return Err(io::Error::other("original parent gone"));
        }
        sys::check(unsafe {
            libc::mount(
                std::ptr::null(),
                c"/".as_ptr(),
                std::ptr::null(),
                libc::MS_REC | libc::MS_PRIVATE,
                std::ptr::null(),
            )
        })?;
        sys::check(unsafe {
            libc::mount(
                c"proc".as_ptr(),
                c"/proc".as_ptr(),
                c"proc".as_ptr(),
                libc::MS_NOSUID | libc::MS_NODEV | libc::MS_NOEXEC,
                std::ptr::null(),
            )
        })?;
        Ok(())
    };
    if let Err(error) = setup() {
        fail(channels.proof, FailureKind::Namespace, error);
    }
    unsafe {
        libc::close(channels.gate);
    }
    let (workload_pidfd, workload_pid) = match sys::clone_pidfd(false) {
        Ok(sys::Spawn::Child) => workload(context, request, &channels),
        Ok(sys::Spawn::Parent { pidfd, pid }) => (pidfd, pid),
        Err(error) => fail(channels.proof, FailureKind::Namespace, error),
    };
    let tracing = matches!(request, Frame::LaunchServer { .. });
    if !tracing {
        unsafe {
            libc::close(channels.proof);
        }
    }
    unsafe {
        libc::close(channels.observation);
        libc::close(channels.stdout);
        libc::close(channels.stderr);
    }
    let mut trace_started = false;
    let mut exec_proved = false;
    if sys::nonblocking(channels.commands).is_err() {
        unsafe { libc::_exit(1) }
    }
    loop {
        if sys::exited(channels.parent).unwrap_or(true) {
            unsafe { libc::_exit(1) }
        }
        let mut status = 0;
        loop {
            let child = unsafe { libc::waitpid(-1, &mut status, libc::WNOHANG | libc::WUNTRACED) };
            if child == workload_pid {
                if libc::WIFSTOPPED(status) {
                    let operation = if !trace_started && libc::WSTOPSIG(status) == libc::SIGSTOP {
                        let options = (libc::PTRACE_O_TRACEEXEC | libc::PTRACE_O_EXITKILL) as usize;
                        if unsafe {
                            libc::ptrace(
                                libc::PTRACE_SETOPTIONS,
                                workload_pid,
                                std::ptr::null_mut::<libc::c_void>(),
                                options as *mut libc::c_void,
                            )
                        } == -1
                        {
                            fail(channels.proof, FailureKind::Exec, sys::error());
                        }
                        trace_started = true;
                        libc::PTRACE_CONT
                    } else if trace_started && status >> 16 == libc::PTRACE_EVENT_EXEC {
                        exec_proved = true;
                        libc::PTRACE_DETACH
                    } else {
                        fail(
                            channels.proof,
                            FailureKind::Exec,
                            io::Error::other("unexpected preexec stop"),
                        );
                    };
                    if unsafe {
                        libc::ptrace(
                            operation,
                            workload_pid,
                            std::ptr::null_mut::<libc::c_void>(),
                            std::ptr::null_mut::<libc::c_void>(),
                        )
                    } == -1
                    {
                        fail(channels.proof, FailureKind::Exec, sys::error());
                    }
                    if exec_proved {
                        let _ = sys::write_all(channels.proof, &[0]);
                        unsafe {
                            libc::close(channels.proof);
                        }
                    }
                    continue;
                }
                let mut bytes = [0u8; 5];
                if libc::WIFEXITED(status) {
                    bytes[0] = 1;
                    bytes[1..].copy_from_slice(&libc::WEXITSTATUS(status).to_le_bytes());
                } else {
                    bytes[0] = 2;
                    bytes[1..].copy_from_slice(&libc::WTERMSIG(status).to_le_bytes());
                }
                let _ = sys::write_all(channels.status, &bytes);
                drop(workload_pidfd);
                unsafe { libc::_exit(0) }
            }
            if child == 0 {
                break;
            }
            if child < 0 {
                let error = sys::error();
                if error.kind() == io::ErrorKind::Interrupted {
                    continue;
                }
                unsafe { libc::_exit(1) }
            }
        }
        let mut fds = [
            libc::pollfd {
                fd: channels.commands,
                events: libc::POLLIN,
                revents: 0,
            },
            libc::pollfd {
                fd: workload_pidfd.as_raw_fd(),
                events: libc::POLLIN,
                revents: 0,
            },
            libc::pollfd {
                fd: channels.parent,
                events: libc::POLLIN,
                revents: 0,
            },
        ];
        if sys::poll(&mut fds, Duration::from_millis(25)).is_err() {
            unsafe { libc::_exit(1) }
        }
        if fds[0].revents != 0 {
            let mut byte = [0u8; 1];
            match sys::read(channels.commands, &mut byte) {
                Ok(1) if byte[0] == b'S' => {
                    let _ = sys::signal(workload_pidfd.as_raw_fd(), libc::SIGTERM);
                }
                Err(error)
                    if error.kind() == io::ErrorKind::WouldBlock
                        || error.kind() == io::ErrorKind::Interrupted => {}
                _ => unsafe { libc::_exit(1) },
            }
        }
    }
}
fn workload(context: &Context, request: &Frame, channels: &Channels) -> ! {
    let setup = || -> io::Result<()> {
        let null = sys::owned(unsafe {
            libc::open(c"/dev/null".as_ptr(), libc::O_RDONLY | libc::O_CLOEXEC)
        })?;
        sys::check(unsafe { libc::dup2(null.as_raw_fd(), 0) })?;
        sys::check(unsafe { libc::dup2(channels.stdout, 1) })?;
        sys::check(unsafe { libc::dup2(channels.stderr, 2) })?;
        std::mem::forget(null);
        sys::close_except(&[0, 1, 2, channels.proof, channels.observation])?;
        unsafe {
            libc::signal(libc::SIGPIPE, libc::SIG_DFL);
        }
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
        } as i32)?;
        Ok(())
    };
    if let Err(error) = setup() {
        fail(channels.proof, FailureKind::Credentials, error);
    }
    match request {
        Frame::LaunchServer {
            executable,
            execution_root,
            arguments,
        } => {
            let root = CString::new(execution_root.as_bytes()).unwrap();
            if let Err(error) = sys::check(unsafe { libc::chdir(root.as_ptr()) }) {
                fail(channels.proof, FailureKind::WorkingDirectory, error);
            }
            if unsafe {
                libc::ptrace(
                    libc::PTRACE_TRACEME,
                    0,
                    std::ptr::null_mut::<libc::c_void>(),
                    std::ptr::null_mut::<libc::c_void>(),
                )
            } == -1
            {
                fail(channels.proof, FailureKind::Exec, sys::error());
            }
            // Raw clone3 does not refresh libc's cached thread identity.
            let own_pid = unsafe { libc::syscall(libc::SYS_getpid) };
            let own_tid = unsafe { libc::syscall(libc::SYS_gettid) };
            if unsafe { libc::syscall(libc::SYS_tgkill, own_pid, own_tid, libc::SIGSTOP) } != 0 {
                fail(channels.proof, FailureKind::Exec, sys::error());
            }
            let executable = CString::new(executable.as_bytes()).unwrap();
            let argv: Vec<CString> = std::iter::once(executable.clone())
                .chain(
                    arguments
                        .iter()
                        .map(|argument| CString::new(argument.as_bytes()).unwrap()),
                )
                .collect();
            let mut argv_ptr: Vec<*const libc::c_char> =
                argv.iter().map(|argument| argument.as_ptr()).collect();
            argv_ptr.push(std::ptr::null());
            let environment: Vec<CString> = context
                .environment
                .iter()
                .map(|(name, value)| {
                    let mut bytes = name.clone();
                    bytes.push(b'=');
                    bytes.extend_from_slice(value);
                    CString::new(bytes).unwrap()
                })
                .collect();
            let mut environment_ptr: Vec<*const libc::c_char> =
                environment.iter().map(|entry| entry.as_ptr()).collect();
            environment_ptr.push(std::ptr::null());
            unsafe {
                libc::close(channels.observation);
                libc::execve(
                    executable.as_ptr(),
                    argv_ptr.as_ptr(),
                    environment_ptr.as_ptr(),
                );
            }
            fail(channels.proof, FailureKind::Exec, sys::error());
        }
        Frame::Observe { kind, path, .. } => {
            if let Err(error) = sys::write_all(channels.proof, &[0]) {
                fail(channels.proof, FailureKind::System, error);
            }
            unsafe {
                libc::close(channels.proof);
            }
            match observation(context, *kind, path.as_deref()) {
                Ok(path) => {
                    let mut bytes = b"BRYL-OBSERVE1\0".to_vec();
                    bytes.extend_from_slice(path.as_bytes());
                    bytes.push(0);
                    let exit = if sys::write_all(channels.observation, &bytes).is_ok() {
                        0
                    } else {
                        1
                    };
                    unsafe { libc::_exit(exit) }
                }
                Err(_) => unsafe { libc::_exit(1) },
            }
        }
        _ => fail(
            channels.proof,
            FailureKind::Protocol,
            io::Error::other("invalid workload"),
        ),
    }
}
fn observation(context: &Context, kind: ObservationKind, path: Option<&str>) -> io::Result<String> {
    let native_path = match kind {
        ObservationKind::Home => {
            let bytes = context
                .environment
                .iter()
                .find(|(name, _)| name == b"HOME")
                .map(|(_, value)| value.clone())
                .ok_or_else(|| io::Error::other("missing ordinary HOME"))?;
            OsString::from_vec(bytes)
        }
        _ => OsString::from(path.ok_or_else(|| io::Error::other("missing selected path"))?),
    };
    let native_path = std::path::PathBuf::from(native_path);
    if !native_path.is_absolute() {
        return Err(io::Error::other("nonabsolute observation path"));
    }
    let canonical = std::fs::canonicalize(native_path)?;
    let path = canonical
        .to_str()
        .ok_or_else(|| io::Error::other("nonUTF8 observation path"))?;
    if path.len() > MAX_PATH_LEN {
        return Err(io::Error::other("observation path bound"));
    }
    let metadata = std::fs::metadata(&canonical)?;
    let mode = match kind {
        ObservationKind::Executable if metadata.is_file() => libc::R_OK | libc::X_OK,
        ObservationKind::Directory | ObservationKind::Home if metadata.is_dir() => {
            libc::R_OK | libc::X_OK
        }
        _ => return Err(io::Error::other("incorrect observation kind")),
    };
    let native = CString::new(path.as_bytes()).unwrap();
    sys::check(unsafe { libc::access(native.as_ptr(), mode) })?;
    Ok(path.to_owned())
}
pub(super) struct Namespace {
    pub pidfd: OwnedFd,
    pub commands: OwnedFd,
    pub status: OwnedFd,
    pub proof: OwnedFd,
    pub observation: OwnedFd,
    pub stdout: OwnedFd,
    pub stderr: OwnedFd,
}
pub(super) fn spawn(context: &Context, request: &Frame) -> io::Result<Namespace> {
    let parent = sys::pidfd_self()?;
    let (gate_read, gate_write) = sys::pipe()?;
    let (command_read, command_write) = sys::pipe()?;
    let (status_read, status_write) = sys::pipe()?;
    let (proof_read, proof_write) = sys::pipe()?;
    let (observation_read, observation_write) = sys::pipe()?;
    let (stdout_read, stdout_write) = sys::pipe()?;
    let (stderr_read, stderr_write) = sys::pipe()?;
    for read in [
        &command_write,
        &status_read,
        &proof_read,
        &observation_read,
        &stdout_read,
        &stderr_read,
    ] {
        sys::nonblocking(read.as_raw_fd())?;
    }
    let pidfd = match sys::clone_pidfd(true)? {
        sys::Spawn::Child => init(
            context,
            request,
            Channels {
                gate: gate_read.as_raw_fd(),
                parent: parent.as_raw_fd(),
                commands: command_read.as_raw_fd(),
                status: status_write.as_raw_fd(),
                proof: proof_write.as_raw_fd(),
                observation: observation_write.as_raw_fd(),
                stdout: stdout_write.as_raw_fd(),
                stderr: stderr_write.as_raw_fd(),
            },
        ),
        sys::Spawn::Parent { pidfd, .. } => pidfd,
    };
    let namespace = Namespace {
        pidfd,
        commands: command_write,
        status: status_read,
        proof: proof_read,
        observation: observation_read,
        stdout: stdout_read,
        stderr: stderr_read,
    };
    if let Err(error) = sys::write_all(gate_write.as_raw_fd(), b"G") {
        let _ = sys::signal(namespace.pidfd.as_raw_fd(), libc::SIGKILL);
        // The caller must retain this owner even when release fails.
        let _ = error;
    }
    Ok(namespace)
}
