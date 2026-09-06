use super::*;

#[derive(Clone, Copy, Eq, PartialEq)]
pub struct MainWindowNoticeDiagnosticKey([u8; 32]);

impl fmt::Debug for MainWindowNoticeDiagnosticKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("MainWindowNoticeDiagnosticKey(..)")
    }
}

impl MainWindowNoticeDiagnosticKey {
    pub const fn from_opaque_bytes(value: [u8; 32]) -> Self {
        Self(value)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MainWindowNoticeOverlayAllocation {
    pub origin_block_start: f32,
    pub inset_block_start: f32,
    pub inset_inline_end: f32,
    pub available_inline: f32,
    pub available_block: f32,
}

impl MainWindowNoticeOverlayAllocation {
    pub const fn new(
        origin_block_start: f32,
        inset_block_start: f32,
        inset_inline_end: f32,
        available_inline: f32,
        available_block: f32,
    ) -> Self {
        Self {
            origin_block_start,
            inset_block_start,
            inset_inline_end,
            available_inline,
            available_block,
        }
    }
}

#[derive(Clone)]
pub struct MainWindowNoticeWidgetRecord {
    pub(super) token: NoticeVisibleToken,
    pub(super) content: NoticeContent,
    pub(super) diagnostic_key: MainWindowNoticeDiagnosticKey,
    pub(super) allocation: MainWindowNoticeOverlayAllocation,
}

impl MainWindowNoticeWidgetRecord {
    pub fn from_projection(
        projection: NoticeProjection<'_>,
        diagnostic_key: MainWindowNoticeDiagnosticKey,
        allocation: MainWindowNoticeOverlayAllocation,
    ) -> Self {
        Self {
            token: projection.token,
            content: projection.content.clone(),
            diagnostic_key,
            allocation,
        }
    }

    pub fn token(&self) -> &NoticeVisibleToken {
        &self.token
    }

    pub fn content(&self) -> &NoticeContent {
        &self.content
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MainWindowNoticeWidgetEvent {
    Dismiss(NoticeVisibleToken),
    Command {
        token: NoticeVisibleToken,
        command: NoticeCommandId,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MainWindowNoticeControlFocus {
    Detail,
    Close,
    Command,
    SafeTarget,
    None,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MainWindowNoticeVisibility {
    Hidden,
    Entering,
    Visible,
    Leaving,
    Inert,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum MainWindowNoticePaintPhase {
    Entering,
    #[default]
    Visible,
    Leaving,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MainWindowNoticeWidgetDiagnostics {
    pub widget_instance_id: u64,
    pub diagnostic_key: Option<MainWindowNoticeDiagnosticKey>,
    pub content_revision: u64,
    pub variant: NoticeVariant,
    pub dismissal: NoticeDismissal,
    pub visible: bool,
    pub visibility: MainWindowNoticeVisibility,
    pub detail_present: bool,
    pub command_count: usize,
    pub allocated_inline: u16,
    pub allocated_block: u16,
    pub overflow: bool,
    pub scroll_offset: i32,
    pub selection_present: bool,
    pub focus: MainWindowNoticeControlFocus,
    pub replacements: u64,
}
