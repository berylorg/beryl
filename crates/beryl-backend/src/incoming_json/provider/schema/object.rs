const HOOK_FRAGMENT: &[FieldSpec] = &[
    field!("text", HookFragmentText, ValueKind::Text, required),
    field!("hookRunId", HookRunId, ValueKind::Text, required),
];
const MEMORY_CITATION: &[FieldSpec] = &[
    field!(
        "entries",
        MemoryCitationEntries,
        ValueKind::List(ListKind::Object(ObjectSchema::MemoryCitationEntry)),
        required
    ),
    field!(
        "threadIds",
        MemoryCitationThreadIds,
        ValueKind::List(ListKind::Text(F::MemoryCitationThreadId)),
        required
    ),
];
const MEMORY_ENTRY: &[FieldSpec] = &[
    field!("path", MemoryCitationPath, ValueKind::Text, required),
    field!(
        "lineStart",
        MemoryCitationLineStart,
        ValueKind::Unsigned32,
        required
    ),
    field!(
        "lineEnd",
        MemoryCitationLineEnd,
        ValueKind::Unsigned32,
        required
    ),
    field!("note", MemoryCitationNote, ValueKind::Text, required),
];
const FILE_CHANGE: &[FieldSpec] = &[
    field!("path", FileChangePath, ValueKind::Text, required),
    field!("diff", FileChangeDiff, ValueKind::Text, required),
    field!(
        "kind",
        FileChangeKind,
        ValueKind::Object(ObjectSchema::FileChangeKind),
        required
    ),
];
const MCP_CONTEXT: &[FieldSpec] = &[
    field!("connectorId", McpConnectorId, ValueKind::Text, required),
    field!("linkId", McpLinkId, ValueKind::Text, optional),
    field!("resourceUri", McpResourceUri, ValueKind::Text, optional),
    field!("appName", McpAppName, ValueKind::Text, optional),
    field!("templateId", McpTemplateId, ValueKind::Text, optional),
    field!("actionName", McpActionName, ValueKind::Text, optional),
];
const MCP_RESULT: &[FieldSpec] = &[
    field!(
        "content",
        McpResultContents,
        ValueKind::List(ListKind::Structured(F::McpResultContent)),
        required
    ),
    field!(
        "structuredContent",
        McpStructuredContent,
        ValueKind::Structured,
        optional
    ),
    field!("_meta", McpMeta, ValueKind::Structured, optional),
];
const MCP_ERROR: &[FieldSpec] = &[field!(
    "message",
    McpErrorMessage,
    ValueKind::Text,
    required
)];
const COLLAB_STATE: &[FieldSpec] = &[
    field!(
        "status",
        CollabAgentStateStatus,
        ValueKind::Enum(AGENT_STATUS),
        required
    ),
    field!(
        "message",
        CollabAgentStateMessage,
        ValueKind::Text,
        optional
    ),
];

pub(super) fn object_fields(schema: ObjectSchema) -> Option<&'static [FieldSpec]> {
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

const ACTION_READ: &[FieldSpec] = &[
    field!("command", CommandActionCommand, ValueKind::Text, required),
    field!("name", CommandActionName, ValueKind::Text, required),
    field!("path", CommandActionPath, ValueKind::Text, required),
];
const ACTION_LIST: &[FieldSpec] = &[
    field!("command", CommandActionCommand, ValueKind::Text, required),
    field!("path", CommandActionPath, ValueKind::Text, optional),
];
const ACTION_SEARCH: &[FieldSpec] = &[
    field!("command", CommandActionCommand, ValueKind::Text, required),
    field!("query", CommandActionQuery, ValueKind::Text, optional),
    field!("path", CommandActionPath, ValueKind::Text, optional),
];
const ACTION_UNKNOWN: &[FieldSpec] = &[field!(
    "command",
    CommandActionCommand,
    ValueKind::Text,
    required
)];
const PATCH_NONE: &[FieldSpec] = &[];
const PATCH_UPDATE: &[FieldSpec] = &[field!(
    "move_path",
    FileChangeMovePath,
    ValueKind::Text,
    optional
)];
const DYNAMIC_TEXT: &[FieldSpec] = &[field!("text", DynamicOutputText, ValueKind::Text, required)];
const DYNAMIC_IMAGE: &[FieldSpec] = &[field!(
    "image_url",
    DynamicOutputImageLocator,
    ValueKind::Text,
    required
)];
const WEB_SEARCH: &[FieldSpec] = &[
    field!("query", WebSearchActionQuery, ValueKind::Text, optional),
    field!(
        "queries",
        WebSearchActionQueryList,
        ValueKind::List(ListKind::Text(F::WebSearchActionQueries)),
        optional
    ),
];
const WEB_OPEN: &[FieldSpec] = &[field!("url", WebSearchUrl, ValueKind::Text, optional)];
const WEB_FIND: &[FieldSpec] = &[
    field!("url", WebSearchUrl, ValueKind::Text, optional),
    field!("pattern", WebSearchPattern, ValueKind::Text, optional),
];

pub(super) fn discriminant(schema: ObjectSchema) -> (F, &'static [(&'static str, E)]) {
    match schema {
        ObjectSchema::CommandAction => (F::CommandActionKind, COMMAND_ACTION),
        ObjectSchema::FileChangeKind => (F::FileChangeKind, PATCH_KIND),
        ObjectSchema::DynamicContent => (F::DynamicContentItemKind, DYNAMIC_CONTENT),
        ObjectSchema::WebSearchAction => (F::WebSearchActionKind, WEB_ACTION),
        _ => unreachable!("fixed schemas have no discriminator"),
    }
}

pub(super) fn variant_fields(schema: ObjectSchema, variant: E) -> &'static [FieldSpec] {
    match (schema, variant) {
        (ObjectSchema::CommandAction, E::Read) => ACTION_READ,
        (ObjectSchema::CommandAction, E::ListFiles) => ACTION_LIST,
        (ObjectSchema::CommandAction, E::Search) => ACTION_SEARCH,
        (ObjectSchema::CommandAction, E::Unknown) => ACTION_UNKNOWN,
        (ObjectSchema::FileChangeKind, E::Add | E::Delete) => PATCH_NONE,
        (ObjectSchema::FileChangeKind, E::Update) => PATCH_UPDATE,
        (ObjectSchema::DynamicContent, E::InputText) => DYNAMIC_TEXT,
        (ObjectSchema::DynamicContent, E::InputImage) => DYNAMIC_IMAGE,
        (ObjectSchema::WebSearchAction, E::Search) => WEB_SEARCH,
        (ObjectSchema::WebSearchAction, E::OpenPage) => WEB_OPEN,
        (ObjectSchema::WebSearchAction, E::FindInPage) => WEB_FIND,
        _ => unreachable!("variant belongs to its discriminator schema"),
    }
}
