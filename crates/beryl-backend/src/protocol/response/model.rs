use std::{fmt, num::NonZeroU8};

use thiserror::Error;

use super::{ModelDisplayName, ModelPageCursor, ProtocolIdentity};

pub const MODEL_PAGE_MAX_RECORDS: usize = 64;

/// Validated record count requested from one bounded `model/list` page.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ModelPageLimit(NonZeroU8);

impl ModelPageLimit {
    /// Creates a nonzero page limit no greater than [`MODEL_PAGE_MAX_RECORDS`].
    pub fn try_new(limit: u32) -> Result<Self, ModelPageLimitError> {
        if limit == 0 || limit > MODEL_PAGE_MAX_RECORDS as u32 {
            return Err(ModelPageLimitError {
                requested: limit,
                maximum: MODEL_PAGE_MAX_RECORDS,
            });
        }
        Ok(Self(
            NonZeroU8::new(limit as u8).expect("a validated model-page limit is nonzero"),
        ))
    }

    #[must_use]
    pub const fn get(self) -> u8 {
        self.0.get()
    }
}

impl Default for ModelPageLimit {
    fn default() -> Self {
        Self(NonZeroU8::new(MODEL_PAGE_MAX_RECORDS as u8).expect("the page maximum is nonzero"))
    }
}

/// Rejection of a caller-supplied model-page count before dispatch.
#[derive(Clone, Copy, Debug, Error, PartialEq, Eq)]
#[error("model page limit {requested} is outside 1..={maximum}")]
pub struct ModelPageLimitError {
    pub requested: u32,
    pub maximum: usize,
}

/// Bounded options for exactly one `model/list` request.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct ModelListOptions {
    cursor: Option<ModelPageCursor>,
    limit: ModelPageLimit,
    include_hidden: bool,
}

impl ModelListOptions {
    /// Creates one first-page request with an explicit validated record limit.
    pub fn page(limit: u32) -> Result<Self, ModelPageLimitError> {
        Ok(Self {
            limit: ModelPageLimit::try_new(limit)?,
            ..Self::default()
        })
    }

    /// Moves one bounded continuation cursor into the next-page request.
    #[must_use]
    pub fn with_cursor(mut self, cursor: ModelPageCursor) -> Self {
        self.cursor = Some(cursor);
        self
    }

    /// Requests hidden records without changing the fixed page boundary.
    #[must_use]
    pub fn include_hidden(mut self) -> Self {
        self.include_hidden = true;
        self
    }

    #[must_use]
    pub fn cursor(&self) -> Option<&str> {
        self.cursor.as_ref().map(ModelPageCursor::as_str)
    }

    #[must_use]
    pub const fn limit(&self) -> ModelPageLimit {
        self.limit
    }

    #[must_use]
    pub const fn includes_hidden(&self) -> bool {
        self.include_hidden
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum ReasoningEffort {
    None,
    Minimal,
    Low,
    Medium,
    High,
    XHigh,
    Max,
    Ultra,
}

impl ReasoningEffort {
    #[must_use]
    pub fn from_wire(value: &str) -> Option<Self> {
        Some(match value {
            "none" => Self::None,
            "minimal" => Self::Minimal,
            "low" => Self::Low,
            "medium" => Self::Medium,
            "high" => Self::High,
            "xhigh" => Self::XHigh,
            "max" => Self::Max,
            "ultra" => Self::Ultra,
            _ => return None,
        })
    }

    const fn bit(self) -> u16 {
        1 << (self as u8)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DefaultReasoningEffort {
    None,
    Minimal,
    Low,
    Medium,
    High,
    XHigh,
    Max,
    Ultra,
    Other,
}

impl DefaultReasoningEffort {
    #[must_use]
    pub fn from_wire(value: &str) -> Option<Self> {
        if value.is_empty() {
            return None;
        }
        Some(match ReasoningEffort::from_wire(value) {
            Some(ReasoningEffort::None) => Self::None,
            Some(ReasoningEffort::Minimal) => Self::Minimal,
            Some(ReasoningEffort::Low) => Self::Low,
            Some(ReasoningEffort::Medium) => Self::Medium,
            Some(ReasoningEffort::High) => Self::High,
            Some(ReasoningEffort::XHigh) => Self::XHigh,
            Some(ReasoningEffort::Max) => Self::Max,
            Some(ReasoningEffort::Ultra) => Self::Ultra,
            None => Self::Other,
        })
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SupportedReasoningEfforts(u16);

impl SupportedReasoningEfforts {
    #[must_use]
    pub const fn empty() -> Self {
        Self(0)
    }

    pub fn insert(&mut self, effort: ReasoningEffort) {
        self.0 |= effort.bit();
    }

    pub fn insert_wire(&mut self, value: &str) -> bool {
        let Some(effort) = ReasoningEffort::from_wire(value) else {
            return false;
        };
        self.insert(effort);
        true
    }

    #[must_use]
    pub const fn contains(self, effort: ReasoningEffort) -> bool {
        self.0 & effort.bit() != 0
    }

    #[must_use]
    pub const fn bits(self) -> u16 {
        self.0
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct ModelRecord {
    id: ProtocolIdentity,
    model: ProtocolIdentity,
    display_name: ModelDisplayName,
    hidden: bool,
    is_default: bool,
    supported_reasoning_efforts: SupportedReasoningEfforts,
    default_reasoning_effort: DefaultReasoningEffort,
}

impl ModelRecord {
    #[must_use]
    pub const fn new(
        id: ProtocolIdentity,
        model: ProtocolIdentity,
        display_name: ModelDisplayName,
        hidden: bool,
        is_default: bool,
        supported_reasoning_efforts: SupportedReasoningEfforts,
        default_reasoning_effort: DefaultReasoningEffort,
    ) -> Self {
        Self {
            id,
            model,
            display_name,
            hidden,
            is_default,
            supported_reasoning_efforts,
            default_reasoning_effort,
        }
    }

    #[must_use]
    pub fn id(&self) -> &str {
        self.id.as_str()
    }

    #[must_use]
    pub fn model(&self) -> &str {
        self.model.as_str()
    }

    #[must_use]
    pub fn display_name(&self) -> &str {
        self.display_name.as_str()
    }

    #[must_use]
    pub const fn hidden(&self) -> bool {
        self.hidden
    }

    #[must_use]
    pub const fn is_default(&self) -> bool {
        self.is_default
    }

    #[must_use]
    pub const fn supported_reasoning_efforts(&self) -> SupportedReasoningEfforts {
        self.supported_reasoning_efforts
    }

    #[must_use]
    pub const fn default_reasoning_effort(&self) -> DefaultReasoningEffort {
        self.default_reasoning_effort
    }
}

#[derive(Clone, Copy, Debug, Error, PartialEq, Eq)]
#[error("model page already contains its maximum of {maximum} records")]
pub struct ModelPageCapacityError {
    pub maximum: usize,
}

#[derive(PartialEq, Eq)]
pub struct ModelPage {
    records: [Option<ModelRecord>; MODEL_PAGE_MAX_RECORDS],
    len: u8,
    next_cursor: Option<ModelPageCursor>,
}

impl ModelPage {
    /// Creates one fixed-resident page.
    #[must_use]
    pub fn new() -> Self {
        Self {
            records: std::array::from_fn(|_| None),
            len: 0,
            next_cursor: None,
        }
    }

    pub fn try_push(&mut self, record: ModelRecord) -> Result<(), ModelPageCapacityError> {
        let index = usize::from(self.len);
        if index == MODEL_PAGE_MAX_RECORDS {
            return Err(ModelPageCapacityError {
                maximum: MODEL_PAGE_MAX_RECORDS,
            });
        }
        self.records[index] = Some(record);
        self.len += 1;
        Ok(())
    }

    pub fn set_next_cursor(&mut self, next_cursor: Option<ModelPageCursor>) {
        self.next_cursor = next_cursor;
    }

    #[must_use]
    pub const fn len(&self) -> usize {
        self.len as usize
    }

    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub fn records(
        &self,
    ) -> impl ExactSizeIterator<Item = &ModelRecord> + DoubleEndedIterator + '_ {
        self.records[..self.len()].iter().map(|record| {
            record
                .as_ref()
                .expect("every model slot below page length is initialized")
        })
    }

    #[must_use]
    pub fn next_cursor(&self) -> Option<&str> {
        self.next_cursor.as_ref().map(ModelPageCursor::as_str)
    }

    #[must_use]
    pub fn take_next_cursor(&mut self) -> Option<ModelPageCursor> {
        self.next_cursor.take()
    }
}

impl Default for ModelPage {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Debug for ModelPage {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ModelPage")
            .field("records", &ModelRecordsDebug(self))
            .field("next_cursor", &self.next_cursor())
            .finish()
    }
}

struct ModelRecordsDebug<'a>(&'a ModelPage);

impl fmt::Debug for ModelRecordsDebug<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.debug_list().entries(self.0.records()).finish()
    }
}
