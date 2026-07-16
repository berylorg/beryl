use beryl_model::CasThreadId;
use serde::{
    Deserialize, Serialize, Serializer,
    ser::{SerializeSeq, SerializeStruct},
};
use thiserror::Error;

use crate::{JsonRpcError, LoadedThreadSession, ManagedBackendError};

/// Maximum number of messages in one normalized thread-injection batch.
pub const THREAD_INJECTION_MAX_ITEMS: usize = 262_144;

/// Maximum canonical UTF-8 message-text bytes in one thread-injection batch.
pub const THREAD_INJECTION_MAX_TEXT_BYTES: usize = 262_144;

/// Exact nonempty text for one normalized thread-injection message.
///
/// Construction preserves the supplied UTF-8 bytes without trimming or
/// normalization. The per-message bound matches the batch bound because a
/// larger message can never belong to a valid batch.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ThreadInjectionMessageText(Box<str>);

/// Validation failure for normalized thread-injection message text.
#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum ThreadInjectionMessageTextError {
    /// The supplied message text contained no UTF-8 bytes.
    #[error("thread-injection message text must not be empty")]
    Empty,
    /// The supplied message text exceeded the canonical per-batch byte bound.
    #[error(
        "thread-injection message text has {byte_count} UTF-8 bytes; the maximum is {max_bytes}"
    )]
    TooManyBytes {
        /// Exact UTF-8 byte count of the rejected text.
        byte_count: usize,
        /// Maximum accepted UTF-8 byte count.
        max_bytes: usize,
    },
}

impl ThreadInjectionMessageText {
    /// Validates exact message text without trimming or normalization.
    pub fn new(text: impl Into<Box<str>>) -> Result<Self, ThreadInjectionMessageTextError> {
        let text = text.into();
        let byte_count = text.len();
        if byte_count == 0 {
            return Err(ThreadInjectionMessageTextError::Empty);
        }
        if byte_count > THREAD_INJECTION_MAX_TEXT_BYTES {
            return Err(ThreadInjectionMessageTextError::TooManyBytes {
                byte_count,
                max_bytes: THREAD_INJECTION_MAX_TEXT_BYTES,
            });
        }

        Ok(Self(text))
    }

    /// Returns the exact validated message text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Returns the exact UTF-8 byte count of the message text.
    #[must_use]
    pub fn byte_count(&self) -> usize {
        self.0.len()
    }
}

/// One message in the closed normalized `thread/inject_items` subset.
///
/// The variants encode the only two role/content pairs accepted by Beryl for
/// Codex App Server 0.144.1. No raw response-item variant is provided.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ThreadInjectionItem {
    /// A user-role message containing exactly one `input_text` content item.
    UserInputText(ThreadInjectionMessageText),
    /// An assistant-role message containing exactly one `output_text` content item.
    AssistantOutputText(ThreadInjectionMessageText),
}

impl ThreadInjectionItem {
    /// Builds a user-role message containing exactly one `input_text` item.
    pub fn user_input_text(
        text: impl Into<Box<str>>,
    ) -> Result<Self, ThreadInjectionMessageTextError> {
        ThreadInjectionMessageText::new(text).map(Self::UserInputText)
    }

    /// Builds an assistant-role message containing exactly one `output_text` item.
    pub fn assistant_output_text(
        text: impl Into<Box<str>>,
    ) -> Result<Self, ThreadInjectionMessageTextError> {
        ThreadInjectionMessageText::new(text).map(Self::AssistantOutputText)
    }

    /// Returns the exact validated message text.
    #[must_use]
    pub fn text(&self) -> &str {
        self.message_text().as_str()
    }

    /// Returns the validated message-text value.
    #[must_use]
    pub const fn message_text(&self) -> &ThreadInjectionMessageText {
        match self {
            Self::UserInputText(text) | Self::AssistantOutputText(text) => text,
        }
    }
}

/// Exact validated, order-preserving thread-injection batch.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ThreadInjectionBatch {
    items: Box<[ThreadInjectionItem]>,
    canonical_text_bytes: usize,
}

/// Validation failure for a normalized thread-injection batch.
#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum ThreadInjectionBatchError {
    /// The supplied batch contained no items.
    #[error("thread-injection batch must contain at least one item")]
    Empty,
    /// The supplied batch exceeded the item-count bound.
    #[error("thread-injection batch has {item_count} items; the maximum is {max_items}")]
    TooManyItems {
        /// Exact item count of the rejected batch.
        item_count: usize,
        /// Maximum accepted item count.
        max_items: usize,
    },
    /// The sum of exact message-text bytes exceeded the canonical byte bound.
    #[error(
        "thread-injection batch has at least {canonical_text_bytes} canonical UTF-8 text bytes; the maximum is {max_bytes}"
    )]
    TooManyTextBytes {
        /// Exact running byte count at the first item that crossed the bound.
        canonical_text_bytes: usize,
        /// Maximum accepted canonical UTF-8 byte count.
        max_bytes: usize,
    },
}

/// Normalized structured rejection for one thread-injection request.
///
/// The raw JSON-RPC error data is deliberately not exposed. Callers receive
/// the stable numeric code, human-readable message, and whether CAS supplied
/// additional private diagnostic data.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ThreadInjectionRejection {
    code: i64,
    message: Box<str>,
    data_was_present: bool,
}

/// Terminal outcome of one consumed fresh-thread injection attempt.
///
/// Every variant consumes the fresh-idle capability. Callers must never retry
/// an unsuccessful attempt against the same CAS thread; they must abandon that
/// thread and establish another fresh projection when policy permits.
#[derive(Debug)]
pub enum ThreadInjectionOutcome {
    /// CAS returned the exact successful response for the ordered batch.
    Succeeded {
        /// The same loaded thread, now usable as recovered native lineage.
        thread: LoadedThreadSession,
    },
    /// CAS returned one matching structured JSON-RPC rejection.
    Rejected {
        /// Exact fresh CAS thread that must now be abandoned.
        thread_id: CasThreadId,
        /// Structured rejection without a raw protocol-JSON escape hatch.
        rejection: ThreadInjectionRejection,
    },
    /// The request lost its transport without proving remote completion.
    TransportLost {
        /// Exact fresh CAS thread that must now be abandoned.
        thread_id: CasThreadId,
        /// Normalized transport failure.
        error: Box<ManagedBackendError>,
    },
    /// No exact successful or rejected completion could be established.
    CompletionUnknown {
        /// Exact fresh CAS thread that must now be abandoned.
        thread_id: CasThreadId,
        /// Failure that prevented exact completion proof.
        error: Box<ManagedBackendError>,
    },
}

impl ThreadInjectionRejection {
    /// Returns the exact JSON-RPC error code supplied by CAS.
    #[must_use]
    pub const fn code(&self) -> i64 {
        self.code
    }

    /// Returns the exact human-readable error message supplied by CAS.
    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }

    /// Reports whether CAS supplied additional raw diagnostic data.
    ///
    /// The data itself remains private to the protocol boundary.
    #[must_use]
    pub const fn data_was_present(&self) -> bool {
        self.data_was_present
    }

    pub(crate) fn from_json_rpc(error: JsonRpcError) -> Self {
        Self {
            code: error.code,
            message: error.message.into_boxed_str(),
            data_was_present: error.data.is_some(),
        }
    }
}

impl ThreadInjectionBatch {
    /// Validates a nonempty batch while preserving the supplied item order.
    pub fn new(items: Vec<ThreadInjectionItem>) -> Result<Self, ThreadInjectionBatchError> {
        if items.is_empty() {
            return Err(ThreadInjectionBatchError::Empty);
        }
        if items.len() > THREAD_INJECTION_MAX_ITEMS {
            return Err(ThreadInjectionBatchError::TooManyItems {
                item_count: items.len(),
                max_items: THREAD_INJECTION_MAX_ITEMS,
            });
        }

        let mut canonical_text_bytes = 0_usize;
        for item in &items {
            canonical_text_bytes =
                canonical_text_bytes.saturating_add(item.message_text().byte_count());
            if canonical_text_bytes > THREAD_INJECTION_MAX_TEXT_BYTES {
                return Err(ThreadInjectionBatchError::TooManyTextBytes {
                    canonical_text_bytes,
                    max_bytes: THREAD_INJECTION_MAX_TEXT_BYTES,
                });
            }
        }

        Ok(Self {
            items: items.into_boxed_slice(),
            canonical_text_bytes,
        })
    }

    /// Returns the messages in their exact injection order.
    #[must_use]
    pub fn items(&self) -> &[ThreadInjectionItem] {
        &self.items
    }

    /// Returns the exact number of messages in the batch.
    #[must_use]
    pub fn item_count(&self) -> usize {
        self.items.len()
    }

    /// Returns the sum of exact UTF-8 bytes across all message text.
    #[must_use]
    pub const fn canonical_text_bytes(&self) -> usize {
        self.canonical_text_bytes
    }

    /// Consumes the batch and returns its messages in exact injection order.
    #[must_use]
    pub fn into_items(self) -> Box<[ThreadInjectionItem]> {
        self.items
    }
}

/// Exact stable Codex App Server method for normalized item injection.
pub(crate) const THREAD_INJECT_ITEMS_METHOD: &str = "thread/inject_items";

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ThreadInjectItemsParams<'a> {
    thread_id: &'a CasThreadId,
    items: ThreadInjectionItemsWire<'a>,
}

#[derive(Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub(crate) struct ThreadInjectItemsResponse {}

impl<'a> ThreadInjectItemsParams<'a> {
    pub(crate) fn new(thread_id: &'a CasThreadId, batch: &'a ThreadInjectionBatch) -> Self {
        Self {
            thread_id,
            items: ThreadInjectionItemsWire(batch.items()),
        }
    }
}

struct ThreadInjectionItemsWire<'a>(&'a [ThreadInjectionItem]);

impl Serialize for ThreadInjectionItemsWire<'_> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut sequence = serializer.serialize_seq(Some(self.0.len()))?;
        for item in self.0 {
            sequence.serialize_element(&ThreadInjectionItemWire(item))?;
        }
        sequence.end()
    }
}

struct ThreadInjectionItemWire<'a>(&'a ThreadInjectionItem);

impl Serialize for ThreadInjectionItemWire<'_> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let (role, content_type, text) = match self.0 {
            ThreadInjectionItem::UserInputText(text) => ("user", "input_text", text.as_str()),
            ThreadInjectionItem::AssistantOutputText(text) => {
                ("assistant", "output_text", text.as_str())
            }
        };

        let mut message = serializer.serialize_struct("ThreadInjectionMessage", 3)?;
        message.serialize_field("type", "message")?;
        message.serialize_field("role", role)?;
        message.serialize_field("content", &[ThreadInjectionTextWire { content_type, text }])?;
        message.end()
    }
}

#[derive(Serialize)]
struct ThreadInjectionTextWire<'a> {
    #[serde(rename = "type")]
    content_type: &'static str,
    text: &'a str,
}
