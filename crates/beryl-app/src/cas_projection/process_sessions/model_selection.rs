use super::*;

#[cfg(all(test, feature = "test-faults"))]
#[path = "../../../tests/unit/thread_model_choice.rs"]
mod tests;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ThreadModelChoice {
    pub(crate) model: beryl_backend::ProtocolIdentity,
    pub(crate) reasoning: Option<beryl_backend::ReasoningEffort>,
}

#[derive(Clone)]
pub(crate) struct PendingThreadModelChoice {
    owner: WeakScheduledExecutionSessions,
    thread: SyndicThreadId,
    binding: ExecutionBinding,
    revision: u64,
    pub(crate) choice: ThreadModelChoice,
}

impl std::fmt::Debug for PendingThreadModelChoice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PendingThreadModelChoice")
            .field("revision", &self.revision)
            .finish_non_exhaustive()
    }
}

impl PartialEq for PendingThreadModelChoice {
    fn eq(&self, other: &Self) -> bool {
        Weak::ptr_eq(&self.owner.state, &other.owner.state)
            && self.thread == other.thread
            && self.binding == other.binding
            && self.revision == other.revision
            && self.choice == other.choice
    }
}

impl Eq for PendingThreadModelChoice {}

impl PendingThreadModelChoice {
    pub(crate) fn revision(&self) -> u64 {
        self.revision
    }

    pub(in crate::cas_projection) fn accepted(&self) {
        let Some(owner) = self.owner.upgrade() else {
            return;
        };
        let mut state = owner.lock();
        if state
            .model_choices
            .get(&self.thread)
            .is_some_and(|current| {
                current.revision == self.revision && current.binding == self.binding
            })
        {
            state.model_choices.remove(&self.thread);
        }
    }
}

#[derive(Clone)]
pub(super) struct StoredThreadModelChoice {
    binding: ExecutionBinding,
    revision: u64,
    choice: ThreadModelChoice,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ThreadModelChoiceError {
    Unavailable,
    Active,
    Capacity,
    Exhausted,
}

impl ScheduledExecutionSessions {
    pub(crate) fn with_current_model_choice_elected<T>(
        &self,
        thread: SyndicThreadId,
        binding: &ExecutionBinding,
        revision: Option<u64>,
        epoch: u64,
        publish: impl FnOnce() -> T,
    ) -> Result<T, ThreadModelChoiceError> {
        let state = self
            .state
            .try_lock()
            .map_err(|_| ThreadModelChoiceError::Unavailable)?;
        if state.closed || state.context.is_none() || state.model_choice_revision != epoch {
            return Err(ThreadModelChoiceError::Unavailable);
        }
        let current = state.model_choices.get(&thread);
        if current.is_some_and(|current| &current.binding != binding)
            || current.map(|current| current.revision) != revision
        {
            return Err(ThreadModelChoiceError::Unavailable);
        }
        Ok(publish())
    }

    pub(crate) fn observed_model_metadata(
        &self,
        thread: SyndicThreadId,
        binding: &ExecutionBinding,
    ) -> Option<beryl_backend::ThreadSessionMetadata> {
        let state = self.lock();
        if state.closed || !state.context.as_ref()?.commands.is_open() {
            return None;
        }
        let slot = state.slots.get(&thread)?;
        if slot.retiring || &slot.binding != binding {
            return None;
        }
        slot.connection.observed_model_metadata(thread)
    }
    pub(crate) fn select_next_turn_model_elected(
        &self,
        thread: SyndicThreadId,
        binding: &ExecutionBinding,
        choice: ThreadModelChoice,
    ) -> Result<(), ThreadModelChoiceError> {
        let mut state = self
            .state
            .try_lock()
            .map_err(|_| ThreadModelChoiceError::Unavailable)?;
        let context = state
            .context
            .as_ref()
            .ok_or(ThreadModelChoiceError::Unavailable)?;
        if state.closed {
            return Err(ThreadModelChoiceError::Unavailable);
        }
        let capacity = context.capacity;
        if state
            .slots
            .get(&thread)
            .is_some_and(|slot| slot.checked_out || slot.retiring || &slot.binding != binding)
            || state.preparing.contains_key(&thread)
            || state.recovering.contains_key(&thread)
        {
            return Err(ThreadModelChoiceError::Active);
        }
        if !state.model_choices.contains_key(&thread) && state.model_choices.len() >= capacity {
            return Err(ThreadModelChoiceError::Capacity);
        }
        let revision = state
            .model_choice_revision
            .checked_add(1)
            .ok_or(ThreadModelChoiceError::Exhausted)?;
        state.model_choice_revision = revision;
        state.model_choices.insert(
            thread,
            StoredThreadModelChoice {
                binding: binding.clone(),
                revision,
                choice,
            },
        );
        Ok(())
    }

    pub(crate) fn pending_model_choice(
        &self,
        thread: SyndicThreadId,
        binding: &ExecutionBinding,
    ) -> Option<PendingThreadModelChoice> {
        self.model_choice_snapshot(thread, binding)?.0
    }

    pub(crate) fn model_choice_snapshot(
        &self,
        thread: SyndicThreadId,
        binding: &ExecutionBinding,
    ) -> Option<(Option<PendingThreadModelChoice>, u64)> {
        let state = self.lock();
        if state.closed || !state.context.as_ref()?.commands.is_open() {
            return None;
        }
        let Some(stored) = state.model_choices.get(&thread) else {
            return Some((None, state.model_choice_revision));
        };
        (&stored.binding == binding).then(|| {
            (
                Some(PendingThreadModelChoice {
                    owner: WeakScheduledExecutionSessions {
                        state: Arc::downgrade(&self.state),
                        work_identity: Arc::downgrade(&self.work_identity),
                    },
                    thread,
                    binding: binding.clone(),
                    revision: stored.revision,
                    choice: stored.choice.clone(),
                }),
                state.model_choice_revision,
            )
        })
    }
}

pub(crate) fn reasoning_wire(effort: beryl_backend::ReasoningEffort) -> &'static str {
    use beryl_backend::ReasoningEffort::*;
    match effort {
        None => "none",
        Minimal => "minimal",
        Low => "low",
        Medium => "medium",
        High => "high",
        XHigh => "xhigh",
        Max => "max",
        Ultra => "ultra",
    }
}
