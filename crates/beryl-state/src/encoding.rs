use std::{error::Error, fmt};

use beryl_model::{
    AdmittedHostPath, Availability, PathFlavor, RootId, RuntimeId, RuntimeMode, RuntimeNativePath,
    UnavailableReason,
};

use crate::{AvailabilitySnapshot, RecordRevision, UnixMillis};

#[derive(Debug)]
pub(crate) enum CodecError {
    Truncated,
    TrailingBytes,
    InvalidTag {
        kind: &'static str,
        tag: u8,
    },
    InvalidLength {
        kind: &'static str,
    },
    InvalidUtf8 {
        kind: &'static str,
    },
    InvalidValue {
        kind: &'static str,
        source: Box<dyn Error + Send + Sync>,
    },
}

impl fmt::Display for CodecError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Truncated => formatter.write_str("record payload is truncated"),
            Self::TrailingBytes => formatter.write_str("record payload has trailing bytes"),
            Self::InvalidTag { kind, tag } => write!(formatter, "invalid {kind} tag {tag}"),
            Self::InvalidLength { kind } => write!(formatter, "invalid {kind} byte length"),
            Self::InvalidUtf8 { kind } => write!(formatter, "{kind} is not valid UTF-8"),
            Self::InvalidValue { kind, source } => write!(formatter, "invalid {kind}: {source}"),
        }
    }
}

impl Error for CodecError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::InvalidValue { source, .. } => Some(source.as_ref()),
            _ => None,
        }
    }
}

pub(crate) struct Encoder {
    bytes: Vec<u8>,
}

impl Encoder {
    pub(crate) fn new() -> Self {
        Self { bytes: Vec::new() }
    }

    pub(crate) fn finish(self) -> Vec<u8> {
        self.bytes
    }

    pub(crate) fn u8(&mut self, value: u8) {
        self.bytes.push(value);
    }

    pub(crate) fn u16(&mut self, value: u16) {
        self.bytes.extend_from_slice(&value.to_be_bytes());
    }

    pub(crate) fn u32(&mut self, value: u32) {
        self.bytes.extend_from_slice(&value.to_be_bytes());
    }

    pub(crate) fn i32(&mut self, value: i32) {
        self.bytes.extend_from_slice(&value.to_be_bytes());
    }

    pub(crate) fn u64(&mut self, value: u64) {
        self.bytes.extend_from_slice(&value.to_be_bytes());
    }

    pub(crate) fn fixed(&mut self, value: &[u8; 16]) {
        self.bytes.extend_from_slice(value);
    }

    pub(crate) fn fixed_32(&mut self, value: &[u8; 32]) {
        self.bytes.extend_from_slice(value);
    }

    pub(crate) fn padded(&mut self, value: &[u8], capacity: usize) {
        debug_assert!(value.len() <= capacity);
        self.bytes.extend_from_slice(value);
        self.bytes
            .resize(self.bytes.len() + capacity - value.len(), 0);
    }

    pub(crate) fn text(&mut self, value: &str) {
        let length = u32::try_from(value.len()).expect("bounded schema text fits u32");
        self.bytes.extend_from_slice(&length.to_be_bytes());
        self.bytes.extend_from_slice(value.as_bytes());
    }

    pub(crate) fn optional_time(&mut self, value: Option<UnixMillis>) {
        match value {
            Some(value) => {
                self.u8(1);
                self.u64(value.get());
            }
            None => self.u8(0),
        }
    }

    pub(crate) fn runtime_mode(&mut self, value: &RuntimeMode) {
        match value {
            RuntimeMode::Host => self.u8(0),
            RuntimeMode::Wsl(distribution) => {
                self.u8(1);
                self.text(distribution.as_str());
            }
        }
    }

    pub(crate) fn path_flavor(&mut self, value: PathFlavor) {
        self.u8(match value {
            PathFlavor::Windows => 0,
            PathFlavor::Posix => 1,
        });
    }

    pub(crate) fn host_path(&mut self, value: &AdmittedHostPath) {
        self.path_flavor(value.flavor());
        self.text(value.as_str());
    }

    pub(crate) fn runtime_path(&mut self, value: &RuntimeNativePath) {
        self.runtime_mode(value.mode());
        self.path_flavor(value.flavor());
        self.text(value.as_str());
    }

    pub(crate) fn availability(&mut self, value: AvailabilitySnapshot) {
        match value.availability() {
            Availability::Unknown => self.u8(0),
            Availability::Available => self.u8(1),
            Availability::Unavailable(reason) => {
                self.u8(2);
                self.u8(encode_unavailable_reason(reason));
            }
        }
        self.optional_time(value.observed_at());
    }
}

pub(crate) struct Decoder<'a> {
    remaining: &'a [u8],
}

impl<'a> Decoder<'a> {
    pub(crate) const fn new(bytes: &'a [u8]) -> Self {
        Self { remaining: bytes }
    }

    pub(crate) fn finish(self) -> Result<(), CodecError> {
        if self.remaining.is_empty() {
            Ok(())
        } else {
            Err(CodecError::TrailingBytes)
        }
    }

    fn take(&mut self, length: usize) -> Result<&'a [u8], CodecError> {
        if self.remaining.len() < length {
            return Err(CodecError::Truncated);
        }
        let (value, remaining) = self.remaining.split_at(length);
        self.remaining = remaining;
        Ok(value)
    }

    pub(crate) fn u8(&mut self) -> Result<u8, CodecError> {
        Ok(self.take(1)?[0])
    }

    pub(crate) fn u16(&mut self) -> Result<u16, CodecError> {
        let bytes: [u8; 2] = self
            .take(2)?
            .try_into()
            .map_err(|_| CodecError::Truncated)?;
        Ok(u16::from_be_bytes(bytes))
    }

    pub(crate) fn u32(&mut self) -> Result<u32, CodecError> {
        let bytes: [u8; 4] = self
            .take(4)?
            .try_into()
            .map_err(|_| CodecError::Truncated)?;
        Ok(u32::from_be_bytes(bytes))
    }

    pub(crate) fn i32(&mut self) -> Result<i32, CodecError> {
        let bytes: [u8; 4] = self
            .take(4)?
            .try_into()
            .map_err(|_| CodecError::Truncated)?;
        Ok(i32::from_be_bytes(bytes))
    }

    pub(crate) fn u64(&mut self) -> Result<u64, CodecError> {
        let bytes: [u8; 8] = self
            .take(8)?
            .try_into()
            .map_err(|_| CodecError::Truncated)?;
        Ok(u64::from_be_bytes(bytes))
    }

    pub(crate) fn fixed(&mut self) -> Result<[u8; 16], CodecError> {
        self.take(16)?.try_into().map_err(|_| CodecError::Truncated)
    }

    pub(crate) fn fixed_32(&mut self) -> Result<[u8; 32], CodecError> {
        self.take(32)?.try_into().map_err(|_| CodecError::Truncated)
    }

    pub(crate) fn bytes(&mut self, length: usize) -> Result<&'a [u8], CodecError> {
        self.take(length)
    }

    pub(crate) fn text(&mut self, kind: &'static str) -> Result<&'a str, CodecError> {
        let length: [u8; 4] = self
            .take(4)?
            .try_into()
            .map_err(|_| CodecError::Truncated)?;
        let length = u32::from_be_bytes(length) as usize;
        std::str::from_utf8(self.take(length)?).map_err(|_| CodecError::InvalidUtf8 { kind })
    }

    pub(crate) fn optional_time(&mut self) -> Result<Option<UnixMillis>, CodecError> {
        match self.u8()? {
            0 => Ok(None),
            1 => Ok(Some(UnixMillis::new(self.u64()?))),
            tag => Err(CodecError::InvalidTag {
                kind: "optional time",
                tag,
            }),
        }
    }

    pub(crate) fn runtime_mode(&mut self) -> Result<RuntimeMode, CodecError> {
        match self.u8()? {
            0 => Ok(RuntimeMode::host()),
            1 => RuntimeMode::wsl(self.text("WSL distribution name")?)
                .map_err(|source| invalid("runtime mode", source)),
            tag => Err(CodecError::InvalidTag {
                kind: "runtime mode",
                tag,
            }),
        }
    }

    pub(crate) fn path_flavor(&mut self) -> Result<PathFlavor, CodecError> {
        match self.u8()? {
            0 => Ok(PathFlavor::Windows),
            1 => Ok(PathFlavor::Posix),
            tag => Err(CodecError::InvalidTag {
                kind: "path flavor",
                tag,
            }),
        }
    }

    pub(crate) fn host_path(&mut self) -> Result<AdmittedHostPath, CodecError> {
        let flavor = self.path_flavor()?;
        let value = self.text("host path")?;
        AdmittedHostPath::from_admitted(flavor, value)
            .map_err(|source| invalid("host path", source))
    }

    pub(crate) fn runtime_path(&mut self) -> Result<RuntimeNativePath, CodecError> {
        let mode = self.runtime_mode()?;
        let flavor = self.path_flavor()?;
        let value = self.text("runtime-native path")?;
        RuntimeNativePath::from_admitted(mode, flavor, value)
            .map_err(|source| invalid("runtime-native path", source))
    }

    pub(crate) fn availability(&mut self) -> Result<AvailabilitySnapshot, CodecError> {
        let availability = match self.u8()? {
            0 => Availability::Unknown,
            1 => Availability::Available,
            2 => Availability::Unavailable(decode_unavailable_reason(self.u8()?)?),
            tag => {
                return Err(CodecError::InvalidTag {
                    kind: "availability",
                    tag,
                });
            }
        };
        AvailabilitySnapshot::from_parts(availability, self.optional_time()?)
            .map_err(|source| invalid("availability snapshot", source))
    }

    pub(crate) fn record_revision(&mut self) -> Result<RecordRevision, CodecError> {
        RecordRevision::new(self.u64()?).map_err(|source| invalid("record revision", source))
    }
}

pub(crate) fn encode_runtime_id(value: RuntimeId) -> Vec<u8> {
    value.as_bytes().to_vec()
}

pub(crate) fn decode_runtime_id(bytes: &[u8]) -> Result<RuntimeId, CodecError> {
    decode_identity(bytes, "runtime identity").map(RuntimeId::from_bytes)
}

pub(crate) fn encode_root_id(value: RootId) -> Vec<u8> {
    value.as_bytes().to_vec()
}

pub(crate) fn decode_root_id(bytes: &[u8]) -> Result<RootId, CodecError> {
    decode_identity(bytes, "root identity").map(RootId::from_bytes)
}

fn decode_identity(bytes: &[u8], kind: &'static str) -> Result<[u8; 16], CodecError> {
    bytes
        .try_into()
        .map_err(|_| CodecError::InvalidLength { kind })
}

fn invalid(kind: &'static str, source: impl Error + Send + Sync + 'static) -> CodecError {
    CodecError::InvalidValue {
        kind,
        source: Box::new(source),
    }
}

fn encode_unavailable_reason(reason: UnavailableReason) -> u8 {
    match reason {
        UnavailableReason::NotFound => 0,
        UnavailableReason::AccessDenied => 1,
        UnavailableReason::EnvironmentUnavailable => 2,
        UnavailableReason::BackendUnavailable => 3,
        UnavailableReason::StoreUnavailable => 4,
        UnavailableReason::OpenElsewhere => 5,
        UnavailableReason::Unsupported => 6,
        UnavailableReason::Invalid => 7,
    }
}

fn decode_unavailable_reason(tag: u8) -> Result<UnavailableReason, CodecError> {
    match tag {
        0 => Ok(UnavailableReason::NotFound),
        1 => Ok(UnavailableReason::AccessDenied),
        2 => Ok(UnavailableReason::EnvironmentUnavailable),
        3 => Ok(UnavailableReason::BackendUnavailable),
        4 => Ok(UnavailableReason::StoreUnavailable),
        5 => Ok(UnavailableReason::OpenElsewhere),
        6 => Ok(UnavailableReason::Unsupported),
        7 => Ok(UnavailableReason::Invalid),
        tag => Err(CodecError::InvalidTag {
            kind: "unavailable reason",
            tag,
        }),
    }
}
