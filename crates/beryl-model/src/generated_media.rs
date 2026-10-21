#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum GeneratedMediaUnavailableReason {
    Missing = 0,
    Changed = 1,
    Oversized = 2,
    Unsupported = 3,
    Unreadable = 4,
    ProducerFailed = 5,
    LostSourceAuthority = 6,
    MalformedOutput = 7,
}

impl GeneratedMediaUnavailableReason {
    pub const fn code(self) -> u8 {
        self as u8
    }

    pub const fn from_code(code: u8) -> Option<Self> {
        match code {
            0 => Some(Self::Missing),
            1 => Some(Self::Changed),
            2 => Some(Self::Oversized),
            3 => Some(Self::Unsupported),
            4 => Some(Self::Unreadable),
            5 => Some(Self::ProducerFailed),
            6 => Some(Self::LostSourceAuthority),
            7 => Some(Self::MalformedOutput),
            _ => None,
        }
    }
}
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct GeneratedSourceIncarnation([u8; 32]);

impl GeneratedSourceIncarnation {
    pub const fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

