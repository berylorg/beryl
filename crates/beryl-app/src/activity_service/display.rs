use beryl_model::{CasThreadId, ExecutionBinding, PathFlavor, SyndicItemId, SyndicThreadId};
use syndic_storage::{
    ActivityCompactFact, ActivityDisplaySource, ActivityDisplayStatus, ActivityDisplayValue,
    ActivityItemSource, ActivityQueryEntryRecord, ProviderItemKind,
};

mod paths;
#[cfg(feature = "test-faults")]
pub mod test_support;
use paths::relative_path;

#[derive(Clone, Debug)]
pub(crate) struct ActivityAgentMetadata {
    thread: CasThreadId,
    nickname: Box<str>,
    model: Option<Box<str>>,
    reasoning: Option<Box<str>>,
}
impl ActivityAgentMetadata {
    pub(crate) fn thread_id(&self) -> &CasThreadId {
        &self.thread
    }
    pub(crate) fn new(
        thread: CasThreadId,
        nickname: &str,
        model: Option<&str>,
        reasoning: Option<&str>,
    ) -> Option<Self> {
        let nickname = exact_label(nickname, 1024)?;
        let model = model.and_then(|value| exact_label(value, 512));
        let reasoning = if model.is_some() {
            reasoning.and_then(|value| exact_label(value, 128))
        } else {
            None
        };
        Some(Self {
            thread,
            nickname,
            model,
            reasoning,
        })
    }
}

fn exact_label(value: &str, maximum: usize) -> Option<Box<str>> {
    (!value.trim().is_empty() && value.len() <= maximum && !value.chars().any(char::is_control))
        .then(|| value.into())
}

#[derive(Clone, Debug)]
pub(crate) struct ActivityDisplayRow {
    source: ActivityItemSource,
    status: ActivityDisplayStatus,
    agent: Box<str>,
    value: Box<str>,
    tooltip: Box<str>,
    truncated: bool,
}
impl ActivityDisplayRow {
    pub(crate) fn source(&self) -> &ActivityItemSource {
        &self.source
    }
    pub(crate) fn item_id(&self) -> SyndicItemId {
        self.source.item_id()
    }
    pub(crate) fn status(&self) -> ActivityDisplayStatus {
        self.status
    }
    pub(crate) fn agent(&self) -> &str {
        &self.agent
    }
    pub(crate) fn value(&self) -> &str {
        &self.value
    }
    pub(crate) fn tooltip(&self) -> &str {
        &self.tooltip
    }
    pub(crate) fn truncated(&self) -> bool {
        self.truncated
    }
    pub(crate) fn decorate_agent(
        &mut self,
        parent: SyndicThreadId,
        metadata: Option<&ActivityAgentMetadata>,
    ) {
        self.agent = agent_label(parent, &self.source, metadata).into_boxed_str();
        self.tooltip = tooltip(&self.agent, &self.value, &mut self.truncated).into_boxed_str();
    }
}

pub(crate) fn format_row(
    selected_parent: SyndicThreadId,
    execution: Option<&ExecutionBinding>,
    _entry: &ActivityQueryEntryRecord,
    source: ActivityDisplaySource,
    metadata: Option<&ActivityAgentMetadata>,
) -> ActivityDisplayRow {
    let mut truncated = false;
    let value = match source.value() {
        ActivityDisplayValue::Command(text) => {
            truncated = text.truncated();
            if text.text().is_empty() {
                kind_name(source.kind()).to_owned()
            } else {
                normalize_powershell(text.text())
            }
        }
        ActivityDisplayValue::Reasoning(text) => {
            truncated = text.truncated();
            if text.text().is_empty() {
                "reasoning".to_owned()
            } else {
                format!("reasoning: {}", text.text())
            }
        }
        ActivityDisplayValue::FileChange(files) => {
            let path = files
                .sole_path()
                .and_then(|path| relative_path(path, execution));
            let subject = path.unwrap_or_else(|| format!("{} file(s)", files.change_count()));
            format!(
                "Patching {subject}, +{} -{}",
                files.additions(),
                files.deletions()
            )
        }
        ActivityDisplayValue::Tool(text) => {
            truncated = text.truncated();
            if text.text().is_empty() {
                kind_name(source.kind()).to_owned()
            } else {
                text.text().to_owned()
            }
        }
        ActivityDisplayValue::Kind => kind_name(source.kind()).to_owned(),
    };
    row(
        selected_parent,
        source.source().clone(),
        source.status(),
        value,
        truncated,
        metadata,
    )
}

pub(crate) fn format_handoff(
    selected_parent: SyndicThreadId,
    entry: &ActivityQueryEntryRecord,
    metadata: Option<&ActivityAgentMetadata>,
) -> Option<ActivityDisplayRow> {
    let ActivityCompactFact::ChildHandoff(fact) = entry.compact_fact()?;
    Some(row(
        selected_parent,
        entry.source().clone(),
        ActivityDisplayStatus::FinishedOk,
        format!("handoff: {} bytes", fact.byte_count()),
        false,
        metadata,
    ))
}

fn row(
    parent: SyndicThreadId,
    source: ActivityItemSource,
    status: ActivityDisplayStatus,
    mut value: String,
    mut truncated: bool,
    metadata: Option<&ActivityAgentMetadata>,
) -> ActivityDisplayRow {
    let agent = agent_label(parent, &source, metadata);
    if truncated {
        value.push('…');
    }
    truncate(&mut value, 1152, &mut truncated);
    let tooltip = tooltip(&agent, &value, &mut truncated);
    ActivityDisplayRow {
        source,
        status,
        agent: agent.into_boxed_str(),
        value: value.into_boxed_str(),
        tooltip: tooltip.into_boxed_str(),
        truncated,
    }
}

fn agent_label(
    parent: SyndicThreadId,
    source: &ActivityItemSource,
    metadata: Option<&ActivityAgentMetadata>,
) -> String {
    if source.thread_id() == parent {
        "Main".to_owned()
    } else if let Some(metadata) =
        metadata.filter(|metadata| &metadata.thread == source.cas_item().turn().thread_id())
    {
        if let Some(model) = &metadata.model {
            if let Some(reasoning) = &metadata.reasoning {
                format!("{} ({model}/{reasoning})", metadata.nickname)
            } else {
                format!("{} ({model})", metadata.nickname)
            }
        } else {
            metadata.nickname.to_string()
        }
    } else {
        String::new()
    }
}

fn tooltip(agent: &str, value: &str, truncated: &mut bool) -> String {
    let mut tooltip = if agent.is_empty() {
        value.to_owned()
    } else {
        format!("Agent {agent}\nActivity {value}")
    };
    truncate(&mut tooltip, 3072, truncated);
    tooltip
}

fn truncate(value: &mut String, maximum: usize, truncated: &mut bool) {
    if value.len() <= maximum {
        return;
    }
    let mut end = maximum - 3;
    while !value.is_char_boundary(end) {
        end -= 1;
    }
    value.truncate(end);
    value.push('…');
    *truncated = true;
}

pub(crate) fn normalize_powershell(command: &str) -> String {
    let bytes = command.as_bytes();
    let quote = bytes
        .first()
        .copied()
        .filter(|byte| *byte == b'"' || *byte == b'\'');
    let start = usize::from(quote.is_some());
    let end = if let Some(quote) = quote {
        bytes[start..]
            .iter()
            .position(|byte| *byte == quote)
            .map(|index| index + start)
    } else {
        bytes
            .iter()
            .position(u8::is_ascii_whitespace)
            .or(Some(bytes.len()))
    };
    let Some(end) = end else {
        return command.to_owned();
    };
    let token = &command[start..end];
    let mut normalized = String::new();
    let mut slash = false;
    for character in token.chars() {
        if character == '\\' {
            if !slash {
                normalized.push(character);
            }
            slash = true;
        } else {
            slash = false;
            normalized.push(character);
        }
    }
    let token_bytes = normalized.as_bytes();
    if token_bytes.len() < 3 || !token_bytes[0].is_ascii_alphabetic() || token_bytes[1] != b':' {
        return command.to_owned();
    }
    let tail = &normalized[2..];
    if !tail.eq_ignore_ascii_case("\\Windows\\System32\\WindowsPowerShell\\v1.0\\powershell.exe")
        && !tail.eq_ignore_ascii_case(
            "\\Windows.old\\System32\\WindowsPowerShell\\v1.0\\powershell.exe",
        )
    {
        return command.to_owned();
    }
    let rest = end + usize::from(quote.is_some());
    format!("powershell.exe{}", &command[rest..])
}

fn kind_name(kind: ProviderItemKind) -> &'static str {
    use ProviderItemKind::*;
    match kind {
        UserMessage => "userMessage",
        HookPrompt => "hookPrompt",
        AgentMessage => "agentMessage",
        Plan => "plan",
        Reasoning => "reasoning",
        CommandExecution => "commandExecution",
        FileChange => "fileChange",
        McpToolCall => "mcpToolCall",
        DynamicToolCall => "dynamicToolCall",
        CollabAgentToolCall => "collabAgentToolCall",
        SubAgentActivity => "subAgentActivity",
        WebSearch => "webSearch",
        ImageView => "imageView",
        Sleep => "sleep",
        StandaloneImageGeneration => "imageGeneration",
        EnteredReviewMode => "enteredReviewMode",
        ExitedReviewMode => "exitedReviewMode",
        ContextCompaction => "contextCompaction",
    }
}
