#[path = "wire/content.rs"]
mod content;
#[path = "wire/frame.rs"]
mod frame;
#[path = "wire/generator.rs"]
mod generator;
#[path = "wire/message.rs"]
mod message;

use std::{fmt, num::NonZeroU64};

pub(crate) use frame::{
    RequestCutoff, await_masked_client_close, verify_masked_text_message,
    write_unmasked_text_message,
};
pub(crate) use message::{
    ExpectedTurnStart, LifecycleMessage, LifecycleStage, TerminalMessage, TurnStartResponse,
};

pub const STARTED_AT_MS: u64 = 38_002;
pub const COMPLETED_AT_MS: u64 = 38_003;

/// Authored text repeated by the fixture without assembling the logical value.
pub const TEXT_PATTERN: &str = "Příliš žluťoučký 🦀 \"stream\" \\\n\t";

const RUNTIME_PATH_STORAGE_BYTES: usize = 4 * 1024;
const CAS_ID_STORAGE_BYTES: usize = 64;

#[derive(Clone, Copy)]
pub struct InputSpec {
    kind: InputKind,
}

#[derive(Clone, Copy)]
enum InputKind {
    MarkerFree {
        repetitions: NonZeroU64,
    },
    AlternatingImages {
        marker_count: NonZeroU64,
        repetitions: NonZeroU64,
        runtime_path: BoundedRuntimePath,
    },
}

#[derive(Clone, Copy)]
struct BoundedRuntimePath {
    bytes: [u8; RUNTIME_PATH_STORAGE_BYTES],
    len: u16,
}

#[derive(Clone, Copy, Eq, PartialEq)]
pub struct RunIdentity {
    run_id: u64,
    thread_id: BoundedCasId,
    turn_id: BoundedCasId,
    item_id: BoundedCasId,
}

#[derive(Clone, Copy, Eq, PartialEq)]
struct BoundedCasId {
    bytes: [u8; CAS_ID_STORAGE_BYTES],
    len: u8,
}

impl RunIdentity {
    #[must_use]
    pub fn new(run_id: u64) -> Self {
        Self {
            run_id,
            thread_id: BoundedCasId::new("phase38-cas-thread-", run_id),
            turn_id: BoundedCasId::new("phase38-cas-turn-", run_id),
            item_id: BoundedCasId::new("phase38-user-item-", run_id),
        }
    }

    pub const fn run_id(self) -> u64 {
        self.run_id
    }

    pub fn thread_id(&self) -> &str {
        self.thread_id.as_str()
    }

    pub fn turn_id(&self) -> &str {
        self.turn_id.as_str()
    }

    pub fn item_id(&self) -> &str {
        self.item_id.as_str()
    }
}

impl BoundedCasId {
    fn new(prefix: &str, run_id: u64) -> Self {
        let mut value = Self {
            bytes: [0; CAS_ID_STORAGE_BYTES],
            len: 0,
        };
        fmt::Write::write_fmt(&mut value, format_args!("{prefix}{run_id}"))
            .expect("phase38 CAS identity fits fixed storage");
        value
    }

    fn as_str(&self) -> &str {
        std::str::from_utf8(&self.bytes[..usize::from(self.len)])
            .expect("phase38 CAS identities are ASCII")
    }
}

impl fmt::Write for BoundedCasId {
    fn write_str(&mut self, value: &str) -> fmt::Result {
        let start = usize::from(self.len);
        let end = start.checked_add(value.len()).ok_or(fmt::Error)?;
        if end > self.bytes.len() {
            return Err(fmt::Error);
        }
        self.bytes[start..end].copy_from_slice(value.as_bytes());
        self.len = u8::try_from(end).map_err(|_| fmt::Error)?;
        Ok(())
    }
}

impl fmt::Debug for RunIdentity {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RunIdentity")
            .field("run_id", &self.run_id)
            .field("thread_id", &self.thread_id.as_str())
            .field("turn_id", &self.turn_id.as_str())
            .field("item_id", &self.item_id.as_str())
            .finish()
    }
}

impl InputSpec {
    #[must_use]
    pub fn marker_free(repetitions: u64) -> Self {
        Self {
            kind: InputKind::MarkerFree {
                repetitions: NonZeroU64::new(repetitions)
                    .expect("marker-free pattern repetition must be nonzero"),
            },
        }
    }

    #[must_use]
    pub fn alternating_images(
        marker_count: u64,
        repetitions_per_text: u64,
        runtime_path: &str,
    ) -> Self {
        let marker_count =
            NonZeroU64::new(marker_count).expect("alternating image marker count must be nonzero");
        marker_count
            .get()
            .checked_mul(2)
            .and_then(|count| count.checked_add(1))
            .expect("alternating descriptor count must fit u64");
        Self {
            kind: InputKind::AlternatingImages {
                marker_count,
                repetitions: NonZeroU64::new(repetitions_per_text)
                    .expect("each alternating text run must be nonempty"),
                runtime_path: BoundedRuntimePath::new(runtime_path),
            },
        }
    }

    pub(crate) const fn marker_count(&self) -> Option<NonZeroU64> {
        match &self.kind {
            InputKind::MarkerFree { .. } => None,
            InputKind::AlternatingImages { marker_count, .. } => Some(*marker_count),
        }
    }

    pub(crate) const fn repetitions(&self) -> NonZeroU64 {
        match &self.kind {
            InputKind::MarkerFree { repetitions }
            | InputKind::AlternatingImages { repetitions, .. } => *repetitions,
        }
    }

    pub(crate) const fn runtime_path_byte(&self, index: usize) -> Option<u8> {
        match &self.kind {
            InputKind::MarkerFree { .. } => None,
            InputKind::AlternatingImages { runtime_path, .. } => {
                if index < runtime_path.len as usize {
                    Some(runtime_path.bytes[index])
                } else {
                    None
                }
            }
        }
    }
}

impl BoundedRuntimePath {
    fn new(path: &str) -> Self {
        assert!(!path.is_empty(), "runtime image path must not be empty");
        assert!(
            path.len() <= RUNTIME_PATH_STORAGE_BYTES,
            "runtime image path exceeded fixed test storage"
        );
        let mut bytes = [0; RUNTIME_PATH_STORAGE_BYTES];
        bytes[..path.len()].copy_from_slice(path.as_bytes());
        Self {
            bytes,
            len: u16::try_from(path.len()).unwrap(),
        }
    }
}

impl fmt::Debug for InputSpec {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.kind {
            InputKind::MarkerFree { repetitions } => formatter
                .debug_struct("MarkerFree")
                .field("repetitions", &repetitions)
                .finish(),
            InputKind::AlternatingImages {
                marker_count,
                repetitions,
                runtime_path,
            } => formatter
                .debug_struct("AlternatingImages")
                .field("marker_count", &marker_count)
                .field("repetitions", &repetitions)
                .field("runtime_path_bytes", &runtime_path.len)
                .finish(),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RequestObservation {
    request_id: u64,
    logical_bytes: u64,
    frame_count: u64,
    maximum_frame_payload_bytes: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RequestOutcome {
    Complete(RequestObservation),
    Aborted(RequestAbortObservation),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RequestAbortObservation {
    request_id: u64,
    compared_bytes: u64,
    frame_count: u64,
    maximum_frame_payload_bytes: usize,
    reason: RequestAbortReason,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RequestAbortReason {
    PeerClose,
    TransportEof,
    ServerByteCutoff,
    ServerFrameCutoff,
}

impl RequestObservation {
    pub(crate) const fn new(
        request_id: u64,
        logical_bytes: u64,
        frame_count: u64,
        maximum_frame_payload_bytes: usize,
    ) -> Self {
        Self {
            request_id,
            logical_bytes,
            frame_count,
            maximum_frame_payload_bytes,
        }
    }

    pub const fn request_id(self) -> u64 {
        self.request_id
    }

    pub const fn logical_bytes(self) -> u64 {
        self.logical_bytes
    }

    pub const fn frame_count(self) -> u64 {
        self.frame_count
    }

    pub const fn maximum_frame_payload_bytes(self) -> usize {
        self.maximum_frame_payload_bytes
    }
}

impl RequestAbortObservation {
    pub(crate) const fn new(
        request_id: u64,
        compared_bytes: u64,
        frame_count: u64,
        maximum_frame_payload_bytes: usize,
        reason: RequestAbortReason,
    ) -> Self {
        Self {
            request_id,
            compared_bytes,
            frame_count,
            maximum_frame_payload_bytes,
            reason,
        }
    }

    pub const fn request_id(self) -> u64 {
        self.request_id
    }

    pub const fn compared_bytes(self) -> u64 {
        self.compared_bytes
    }

    pub const fn frame_count(self) -> u64 {
        self.frame_count
    }

    pub const fn maximum_frame_payload_bytes(self) -> usize {
        self.maximum_frame_payload_bytes
    }

    pub const fn reason(self) -> RequestAbortReason {
        self.reason
    }
}
