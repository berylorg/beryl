use super::SyndicPointReadLimit;
use crate::{
    ActivityItemSource, ActivityQueryEntryRecord, CanonicalItemRecord, ItemSourceEventOrdinal,
    ProviderFrameObservationSummaryV1, ProviderItemKind, ProviderItemLifecycle,
    SealedProviderFrameReference, SourceEventPayload, SourceEventSequence, SyndicReadError,
    SyndicStorage,
};
use beryl_home_store::{CursorReadLimits, HomeStore};

mod parser;
mod source;
mod text;
use parser::Parser;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ActivityDisplayStatus {
    Running,
    FinishedOk,
    FinishedError,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ActivityDisplayText {
    text: Box<str>,
    truncated: bool,
}
impl ActivityDisplayText {
    pub fn text(&self) -> &str {
        &self.text
    }
    pub const fn truncated(&self) -> bool {
        self.truncated
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ActivityFileChangeDisplay {
    changes: u64,
    additions: u64,
    deletions: u64,
    path: Option<Box<str>>,
}
impl ActivityFileChangeDisplay {
    pub const fn change_count(&self) -> u64 {
        self.changes
    }
    pub const fn additions(&self) -> u64 {
        self.additions
    }
    pub const fn deletions(&self) -> u64 {
        self.deletions
    }
    pub fn sole_path(&self) -> Option<&str> {
        self.path.as_deref()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ActivityDisplayValue {
    Command(ActivityDisplayText),
    Reasoning(ActivityDisplayText),
    FileChange(ActivityFileChangeDisplay),
    Tool(ActivityDisplayText),
    Kind,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ActivityDisplaySource {
    source: ActivityItemSource,
    source_event: SourceEventSequence,
    kind: ProviderItemKind,
    status: ActivityDisplayStatus,
    value: ActivityDisplayValue,
}
impl ActivityDisplaySource {
    pub const fn source(&self) -> &ActivityItemSource {
        &self.source
    }
    pub const fn source_event(&self) -> SourceEventSequence {
        self.source_event
    }
    pub const fn kind(&self) -> ProviderItemKind {
        self.kind
    }
    pub const fn status(&self) -> ActivityDisplayStatus {
        self.status
    }
    pub const fn value(&self) -> &ActivityDisplayValue {
        &self.value
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ActivityDisplayReadError {
    #[error("Activity display preparation was cancelled")]
    Cancelled,
    #[error("Activity display source changed")]
    Stale,
    #[error("Activity display source is invalid: {0}")]
    Invalid(&'static str),
    #[error("Activity display preparation is already terminal")]
    Terminal,
    #[error("Activity display read failed: {0}")]
    Read(#[from] SyndicReadError),
    #[error("Activity display storage read failed: {0}")]
    Storage(#[from] beryl_home_store::ReadError),
}

pub enum ActivityDisplayStep {
    Pending,
    Complete(ActivityDisplaySource),
}

pub struct ActivityDisplayPreparation {
    entry: ActivityQueryEntryRecord,
    item: CanonicalItemRecord,
    parser: Option<Parser>,
    after: Option<ItemSourceEventOrdinal>,
    event: Option<SourceEventSequence>,
    last_event: Option<SourceEventSequence>,
    facts: parser::Facts,
    terminal: bool,
}

fn point() -> SyndicPointReadLimit {
    SyndicPointReadLimit::new(16_384).expect("fixed point limit")
}

impl SyndicStorage {
    pub fn prepare_activity_display(
        &self,
        home: &HomeStore,
        entry: &ActivityQueryEntryRecord,
    ) -> Result<ActivityDisplayPreparation, ActivityDisplayReadError> {
        let item = self
            .canonical_item(home, entry.item_id(), point())?
            .ok_or(ActivityDisplayReadError::Stale)?;
        qualify(entry, &item)?;
        let complete = entry.provider_lifecycle() == ProviderItemLifecycle::Completed;
        Ok(ActivityDisplayPreparation {
            entry: entry.clone(),
            item,
            parser: None,
            after: None,
            event: complete.then_some(entry.source_event()),
            last_event: None,
            facts: parser::Facts::default(),
            terminal: false,
        })
    }
}

impl ActivityDisplayPreparation {
    pub fn step(
        &mut self,
        home: &HomeStore,
        storage: &SyndicStorage,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<ActivityDisplayStep, ActivityDisplayReadError> {
        if self.terminal {
            return Err(ActivityDisplayReadError::Terminal);
        }
        let result = self.advance(home, storage, cancelled);
        if !matches!(result, Ok(ActivityDisplayStep::Pending)) {
            self.terminal = true;
            self.parser = None;
        }
        result
    }

    fn advance(
        &mut self,
        home: &HomeStore,
        storage: &SyndicStorage,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<ActivityDisplayStep, ActivityDisplayReadError> {
        if cancelled() {
            return Err(ActivityDisplayReadError::Cancelled);
        }
        let current = storage.canonical_item(home, self.entry.item_id(), point())?;
        if current.as_ref() != Some(&self.item) {
            return Err(ActivityDisplayReadError::Stale);
        }
        if let Some(parser) = &mut self.parser {
            if !parser.step(home, storage, &mut self.facts, cancelled)? {
                return Ok(ActivityDisplayStep::Pending);
            }
            self.parser = None;
            if self.last_event == Some(self.entry.source_event()) {
                return self.finish(home, storage, cancelled);
            }
            return Ok(ActivityDisplayStep::Pending);
        }
        if let Some(sequence) = self.event.take() {
            let event = storage
                .source_event(home, self.entry.source().turn_id(), sequence, point())?
                .ok_or(ActivityDisplayReadError::Stale)?;
            let SourceEventPayload::ItemFrame { item_id, frame } = event.payload() else {
                return Err(ActivityDisplayReadError::Invalid("item event has no frame"));
            };
            if *item_id != self.entry.item_id()
                || event.source() != Some(self.entry.source().cas_item().turn())
                || event.sequence() != sequence
            {
                return Err(ActivityDisplayReadError::Invalid(
                    "item event identity mismatch",
                ));
            }
            qualify_frame(&self.item, frame)?;
            self.parser = Some(Parser::new(frame, self.entry.provider_kind())?);
            self.last_event = Some(sequence);
            return Ok(ActivityDisplayStep::Pending);
        }
        let page = storage.item_source_events(
            home,
            self.entry.item_id(),
            self.after,
            CursorReadLimits::new(1, 4096).expect("fixed event page limit"),
        )?;
        let index = page
            .records()
            .first()
            .ok_or(ActivityDisplayReadError::Invalid(
                "missing item source event",
            ))?;
        if index.item_id() != self.entry.item_id()
            || index.turn_id() != self.entry.source().turn_id()
            || index.source_event() > self.entry.source_event()
        {
            return Err(ActivityDisplayReadError::Invalid(
                "item event frontier mismatch",
            ));
        }
        self.after = Some(index.ordinal());
        self.event = Some(index.source_event());
        Ok(ActivityDisplayStep::Pending)
    }

    fn finish(
        &mut self,
        home: &HomeStore,
        storage: &SyndicStorage,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<ActivityDisplayStep, ActivityDisplayReadError> {
        if cancelled() {
            return Err(ActivityDisplayReadError::Cancelled);
        }
        if storage
            .canonical_item(home, self.entry.item_id(), point())?
            .as_ref()
            != Some(&self.item)
        {
            return Err(ActivityDisplayReadError::Stale);
        }
        Ok(ActivityDisplayStep::Complete(ActivityDisplaySource {
            source: self.entry.source().clone(),
            source_event: self.entry.source_event(),
            kind: self.entry.provider_kind(),
            status: if self.entry.provider_lifecycle() != ProviderItemLifecycle::Completed {
                ActivityDisplayStatus::Running
            } else if self.facts.failed {
                ActivityDisplayStatus::FinishedError
            } else {
                ActivityDisplayStatus::FinishedOk
            },
            value: self.facts.value(self.entry.provider_kind()),
        }))
    }
}

fn qualify(
    entry: &ActivityQueryEntryRecord,
    item: &CanonicalItemRecord,
) -> Result<(), ActivityDisplayReadError> {
    if item.id() != entry.item_id()
        || item.turn_id() != entry.source().turn_id()
        || item.cas_source() != Some(entry.source().cas_item())
        || item.source_event() != Some(entry.source_event())
        || item.provider_kind() != entry.provider_kind()
        || item.provider_lifecycle() != entry.provider_lifecycle()
    {
        return Err(ActivityDisplayReadError::Stale);
    }
    if entry.compact_fact().is_some() {
        return Err(ActivityDisplayReadError::Invalid(
            "compact handoff needs no source extraction",
        ));
    }
    Ok(())
}

fn qualify_frame(
    item: &CanonicalItemRecord,
    frame: &SealedProviderFrameReference,
) -> Result<(), ActivityDisplayReadError> {
    let current = item.provider().ok_or(ActivityDisplayReadError::Invalid(
        "item has no provider frame",
    ))?;
    if frame.content().id() != current.content().id()
        || frame.frame().item_id() != current.frame().item_id()
        || frame.frame().item_kind() != item.provider_kind()
        || frame.frame().encoded_end() > current.content().summary().encoded_bytes()
    {
        return Err(ActivityDisplayReadError::Invalid(
            "frame exceeds canonical authority",
        ));
    }
    if matches!(
        frame.observation(),
        ProviderFrameObservationSummaryV1::Completed(_)
    ) && frame != current
    {
        return Err(ActivityDisplayReadError::Stale);
    }
    Ok(())
}
