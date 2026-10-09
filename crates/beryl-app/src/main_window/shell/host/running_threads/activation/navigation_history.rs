use beryl_model::SyndicThreadId;
use std::collections::VecDeque;

const HISTORY_CAPACITY: usize = 64;

#[derive(Default)]
pub(in crate::main_window::shell::host::running_threads) struct ThreadNavigationHistory {
    entries: VecDeque<SyndicThreadId>,
    pending: Option<(Option<SyndicThreadId>, SyndicThreadId)>,
}

impl ThreadNavigationHistory {
    pub(super) fn begin(&mut self, prior: Option<SyndicThreadId>, target: SyndicThreadId) {
        self.pending = Some((prior, target));
    }

    pub(super) fn cancel(&mut self) {
        self.pending = None;
    }

    #[cfg(test)]
    pub(super) fn test_entries(&self) -> Vec<SyndicThreadId> {
        self.entries.iter().copied().collect()
    }

    pub(super) fn settle(&mut self, selected: Option<SyndicThreadId>) {
        let Some((prior, target)) = self.pending.take() else {
            return;
        };
        if selected != Some(target) || prior == Some(target) {
            return;
        }
        if let Some(prior) = prior {
            self.push(prior);
        }
        self.push(target);
    }

    fn push(&mut self, thread: SyndicThreadId) {
        if self.entries.back() == Some(&thread) {
            return;
        }
        if self.entries.len() == HISTORY_CAPACITY {
            self.entries.pop_front();
        }
        self.entries.push_back(thread);
    }
}

#[cfg(test)]
mod tests {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/unit/thread_navigation_history.rs"
    ));
}
