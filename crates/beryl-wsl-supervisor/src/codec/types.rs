use std::fmt;
use std::io;

pub const PROTOCOL_VERSION: u16 = 1;
pub const HEADER_LEN: usize = 48;
pub const MAX_PAYLOAD_LEN: usize = 128 * 1024;
pub const MAX_PENDING_FRAMES: usize = 8;
pub const MAX_PATH_LEN: usize = 4096;
pub const MAX_DIAGNOSTIC_LEN: usize = 4096;
pub type Nonce = [u8; 32];
pub(super) const MAGIC: &[u8; 8] = b"BRYLWSL1";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Role {
    ContextBroker,
    Supervisor,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ObservationKind {
    Executable,
    Directory,
    Home,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExitStatus {
    Exited(i32),
    Signaled(i32),
}
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct DiagnosticTail {
    pub bytes: Vec<u8>,
    pub truncated: bool,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkloadResult {
    pub exit: ExitStatus,
    pub observation: Option<String>,
    pub stdout: DiagnosticTail,
    pub stderr: DiagnosticTail,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FailureKind {
    Protocol,
    Unsupported,
    Context,
    Namespace,
    Credentials,
    WorkingDirectory,
    Exec,
    Observation,
    Control,
    Timeout,
    System,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Frame {
    Initialize,
    LaunchServer {
        executable: String,
        execution_root: String,
        arguments: Vec<String>,
    },
    Observe {
        kind: ObservationKind,
        path: Option<String>,
        timeout_ms: u32,
    },
    Stop,
    BrokerClose,
    Ready {
        role: Role,
    },
    WorkloadStarted,
    ShutdownPending,
    OwnedNamespaceClosed {
        result: WorkloadResult,
    },
    LinuxCompanionsClosed,
    Failure {
        kind: FailureKind,
        errno: Option<i32>,
    },
}
#[derive(Debug)]
pub enum CodecError {
    Io(io::Error),
    Invalid(&'static str),
}
impl fmt::Display for CodecError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => error.fmt(formatter),
            Self::Invalid(reason) => formatter.write_str(reason),
        }
    }
}
impl std::error::Error for CodecError {}
impl From<io::Error> for CodecError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}
pub(super) type Result<T> = std::result::Result<T, CodecError>;
pub(super) fn invalid<T>(reason: &'static str) -> Result<T> {
    Err(CodecError::Invalid(reason))
}
