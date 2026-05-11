use std::path::PathBuf;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToolActivityLifecycle {
    Started,
    Updated,
    Completed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToolActivitySource {
    Reasoning,
    CommandExecution,
    FileChange,
    McpToolCall,
    DynamicToolCall,
    CollabAgentToolCall,
    WebSearch,
    ImageView,
    ImageGeneration,
    ContextCompaction,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ToolActivityAgentLabel {
    pub thread_id: String,
    pub label: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ToolActivityFileChangeSummary {
    pub file_count: usize,
    pub additions: usize,
    pub deletions: usize,
    pub single_file_path: Option<PathBuf>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ToolActivityCollabAgentSpawnMetadata {
    pub model: Option<String>,
    pub reasoning_effort: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ToolActivityEvent {
    pub thread_id: String,
    pub turn_id: String,
    pub item_id: String,
    pub item_type: String,
    pub source: ToolActivitySource,
    pub lifecycle: ToolActivityLifecycle,
    pub raw_tool_name: Option<String>,
    pub raw_tool_server: Option<String>,
    pub raw_tool_namespace: Option<String>,
    pub raw_resource_uri: Option<String>,
    pub raw_command: Option<String>,
    pub command_exec_process_id: Option<String>,
    pub raw_item_status: Option<String>,
    pub reasoning_summary_index: Option<usize>,
    pub reasoning_summary_delta: Option<String>,
    pub reasoning_summary_text: Option<String>,
    pub file_change_summary: Option<ToolActivityFileChangeSummary>,
    pub collab_agent_spawn_metadata: Option<ToolActivityCollabAgentSpawnMetadata>,
    pub receiver_thread_ids: Vec<String>,
    pub agent_label_updates: Vec<ToolActivityAgentLabel>,
}

impl ToolActivityAgentLabel {
    pub(crate) fn new(thread_id: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            thread_id: thread_id.into(),
            label: label.into(),
        }
    }
}

impl ToolActivityCollabAgentSpawnMetadata {
    pub(crate) fn from_raw(model: Option<&str>, reasoning_effort: Option<&str>) -> Option<Self> {
        let model = non_blank(model);
        let reasoning_effort = non_blank(reasoning_effort);
        (model.is_some() || reasoning_effort.is_some()).then_some(Self {
            model,
            reasoning_effort,
        })
    }
}

impl ToolActivitySource {
    pub fn from_item_type(item_type: &str) -> Option<Self> {
        match item_type {
            "reasoning" => Some(Self::Reasoning),
            "commandExecution" => Some(Self::CommandExecution),
            "fileChange" => Some(Self::FileChange),
            "mcpToolCall" => Some(Self::McpToolCall),
            "dynamicToolCall" => Some(Self::DynamicToolCall),
            "collabAgentToolCall" => Some(Self::CollabAgentToolCall),
            "webSearch" => Some(Self::WebSearch),
            "imageView" => Some(Self::ImageView),
            "imageGeneration" => Some(Self::ImageGeneration),
            "contextCompaction" => Some(Self::ContextCompaction),
            _ => None,
        }
    }

    pub fn is_operational_tool(self) -> bool {
        !matches!(self, Self::Reasoning)
    }
}

impl ToolActivityEvent {
    pub(crate) fn new(
        thread_id: impl Into<String>,
        turn_id: impl Into<String>,
        item_id: impl Into<String>,
        item_type: impl Into<String>,
        source: ToolActivitySource,
        lifecycle: ToolActivityLifecycle,
    ) -> Self {
        Self {
            thread_id: thread_id.into(),
            turn_id: turn_id.into(),
            item_id: item_id.into(),
            item_type: item_type.into(),
            source,
            lifecycle,
            raw_tool_name: None,
            raw_tool_server: None,
            raw_tool_namespace: None,
            raw_resource_uri: None,
            raw_command: None,
            command_exec_process_id: None,
            raw_item_status: None,
            reasoning_summary_index: None,
            reasoning_summary_delta: None,
            reasoning_summary_text: None,
            file_change_summary: None,
            collab_agent_spawn_metadata: None,
            receiver_thread_ids: Vec::new(),
            agent_label_updates: Vec::new(),
        }
    }

    pub(crate) fn with_raw_tool_name(mut self, raw_tool_name: Option<&str>) -> Self {
        self.raw_tool_name = non_empty(raw_tool_name).map(str::to_string);
        self
    }

    pub(crate) fn with_raw_tool_server(mut self, raw_tool_server: Option<&str>) -> Self {
        self.raw_tool_server = non_empty(raw_tool_server).map(str::to_string);
        self
    }

    pub(crate) fn with_raw_tool_namespace(mut self, raw_tool_namespace: Option<&str>) -> Self {
        self.raw_tool_namespace = non_empty(raw_tool_namespace).map(str::to_string);
        self
    }

    pub(crate) fn with_raw_resource_uri(mut self, raw_resource_uri: Option<&str>) -> Self {
        self.raw_resource_uri = non_empty(raw_resource_uri).map(str::to_string);
        self
    }

    pub(crate) fn with_raw_command(mut self, raw_command: Option<&str>) -> Self {
        self.raw_command = non_empty(raw_command).map(str::to_string);
        self
    }

    pub(crate) fn with_command_exec_process_id(
        mut self,
        command_exec_process_id: Option<&str>,
    ) -> Self {
        self.command_exec_process_id = non_empty(command_exec_process_id).map(str::to_string);
        self
    }

    pub(crate) fn with_raw_item_status(mut self, raw_item_status: Option<&str>) -> Self {
        self.raw_item_status = non_empty(raw_item_status).map(str::to_string);
        self
    }

    pub(crate) fn with_reasoning_summary_index(
        mut self,
        reasoning_summary_index: Option<usize>,
    ) -> Self {
        self.reasoning_summary_index = reasoning_summary_index;
        self
    }

    pub(crate) fn with_reasoning_summary_delta(mut self, delta: Option<&str>) -> Self {
        self.reasoning_summary_delta = non_empty(delta).map(str::to_string);
        self
    }

    pub(crate) fn with_reasoning_summary_text(mut self, text: Option<String>) -> Self {
        self.reasoning_summary_text = text.and_then(non_empty_string);
        self
    }

    pub(crate) fn with_file_change_summary(
        mut self,
        summary: Option<ToolActivityFileChangeSummary>,
    ) -> Self {
        self.file_change_summary = summary;
        self
    }

    pub(crate) fn with_collab_agent_spawn_metadata(
        mut self,
        metadata: Option<ToolActivityCollabAgentSpawnMetadata>,
    ) -> Self {
        self.collab_agent_spawn_metadata = metadata;
        self
    }

    pub(crate) fn with_receiver_thread_ids(mut self, receiver_thread_ids: Vec<String>) -> Self {
        self.receiver_thread_ids = receiver_thread_ids
            .into_iter()
            .filter_map(non_empty_string)
            .collect();
        self
    }

    pub(crate) fn with_agent_label_updates(
        mut self,
        agent_label_updates: Vec<ToolActivityAgentLabel>,
    ) -> Self {
        self.agent_label_updates = agent_label_updates;
        self
    }
}

fn non_empty(value: Option<&str>) -> Option<&str> {
    value.filter(|value| !value.is_empty())
}

fn non_empty_string(value: String) -> Option<String> {
    (!value.is_empty()).then_some(value)
}

fn non_blank(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}
