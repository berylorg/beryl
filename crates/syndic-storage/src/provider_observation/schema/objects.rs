const HOOK_FRAGMENT: &[FieldSpec] = &[
    field!(HookFragmentText, ValueKind::Text, required),
    field!(HookRunId, ValueKind::Text, required),
];
const MEMORY_CITATION: &[FieldSpec] = &[
    field!(
        MemoryCitationEntries,
        ValueKind::List(ListKind::MemoryCitationEntries),
        required
    ),
    field!(
        MemoryCitationThreadIds,
        ValueKind::List(ListKind::MemoryCitationThreadIds),
        required
    ),
];
const MEMORY_ENTRY: &[FieldSpec] = &[
    field!(MemoryCitationPath, ValueKind::Text, required),
    field!(MemoryCitationLineStart, ValueKind::Unsigned32, required),
    field!(MemoryCitationLineEnd, ValueKind::Unsigned32, required),
    field!(MemoryCitationNote, ValueKind::Text, required),
];
const FILE_CHANGE: &[FieldSpec] = &[
    field!(FileChangePath, ValueKind::Text, required),
    field!(FileChangeDiff, ValueKind::Text, required),
    field!(
        FileChangeKind,
        ValueKind::Object(ObjectSchema::FileChangeKind),
        required
    ),
];
const MCP_CONTEXT: &[FieldSpec] = &[
    field!(McpConnectorId, ValueKind::Text, required),
    field!(McpLinkId, ValueKind::Text, optional),
    field!(McpResourceUri, ValueKind::Text, optional),
    field!(McpAppName, ValueKind::Text, optional),
    field!(McpTemplateId, ValueKind::Text, optional),
    field!(McpActionName, ValueKind::Text, optional),
];
const MCP_RESULT: &[FieldSpec] = &[
    field!(
        McpResultContents,
        ValueKind::List(ListKind::McpResultContents),
        required
    ),
    field!(McpStructuredContent, ValueKind::Structured, optional),
    field!(McpMeta, ValueKind::Structured, optional),
];
const MCP_ERROR: &[FieldSpec] = &[field!(McpErrorMessage, ValueKind::Text, required)];
const COLLAB_STATE: &[FieldSpec] = &[
    field!(
        CollabAgentStateStatus,
        ValueKind::Enum(EnumDomain::AgentStatus),
        required
    ),
    field!(CollabAgentStateMessage, ValueKind::Text, optional),
];

const ACTION_READ: &[FieldSpec] = &[
    field!(CommandActionCommand, ValueKind::Text, required),
    field!(CommandActionName, ValueKind::Text, required),
    field!(CommandActionPath, ValueKind::Text, required),
];
const ACTION_LIST: &[FieldSpec] = &[
    field!(CommandActionCommand, ValueKind::Text, required),
    field!(CommandActionPath, ValueKind::Text, optional),
];
const ACTION_SEARCH: &[FieldSpec] = &[
    field!(CommandActionCommand, ValueKind::Text, required),
    field!(CommandActionQuery, ValueKind::Text, optional),
    field!(CommandActionPath, ValueKind::Text, optional),
];
const ACTION_UNKNOWN: &[FieldSpec] = &[field!(CommandActionCommand, ValueKind::Text, required)];
const PATCH_NONE: &[FieldSpec] = &[];
const PATCH_UPDATE: &[FieldSpec] = &[field!(FileChangeMovePath, ValueKind::Text, optional)];
const DYNAMIC_TEXT: &[FieldSpec] = &[field!(DynamicOutputText, ValueKind::Text, required)];
const WEB_SEARCH: &[FieldSpec] = &[
    field!(WebSearchActionQuery, ValueKind::Text, optional),
    field!(
        WebSearchActionQueryList,
        ValueKind::List(ListKind::WebSearchActionQueries),
        optional
    ),
];
const WEB_OPEN: &[FieldSpec] = &[field!(WebSearchUrl, ValueKind::Text, optional)];
const WEB_FIND: &[FieldSpec] = &[
    field!(WebSearchUrl, ValueKind::Text, optional),
    field!(WebSearchPattern, ValueKind::Text, optional),
];

pub(crate) fn object_fields(schema: ObjectSchema) -> Option<&'static [FieldSpec]> {
    Some(match schema {
        ObjectSchema::HookFragment => HOOK_FRAGMENT,
        ObjectSchema::MemoryCitation => MEMORY_CITATION,
        ObjectSchema::MemoryCitationEntry => MEMORY_ENTRY,
        ObjectSchema::FileChange => FILE_CHANGE,
        ObjectSchema::McpAppContext => MCP_CONTEXT,
        ObjectSchema::McpResult => MCP_RESULT,
        ObjectSchema::McpError => MCP_ERROR,
        ObjectSchema::CollabAgentState => COLLAB_STATE,
        ObjectSchema::CommandAction
        | ObjectSchema::FileChangeKind
        | ObjectSchema::DynamicContent
        | ObjectSchema::WebSearchAction => return None,
    })
}

pub(crate) fn discriminant(schema: ObjectSchema) -> Option<(F, EnumDomain)> {
    Some(match schema {
        ObjectSchema::CommandAction => (F::CommandActionKind, EnumDomain::CommandAction),
        ObjectSchema::FileChangeKind => (F::FileChangeKind, EnumDomain::PatchKind),
        ObjectSchema::DynamicContent => (F::DynamicContentItemKind, EnumDomain::DynamicContent),
        ObjectSchema::WebSearchAction => (F::WebSearchActionKind, EnumDomain::WebAction),
        _ => return None,
    })
}

pub(crate) fn variant_fields(schema: ObjectSchema, variant: E) -> Option<&'static [FieldSpec]> {
    Some(match (schema, variant) {
        (ObjectSchema::CommandAction, E::Read) => ACTION_READ,
        (ObjectSchema::CommandAction, E::ListFiles) => ACTION_LIST,
        (ObjectSchema::CommandAction, E::Search) => ACTION_SEARCH,
        (ObjectSchema::CommandAction, E::Unknown) => ACTION_UNKNOWN,
        (ObjectSchema::FileChangeKind, E::Add | E::Delete) => PATCH_NONE,
        (ObjectSchema::FileChangeKind, E::Update) => PATCH_UPDATE,
        (ObjectSchema::DynamicContent, E::InputText) => DYNAMIC_TEXT,
        (ObjectSchema::WebSearchAction, E::Search) => WEB_SEARCH,
        (ObjectSchema::WebSearchAction, E::OpenPage) => WEB_OPEN,
        (ObjectSchema::WebSearchAction, E::FindInPage) => WEB_FIND,
        (ObjectSchema::WebSearchAction, E::Other) => PATCH_NONE,
        _ => return None,
    })
}
