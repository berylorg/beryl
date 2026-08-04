use crate::{ProviderEnumValue as E, ProviderField as F, ProviderItemKind};

#[derive(Clone, Copy)]
pub(super) struct FieldSpec {
    pub(super) name: &'static str,
    pub(super) field: F,
    pub(super) value: ValueKind,
    pub(super) required: bool,
    pub(super) nullable: bool,
}

impl FieldSpec {
    pub(super) const fn required_text(field: F) -> Self {
        Self {
            name: "",
            field,
            value: ValueKind::Text,
            required: true,
            nullable: false,
        }
    }
}

#[derive(Clone, Copy)]
pub(super) enum ValueKind {
    ItemId,
    Text,
    Enum(&'static [(&'static str, E)]),
    Unsigned,
    Signed,
    Signed32,
    Unsigned32,
    Boolean,
    Structured,
    Object(ObjectSchema),
    List(ListKind),
    DiscardString,
    AgentStates,
}

#[derive(Clone, Copy)]
pub(super) enum ListKind {
    Text(F),
    Object(ObjectSchema),
    Structured(F),
    DiscardText,
}

#[derive(Clone, Copy, Eq, PartialEq)]
pub(super) enum ObjectSchema {
    HookFragment,
    MemoryCitation,
    MemoryCitationEntry,
    CommandAction,
    FileChange,
    FileChangeKind,
    McpAppContext,
    McpResult,
    McpError,
    DynamicContent,
    CollabAgentState,
    WebSearchAction,
}

macro_rules! field {
    ($name:literal, $field:ident, $kind:expr, required) => {
        FieldSpec {
            name: $name,
            field: F::$field,
            value: $kind,
            required: true,
            nullable: false,
        }
    };
    ($name:literal, $field:ident, $kind:expr, optional) => {
        FieldSpec {
            name: $name,
            field: F::$field,
            value: $kind,
            required: false,
            nullable: true,
        }
    };
    ($name:literal, $field:ident, $kind:expr, default) => {
        FieldSpec {
            name: $name,
            field: F::$field,
            value: $kind,
            required: false,
            nullable: false,
        }
    };
}

pub(super) const PHASE: &[(&str, E)] = &[
    ("commentary", E::Commentary),
    ("final_answer", E::FinalAnswer),
];
pub(super) const COMMAND_SOURCE: &[(&str, E)] = &[
    ("agent", E::Agent),
    ("userShell", E::UserShell),
    ("unifiedExecStartup", E::UnifiedExecStartup),
    ("unifiedExecInteraction", E::UnifiedExecInteraction),
];
pub(super) const STATUS4: &[(&str, E)] = &[
    ("inProgress", E::InProgress),
    ("completed", E::Completed),
    ("failed", E::Failed),
    ("declined", E::Declined),
];
pub(super) const STATUS3: &[(&str, E)] = &[
    ("inProgress", E::InProgress),
    ("completed", E::Completed),
    ("failed", E::Failed),
];
pub(super) const COLLAB_TOOL: &[(&str, E)] = &[
    ("spawnAgent", E::SpawnAgent),
    ("sendInput", E::SendInput),
    ("resumeAgent", E::ResumeAgent),
    ("wait", E::Wait),
    ("closeAgent", E::CloseAgent),
];
pub(super) const AGENT_STATUS: &[(&str, E)] = &[
    ("pendingInit", E::PendingInit),
    ("running", E::Running),
    ("interrupted", E::Interrupted),
    ("completed", E::Completed),
    ("errored", E::Errored),
    ("shutdown", E::Shutdown),
    ("notFound", E::NotFound),
];
pub(super) const SUBAGENT_KIND: &[(&str, E)] = &[
    ("started", E::SubAgentStarted),
    ("interacted", E::SubAgentInteracted),
    ("interrupted", E::SubAgentInterrupted),
];
pub(super) const COMMAND_ACTION: &[(&str, E)] = &[
    ("read", E::Read),
    ("listFiles", E::ListFiles),
    ("search", E::Search),
    ("unknown", E::Unknown),
];
pub(super) const PATCH_KIND: &[(&str, E)] = &[
    ("add", E::Add),
    ("delete", E::Delete),
    ("update", E::Update),
];
pub(super) const DYNAMIC_CONTENT: &[(&str, E)] =
    &[("inputText", E::InputText), ("inputImage", E::InputImage)];
pub(super) const WEB_ACTION: &[(&str, E)] = &[
    ("search", E::Search),
    ("openPage", E::OpenPage),
    ("findInPage", E::FindInPage),
];

include!("schema/item.rs");
include!("schema/object.rs");
