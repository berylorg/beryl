use super::*;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SubmissionUnavailable {
    RepairRequired,
    BindingUnavailable,
    RuntimeUnavailable,
    AuthorityUnavailable,
}

impl SubmissionUnavailable {
    pub const fn reason(self) -> &'static str {
        match self {
            Self::RepairRequired => "The selected turn requires repair before submission.",
            Self::BindingUnavailable => "The selected execution binding is unavailable.",
            Self::RuntimeUnavailable => "The selected runtime is unavailable.",
            Self::AuthorityUnavailable => "Submission authority is unavailable.",
        }
    }
}

impl ExactStopWorker {
    #[cfg(feature = "test-faults")]
    pub fn test_with_submission_authority_contended<T>(&self, callback: impl FnOnce() -> T) -> T {
        self.command_authorizer.test_with_gate_lock(callback)
    }

    pub fn selected_submission_availability(
        &self,
        thread: SyndicThreadId,
    ) -> Result<(), SubmissionUnavailable> {
        self.selected_submission_proof(thread).map(|_| ())
    }

    pub(crate) fn selected_submission_proof(
        &self,
        thread: SyndicThreadId,
    ) -> Result<SubmissionProof, SubmissionUnavailable> {
        let _command = self
            .command_authorizer
            .authorize()
            .map_err(|_| SubmissionUnavailable::AuthorityUnavailable)?;
        self.read()
            .ok_or(SubmissionUnavailable::AuthorityUnavailable)?
            .selected_submission_proof(thread, &self.runtime_failure)
            .map(|binding| SubmissionProof {
                commands: self.command_authorizer.clone(),
                runtime: self.runtime_failure.clone(),
                publication: None,
                gui_readiness: false,
                binding,
            })
    }
}

impl ExactStopRead {
    fn selected_submission_proof(
        &self,
        thread: SyndicThreadId,
        runtime: &super::super::RuntimeFailureReader,
    ) -> Result<SubmissionBindingProof, SubmissionUnavailable> {
        use SubmissionUnavailable as Denial;
        let read = || -> Option<Result<SubmissionBindingProof, Denial>> {
            let home = self.home.as_deref()?;
            let revision = home.home_revision().ok()?;
            let limit = SyndicPointReadLimit::new(1_000_000).ok()?;
            let record = self.storage.thread(home, thread, limit).ok()??;
            let gate = self.storage.input_gate(home, thread, limit).ok()??;
            let execution = self.storage.thread_execution(home, thread, limit).ok()??;
            let binding = self.storage.current_binding(home, thread, limit).ok()?;
            let availability = if matches!(
                gate.state(),
                syndic_storage::InputGateState::RepairRequired(_)
            ) {
                Err(Denial::RepairRequired)
            } else {
                let valid = binding
                    .as_ref()
                    .and_then(|binding| match binding.binding().state() {
                        syndic_storage::BindingState::Valid(valid) => Some(valid),
                        syndic_storage::BindingState::Active(active) => Some(active.usable()),
                        _ => None,
                    });
                if let Some(valid) = valid {
                    if valid.execution() != execution.execution() {
                        return Some(Err(Denial::BindingUnavailable));
                    }
                    let connections = self.connections.lock().ok()?;
                    let mut exact = None;
                    for connection in connections.iter() {
                        if connection.runtime_id() != valid.execution().runtime_id() {
                            continue;
                        }
                        if let Some(stamp) = connection.manual_projection(
                            thread,
                            valid.cas_thread_id(),
                            binding.as_ref()?.binding().revision(),
                        ) {
                            if exact.is_some() || !connection.manual_projection_current(&stamp) {
                                return Some(Err(Denial::BindingUnavailable));
                            }
                            exact = Some((connection, stamp));
                        }
                    }
                    exact
                        .filter(|(connection, stamp)| connection.manual_projection_current(stamp))
                        .map(|(connection, stamp)| {
                            SubmissionBindingProof::Loaded(
                                Arc::downgrade(connection),
                                stamp,
                                execution.execution().clone(),
                            )
                        })
                        .ok_or(Denial::BindingUnavailable)
                } else if (record.committed_tail().is_none()
                    || self.initial_context_draft(home, &record, limit))
                    && binding.as_ref().is_none_or(|binding| {
                        matches!(
                            binding.binding().state(),
                            syndic_storage::BindingState::Unbound { .. }
                        )
                    })
                    && matches!(gate.state(), syndic_storage::InputGateState::Idle)
                {
                    runtime
                        .initial_submission_preparation_open(execution.execution())
                        .then(|| SubmissionBindingProof::Initial(execution.execution().clone()))
                        .ok_or(Denial::RuntimeUnavailable)
                } else {
                    Err(Denial::BindingUnavailable)
                }
            };
            if home.home_revision().ok()? != revision || !self.command_authorizer.is_open() {
                return None;
            }
            Some(availability)
        };
        read().unwrap_or(Err(Denial::AuthorityUnavailable))
    }

    fn initial_context_draft(
        &self,
        home: &beryl_home_store::HomeStore,
        record: &syndic_storage::ThreadRecord,
        limit: SyndicPointReadLimit,
    ) -> bool {
        let owner = beryl_model::DiscussionContextOwnerId::Draft(record.current_draft_id());
        if record.context_owner_id() != Some(owner) {
            return false;
        }
        self.storage
            .current_draft(home, record.id(), limit)
            .ok()
            .flatten()
            .is_some_and(|current| {
                current.thread() == record
                    && current.draft().submission_intent()
                        == syndic_storage::DraftSubmissionIntent::DiscussionContext(owner)
            })
    }
}

#[derive(Clone)]
pub struct SubmissionProof {
    commands: LiveCommandAuthorizer,
    runtime: super::super::RuntimeFailureReader,
    binding: SubmissionBindingProof,
    publication: Option<Weak<()>>,
    gui_readiness: bool,
}

#[derive(Clone)]
enum SubmissionBindingProof {
    Initial(ExecutionBinding),
    Loaded(
        Weak<ProjectionConnection>,
        crate::cas_projection::connection::registry::ManualProjectionStamp,
        ExecutionBinding,
    ),
}

impl SubmissionProof {
    pub(crate) fn requiring_gui_readiness(mut self) -> Result<Self, SubmissionUnavailable> {
        self.gui_readiness = true;
        self.availability()?;
        Ok(self)
    }

    pub(crate) fn with_publication(mut self, publication: Weak<()>) -> Self {
        self.publication = Some(publication);
        self
    }

    pub(crate) fn availability(&self) -> Result<(), SubmissionUnavailable> {
        if !self.commands.try_is_open()
            || self
                .publication
                .as_ref()
                .is_some_and(|publication| publication.strong_count() == 0)
        {
            return Err(SubmissionUnavailable::AuthorityUnavailable);
        }
        match &self.binding {
            SubmissionBindingProof::Initial(binding) => (if self.gui_readiness {
                self.runtime.gui_submission_ready(binding)
            } else {
                self.runtime.initial_submission_preparation_open(binding)
            })
            .then_some(())
            .ok_or(SubmissionUnavailable::RuntimeUnavailable),
            SubmissionBindingProof::Loaded(connection, stamp, execution) => {
                (if self.gui_readiness && !self.runtime.gui_submission_ready(execution) {
                    return Err(SubmissionUnavailable::RuntimeUnavailable);
                } else {
                    connection
                        .upgrade()
                        .is_some_and(|connection| connection.manual_projection_current(stamp))
                })
                .then_some(())
                .ok_or(SubmissionUnavailable::BindingUnavailable)
            }
        }
    }

    pub(crate) fn elect_local(&self) -> bool {
        self.availability().is_ok()
    }
}
