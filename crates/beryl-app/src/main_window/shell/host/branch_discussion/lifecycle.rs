use super::*;

impl MainWindowShellRoot {
    pub(in crate::main_window::shell::host) fn suspend_captured_branch(&mut self) {
        self.branch_discussion.qualification_task = None;
        if let Some(operation) = &self.branch_discussion.operation {
            if let Ok(state) = operation.try_lock() {
                state.cancellation.cancel();
            }
        }
        self.branch_discussion.task = None;
    }

    pub(in crate::main_window::shell::host) fn release_captured_branch_sources(&mut self) {
        for operation in self
            .branch_discussion
            .operation
            .iter()
            .chain(&self.branch_discussion.retained)
        {
            if let Ok(mut state) = operation.try_lock() {
                state.reader = None;
                if state.stage == BranchStage::Creating && state.outcome.is_none() {
                    state.outcome = Some(Err(
                        "Discussion creation was cancelled before durable admission.".into(),
                    ));
                }
            }
        }
    }

    pub(in crate::main_window::shell::host) fn sync_captured_branch(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.sync_captured_branch_menu(cx);
        if self.running_threads.reads_suspended
            || self.running_threads.views_retired
            || self.branch_discussion.task.is_some()
        {
            return;
        }
        let Some(operation) = self.branch_discussion.operation.clone() else {
            return;
        };
        let mut state = operation
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if state.reader.as_ref().is_none_or(|reader| !reader.current()) {
            let Some(reader) = self
                .running_threads
                .reader
                .as_ref()
                .filter(|reader| reader.current() && reader.branch_home_id() == state.home_id)
            else {
                return;
            };
            state.reader = Some(reader.clone());
        }
        let ready = matches!(
            state.stage,
            BranchStage::Creating | BranchStage::Reconciling
        ) && state.outcome.is_some();
        drop(state);
        if ready {
            self.settle_captured_branch(operation, window, cx);
        }
    }
}
