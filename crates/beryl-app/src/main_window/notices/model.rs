use std::sync::Arc;

use beryl_model::WindowId;

pub const NOTICE_RECORD_CAPACITY: usize = 16;
pub const NOTICE_GENERAL_CAPACITY: usize = NOTICE_RECORD_CAPACITY - 3;
pub const NOTICE_TITLE_BYTES: usize = 256;
pub const NOTICE_DETAIL_BYTES: usize = 4096;
pub const NOTICE_COMMAND_CAPACITY: usize = 3;
pub const NOTICE_COMMAND_LABEL_BYTES: usize = 96;
pub const NOTICE_COMMAND_REASON_BYTES: usize = 256;
pub const NOTICE_RECORD_TEXT_BYTES: usize = NOTICE_TITLE_BYTES
    + NOTICE_DETAIL_BYTES
    + NOTICE_COMMAND_CAPACITY * (NOTICE_COMMAND_LABEL_BYTES + NOTICE_COMMAND_REASON_BYTES);

#[derive(Clone)]
pub(super) struct Identity(Arc<()>);

impl Identity {
    pub(super) fn new() -> Self {
        Self(Arc::new(()))
    }
}

impl PartialEq for Identity {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

impl Eq for Identity {}

impl std::fmt::Debug for Identity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("opaque")
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NoticeConditionId(Identity);

impl NoticeConditionId {
    pub fn new() -> Self {
        Self(Identity::new())
    }
}

impl Default for NoticeConditionId {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum NoticeKind {
    HomeFailure,
    ExactStopFeedback,
    Lifecycle,
    RuntimeUnavailable,
    Error,
    Recovery,
    Warning,
    Information,
}

impl NoticeKind {
    pub(super) fn protected(self) -> bool {
        matches!(
            self,
            Self::HomeFailure | Self::ExactStopFeedback | Self::RuntimeUnavailable
        )
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NoticeVariant {
    Warning,
    Error,
    Info,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NoticeDismissal {
    Dismissible,
    Persistent,
}

#[derive(Clone, Eq, PartialEq)]
pub struct NoticeText {
    text: Box<str>,
    truncated: bool,
}

impl NoticeText {
    fn bounded(source: &str, limit: usize) -> Self {
        let truncated = source.len() > limit;
        let text = if truncated {
            let mut end = limit - '…'.len_utf8();
            while !source.is_char_boundary(end) {
                end -= 1;
            }
            let mut text = String::with_capacity(limit);
            text.push_str(&source[..end]);
            text.push('…');
            text.into_boxed_str()
        } else {
            source.into()
        };
        Self { text, truncated }
    }

    pub fn as_str(&self) -> &str {
        &self.text
    }

    pub fn is_truncated(&self) -> bool {
        self.truncated
    }
}

impl std::fmt::Debug for NoticeText {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NoticeText")
            .field("bytes", &self.text.len())
            .field("truncated", &self.truncated)
            .finish()
    }
}

#[derive(Clone, Copy, Eq, PartialEq)]
pub struct NoticeCommandId(u64);

impl NoticeCommandId {
    pub const fn new(value: u64) -> Self {
        Self(value)
    }
}

impl std::fmt::Debug for NoticeCommandId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("NoticeCommandId(opaque)")
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NoticeCommandState {
    Enabled,
    Loading,
    Disabled,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NoticeCommand {
    pub id: NoticeCommandId,
    label: NoticeText,
    state: NoticeCommandState,
    disabled_reason: Option<NoticeText>,
}

impl NoticeCommand {
    pub fn enabled(id: NoticeCommandId, label: &str) -> Self {
        Self {
            id,
            label: NoticeText::bounded(label, NOTICE_COMMAND_LABEL_BYTES),
            state: NoticeCommandState::Enabled,
            disabled_reason: None,
        }
    }

    pub fn loading(id: NoticeCommandId, label: &str) -> Self {
        Self {
            state: NoticeCommandState::Loading,
            ..Self::enabled(id, label)
        }
    }

    pub fn disabled(id: NoticeCommandId, label: &str, reason: &str) -> Self {
        Self {
            state: NoticeCommandState::Disabled,
            disabled_reason: Some(NoticeText::bounded(reason, NOTICE_COMMAND_REASON_BYTES)),
            ..Self::enabled(id, label)
        }
    }

    pub fn label(&self) -> &NoticeText {
        &self.label
    }

    pub fn state(&self) -> NoticeCommandState {
        self.state
    }

    pub fn disabled_reason(&self) -> Option<&NoticeText> {
        self.disabled_reason.as_ref()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NoticeContent {
    pub variant: NoticeVariant,
    pub dismissal: NoticeDismissal,
    title: NoticeText,
    detail: NoticeText,
    commands: [Option<NoticeCommand>; NOTICE_COMMAND_CAPACITY],
}

impl NoticeContent {
    pub fn new(
        variant: NoticeVariant,
        dismissal: NoticeDismissal,
        title: &str,
        detail: &str,
    ) -> Self {
        Self {
            variant,
            dismissal,
            title: NoticeText::bounded(title, NOTICE_TITLE_BYTES),
            detail: NoticeText::bounded(detail, NOTICE_DETAIL_BYTES),
            commands: Default::default(),
        }
    }

    pub fn with_commands(mut self, commands: &[NoticeCommand]) -> Result<Self, NoticeRejection> {
        if commands.len() > NOTICE_COMMAND_CAPACITY {
            return Err(NoticeRejection::TooManyCommands);
        }
        for (index, command) in commands.iter().enumerate() {
            if commands[..index].iter().any(|prior| prior.id == command.id) {
                return Err(NoticeRejection::DuplicateCommand);
            }
        }
        self.commands = std::array::from_fn(|index| commands.get(index).cloned());
        Ok(self)
    }

    pub fn title(&self) -> &NoticeText {
        &self.title
    }

    pub fn detail(&self) -> &NoticeText {
        &self.detail
    }

    pub fn commands(&self) -> impl Iterator<Item = &NoticeCommand> {
        self.commands.iter().flatten()
    }

    pub fn retained_text_bytes(&self) -> usize {
        self.title.text.len()
            + self.detail.text.len()
            + self
                .commands()
                .map(|command| {
                    command.label.text.len()
                        + command
                            .disabled_reason
                            .as_ref()
                            .map_or(0, |text| text.text.len())
                })
                .sum::<usize>()
    }
}

#[derive(Clone, Debug)]
pub struct NoticeRecord {
    pub window_id: WindowId,
    pub condition: NoticeConditionId,
    pub revision: u64,
    pub kind: NoticeKind,
    pub content: NoticeContent,
}

#[derive(Clone, Eq, PartialEq)]
pub struct NoticeRecordToken {
    pub(super) owner: Identity,
    pub(super) admission: Identity,
    pub(super) window_id: WindowId,
    pub(super) condition: NoticeConditionId,
    pub(super) revision: u64,
}

impl NoticeRecordToken {
    pub fn window_id(&self) -> WindowId {
        self.window_id
    }

    pub fn condition(&self) -> &NoticeConditionId {
        &self.condition
    }

    pub fn revision(&self) -> u64 {
        self.revision
    }
}

impl std::fmt::Debug for NoticeRecordToken {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NoticeRecordToken")
            .field("revision", &self.revision)
            .finish_non_exhaustive()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NoticeVisibleToken {
    pub(super) record: NoticeRecordToken,
    pub(super) exposure: Identity,
}

impl NoticeVisibleToken {
    pub fn record(&self) -> &NoticeRecordToken {
        &self.record
    }
}

#[derive(Debug)]
pub struct NoticeProjection<'a> {
    pub token: NoticeVisibleToken,
    pub kind: NoticeKind,
    pub content: &'a NoticeContent,
    pub report_count: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NoticeRejection {
    WrongWindow,
    StaleRecord,
    StaleRevision,
    StaleVisibility,
    Persistent,
    ConditionKindMismatch,
    ProtectedConditionOccupied,
    InvalidProtectedReplacement,
    TooManyCommands,
    DuplicateCommand,
    Disposed,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum NoticeAdmission {
    Admitted(NoticeRecordToken),
    Updated(NoticeRecordToken),
    Omitted,
    Rejected(NoticeRejection),
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct NoticeDiagnostics {
    pub retained_records: usize,
    pub pending_records: usize,
    pub retained_text_bytes: usize,
    pub admitted: u64,
    pub updated: u64,
    pub omitted: u64,
    pub replaced: u64,
    pub dismissed: u64,
    pub removed: u64,
    pub rejected: u64,
    pub disposed: bool,
}
