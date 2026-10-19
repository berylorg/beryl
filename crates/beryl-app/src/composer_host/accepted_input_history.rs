use std::collections::VecDeque;
use std::num::NonZeroUsize;
use std::sync::{Arc, Mutex};

use beryl_model::{BerylHomeId, SyndicThreadId, WindowId};
use syndic_storage::ComposerAcceptedInputReferenceV1;

pub struct ComposerAcceptedInputHistoryOwner {
    state: Mutex<State>,
    entries_per_thread: usize,
    inactive_scopes: usize,
}

#[derive(Default)]
struct State {
    closed: bool,
    window: Option<WindowId>,
    active: Option<(BerylHomeId, SyndicThreadId)>,
    scopes: VecDeque<Scope>,
}

struct Scope {
    home: BerylHomeId,
    thread: SyndicThreadId,
    entries: VecDeque<ComposerAcceptedInputReferenceV1>,
}

impl Default for ComposerAcceptedInputHistoryOwner {
    fn default() -> Self {
        Self::new(
            NonZeroUsize::new(64).unwrap(),
            NonZeroUsize::new(64).unwrap(),
        )
    }
}

impl ComposerAcceptedInputHistoryOwner {
    pub fn new(entries_per_thread: NonZeroUsize, inactive_scopes: NonZeroUsize) -> Self {
        Self {
            state: Mutex::new(State::default()),
            entries_per_thread: entries_per_thread.get(),
            inactive_scopes: inactive_scopes.get(),
        }
    }

    pub(crate) fn bind_window(&self, window: WindowId) -> Result<(), String> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| "Composer history owner is unavailable")?;
        if state.closed {
            return Err("Composer history window is closed".into());
        }
        if state.window.is_some_and(|current| current != window) {
            return Err("Composer history belongs to another window".into());
        }
        state.window = Some(window);
        Ok(())
    }

    pub(crate) fn select(&self, home: BerylHomeId, thread: SyndicThreadId) {
        let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        if state.closed {
            return;
        }
        state.active = Some((home, thread));
        if let Some(index) = state
            .scopes
            .iter()
            .position(|scope| scope.home == home && scope.thread == thread)
        {
            let scope = state.scopes.remove(index).unwrap();
            state.scopes.push_back(scope);
        }
        self.evict_inactive_scopes(&mut state);
    }

    pub(crate) fn accepted(&self, home: BerylHomeId, source: ComposerAcceptedInputReferenceV1) {
        let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        if state.closed {
            return;
        }
        let thread = source.thread_id();
        let index = state
            .scopes
            .iter()
            .position(|scope| scope.home == home && scope.thread == thread);
        let mut scope = index
            .and_then(|index| state.scopes.remove(index))
            .unwrap_or_else(|| Scope {
                home,
                thread,
                entries: VecDeque::with_capacity(self.entries_per_thread),
            });
        let delivered = scope.entries.iter().any(|entry| {
            entry.source_draft_id() == source.source_draft_id()
                && entry.session_id() == source.session_id()
                && entry.operation_id() == source.operation_id()
        });
        if !delivered
            && !scope
                .entries
                .back()
                .is_some_and(|entry| entry.same_content(&source))
        {
            if scope.entries.len() == self.entries_per_thread {
                scope.entries.pop_front();
            }
            scope.entries.push_back(source);
        }
        state.scopes.push_back(scope);
        self.evict_inactive_scopes(&mut state);
    }

    fn evict_inactive_scopes(&self, state: &mut State) {
        let active_present = state
            .scopes
            .iter()
            .any(|scope| state.active == Some((scope.home, scope.thread)));
        while state.scopes.len() > self.inactive_scopes + usize::from(active_present) {
            let eligible = state
                .scopes
                .iter()
                .position(|scope| state.active != Some((scope.home, scope.thread)));
            if let Some(index) = eligible {
                state.scopes.remove(index);
            } else {
                break;
            }
        }
    }

    pub fn entry(
        &self,
        home: BerylHomeId,
        thread: SyndicThreadId,
        index: usize,
    ) -> Option<ComposerAcceptedInputReferenceV1> {
        let state = self.state.lock().ok()?;
        state
            .scopes
            .iter()
            .find(|scope| scope.home == home && scope.thread == thread)?
            .entries
            .get(index)
            .cloned()
    }

    pub fn entry_count(&self, home: BerylHomeId, thread: SyndicThreadId) -> usize {
        let state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        state
            .scopes
            .iter()
            .find(|scope| scope.home == home && scope.thread == thread)
            .map_or(0, |scope| scope.entries.len())
    }

    pub fn scope_count(&self) -> usize {
        self.state
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .scopes
            .len()
    }

    pub fn close(&self) {
        let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        state.closed = true;
        state.active = None;
        state.scopes.clear();
    }
}

impl super::SyndicComposerHost {
    pub fn bind_accepted_input_history(&mut self, owner: Arc<ComposerAcceptedInputHistoryOwner>) {
        self.accepted_input_history = Some(owner);
    }
}
