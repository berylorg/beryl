use super::types::MAGIC;
use super::*;
struct Cursor<'a> {
    bytes: &'a [u8],
    offset: usize,
}
impl<'a> Cursor<'a> {
    fn reader(&mut self) -> Result<ReaderId> {
        let id: ReaderId = self.take(16)?.try_into().unwrap();
        if id == [0; 16] {
            return invalid("zero reader identity");
        }
        Ok(id)
    }
    fn wide(&mut self) -> Result<u64> {
        Ok(u64::from_le_bytes(self.take(8)?.try_into().unwrap()))
    }
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
    if !matches!(tag, 1..=10 | 101..=110) {
        return invalid("unknown frame tag");
    }
    let length = u32::from_le_bytes(bytes[12..16].try_into().unwrap()) as usize;
    if length > MAX_PAYLOAD_LEN {
        return invalid("frame exceeds bound");
    }
    let reader_length = match tag {
        6 => length == 4,
        7 => (21..=20 + MAX_PATH_LEN).contains(&length),
        8 => length == 36,
        9 | 10 | 107 => length == 24,
        108 => (37..=37 + MAX_READER_PAGE).contains(&length),
        109 | 110 => length == 25,
        _ => true,
    };
    if !reader_length {
        return invalid("reader frame exceeds bound");
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
        6 => {
            let limit = payload.number()?;
            if limit == 0 {
                return invalid("zero reader allowance");
            }
            Frame::ConfigureReaders { limit }
        }
        7 => {
            let id = payload.reader()?;
            let path = payload.text(MAX_PATH_LEN, true)?;
            if !path.starts_with('/') {
                return invalid("reader path is not absolute");
            }
            Frame::ReaderOpen { id, path }
        }
        8 => {
            let id = payload.reader()?;
            let sequence = payload.wide()?;
            let offset = payload.wide()?;
            let allowance = payload.number()?;
            if sequence == 0
                || allowance == 0
                || allowance as usize > MAX_READER_PAGE
                || offset.checked_add(u64::from(allowance)).is_none()
            {
                return invalid("invalid reader request");
            }
            Frame::ReaderRead {
                id,
                sequence,
                offset,
                allowance,
            }
        }
        9 | 10 => {
            let id = payload.reader()?;
            let sequence = payload.wide()?;
            if sequence == 0 {
                return invalid("zero reader sequence");
            }
            if tag == 9 {
                Frame::ReaderCancel { id, sequence }
            } else {
                Frame::ReaderClose { id, sequence }
            }
        }
        107 => {
            let id = payload.reader()?;
            let length = payload.wide()?;
            if length == 0 {
                return invalid("empty reader source");
            }
            Frame::ReaderOpened { id, length }
        }
        108 => {
            let id = payload.reader()?;
            let sequence = payload.wide()?;
            let offset = payload.wide()?;
            let bytes = payload.bytes(MAX_READER_PAGE)?;
            let eof = payload.flag()?;
            if sequence == 0 || offset.checked_add(bytes.len() as u64).is_none() {
                return invalid("invalid reader page");
            }
            Frame::ReaderPage {
                id,
                sequence,
                offset,
                bytes,
                eof,
            }
        }
        109 => {
            let id = payload.reader()?;
            let sequence = payload.wide()?;
            let kind = match payload.byte()? {
                1 => ReaderFailure::Refused,
                2 => ReaderFailure::Missing,
                3 => ReaderFailure::Unreadable,
                4 => ReaderFailure::Changed,
                5 => ReaderFailure::Protocol,
                6 => ReaderFailure::Cancelled,
                _ => return invalid("invalid reader failure"),
            };
            Frame::ReaderFailed { id, sequence, kind }
        }
        110 => Frame::ReaderClosed {
            id: payload.reader()?,
            sequence: payload.wide()?,
            cancelled: payload.flag()?,
        },
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

pub(super) fn decode_owned(expected_nonce: &Nonce, mut bytes: Vec<u8>) -> Result<Frame> {
    let (_, length, tag) = header(&bytes, Some(expected_nonce))?;
    if tag != 108 {
        return decode_frame(expected_nonce, &bytes);
    }
    if bytes.len() != HEADER_LEN + length {
        return invalid("truncated or trailing frame");
    }
    let mut payload = Cursor {
        bytes: &bytes[HEADER_LEN..],
        offset: 0,
    };
    let id = payload.reader()?;
    let sequence = payload.wide()?;
    let offset = payload.wide()?;
    let count = payload.number()? as usize;
    if sequence == 0 || count > MAX_READER_PAGE || offset.checked_add(count as u64).is_none() {
        return invalid("invalid reader page");
    }
    let start = HEADER_LEN + payload.offset;
    payload.take(count)?;
    let eof = payload.flag()?;
    if payload.offset != length {
        return invalid("trailing payload");
    }
    bytes.copy_within(start..start + count, 0);
    bytes.truncate(count);
    Ok(Frame::ReaderPage {
        id,
        sequence,
        offset,
        bytes,
        eof,
    })
}
