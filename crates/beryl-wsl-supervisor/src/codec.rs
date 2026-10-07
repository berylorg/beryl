mod decode;
mod encode;
mod reader;
mod types;
pub use decode::decode_frame;
pub use encode::encode_frame;
pub use reader::{ControlState, FrameReader, read_initial_frame};
pub use types::{
    CodecError, DiagnosticTail, ExitStatus, FailureKind, Frame, HEADER_LEN, MAX_DIAGNOSTIC_LEN,
    MAX_PATH_LEN, MAX_PAYLOAD_LEN, MAX_PENDING_FRAMES, Nonce, ObservationKind, PROTOCOL_VERSION,
    Role, WorkloadResult,
};
use types::{Result, invalid};
fn validate_text(text: &str, maximum: usize, path: bool) -> Result<()> {
    if text.len() > maximum || text.as_bytes().contains(&0) || (path && !text.starts_with('/')) {
        return invalid("invalid text or absolute path");
    }
    Ok(())
}
