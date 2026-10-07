use super::types::MAGIC;
use super::*;
struct Cursor<'a> {
    bytes: &'a [u8],
    offset: usize,
}
impl<'a> Cursor<'a> {
    fn take(&mut self, count: usize) -> Result<&'a [u8]> {
        let end = self
            .offset
            .checked_add(count)
            .ok_or(CodecError::Invalid("overflow"))?;
        let bytes = self
            .bytes
            .get(self.offset..end)
            .ok_or(CodecError::Invalid("truncated payload"))?;
        self.offset = end;
        Ok(bytes)
    }
    fn byte(&mut self) -> Result<u8> {
        Ok(self.take(1)?[0])
    }
    fn flag(&mut self) -> Result<bool> {
        match self.byte()? {
            0 => Ok(false),
            1 => Ok(true),
            _ => invalid("invalid boolean"),
        }
    }
    fn number(&mut self) -> Result<u32> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }
    fn bytes(&mut self, maximum: usize) -> Result<Vec<u8>> {
        let count = self.number()? as usize;
        if count > maximum {
            return invalid("field exceeds bound");
        }
        Ok(self.take(count)?.to_vec())
    }
    fn text(&mut self, maximum: usize, path: bool) -> Result<String> {
        let bytes = self.bytes(maximum)?;
        let text = String::from_utf8(bytes).map_err(|_| CodecError::Invalid("non UTF-8 text"))?;
        validate_text(&text, maximum, path)?;
        Ok(text)
    }
    fn tail(&mut self) -> Result<DiagnosticTail> {
        Ok(DiagnosticTail {
            truncated: self.flag()?,
            bytes: self.bytes(MAX_DIAGNOSTIC_LEN)?,
        })
    }
}
pub(super) fn header(bytes: &[u8], expected: Option<&Nonce>) -> Result<(Nonce, usize, u16)> {
    if bytes.len() < HEADER_LEN {
        return invalid("truncated header");
    }
    if &bytes[..8] != MAGIC
        || u16::from_le_bytes(bytes[8..10].try_into().unwrap()) != PROTOCOL_VERSION
    {
        return invalid("invalid magic or version");
    }
    let tag = u16::from_le_bytes(bytes[10..12].try_into().unwrap());
    if !matches!(tag, 1..=5 | 101..=106) {
        return invalid("unknown frame tag");
    }
    let length = u32::from_le_bytes(bytes[12..16].try_into().unwrap()) as usize;
    if length > MAX_PAYLOAD_LEN {
        return invalid("frame exceeds bound");
    }
    let nonce: Nonce = bytes[16..48].try_into().unwrap();
    if expected.is_some_and(|expected| *expected != nonce) {
        return invalid("nonce mismatch");
    }
    Ok((nonce, length, tag))
}
pub fn decode_frame(expected_nonce: &Nonce, bytes: &[u8]) -> Result<Frame> {
    let (_, length, tag) = header(bytes, Some(expected_nonce))?;
    if bytes.len() != HEADER_LEN + length {
        return invalid("truncated or trailing frame");
    }
    let mut payload = Cursor {
        bytes: &bytes[HEADER_LEN..],
        offset: 0,
    };
    let frame = match tag {
        1 => Frame::Initialize,
        2 => {
            let executable = payload.text(MAX_PATH_LEN, true)?;
            let execution_root = payload.text(MAX_PATH_LEN, true)?;
            let count = payload.number()? as usize;
            if count > 64 {
                return invalid("too many arguments");
            }
            let mut arguments = Vec::with_capacity(count);
            for _ in 0..count {
                arguments.push(payload.text(8192, false)?);
            }
            Frame::LaunchServer {
                executable,
                execution_root,
                arguments,
            }
        }
        3 => {
            let kind = match payload.byte()? {
                1 => ObservationKind::Executable,
                2 => ObservationKind::Directory,
                3 => ObservationKind::Home,
                _ => return invalid("unknown observation"),
            };
            let timeout_ms = payload.number()?;
            if timeout_ms == 0 || timeout_ms > 30_000 {
                return invalid("invalid observation deadline");
            }
            let path = if kind == ObservationKind::Home {
                None
            } else {
                Some(payload.text(MAX_PATH_LEN, true)?)
            };
            Frame::Observe {
                kind,
                path,
                timeout_ms,
            }
        }
        4 => Frame::Stop,
        5 => Frame::BrokerClose,
        101 => Frame::Ready {
            role: match payload.byte()? {
                1 => Role::ContextBroker,
                2 => Role::Supervisor,
                _ => return invalid("unknown role"),
            },
        },
        102 => Frame::WorkloadStarted,
        103 => Frame::ShutdownPending,
        104 => {
            let kind = payload.byte()?;
            let code = payload.number()? as i32;
            let exit = match kind {
                1 if (0..=255).contains(&code) => ExitStatus::Exited(code),
                2 if (1..=64).contains(&code) => ExitStatus::Signaled(code),
                _ => return invalid("invalid exit status"),
            };
            let observation = if payload.flag()? {
                Some(payload.text(MAX_PATH_LEN, true)?)
            } else {
                None
            };
            Frame::OwnedNamespaceClosed {
                result: WorkloadResult {
                    exit,
                    observation,
                    stdout: payload.tail()?,
                    stderr: payload.tail()?,
                },
            }
        }
        105 => Frame::LinuxCompanionsClosed,
        106 => {
            let kind = match payload.byte()? {
                1 => FailureKind::Protocol,
                2 => FailureKind::Unsupported,
                3 => FailureKind::Context,
                4 => FailureKind::Namespace,
                5 => FailureKind::Credentials,
                6 => FailureKind::WorkingDirectory,
                7 => FailureKind::Exec,
                8 => FailureKind::Observation,
                9 => FailureKind::Control,
                10 => FailureKind::Timeout,
                11 => FailureKind::System,
                _ => return invalid("unknown failure"),
            };
            let errno = if payload.flag()? {
                let number = payload.number()?;
                if number == 0 || number > 4095 {
                    return invalid("invalid errno");
                }
                Some(number as i32)
            } else {
                None
            };
            Frame::Failure { kind, errno }
        }
        _ => return invalid("unknown frame"),
    };
    if payload.offset != length {
        return invalid("trailing payload");
    }
    Ok(frame)
}
