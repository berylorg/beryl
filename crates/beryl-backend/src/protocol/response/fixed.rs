use std::fmt;

use thiserror::Error;

pub const PROTOCOL_IDENTITY_MAX_BYTES: usize = 256;
pub const MODEL_DISPLAY_NAME_MAX_BYTES: usize = 1_024;
pub const MODEL_CURSOR_MAX_BYTES: usize = 1_024;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum BoundedResponseTextError {
    #[error("bounded response text must not be empty")]
    Empty,
    #[error("bounded response text is {actual} UTF-8 bytes; maximum is {maximum}")]
    TooLong { actual: usize, maximum: usize },
    #[error("initialize user-agent product must be one token")]
    InvalidUserAgentProduct,
}

#[derive(Clone, PartialEq, Eq)]
pub(super) struct InlineUtf8<const N: usize> {
    bytes: [u8; N],
    len: u16,
}

impl<const N: usize> InlineUtf8<N> {
    pub(super) fn try_new(value: &str) -> Result<Self, BoundedResponseTextError> {
        if value.is_empty() {
            return Err(BoundedResponseTextError::Empty);
        }
        if value.len() > N {
            return Err(BoundedResponseTextError::TooLong {
                actual: value.len(),
                maximum: N,
            });
        }
        let len = u16::try_from(value.len()).expect("response text limits fit u16");
        let mut bytes = [0; N];
        bytes[..value.len()].copy_from_slice(value.as_bytes());
        Ok(Self { bytes, len })
    }

    pub(super) fn projected(value: &str) -> (Self, bool) {
        let mut end = value.len().min(N);
        while !value.is_char_boundary(end) {
            end -= 1;
        }
        let mut bytes = [0; N];
        bytes[..end].copy_from_slice(&value.as_bytes()[..end]);
        (
            Self {
                bytes,
                len: u16::try_from(end).expect("response text limits fit u16"),
            },
            end != value.len(),
        )
    }

    pub(super) fn as_str(&self) -> &str {
        std::str::from_utf8(&self.bytes[..usize::from(self.len)])
            .expect("inline response text is constructed from UTF-8")
    }
}

impl<const N: usize> fmt::Debug for InlineUtf8<N> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_tuple("InlineUtf8")
            .field(&self.as_str())
            .finish()
    }
}

#[derive(Clone, PartialEq, Eq)]
pub struct ProtocolIdentity(InlineUtf8<PROTOCOL_IDENTITY_MAX_BYTES>);

impl ProtocolIdentity {
    pub fn try_new(value: &str) -> Result<Self, BoundedResponseTextError> {
        InlineUtf8::try_new(value).map(Self)
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl fmt::Debug for ProtocolIdentity {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_tuple("ProtocolIdentity")
            .field(&self.as_str())
            .finish()
    }
}

macro_rules! bounded_text {
    ($name:ident, $limit:ident) => {
        #[derive(PartialEq, Eq)]
        pub struct $name(InlineUtf8<$limit>);

        impl $name {
            pub fn try_new(value: &str) -> Result<Self, BoundedResponseTextError> {
                InlineUtf8::try_new(value).map(Self)
            }

            #[must_use]
            pub fn as_str(&self) -> &str {
                self.0.as_str()
            }
        }

        impl fmt::Debug for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter
                    .debug_tuple(stringify!($name))
                    .field(&self.as_str())
                    .finish()
            }
        }
    };
}

bounded_text!(ModelDisplayName, MODEL_DISPLAY_NAME_MAX_BYTES);
bounded_text!(ModelPageCursor, MODEL_CURSOR_MAX_BYTES);
