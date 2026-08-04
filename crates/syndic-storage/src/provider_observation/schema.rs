use super::{
    ProviderDeltaKind, ProviderEnumValue as E, ProviderField as F, ProviderObservationBegin,
    ProviderObservationItemKind,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct FieldSpec {
    pub(crate) field: F,
    pub(crate) value: ValueKind,
    pub(crate) required: bool,
    pub(crate) nullable: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ValueKind {
    Text,
    Identity,
    Enum(EnumDomain),
    Unsigned,
    Signed,
    Signed32,
    Unsigned32,
    Boolean,
    Structured,
    Object(ObjectSchema),
    List(ListKind),
    AgentStates,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub(crate) enum ListKind {
    HookFragments,
    MemoryCitationEntries,
    MemoryCitationThreadIds,
    ReasoningSummaries,
    CommandActions,
    FileChanges,
    McpResultContents,
    DynamicContentItems,
    CollabReceiverThreadIds,
    WebSearchActionQueries,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub(crate) enum ObjectSchema {
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub(crate) enum EnumDomain {
    Phase,
    CommandSource,
    Status4,
    Status3,
    CollabTool,
    AgentStatus,
    SubAgentKind,
    CommandAction,
    PatchKind,
    DynamicContent,
    WebAction,
}

macro_rules! field {
    ($field:ident, $kind:expr, required) => {
        FieldSpec {
            field: F::$field,
            value: $kind,
            required: true,
            nullable: false,
        }
    };
    ($field:ident, $kind:expr, optional) => {
        FieldSpec {
            field: F::$field,
            value: $kind,
            required: false,
            nullable: true,
        }
    };
    ($field:ident, $kind:expr, default) => {
        FieldSpec {
            field: F::$field,
            value: $kind,
            required: false,
            nullable: false,
        }
    };
}

include!("schema/items.rs");
include!("schema/objects.rs");

const LIFECYCLE_TIMESTAMP: FieldSpec = field!(LifecycleObservedAt, ValueKind::Unsigned, required);

const DELTA_TEXT: &[FieldSpec] = &[
    field!(ItemId, ValueKind::Identity, required),
    field!(DeltaText, ValueKind::Text, required),
];
const DELTA_SUMMARY_PART: &[FieldSpec] = &[
    field!(ItemId, ValueKind::Identity, required),
    field!(DeltaSummaryIndex, ValueKind::Unsigned, required),
];
const DELTA_SUMMARY_TEXT: &[FieldSpec] = &[
    field!(ItemId, ValueKind::Identity, required),
    field!(DeltaSummaryIndex, ValueKind::Unsigned, required),
    field!(DeltaText, ValueKind::Text, required),
];
const DELTA_REASONING_TEXT: &[FieldSpec] = &[
    field!(ItemId, ValueKind::Identity, required),
    field!(DeltaContentIndex, ValueKind::Unsigned, required),
];
const DELTA_CHANGES: &[FieldSpec] = &[
    field!(ItemId, ValueKind::Identity, required),
    field!(
        DeltaChanges,
        ValueKind::List(ListKind::FileChanges),
        required
    ),
];
const DELTA_MCP_PROGRESS: &[FieldSpec] = &[
    field!(ItemId, ValueKind::Identity, required),
    field!(McpProgressMessage, ValueKind::Text, required),
];

pub(crate) fn top_field(begin: ProviderObservationBegin, field: F) -> Option<FieldSpec> {
    if matches!(begin, ProviderObservationBegin::Item { .. }) && field == F::LifecycleObservedAt {
        return Some(LIFECYCLE_TIMESTAMP);
    }
    top_fields(begin)
        .iter()
        .copied()
        .find(|spec| spec.field == field)
}

pub(crate) fn top_fields(begin: ProviderObservationBegin) -> &'static [FieldSpec] {
    match begin {
        ProviderObservationBegin::Item { kind, .. } => item_fields(kind),
        ProviderObservationBegin::Delta { kind } => match kind {
            ProviderDeltaKind::AgentMessage
            | ProviderDeltaKind::Plan
            | ProviderDeltaKind::CommandExecutionOutput
            | ProviderDeltaKind::FileChangeOutput => DELTA_TEXT,
            ProviderDeltaKind::ReasoningSummaryPartAdded => DELTA_SUMMARY_PART,
            ProviderDeltaKind::ReasoningSummaryText => DELTA_SUMMARY_TEXT,
            ProviderDeltaKind::ReasoningTextObserved => DELTA_REASONING_TEXT,
            ProviderDeltaKind::FileChangePatchUpdated => DELTA_CHANGES,
            ProviderDeltaKind::McpToolCallProgress => DELTA_MCP_PROGRESS,
        },
    }
}

pub(crate) const fn list_value(kind: ListKind) -> (F, ValueKind) {
    match kind {
        ListKind::HookFragments => (
            F::HookFragments,
            ValueKind::Object(ObjectSchema::HookFragment),
        ),
        ListKind::MemoryCitationEntries => (
            F::MemoryCitationEntries,
            ValueKind::Object(ObjectSchema::MemoryCitationEntry),
        ),
        ListKind::MemoryCitationThreadIds => (F::MemoryCitationThreadId, ValueKind::Text),
        ListKind::ReasoningSummaries => (F::ReasoningSummary, ValueKind::Text),
        ListKind::CommandActions => (
            F::CommandActions,
            ValueKind::Object(ObjectSchema::CommandAction),
        ),
        ListKind::FileChanges => (F::FileChanges, ValueKind::Object(ObjectSchema::FileChange)),
        ListKind::McpResultContents => (F::McpResultContent, ValueKind::Structured),
        ListKind::DynamicContentItems => (
            F::DynamicContentItems,
            ValueKind::Object(ObjectSchema::DynamicContent),
        ),
        ListKind::CollabReceiverThreadIds => (F::CollabReceiverThreadId, ValueKind::Identity),
        ListKind::WebSearchActionQueries => (F::WebSearchActionQueries, ValueKind::Text),
    }
}

pub(crate) const fn enum_allowed(domain: EnumDomain, value: E) -> bool {
    match domain {
        EnumDomain::Phase => matches!(value, E::Commentary | E::FinalAnswer),
        EnumDomain::CommandSource => matches!(
            value,
            E::Agent | E::UserShell | E::UnifiedExecStartup | E::UnifiedExecInteraction
        ),
        EnumDomain::Status4 => {
            matches!(
                value,
                E::InProgress | E::Completed | E::Failed | E::Declined
            )
        }
        EnumDomain::Status3 => matches!(value, E::InProgress | E::Completed | E::Failed),
        EnumDomain::CollabTool => matches!(
            value,
            E::SpawnAgent | E::SendInput | E::ResumeAgent | E::Wait | E::CloseAgent
        ),
        EnumDomain::AgentStatus => matches!(
            value,
            E::PendingInit
                | E::Running
                | E::Interrupted
                | E::Completed
                | E::Errored
                | E::Shutdown
                | E::NotFound
        ),
        EnumDomain::SubAgentKind => matches!(
            value,
            E::SubAgentStarted | E::SubAgentInteracted | E::SubAgentInterrupted
        ),
        EnumDomain::CommandAction => {
            matches!(value, E::Read | E::ListFiles | E::Search | E::Unknown)
        }
        EnumDomain::PatchKind => matches!(value, E::Add | E::Delete | E::Update),
        // Inline image payloads are rejected before the backend emits an observation.
        EnumDomain::DynamicContent => matches!(value, E::InputText),
        EnumDomain::WebAction => {
            matches!(value, E::Search | E::OpenPage | E::FindInPage | E::Other)
        }
    }
}

pub(crate) fn required_fields_present(fields: &[FieldSpec], seen: [u64; 2]) -> bool {
    fields
        .iter()
        .filter(|field| field.required)
        .all(|field| field_seen(seen, field.field))
}

pub(crate) fn variant_field(object: ObjectSchema, field: F) -> Option<FieldSpec> {
    E::ALL.iter().find_map(|variant| {
        variant_fields(object, *variant).and_then(|fields| {
            fields
                .iter()
                .copied()
                .find(|candidate| candidate.field == field)
        })
    })
}

pub(crate) fn only_fields_seen(fields: &[FieldSpec], seen: [u64; 2]) -> bool {
    F::ALL.iter().copied().all(|field| {
        !field_seen(seen, field) || fields.iter().any(|candidate| candidate.field == field)
    })
}

pub(crate) const fn field_seen(seen: [u64; 2], field: F) -> bool {
    let tag = field as usize;
    seen[tag / 64] & (1_u64 << (tag % 64)) != 0
}

pub(crate) fn mark_field(seen: &mut [u64; 2], field: F) -> bool {
    let tag = usize::from(field.tag());
    let bit = 1_u64 << (tag % 64);
    let slot = &mut seen[tag / 64];
    let duplicate = *slot & bit != 0;
    *slot |= bit;
    duplicate
}
