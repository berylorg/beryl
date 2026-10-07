use super::types::MAGIC;
use super::*;
struct Payload(Vec<u8>);
impl Payload {
    fn byte(&mut self, value: u8) {
        self.0.push(value);
    }
    fn number(&mut self, value: u32) {
        self.0.extend_from_slice(&value.to_le_bytes());
    }
    fn bytes(&mut self, bytes: &[u8], maximum: usize) -> Result<()> {
        if bytes.len() > maximum
            || self.0.len().saturating_add(4).saturating_add(bytes.len()) > MAX_PAYLOAD_LEN
        {
            return invalid("field or aggregate exceeds bound");
        }
        self.number(bytes.len() as u32);
        self.0.extend_from_slice(bytes);
        Ok(())
    }
    fn text(&mut self, text: &str, maximum: usize, path: bool) -> Result<()> {
        validate_text(text, maximum, path)?;
        self.bytes(text.as_bytes(), maximum)
    }
    fn tail(&mut self, tail: &DiagnosticTail) -> Result<()> {
        self.byte(u8::from(tail.truncated));
        self.bytes(&tail.bytes, MAX_DIAGNOSTIC_LEN)
    }
}

pub fn encode_frame(nonce: &Nonce, frame: &Frame) -> Result<Vec<u8>> {
    let mut payload = Payload(Vec::new());
    let tag: u16 = match frame {
        Frame::Initialize => 1,
        Frame::LaunchServer {
            executable,
            execution_root,
            arguments,
        } => {
            payload.text(executable, MAX_PATH_LEN, true)?;
            payload.text(execution_root, MAX_PATH_LEN, true)?;
            if arguments.len() > 64 {
                return invalid("too many arguments");
            }
            payload.number(arguments.len() as u32);
            for argument in arguments {
                payload.text(argument, 8192, false)?;
            }
            2
        }
        Frame::Observe {
            kind,
            path,
            timeout_ms,
        } => {
            if *timeout_ms == 0 || *timeout_ms > 30_000 {
                return invalid("invalid observation deadline");
            }
            payload.byte(match kind {
                ObservationKind::Executable => 1,
                ObservationKind::Directory => 2,
                ObservationKind::Home => 3,
            });
            payload.number(*timeout_ms);
            match (kind, path) {
                (ObservationKind::Home, None) => {}
                (ObservationKind::Executable | ObservationKind::Directory, Some(path)) => {
                    payload.text(path, MAX_PATH_LEN, true)?
                }
                _ => return invalid("invalid observation path"),
            }
            3
        }
        Frame::Stop => 4,
        Frame::BrokerClose => 5,
        Frame::Ready { role } => {
            payload.byte(match role {
                Role::ContextBroker => 1,
                Role::Supervisor => 2,
            });
            101
        }
        Frame::WorkloadStarted => 102,
        Frame::ShutdownPending => 103,
        Frame::OwnedNamespaceClosed { result } => {
            match result.exit {
                ExitStatus::Exited(code) if (0..=255).contains(&code) => {
                    payload.byte(1);
                    payload.number(code as u32);
                }
                ExitStatus::Signaled(signal) if (1..=64).contains(&signal) => {
                    payload.byte(2);
                    payload.number(signal as u32);
                }
                _ => return invalid("invalid exit status"),
            }
            payload.byte(u8::from(result.observation.is_some()));
            if let Some(path) = &result.observation {
                payload.text(path, MAX_PATH_LEN, true)?;
            }
            payload.tail(&result.stdout)?;
            payload.tail(&result.stderr)?;
            104
        }
        Frame::LinuxCompanionsClosed => 105,
        Frame::Failure { kind, errno } => {
            payload.byte(match kind {
                FailureKind::Protocol => 1,
                FailureKind::Unsupported => 2,
                FailureKind::Context => 3,
                FailureKind::Namespace => 4,
                FailureKind::Credentials => 5,
                FailureKind::WorkingDirectory => 6,
                FailureKind::Exec => 7,
                FailureKind::Observation => 8,
                FailureKind::Control => 9,
                FailureKind::Timeout => 10,
                FailureKind::System => 11,
            });
            payload.byte(u8::from(errno.is_some()));
            if let Some(errno) = errno {
                if *errno <= 0 || *errno > 4095 {
                    return invalid("invalid errno");
                }
                payload.number(*errno as u32);
            }
            106
        }
    };
    if payload.0.len() > MAX_PAYLOAD_LEN {
        return invalid("frame exceeds bound");
    }
    let mut output = Vec::with_capacity(HEADER_LEN + payload.0.len());
    output.extend_from_slice(MAGIC);
    output.extend_from_slice(&PROTOCOL_VERSION.to_le_bytes());
    output.extend_from_slice(&tag.to_le_bytes());
    output.extend_from_slice(&(payload.0.len() as u32).to_le_bytes());
    output.extend_from_slice(nonce);
    output.extend_from_slice(&payload.0);
    Ok(output)
}
