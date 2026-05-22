use std::collections::VecDeque;

use beryl_model::{
    conversation::ConversationThreadId,
    workspace::{BerylWorkspaceId, WorkspaceId},
};

pub(crate) const DEFAULT_THREAD_NAVIGATION_HISTORY_LIMIT: usize = 64;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ThreadNavigationEntry {
    thread_id: ConversationThreadId,
    execution_target: WorkspaceId,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct WorkspaceThreadNavigationHistory {
    workspace_id: BerylWorkspaceId,
    history: ThreadNavigationHistory,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ThreadNavigationActivationSource {
    ThreadSelector,
    TranscriptThreadLink,
    BranchBreadcrumb,
    BackwardNavigation,
    ForwardNavigation,
    NonHistory,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PendingThreadNavigationActivation {
    workspace_id: BerylWorkspaceId,
    source: ThreadNavigationActivationSource,
    origin: Option<ThreadNavigationEntry>,
    target: ThreadNavigationEntry,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ThreadNavigationHistory {
    current: Option<ThreadNavigationEntry>,
    backward: VecDeque<ThreadNavigationEntry>,
    forward: VecDeque<ThreadNavigationEntry>,
    max_stack_entries: usize,
}

impl ThreadNavigationEntry {
    pub(crate) fn new(
        thread_id: ConversationThreadId,
        execution_target: WorkspaceId,
    ) -> Option<Self> {
        (!thread_id.as_str().is_empty()).then_some(Self {
            thread_id,
            execution_target,
        })
    }

    pub(crate) fn from_thread_id(
        thread_id: impl Into<String>,
        execution_target: WorkspaceId,
    ) -> Option<Self> {
        Self::new(ConversationThreadId::new(thread_id), execution_target)
    }

    pub(crate) fn thread_id(&self) -> &ConversationThreadId {
        &self.thread_id
    }

    pub(crate) fn execution_target(&self) -> &WorkspaceId {
        &self.execution_target
    }
}

impl WorkspaceThreadNavigationHistory {
    pub(crate) fn new(workspace_id: BerylWorkspaceId) -> Self {
        Self {
            workspace_id,
            history: ThreadNavigationHistory::default(),
        }
    }

    pub(crate) fn with_limit(workspace_id: BerylWorkspaceId, max_stack_entries: usize) -> Self {
        Self {
            workspace_id,
            history: ThreadNavigationHistory::with_limit(max_stack_entries),
        }
    }

    pub(crate) fn workspace_id(&self) -> &BerylWorkspaceId {
        &self.workspace_id
    }

    pub(crate) fn history(&self) -> &ThreadNavigationHistory {
        &self.history
    }

    pub(crate) fn history_mut(&mut self) -> &mut ThreadNavigationHistory {
        &mut self.history
    }
}

impl ThreadNavigationActivationSource {
    pub(crate) fn records_history(self) -> bool {
        !matches!(self, Self::NonHistory)
    }

    fn records_new_selection(self) -> bool {
        matches!(
            self,
            Self::ThreadSelector | Self::TranscriptThreadLink | Self::BranchBreadcrumb
        )
    }
}

impl PendingThreadNavigationActivation {
    pub(crate) fn new(
        workspace_id: BerylWorkspaceId,
        source: ThreadNavigationActivationSource,
        origin: Option<ThreadNavigationEntry>,
        target: ThreadNavigationEntry,
    ) -> Option<Self> {
        source.records_history().then_some(Self {
            workspace_id,
            source,
            origin,
            target,
        })
    }

    pub(crate) fn workspace_id(&self) -> &BerylWorkspaceId {
        &self.workspace_id
    }

    pub(crate) fn target(&self) -> &ThreadNavigationEntry {
        &self.target
    }

    pub(crate) fn commit(self, history: &mut ThreadNavigationHistory) -> bool {
        if self.source.records_new_selection() {
            if history.current() != self.origin.as_ref() {
                history.replace_current_thread(self.origin);
            }
            return history.record_selected_thread(Some(self.target));
        }

        match self.source {
            ThreadNavigationActivationSource::BackwardNavigation => {
                if history.back_target() == Some(&self.target) {
                    history.commit_backward().is_some()
                } else {
                    false
                }
            }
            ThreadNavigationActivationSource::ForwardNavigation => {
                if history.forward_target() == Some(&self.target) {
                    history.commit_forward().is_some()
                } else {
                    false
                }
            }
            ThreadNavigationActivationSource::NonHistory
            | ThreadNavigationActivationSource::ThreadSelector
            | ThreadNavigationActivationSource::TranscriptThreadLink
            | ThreadNavigationActivationSource::BranchBreadcrumb => false,
        }
    }
}

impl Default for ThreadNavigationHistory {
    fn default() -> Self {
        Self::with_limit(DEFAULT_THREAD_NAVIGATION_HISTORY_LIMIT)
    }
}

impl ThreadNavigationHistory {
    pub(crate) fn with_limit(max_stack_entries: usize) -> Self {
        Self {
            current: None,
            backward: VecDeque::new(),
            forward: VecDeque::new(),
            max_stack_entries,
        }
    }

    pub(crate) fn current(&self) -> Option<&ThreadNavigationEntry> {
        self.current.as_ref()
    }

    pub(crate) fn back_target(&self) -> Option<&ThreadNavigationEntry> {
        self.backward.back()
    }

    pub(crate) fn forward_target(&self) -> Option<&ThreadNavigationEntry> {
        self.forward.back()
    }

    pub(crate) fn backward_targets(
        &self,
    ) -> impl DoubleEndedIterator<Item = &ThreadNavigationEntry> {
        self.backward.iter()
    }

    pub(crate) fn forward_targets(
        &self,
    ) -> impl DoubleEndedIterator<Item = &ThreadNavigationEntry> {
        self.forward.iter()
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.current.is_none() && self.backward.is_empty() && self.forward.is_empty()
    }

    pub(crate) fn record_selected_thread(&mut self, entry: Option<ThreadNavigationEntry>) -> bool {
        let Some(entry) = entry else {
            return false;
        };
        if self.current.as_ref() == Some(&entry) {
            return false;
        }

        if let Some(previous_current) = self.current.replace(entry) {
            Self::push_bounded(&mut self.backward, previous_current, self.max_stack_entries);
        }
        self.forward.clear();
        true
    }

    pub(crate) fn replace_current_thread(&mut self, entry: Option<ThreadNavigationEntry>) -> bool {
        if self.current == entry {
            return false;
        }
        self.current = entry;
        self.forward.clear();
        true
    }

    pub(crate) fn commit_backward(&mut self) -> Option<ThreadNavigationEntry> {
        let target = self.backward.pop_back()?;
        if let Some(previous_current) = self.current.replace(target.clone()) {
            Self::push_bounded(&mut self.forward, previous_current, self.max_stack_entries);
        }
        Some(target)
    }

    pub(crate) fn commit_forward(&mut self) -> Option<ThreadNavigationEntry> {
        let target = self.forward.pop_back()?;
        if let Some(previous_current) = self.current.replace(target.clone()) {
            Self::push_bounded(&mut self.backward, previous_current, self.max_stack_entries);
        }
        Some(target)
    }

    pub(crate) fn discard_entries_for_execution_target(
        &mut self,
        execution_target: &WorkspaceId,
    ) -> bool {
        if self
            .current
            .as_ref()
            .is_some_and(|entry| entry.execution_target() == execution_target)
        {
            let changed = !self.is_empty();
            self.current = None;
            self.backward.clear();
            self.forward.clear();
            return changed;
        }

        let backward_len = self.backward.len();
        let forward_len = self.forward.len();
        self.backward
            .retain(|entry| entry.execution_target() != execution_target);
        self.forward
            .retain(|entry| entry.execution_target() != execution_target);
        backward_len != self.backward.len() || forward_len != self.forward.len()
    }

    fn push_bounded(
        stack: &mut VecDeque<ThreadNavigationEntry>,
        entry: ThreadNavigationEntry,
        max_stack_entries: usize,
    ) {
        if max_stack_entries == 0 {
            stack.clear();
            return;
        }
        stack.push_back(entry);
        while stack.len() > max_stack_entries {
            stack.pop_front();
        }
    }
}
