use beryl_model::SyndicThreadId;
use std::collections::VecDeque;

const HISTORY_CAPACITY: usize = 64;

#[derive(Default)]
pub(in crate::main_window::shell::host) struct ThreadNavigationHistory {
    entries: VecDeque<SyndicThreadId>,
    cursor: Option<usize>,
    active: Option<SyndicThreadId>,
    pending: Option<PendingNavigation>,
}

enum PendingNavigation {
    Selection(Option<SyndicThreadId>, SyndicThreadId),
    Movement {
        prior: SyndicThreadId,
        target: SyndicThreadId,
        position: usize,
    },
}

impl ThreadNavigationHistory {
    pub(super) fn begin(&mut self, prior: Option<SyndicThreadId>, target: SyndicThreadId) {
        if self.pending.is_none() {
            self.pending = Some(PendingNavigation::Selection(prior, target));
        }
    }

    pub(in crate::main_window::shell::host) fn target(
        &self,
        forward: bool,
    ) -> Option<SyndicThreadId> {
        if self.pending.is_some() {
            return None;
        }
        let cursor = self.cursor?;
        let recorded = self.entries.get(cursor).copied() == self.active;
        let position = if forward {
            if !recorded {
                return None;
            }
            cursor.checked_add(1)?
        } else if recorded {
            cursor.checked_sub(1)?
        } else {
            cursor
        };
        self.entries.get(position).copied()
    }

    pub(in crate::main_window::shell::host) fn begin_movement(
        &mut self,
        forward: bool,
    ) -> Option<SyndicThreadId> {
        let target = self.target(forward)?;
        let cursor = self.cursor?;
        let prior = self.active?;
        let position = if forward {
            cursor + 1
        } else if self.entries.get(cursor) == Some(&prior) {
            cursor - 1
        } else {
            cursor
        };
        self.pending = Some(PendingNavigation::Movement {
            prior,
            target,
            position,
        });
        Some(target)
    }

    pub(in crate::main_window::shell::host) fn synchronize(
        &mut self,
        active: Option<SyndicThreadId>,
    ) {
        if self.pending.is_some() || self.active == active {
            return;
        }
        self.active = active;
    }

    pub(super) fn settle_acquisition(&mut self, active: Option<SyndicThreadId>) {
        if self.pending.is_some() || self.active == active {
            return;
        }
        if let Some(cursor) = self.cursor {
            self.entries.truncate(cursor + 1);
        }
        self.active = active;
    }

    pub(in crate::main_window::shell::host) fn cancel(&mut self) {
        self.pending = None;
    }

    #[cfg(test)]
    pub(super) fn test_entries(&self) -> Vec<SyndicThreadId> {
        self.entries.iter().copied().collect()
    }

    pub(super) fn settle(&mut self, selected: Option<SyndicThreadId>) {
        let Some(pending) = self.pending.take() else {
            return;
        };
        let (prior, target) = match pending {
            PendingNavigation::Selection(prior, target) => (prior, target),
            PendingNavigation::Movement {
                prior,
                target,
                position,
            } => {
                if selected != Some(target) || prior == target {
                    return;
                }
                let mut position = position;
                if self
                    .cursor
                    .is_some_and(|cursor| self.entries.get(cursor) != Some(&prior))
                {
                    if let Some(cursor) = self.cursor {
                        self.entries.truncate(cursor + 1);
                    }
                    let expires = self.entries.len() == HISTORY_CAPACITY;
                    self.push(prior);
                    if expires {
                        position = position.saturating_sub(1);
                    }
                }
                self.cursor = Some(position);
                self.active = selected;
                return;
            }
        };
        if selected != Some(target) || prior == Some(target) {
            return;
        }
        if let Some(cursor) = self.cursor {
            self.entries.truncate(cursor + 1);
        }
        if let Some(prior) = prior {
            self.push(prior);
        }
        self.push(target);
        self.cursor = self.entries.len().checked_sub(1);
        self.active = selected;
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
