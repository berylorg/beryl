use beryl_model::{CasThreadId, ExecutionBinding, SyndicItemId, SyndicThreadId};
use syndic_storage::{
    ActivityDisplaySource, ActivityDisplayStatus, ActivityItemSource, ActivityQueryEntryRecord,
};

#[derive(Clone, Debug)]
pub struct ActivityAgentMetadata(super::ActivityAgentMetadata);

impl ActivityAgentMetadata {
    pub fn new(
        thread: CasThreadId,
        nickname: &str,
        model: Option<&str>,
        reasoning: Option<&str>,
    ) -> Option<Self> {
        super::ActivityAgentMetadata::new(thread, nickname, model, reasoning).map(Self)
    }

    pub fn thread_id(&self) -> &CasThreadId {
        self.0.thread_id()
    }
}

#[derive(Clone, Debug)]
pub struct ActivityDisplayRow(super::ActivityDisplayRow);

impl ActivityDisplayRow {
    pub fn source(&self) -> &ActivityItemSource {
        self.0.source()
    }
    pub fn item_id(&self) -> SyndicItemId {
        self.0.item_id()
    }
    pub fn status(&self) -> ActivityDisplayStatus {
        self.0.status()
    }
    pub fn agent(&self) -> &str {
        self.0.agent()
    }
    pub fn value(&self) -> &str {
        self.0.value()
    }
    pub fn tooltip(&self) -> &str {
        self.0.tooltip()
    }
    pub fn truncated(&self) -> bool {
        self.0.truncated()
    }
    pub fn decorate_agent(
        &mut self,
        parent: SyndicThreadId,
        metadata: Option<&ActivityAgentMetadata>,
    ) {
        self.0
            .decorate_agent(parent, metadata.map(|value| &value.0));
    }
}

pub fn normalize_powershell(command: &str) -> String {
    super::normalize_powershell(command)
}

pub fn format_row(
    selected_parent: SyndicThreadId,
    execution: Option<&ExecutionBinding>,
    entry: &ActivityQueryEntryRecord,
    source: ActivityDisplaySource,
    metadata: Option<&ActivityAgentMetadata>,
) -> ActivityDisplayRow {
    ActivityDisplayRow(super::format_row(
        selected_parent,
        execution,
        entry,
        source,
        metadata.map(|value| &value.0),
    ))
}

pub fn format_handoff(
    selected_parent: SyndicThreadId,
    entry: &ActivityQueryEntryRecord,
    metadata: Option<&ActivityAgentMetadata>,
) -> Option<ActivityDisplayRow> {
    super::format_handoff(selected_parent, entry, metadata.map(|value| &value.0))
        .map(ActivityDisplayRow)
}
