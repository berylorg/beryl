use beryl_backend::{ProviderDeltaKind, ProviderField, ProviderItemKind};

#[derive(Clone, Copy)]
pub struct ItemFixture {
    pub kind: ProviderItemKind,
    pub started: &'static str,
    pub completed: &'static str,
}

pub fn item_fixtures() -> [ItemFixture; 17] {
    [
        fixture(
            ProviderItemKind::HookPrompt,
            r#"{"type":"hookPrompt","id":"item_1","fragments":[]}"#,
        ),
        fixture(
            ProviderItemKind::AgentMessage,
            r#"{"type":"agentMessage","id":"item_1","text":"text"}"#,
        ),
        fixture(
            ProviderItemKind::Plan,
            r#"{"type":"plan","id":"item_1","text":"plan"}"#,
        ),
        fixture(
            ProviderItemKind::Reasoning,
            r#"{"type":"reasoning","id":"item_1"}"#,
        ),
        lifecycle_fixture(
            ProviderItemKind::CommandExecution,
            r#"{"type":"commandExecution","id":"item_1","pluginId":"plugin","scriptPath":"scripts/run.ps1","command":"cmd","cwd":"C:/","status":"inProgress","commandActions":[]}"#,
            r#"{"type":"commandExecution","id":"item_1","pluginId":null,"scriptPath":null,"command":"cmd","cwd":"C:/","status":"completed","commandActions":[]}"#,
        ),
        lifecycle_fixture(
            ProviderItemKind::FileChange,
            r#"{"type":"fileChange","id":"item_1","status":"inProgress","changes":[]}"#,
            r#"{"type":"fileChange","id":"item_1","status":"completed","changes":[]}"#,
        ),
        lifecycle_fixture(
            ProviderItemKind::McpToolCall,
            r#"{"type":"mcpToolCall","id":"item_1","server":"server","tool":"tool","status":"inProgress","arguments":{}}"#,
            r#"{"type":"mcpToolCall","id":"item_1","server":"server","tool":"tool","status":"completed","arguments":{}}"#,
        ),
        lifecycle_fixture(
            ProviderItemKind::DynamicToolCall,
            r#"{"type":"dynamicToolCall","id":"item_1","tool":"tool","arguments":{},"status":"inProgress"}"#,
            r#"{"type":"dynamicToolCall","id":"item_1","tool":"tool","arguments":{},"status":"completed"}"#,
        ),
        lifecycle_fixture(
            ProviderItemKind::CollabAgentToolCall,
            r#"{"type":"collabAgentToolCall","id":"item_1","tool":"spawnAgent","status":"inProgress","senderThreadId":"thread_1","receiverThreadIds":[],"agentsStates":{}}"#,
            r#"{"type":"collabAgentToolCall","id":"item_1","tool":"spawnAgent","status":"completed","senderThreadId":"thread_1","receiverThreadIds":[],"agentsStates":{}}"#,
        ),
        fixture(
            ProviderItemKind::SubAgentActivity,
            r#"{"type":"subAgentActivity","id":"item_1","kind":"started","agentThreadId":"thread_2","agentPath":"agent"}"#,
        ),
        fixture(
            ProviderItemKind::WebSearch,
            r#"{"type":"webSearch","id":"item_1","query":"query"}"#,
        ),
        fixture(
            ProviderItemKind::ImageView,
            r#"{"type":"imageView","id":"item_1","path":"C:/image.png"}"#,
        ),
        fixture(
            ProviderItemKind::Sleep,
            r#"{"type":"sleep","id":"item_1","durationMs":1}"#,
        ),
        lifecycle_fixture(
            ProviderItemKind::StandaloneImageGeneration,
            r#"{"type":"imageGeneration","id":"item_1","status":"inProgress","result":"discarded"}"#,
            r#"{"type":"imageGeneration","id":"item_1","status":"completed","result":"discarded"}"#,
        ),
        fixture(
            ProviderItemKind::EnteredReviewMode,
            r#"{"type":"enteredReviewMode","id":"item_1","review":"review"}"#,
        ),
        fixture(
            ProviderItemKind::ExitedReviewMode,
            r#"{"type":"exitedReviewMode","id":"item_1","review":"review"}"#,
        ),
        fixture(
            ProviderItemKind::ContextCompaction,
            r#"{"type":"contextCompaction","id":"item_1"}"#,
        ),
    ]
}

const fn fixture(kind: ProviderItemKind, item: &'static str) -> ItemFixture {
    lifecycle_fixture(kind, item, item)
}

const fn lifecycle_fixture(
    kind: ProviderItemKind,
    started: &'static str,
    completed: &'static str,
) -> ItemFixture {
    ItemFixture {
        kind,
        started,
        completed,
    }
}

#[derive(Clone, Copy)]
pub struct StatusFixture {
    pub kind: ProviderItemKind,
    pub field: ProviderField,
    item_type: &'static str,
    fields: &'static str,
    pub terminal_statuses: &'static [&'static str],
}

impl StatusFixture {
    pub fn item(self, status: &str, status_first: bool) -> String {
        if status_first {
            format!(
                "{{\"type\":\"{}\",\"status\":\"{status}\",{}}}",
                self.item_type, self.fields
            )
        } else {
            format!(
                "{{\"type\":\"{}\",{},\"status\":\"{status}\"}}",
                self.item_type, self.fields
            )
        }
    }
}

pub fn status_fixtures() -> [StatusFixture; 6] {
    [
        StatusFixture {
            kind: ProviderItemKind::CommandExecution,
            field: ProviderField::CommandStatus,
            item_type: "commandExecution",
            fields: r#""id":"item_1","command":"cmd","cwd":"C:/","commandActions":[]"#,
            terminal_statuses: &["completed", "failed", "declined"],
        },
        StatusFixture {
            kind: ProviderItemKind::FileChange,
            field: ProviderField::FileChangeStatus,
            item_type: "fileChange",
            fields: r#""id":"item_1","changes":[]"#,
            terminal_statuses: &["completed", "failed", "declined"],
        },
        StatusFixture {
            kind: ProviderItemKind::McpToolCall,
            field: ProviderField::McpStatus,
            item_type: "mcpToolCall",
            fields: r#""id":"item_1","server":"server","tool":"tool","arguments":{}"#,
            terminal_statuses: &["completed", "failed"],
        },
        StatusFixture {
            kind: ProviderItemKind::DynamicToolCall,
            field: ProviderField::DynamicStatus,
            item_type: "dynamicToolCall",
            fields: r#""id":"item_1","tool":"tool","arguments":{}"#,
            terminal_statuses: &["completed", "failed"],
        },
        StatusFixture {
            kind: ProviderItemKind::CollabAgentToolCall,
            field: ProviderField::CollabStatus,
            item_type: "collabAgentToolCall",
            fields: r#""id":"item_1","tool":"spawnAgent","senderThreadId":"thread_1","receiverThreadIds":[],"agentsStates":{}"#,
            terminal_statuses: &["completed", "failed"],
        },
        StatusFixture {
            kind: ProviderItemKind::StandaloneImageGeneration,
            field: ProviderField::ImageGenerationStatus,
            item_type: "imageGeneration",
            fields: r#""id":"item_1","result":"discarded""#,
            terminal_statuses: &["completed", "failed"],
        },
    ]
}

pub fn delta_fixtures() -> [(&'static str, ProviderDeltaKind, &'static str); 9] {
    [
        (
            "item/agentMessage/delta",
            ProviderDeltaKind::AgentMessage,
            r#""delta":"text""#,
        ),
        (
            "item/plan/delta",
            ProviderDeltaKind::Plan,
            r#""delta":"text""#,
        ),
        (
            "item/reasoning/summaryPartAdded",
            ProviderDeltaKind::ReasoningSummaryPartAdded,
            r#""summaryIndex":0"#,
        ),
        (
            "item/reasoning/summaryTextDelta",
            ProviderDeltaKind::ReasoningSummaryText,
            r#""delta":"text","summaryIndex":0"#,
        ),
        (
            "item/reasoning/textDelta",
            ProviderDeltaKind::ReasoningTextObserved,
            r#""delta":"discarded","contentIndex":0"#,
        ),
        (
            "item/commandExecution/outputDelta",
            ProviderDeltaKind::CommandExecutionOutput,
            r#""delta":"output""#,
        ),
        (
            "item/fileChange/outputDelta",
            ProviderDeltaKind::FileChangeOutput,
            r#""delta":"output""#,
        ),
        (
            "item/fileChange/patchUpdated",
            ProviderDeltaKind::FileChangePatchUpdated,
            r#""changes":[]"#,
        ),
        (
            "item/mcpToolCall/progress",
            ProviderDeltaKind::McpToolCallProgress,
            r#""message":"progress""#,
        ),
    ]
}
