use std::fmt;

use sha2::{Digest, Sha256};

use crate::RecoveryItemSequenceDigest;

const RECOVERY_DIGEST_DOMAIN: &[u8] = b"beryl.syndic.recovery-item-sequence.v1\0";

/// Closed role and text-shape tag covered by one recovery-sequence digest.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum RecoveryItemSequenceRole {
    /// One user-role message containing one `input_text` value.
    UserInputText,
    /// One assistant-role message containing one `output_text` value.
    AssistantOutputText,
}

impl RecoveryItemSequenceRole {
    const fn digest_tag(self) -> u8 {
        match self {
            Self::UserInputText => 0,
            Self::AssistantOutputText => 1,
        }
    }
}

/// Incremental exact V1 recovery-sequence digest accumulator.
///
/// The accumulator retains only SHA-256 state and compact counters. Callers
/// stream each item's bytes between `begin_item` and `finish_item`.
pub struct RecoveryItemSequenceAccumulator {
    hash: Sha256,
    expected_items: u64,
    expected_utf8_bytes: u64,
    observed_items: u64,
    observed_utf8_bytes: u64,
    active_remaining: Option<u64>,
}

/// Structural disagreement while incrementally proving a recovery sequence.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RecoveryItemSequenceError {
    /// A new item began before the preceding item's declared bytes ended.
    ItemStillActive { remaining_bytes: u64 },
    /// An item did not use the next exact one-based sequence ordinal.
    UnexpectedOrdinal { expected: u64, actual: u64 },
    /// The sequence contained more items than its compact preflight declared.
    TooManyItems { expected: u64 },
    /// Recovery items must contain at least one UTF-8 byte.
    EmptyItem,
    /// Declared item bytes would exceed the compact preflight total.
    TooManyUtf8Bytes { expected: u64, actual: u64 },
    /// Text bytes arrived without an active item.
    NoActiveItem,
    /// Text bytes crossed the active item's declared end.
    ItemTextOverflow { remaining: u64, supplied: u64 },
    /// The sequence ended with a different item count than preflight.
    ItemCountMismatch { expected: u64, actual: u64 },
    /// The sequence ended with a different UTF-8 byte count than preflight.
    Utf8ByteCountMismatch { expected: u64, actual: u64 },
}

impl RecoveryItemSequenceAccumulator {
    /// Starts an accumulator for the exact compact preflight totals.
    #[must_use]
    pub fn new(expected_items: u64, expected_utf8_bytes: u64) -> Self {
        let mut hash = Sha256::new();
        hash.update(RECOVERY_DIGEST_DOMAIN);
        hash.update(expected_items.to_be_bytes());
        hash.update(expected_utf8_bytes.to_be_bytes());
        Self {
            hash,
            expected_items,
            expected_utf8_bytes,
            observed_items: 0,
            observed_utf8_bytes: 0,
            active_remaining: None,
        }
    }

    /// Begins the next exact item before any of its text bytes are supplied.
    pub fn begin_item(
        &mut self,
        ordinal: u64,
        role: RecoveryItemSequenceRole,
        utf8_bytes: u64,
    ) -> Result<(), RecoveryItemSequenceError> {
        if let Some(remaining_bytes) = self.active_remaining {
            return Err(RecoveryItemSequenceError::ItemStillActive { remaining_bytes });
        }
        let expected = self.observed_items.saturating_add(1);
        if ordinal != expected {
            return Err(RecoveryItemSequenceError::UnexpectedOrdinal {
                expected,
                actual: ordinal,
            });
        }
        if self.observed_items >= self.expected_items {
            return Err(RecoveryItemSequenceError::TooManyItems {
                expected: self.expected_items,
            });
        }
        if utf8_bytes == 0 {
            return Err(RecoveryItemSequenceError::EmptyItem);
        }
        let actual = self.observed_utf8_bytes.saturating_add(utf8_bytes);
        if actual > self.expected_utf8_bytes {
            return Err(RecoveryItemSequenceError::TooManyUtf8Bytes {
                expected: self.expected_utf8_bytes,
                actual,
            });
        }

        self.hash.update(ordinal.to_be_bytes());
        self.hash.update([role.digest_tag()]);
        self.hash.update(utf8_bytes.to_be_bytes());
        self.active_remaining = Some(utf8_bytes);
        Ok(())
    }

    /// Adds the next exact bytes of the active item.
    pub fn update_text(&mut self, bytes: &[u8]) -> Result<(), RecoveryItemSequenceError> {
        let remaining = self
            .active_remaining
            .ok_or(RecoveryItemSequenceError::NoActiveItem)?;
        let supplied = u64::try_from(bytes.len()).unwrap_or(u64::MAX);
        if supplied > remaining {
            return Err(RecoveryItemSequenceError::ItemTextOverflow {
                remaining,
                supplied,
            });
        }
        self.hash.update(bytes);
        self.observed_utf8_bytes = self
            .observed_utf8_bytes
            .checked_add(supplied)
            .expect("accepted item bytes stay within a u64 preflight total");
        self.active_remaining = Some(remaining - supplied);
        Ok(())
    }

    /// Finishes the active item after its exact declared byte length arrived.
    pub fn finish_item(&mut self) -> Result<(), RecoveryItemSequenceError> {
        match self.active_remaining {
            None => return Err(RecoveryItemSequenceError::NoActiveItem),
            Some(remaining_bytes) if remaining_bytes != 0 => {
                return Err(RecoveryItemSequenceError::ItemStillActive { remaining_bytes });
            }
            Some(_) => {}
        }
        self.active_remaining = None;
        self.observed_items = self
            .observed_items
            .checked_add(1)
            .expect("accepted item count stays within a u64 preflight total");
        Ok(())
    }

    /// Finishes the sequence only when every compact preflight total agrees.
    pub fn finish(self) -> Result<RecoveryItemSequenceDigest, RecoveryItemSequenceError> {
        if let Some(remaining_bytes) = self.active_remaining {
            return Err(RecoveryItemSequenceError::ItemStillActive { remaining_bytes });
        }
        if self.observed_items != self.expected_items {
            return Err(RecoveryItemSequenceError::ItemCountMismatch {
                expected: self.expected_items,
                actual: self.observed_items,
            });
        }
        if self.observed_utf8_bytes != self.expected_utf8_bytes {
            return Err(RecoveryItemSequenceError::Utf8ByteCountMismatch {
                expected: self.expected_utf8_bytes,
                actual: self.observed_utf8_bytes,
            });
        }
        Ok(RecoveryItemSequenceDigest::from_bytes(
            self.hash.finalize().into(),
        ))
    }
}

impl fmt::Display for RecoveryItemSequenceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ItemStillActive { remaining_bytes } => write!(
                formatter,
                "recovery item still has {remaining_bytes} declared bytes remaining"
            ),
            Self::UnexpectedOrdinal { expected, actual } => write!(
                formatter,
                "recovery item ordinal {actual} did not match expected ordinal {expected}"
            ),
            Self::TooManyItems { expected } => write!(
                formatter,
                "recovery sequence exceeded its declared {expected}-item total"
            ),
            Self::EmptyItem => formatter.write_str("recovery item text must not be empty"),
            Self::TooManyUtf8Bytes { expected, actual } => write!(
                formatter,
                "recovery sequence declared {actual} UTF-8 bytes beyond its {expected}-byte total"
            ),
            Self::NoActiveItem => formatter.write_str("recovery sequence has no active item"),
            Self::ItemTextOverflow {
                remaining,
                supplied,
            } => write!(
                formatter,
                "recovery item received {supplied} bytes with only {remaining} bytes remaining"
            ),
            Self::ItemCountMismatch { expected, actual } => write!(
                formatter,
                "recovery sequence ended with {actual} items instead of {expected}"
            ),
            Self::Utf8ByteCountMismatch { expected, actual } => write!(
                formatter,
                "recovery sequence ended with {actual} UTF-8 bytes instead of {expected}"
            ),
        }
    }
}

impl std::error::Error for RecoveryItemSequenceError {}
