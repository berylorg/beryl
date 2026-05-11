use std::path::Path;

use super::*;

impl WorkspaceConversationState {
    pub fn selected_runtime(&self) -> Option<&RuntimeMode> {
        self.selected_runtime.as_ref()
    }

    pub fn explicit_members(&self) -> &[WorkspaceMember] {
        &self.explicit_members
    }

    pub fn primary_explicit_member_id(&self) -> Option<&WorkspaceMemberId> {
        self.primary_explicit_member().map(|member| member.id())
    }

    pub fn primary_explicit_member(&self) -> Option<&WorkspaceMember> {
        let Some(index) = self.primary_explicit_member_index() else {
            return None;
        };
        self.explicit_members.get(index)
    }

    pub fn primary_member(&self) -> Option<PrimaryWorkspaceMember<'_>> {
        let runtime = self.selected_runtime.as_ref()?;
        if let Some(member) = self.primary_explicit_member() {
            Some(PrimaryWorkspaceMember::Explicit(member))
        } else {
            Some(PrimaryWorkspaceMember::ImplicitHome(runtime))
        }
    }

    pub fn active_thread(&self) -> Option<&ConversationThreadId> {
        self.active_thread.as_ref()
    }

    pub fn active_thread_registration(&self) -> Option<&RegisteredConversationThread> {
        let active_thread = self.active_thread.as_ref()?;
        self.thread_registration(active_thread)
    }

    pub fn threads(&self) -> &[RegisteredConversationThread] {
        &self.threads
    }

    pub fn thread_registration(
        &self,
        thread_id: &ConversationThreadId,
    ) -> Option<&RegisteredConversationThread> {
        self.threads
            .iter()
            .find(|thread| thread.thread_id() == thread_id)
    }

    pub fn thread_token_usage_snapshot(
        &self,
        thread_id: &ConversationThreadId,
    ) -> Option<&ConversationThreadTokenUsageSnapshot> {
        self.thread_registration(thread_id)
            .and_then(RegisteredConversationThread::token_usage_snapshot)
    }

    pub fn select_runtime(
        &mut self,
        runtime: RuntimeMode,
    ) -> Result<bool, WorkspaceConversationStateError> {
        if !self.explicit_members.is_empty() && self.selected_runtime.as_ref() != Some(&runtime) {
            return Err(WorkspaceConversationStateError::RuntimeEnvironmentLocked);
        }
        if self.selected_runtime.as_ref() == Some(&runtime) {
            return Ok(false);
        }

        let previous_runtime = self.selected_runtime.clone();
        self.selected_runtime = Some(runtime);
        let mut changed = true;
        if let Some(previous_runtime) = previous_runtime {
            changed |= self.mark_threads_rebind_required_for_runtime_change(&previous_runtime);
        }
        Ok(changed)
    }

    pub fn clear_runtime(&mut self) -> Result<bool, WorkspaceConversationStateError> {
        if !self.explicit_members.is_empty() {
            return Err(WorkspaceConversationStateError::RuntimeEnvironmentLocked);
        }
        if self.selected_runtime.is_none() {
            return Ok(false);
        }

        self.selected_runtime = None;
        let _ = self.mark_threads_rebind_required_for_cleared_runtime();
        Ok(true)
    }

    pub fn set_primary_explicit_member(
        &mut self,
        member_id: &WorkspaceMemberId,
    ) -> Result<bool, WorkspaceConversationStateError> {
        if self
            .explicit_members
            .iter()
            .all(|member| member.id() != member_id)
        {
            return Err(WorkspaceConversationStateError::MissingWorkspaceMember {
                member_id: member_id.clone(),
            });
        }
        if self.primary_explicit_member_id() == Some(member_id) {
            return Ok(false);
        }

        self.primary_explicit_member_id = Some(member_id.clone());
        Ok(true)
    }

    pub fn detach_explicit_member(
        &mut self,
        member_id: &WorkspaceMemberId,
    ) -> Result<bool, WorkspaceConversationStateError> {
        let Some(index) = self
            .explicit_members
            .iter()
            .position(|member| member.id() == member_id)
        else {
            return Err(WorkspaceConversationStateError::MissingWorkspaceMember {
                member_id: member_id.clone(),
            });
        };

        self.explicit_members.remove(index);
        self.mark_threads_rebind_required_for_detached_member(member_id);
        if self.explicit_members.is_empty() {
            self.primary_explicit_member_id = None;
        } else if self.primary_explicit_member_id.as_ref() == Some(member_id) {
            self.primary_explicit_member_id = self
                .explicit_members
                .first()
                .map(|member| member.id().clone());
        }

        Ok(true)
    }

    pub fn designate_primary_execution_target(
        &mut self,
        execution_target: &WorkspaceId,
    ) -> Result<bool, WorkspaceConversationStateError> {
        let mut changed = self.attach_execution_target(execution_target)?;
        let existing_member_id = self
            .explicit_members
            .iter()
            .find(|member| member.canonical_path() == execution_target.canonical_path())
            .map(|member| member.id().clone())
            .expect("attached execution target must be present in explicit member list");
        changed |= self.set_primary_explicit_member(&existing_member_id)?;
        Ok(changed)
    }

    pub fn attach_execution_target(
        &mut self,
        execution_target: &WorkspaceId,
    ) -> Result<bool, WorkspaceConversationStateError> {
        let changed = self.select_runtime(execution_target.runtime_mode().clone())?;

        if self
            .explicit_members
            .iter()
            .any(|member| member.canonical_path() == execution_target.canonical_path())
        {
            return Ok(changed);
        }

        self.ensure_no_member_overlap(execution_target.canonical_path())?;
        let replaces_implicit_home =
            self.explicit_members.is_empty() && self.selected_runtime.is_some();
        let member_id = self.allocate_member_id();
        let should_promote_to_primary =
            self.explicit_members.is_empty() && self.primary_explicit_member_id.is_none();
        self.explicit_members.push(WorkspaceMember::new(
            member_id.clone(),
            execution_target.canonical_path().to_path_buf(),
        ));
        if should_promote_to_primary {
            self.primary_explicit_member_id = Some(member_id);
        }
        if replaces_implicit_home {
            self.mark_implicit_home_threads_rebind_required();
        }
        Ok(true)
    }

    pub fn clear_active_thread(&mut self) -> bool {
        if self.active_thread.is_none() {
            return false;
        }

        self.active_thread = None;
        true
    }

    pub fn remember_thread(&mut self, thread: RegisteredConversationThread) -> bool {
        let mut thread = thread;
        if thread.member_binding.is_none() {
            thread.member_binding = self.binding_for_execution_target(thread.execution_target());
        }

        if let Some(index) = self
            .threads
            .iter()
            .position(|candidate| candidate.thread_id() == thread.thread_id())
        {
            let existing = &self.threads[index];
            let existing_ignored_backend_name =
                existing.ignored_backend_name_for_automatic_title.clone();
            if existing
                .ignored_backend_name_for_automatic_title()
                .is_some_and(|ignored| thread.backend_name() == Some(ignored))
            {
                thread.backend_name = None;
            }
            if existing.gui_title.as_ref().is_some_and(|title| {
                title.source() != ConversationThreadTitleSource::BackendMetadata
            }) {
                thread.gui_title.clone_from(&existing.gui_title);
            }
            if thread.backend_name().is_none() && existing.backend_name().is_some() {
                thread.backend_name.clone_from(&existing.backend_name);
            }
            if existing.member_binding.is_some() {
                thread.member_binding.clone_from(&existing.member_binding);
            }
            if existing.rebind_required.is_some() {
                thread.rebind_required.clone_from(&existing.rebind_required);
            }
            if existing.token_usage_snapshot.is_some() {
                thread
                    .token_usage_snapshot
                    .clone_from(&existing.token_usage_snapshot);
            }
            if existing.beryl_created() {
                thread.mark_beryl_created();
            }
            if thread.backend_name().is_some() {
                thread.ignored_backend_name_for_automatic_title = None;
            } else if thread.ignored_backend_name_for_automatic_title().is_none() {
                thread.ignored_backend_name_for_automatic_title = existing_ignored_backend_name;
            }
            thread.automatic_title_generation_state = merged_title_generation_state(
                existing.automatic_title_generation_state,
                thread.automatic_title_generation_state,
            );
            if self.threads[index] == thread {
                return false;
            }
            self.threads[index] = thread;
        } else {
            self.threads.push(thread);
        }

        self.threads.sort_by(|left, right| {
            right
                .updated_at_millis()
                .cmp(&left.updated_at_millis())
                .then_with(|| right.created_at_millis().cmp(&left.created_at_millis()))
                .then_with(|| left.thread_id().as_str().cmp(right.thread_id().as_str()))
        });
        true
    }

    pub fn activate_thread(
        &mut self,
        thread_id: &ConversationThreadId,
    ) -> Option<&RegisteredConversationThread> {
        let active_thread_id = self.thread_registration(thread_id)?.thread_id().clone();
        self.active_thread = Some(active_thread_id);
        self.thread_registration(thread_id)
    }

    pub fn set_thread_generated_title_if_absent(
        &mut self,
        thread_id: &ConversationThreadId,
        title: impl Into<String>,
        recorded_at_millis: u64,
    ) -> Result<bool, WorkspaceConversationStateError> {
        self.thread_registration_mut(thread_id)
            .ok_or_else(|| WorkspaceConversationStateError::MissingThread {
                thread_id: thread_id.clone(),
            })?
            .set_generated_title_if_absent(title, recorded_at_millis)
    }

    pub fn thread_automatic_title_generation_eligible(
        &self,
        thread_id: &ConversationThreadId,
    ) -> bool {
        self.thread_registration(thread_id)
            .is_some_and(RegisteredConversationThread::automatic_title_generation_eligible)
    }

    pub fn mark_thread_beryl_created(
        &mut self,
        thread_id: &ConversationThreadId,
    ) -> Result<bool, WorkspaceConversationStateError> {
        let changed = self
            .thread_registration_mut(thread_id)
            .ok_or_else(|| WorkspaceConversationStateError::MissingThread {
                thread_id: thread_id.clone(),
            })?
            .mark_beryl_created();
        Ok(changed)
    }

    pub fn mark_thread_automatic_title_generation_started(
        &mut self,
        thread_id: &ConversationThreadId,
    ) -> Result<bool, WorkspaceConversationStateError> {
        let changed = self
            .thread_registration_mut(thread_id)
            .ok_or_else(|| WorkspaceConversationStateError::MissingThread {
                thread_id: thread_id.clone(),
            })?
            .mark_automatic_title_generation_started();
        Ok(changed)
    }

    pub fn mark_thread_automatic_title_generation_applied(
        &mut self,
        thread_id: &ConversationThreadId,
    ) -> Result<bool, WorkspaceConversationStateError> {
        let changed = self
            .thread_registration_mut(thread_id)
            .ok_or_else(|| WorkspaceConversationStateError::MissingThread {
                thread_id: thread_id.clone(),
            })?
            .mark_automatic_title_generation_applied();
        Ok(changed)
    }

    pub fn mark_thread_automatic_title_generation_abandoned(
        &mut self,
        thread_id: &ConversationThreadId,
    ) -> Result<bool, WorkspaceConversationStateError> {
        let changed = self
            .thread_registration_mut(thread_id)
            .ok_or_else(|| WorkspaceConversationStateError::MissingThread {
                thread_id: thread_id.clone(),
            })?
            .mark_automatic_title_generation_abandoned();
        Ok(changed)
    }

    pub fn set_thread_manual_title(
        &mut self,
        thread_id: &ConversationThreadId,
        title: impl Into<String>,
        recorded_at_millis: u64,
    ) -> Result<bool, WorkspaceConversationStateError> {
        self.thread_registration_mut(thread_id)
            .ok_or_else(|| WorkspaceConversationStateError::MissingThread {
                thread_id: thread_id.clone(),
            })?
            .set_manual_title(title, recorded_at_millis)
    }

    pub fn set_thread_backend_name(
        &mut self,
        thread_id: &ConversationThreadId,
        backend_name: Option<String>,
    ) -> Result<bool, WorkspaceConversationStateError> {
        let changed = self
            .thread_registration_mut(thread_id)
            .ok_or_else(|| WorkspaceConversationStateError::MissingThread {
                thread_id: thread_id.clone(),
            })?
            .set_backend_name(backend_name);
        Ok(changed)
    }

    pub fn set_authoritative_thread_backend_name(
        &mut self,
        thread_id: &ConversationThreadId,
        backend_name: Option<String>,
    ) -> Result<bool, WorkspaceConversationStateError> {
        let changed = self
            .thread_registration_mut(thread_id)
            .ok_or_else(|| WorkspaceConversationStateError::MissingThread {
                thread_id: thread_id.clone(),
            })?
            .set_authoritative_backend_name(backend_name);
        Ok(changed)
    }

    pub fn record_thread_token_usage_snapshot(
        &mut self,
        thread_id: &ConversationThreadId,
        snapshot: ConversationThreadTokenUsageSnapshot,
    ) -> Result<bool, WorkspaceConversationStateError> {
        let changed = self
            .thread_registration_mut(thread_id)
            .ok_or_else(|| WorkspaceConversationStateError::MissingThread {
                thread_id: thread_id.clone(),
            })?
            .record_token_usage_snapshot(snapshot);
        Ok(changed)
    }

    pub fn mark_thread_rebind_required(
        &mut self,
        thread_id: &ConversationThreadId,
        detail: impl Into<String>,
    ) -> Result<bool, WorkspaceConversationStateError> {
        let requirement = ConversationThreadRebindRequirement::new(detail)
            .ok_or(WorkspaceConversationStateError::EmptyRebindRequirement)?;
        let thread = self.thread_registration_mut(thread_id).ok_or_else(|| {
            WorkspaceConversationStateError::MissingThread {
                thread_id: thread_id.clone(),
            }
        })?;
        if thread.rebind_required.as_ref() == Some(&requirement) {
            return Ok(false);
        }

        thread.rebind_required = Some(requirement);
        Ok(true)
    }

    pub fn clear_thread_rebind_required(
        &mut self,
        thread_id: &ConversationThreadId,
    ) -> Result<bool, WorkspaceConversationStateError> {
        let thread = self.thread_registration_mut(thread_id).ok_or_else(|| {
            WorkspaceConversationStateError::MissingThread {
                thread_id: thread_id.clone(),
            }
        })?;
        if thread.rebind_required.is_none() {
            return Ok(false);
        }

        thread.rebind_required = None;
        Ok(true)
    }

    pub fn binding_for_execution_target(
        &self,
        execution_target: &WorkspaceId,
    ) -> Option<ConversationThreadMemberBinding> {
        if self.selected_runtime.as_ref()? != execution_target.runtime_mode() {
            return None;
        }

        if let Some(member) = self
            .explicit_members
            .iter()
            .find(|member| member.canonical_path() == execution_target.canonical_path())
        {
            return Some(ConversationThreadMemberBinding::explicit(
                member.id().clone(),
                execution_target.clone(),
            ));
        }

        if self.explicit_members.is_empty() {
            return Some(ConversationThreadMemberBinding::implicit_home(
                execution_target.clone(),
            ));
        }

        None
    }

    pub fn thread_registration_mut(
        &mut self,
        thread_id: &ConversationThreadId,
    ) -> Option<&mut RegisteredConversationThread> {
        self.threads
            .iter_mut()
            .find(|thread| thread.thread_id() == thread_id)
    }

    fn primary_explicit_member_index(&self) -> Option<usize> {
        if self.explicit_members.is_empty() {
            return None;
        }

        self.primary_explicit_member_id
            .as_ref()
            .and_then(|primary_id| {
                self.explicit_members
                    .iter()
                    .position(|member| member.id() == primary_id)
            })
            .or(Some(0))
    }

    fn ensure_no_member_overlap(
        &self,
        candidate_path: &Path,
    ) -> Result<(), WorkspaceConversationStateError> {
        for existing in &self.explicit_members {
            if paths_overlap(existing.canonical_path(), candidate_path) {
                return Err(WorkspaceConversationStateError::WorkspaceMemberOverlap {
                    existing_member_id: existing.id().clone(),
                    existing_path: existing.canonical_path().display().to_string(),
                    candidate_path: candidate_path.display().to_string(),
                });
            }
        }

        Ok(())
    }

    fn allocate_member_id(&mut self) -> WorkspaceMemberId {
        let mut next = self.next_member_number.max(1);
        loop {
            let candidate =
                WorkspaceMemberId::new(format!("member_{next}")).expect("generated ids are valid");
            next += 1;
            if self
                .explicit_members
                .iter()
                .all(|member| member.id() != &candidate)
            {
                self.next_member_number = next;
                return candidate;
            }
        }
    }

    fn mark_threads_rebind_required_for_detached_member(
        &mut self,
        member_id: &WorkspaceMemberId,
    ) -> bool {
        self.mark_threads_rebind_required_where(
            |binding| binding.explicit_member_id() == Some(member_id),
            "The workspace member originally bound to this thread was detached from the Beryl workspace.",
        )
    }

    fn mark_implicit_home_threads_rebind_required(&mut self) -> bool {
        self.mark_threads_rebind_required_where(
            ConversationThreadMemberBinding::is_implicit_home,
            "The implicit home member originally bound to this thread was replaced by explicit workspace members.",
        )
    }

    fn mark_threads_rebind_required_for_runtime_change(
        &mut self,
        previous_runtime: &RuntimeMode,
    ) -> bool {
        self.mark_threads_rebind_required_where(
            |binding| binding.runtime_mode() == previous_runtime,
            "The runtime environment originally bound to this thread is no longer selected for this Beryl workspace.",
        )
    }

    fn mark_threads_rebind_required_for_cleared_runtime(&mut self) -> bool {
        self.mark_threads_rebind_required_where(
            |_| true,
            "The workspace runtime environment was cleared, so the original thread execution context is unavailable.",
        )
    }

    fn mark_threads_rebind_required_where(
        &mut self,
        mut should_mark: impl FnMut(&ConversationThreadMemberBinding) -> bool,
        detail: &'static str,
    ) -> bool {
        let requirement = ConversationThreadRebindRequirement::new(detail)
            .expect("static rebind detail is valid");
        let mut changed = false;

        for thread in &mut self.threads {
            let should_mark_thread = thread.member_binding.as_ref().is_some_and(&mut should_mark);
            if should_mark_thread && thread.rebind_required.as_ref() != Some(&requirement) {
                thread.rebind_required = Some(requirement.clone());
                changed = true;
            }
        }

        changed
    }
}

fn paths_overlap(left: &Path, right: &Path) -> bool {
    left == right || left.starts_with(right) || right.starts_with(left)
}

fn merged_title_generation_state(
    existing: ThreadAutomaticTitleGenerationState,
    incoming: ThreadAutomaticTitleGenerationState,
) -> ThreadAutomaticTitleGenerationState {
    match (existing, incoming) {
        (ThreadAutomaticTitleGenerationState::Applied, _)
        | (_, ThreadAutomaticTitleGenerationState::Applied) => {
            ThreadAutomaticTitleGenerationState::Applied
        }
        (_, ThreadAutomaticTitleGenerationState::NotStarted) => existing,
        (_, incoming) => incoming,
    }
}
