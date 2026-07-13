mod error;
mod state;
mod thread_metadata;
mod token_usage;
mod wire;

use serde::{Deserialize, Serialize};

use crate::workspace::{RuntimeMode, WorkspaceId, WorkspaceMember, WorkspaceMemberId};

pub use thread_metadata::{
    ConversationThreadMemberBinding, ConversationThreadRebindRequirement, ConversationThreadTitle,
    ConversationThreadTitleSource,
};
pub use token_usage::{ConversationThreadTokenUsageSnapshot, ConversationTokenUsageBreakdown};

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ConversationThreadId(String);

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SyndicConversationId(String);

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SyndicConversationViewId(String);

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ConversationTurnId(String);

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RegisteredConversationThread {
    thread_id: ConversationThreadId,
    #[serde(default)]
    syndic_conversation_id: Option<SyndicConversationId>,
    #[serde(default)]
    syndic_view_id: Option<SyndicConversationViewId>,
    execution_target: WorkspaceId,
    preview: String,
    #[serde(default)]
    gui_title: Option<ConversationThreadTitle>,
    #[serde(default)]
    member_binding: Option<ConversationThreadMemberBinding>,
    #[serde(default)]
    rebind_required: Option<ConversationThreadRebindRequirement>,
    #[serde(default)]
    token_usage_snapshot: Option<ConversationThreadTokenUsageSnapshot>,
    #[serde(default)]
    beryl_created: bool,
    #[serde(default)]
    branch_parent_thread_id: Option<ConversationThreadId>,
    #[serde(default)]
    branch_source_turn_id: Option<ConversationTurnId>,
    #[serde(default)]
    branch_bootstrap_turn_id: Option<ConversationTurnId>,
    #[serde(default)]
    branch_title_retitle_state: BranchThreadTitleRetitleState,
    #[serde(default)]
    catalog_status: ConversationCatalogStatus,
    #[serde(
        default,
        rename = "automatic_title_generation_state",
        alias = "automatic_title_generation_attempted",
        deserialize_with = "deserialize_thread_automatic_title_generation_state"
    )]
    automatic_title_generation_state: ThreadAutomaticTitleGenerationState,
    created_at_millis: i64,
    updated_at_millis: i64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConversationCatalogStatus {
    #[default]
    Visible,
    Archived,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ThreadAutomaticTitleGenerationState {
    #[default]
    NotStarted,
    InFlight,
    Abandoned,
    Applied,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BranchThreadTitleRetitleState {
    #[default]
    NotBranch,
    AwaitingFirstRealUserTurn,
    RetitleInFlight,
    Finished,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WorkspaceConversationStateError {
    RuntimeEnvironmentLocked,
    RuntimeEnvironmentNotSelected,
    MissingWorkspaceMember {
        member_id: WorkspaceMemberId,
    },
    UnavailableWorkspaceMember {
        member_id: WorkspaceMemberId,
    },
    MissingThread {
        thread_id: ConversationThreadId,
    },
    EmptyThreadTitle,
    EmptyRebindRequirement,
    WorkspaceMemberOverlap {
        existing_member_id: WorkspaceMemberId,
        existing_path: String,
        candidate_path: String,
    },
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct WorkspaceConversationState {
    #[serde(default, alias = "selected_runtime")]
    default_runtime: Option<RuntimeMode>,
    #[serde(default)]
    explicit_members: Vec<WorkspaceMember>,
    #[serde(default)]
    primary_explicit_member_id: Option<WorkspaceMemberId>,
    #[serde(default)]
    next_member_number: u64,
    #[serde(default)]
    threads: Vec<RegisteredConversationThread>,
    #[serde(default)]
    active_thread: Option<ConversationThreadId>,
}

#[derive(Debug)]
pub enum PrimaryWorkspaceMember<'a> {
    Explicit(&'a WorkspaceMember),
    ImplicitHome(&'a RuntimeMode),
}

impl ConversationThreadId {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl SyndicConversationId {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl SyndicConversationViewId {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl ConversationTurnId {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl RegisteredConversationThread {
    pub fn new(
        thread_id: ConversationThreadId,
        execution_target: WorkspaceId,
        preview: impl Into<String>,
        created_at_millis: i64,
        updated_at_millis: i64,
    ) -> Self {
        Self {
            thread_id,
            syndic_conversation_id: None,
            syndic_view_id: None,
            execution_target,
            preview: preview.into(),
            gui_title: None,
            member_binding: None,
            rebind_required: None,
            token_usage_snapshot: None,
            beryl_created: false,
            branch_parent_thread_id: None,
            branch_source_turn_id: None,
            branch_bootstrap_turn_id: None,
            branch_title_retitle_state: BranchThreadTitleRetitleState::NotBranch,
            catalog_status: ConversationCatalogStatus::Visible,
            automatic_title_generation_state: ThreadAutomaticTitleGenerationState::NotStarted,
            created_at_millis,
            updated_at_millis,
        }
    }

    pub fn thread_id(&self) -> &ConversationThreadId {
        &self.thread_id
    }

    pub fn syndic_conversation_id(&self) -> Option<&SyndicConversationId> {
        self.syndic_conversation_id.as_ref()
    }

    pub fn syndic_view_id(&self) -> Option<&SyndicConversationViewId> {
        self.syndic_view_id.as_ref()
    }

    pub fn has_syndic_view_registration(&self) -> bool {
        self.syndic_conversation_id.is_some() && self.syndic_view_id.is_some()
    }

    pub fn catalog_status(&self) -> ConversationCatalogStatus {
        self.catalog_status
    }

    pub fn visible_in_catalog(&self) -> bool {
        self.has_syndic_view_registration()
            && matches!(self.catalog_status, ConversationCatalogStatus::Visible)
    }

    pub fn execution_target(&self) -> &WorkspaceId {
        &self.execution_target
    }

    pub fn preview(&self) -> &str {
        &self.preview
    }

    pub fn title(&self) -> Option<&str> {
        self.manual_title().or_else(|| self.generated_title())
    }

    pub fn gui_title(&self) -> Option<&ConversationThreadTitle> {
        self.gui_title.as_ref()
    }

    pub fn member_binding(&self) -> Option<&ConversationThreadMemberBinding> {
        self.member_binding.as_ref()
    }

    pub fn rebind_required(&self) -> Option<&ConversationThreadRebindRequirement> {
        self.rebind_required.as_ref()
    }

    pub fn token_usage_snapshot(&self) -> Option<&ConversationThreadTokenUsageSnapshot> {
        self.token_usage_snapshot.as_ref()
    }

    pub fn requires_rebind(&self) -> bool {
        self.rebind_required.is_some()
    }

    pub fn created_at_millis(&self) -> i64 {
        self.created_at_millis
    }

    pub fn updated_at_millis(&self) -> i64 {
        self.updated_at_millis
    }

    pub fn beryl_created(&self) -> bool {
        self.beryl_created
    }

    pub fn branch_parent_thread_id(&self) -> Option<&ConversationThreadId> {
        self.branch_parent_thread_id.as_ref()
    }

    pub fn branch_source_turn_id(&self) -> Option<&ConversationTurnId> {
        self.branch_source_turn_id.as_ref()
    }

    pub fn branch_bootstrap_turn_id(&self) -> Option<&ConversationTurnId> {
        self.branch_bootstrap_turn_id.as_ref()
    }

    pub fn branch_title_retitle_state(&self) -> BranchThreadTitleRetitleState {
        self.branch_title_retitle_state
    }

    pub fn branch_title_retitle_pending(&self) -> bool {
        self.branch_title_retitle_state == BranchThreadTitleRetitleState::AwaitingFirstRealUserTurn
    }

    pub fn automatic_title_generation_attempted(&self) -> bool {
        self.automatic_title_generation_state != ThreadAutomaticTitleGenerationState::NotStarted
    }

    pub fn automatic_title_generation_state(&self) -> ThreadAutomaticTitleGenerationState {
        self.automatic_title_generation_state
    }

    pub fn automatic_title_generation_eligible(&self) -> bool {
        self.beryl_created
            && matches!(
                self.automatic_title_generation_state,
                ThreadAutomaticTitleGenerationState::NotStarted
                    | ThreadAutomaticTitleGenerationState::Abandoned
            )
            && self.title().is_none()
    }

    pub fn with_syndic_view_registration(
        mut self,
        conversation_id: SyndicConversationId,
        view_id: SyndicConversationViewId,
    ) -> Self {
        self.syndic_conversation_id = Some(conversation_id);
        self.syndic_view_id = Some(view_id);
        self
    }

    pub fn with_beryl_created(mut self) -> Self {
        self.beryl_created = true;
        self
    }

    pub fn with_branch_parent_thread_id(mut self, parent_thread_id: ConversationThreadId) -> Self {
        self.branch_parent_thread_id = Some(parent_thread_id);
        self
    }

    pub fn with_transcript_branch_bootstrap(
        mut self,
        source_turn_id: ConversationTurnId,
        bootstrap_turn_id: Option<ConversationTurnId>,
    ) -> Self {
        self.branch_source_turn_id = Some(source_turn_id);
        self.branch_bootstrap_turn_id = bootstrap_turn_id;
        self.branch_title_retitle_state = BranchThreadTitleRetitleState::AwaitingFirstRealUserTurn;
        self
    }

    pub fn with_member_binding(mut self, binding: ConversationThreadMemberBinding) -> Self {
        self.member_binding = Some(binding);
        self
    }

    pub fn set_syndic_view_registration(
        &mut self,
        conversation_id: SyndicConversationId,
        view_id: SyndicConversationViewId,
    ) -> bool {
        let changed = self.syndic_conversation_id.as_ref() != Some(&conversation_id)
            || self.syndic_view_id.as_ref() != Some(&view_id);
        if changed {
            self.syndic_conversation_id = Some(conversation_id);
            self.syndic_view_id = Some(view_id);
        }
        changed
    }

    pub fn set_catalog_status(&mut self, status: ConversationCatalogStatus) -> bool {
        if self.catalog_status == status {
            return false;
        }

        self.catalog_status = status;
        true
    }

    pub fn mark_beryl_created(&mut self) -> bool {
        if self.beryl_created {
            return false;
        }

        self.beryl_created = true;
        true
    }

    pub fn mark_automatic_title_generation_started(&mut self) -> bool {
        if !matches!(
            self.automatic_title_generation_state,
            ThreadAutomaticTitleGenerationState::NotStarted
                | ThreadAutomaticTitleGenerationState::Abandoned
        ) {
            return false;
        }

        self.automatic_title_generation_state = ThreadAutomaticTitleGenerationState::InFlight;
        true
    }

    pub fn mark_automatic_title_generation_abandoned(&mut self) -> bool {
        if self.automatic_title_generation_state != ThreadAutomaticTitleGenerationState::InFlight {
            return false;
        }

        self.automatic_title_generation_state = ThreadAutomaticTitleGenerationState::Abandoned;
        true
    }

    pub fn mark_automatic_title_generation_applied(&mut self) -> bool {
        if self.automatic_title_generation_state == ThreadAutomaticTitleGenerationState::Applied {
            return false;
        }

        self.automatic_title_generation_state = ThreadAutomaticTitleGenerationState::Applied;
        true
    }

    pub fn mark_branch_title_retitle_started(&mut self) -> bool {
        if self.branch_title_retitle_state
            != BranchThreadTitleRetitleState::AwaitingFirstRealUserTurn
        {
            return false;
        }

        self.branch_title_retitle_state = BranchThreadTitleRetitleState::RetitleInFlight;
        true
    }

    pub fn mark_branch_title_retitle_finished(&mut self) -> bool {
        if matches!(
            self.branch_title_retitle_state,
            BranchThreadTitleRetitleState::NotBranch | BranchThreadTitleRetitleState::Finished
        ) {
            return false;
        }

        self.branch_title_retitle_state = BranchThreadTitleRetitleState::Finished;
        true
    }

    pub fn record_token_usage_snapshot(
        &mut self,
        snapshot: ConversationThreadTokenUsageSnapshot,
    ) -> bool {
        if self.token_usage_snapshot.as_ref() == Some(&snapshot) {
            return false;
        }

        self.token_usage_snapshot = Some(snapshot);
        true
    }

    pub fn set_manual_title(
        &mut self,
        title: impl Into<String>,
        recorded_at_millis: u64,
    ) -> Result<bool, WorkspaceConversationStateError> {
        let title = ConversationThreadTitle::new(
            title,
            ConversationThreadTitleSource::Manual,
            recorded_at_millis,
        )
        .ok_or(WorkspaceConversationStateError::EmptyThreadTitle)?;
        if self.gui_title.as_ref() == Some(&title) {
            return Ok(false);
        }

        self.gui_title = Some(title);
        Ok(true)
    }

    pub fn set_generated_title_if_absent(
        &mut self,
        title: impl Into<String>,
        recorded_at_millis: u64,
    ) -> Result<bool, WorkspaceConversationStateError> {
        if self.title().is_some() {
            return Ok(false);
        }

        self.gui_title = Some(
            ConversationThreadTitle::new(
                title,
                ConversationThreadTitleSource::FirstCompletedTurn,
                recorded_at_millis,
            )
            .ok_or(WorkspaceConversationStateError::EmptyThreadTitle)?,
        );
        Ok(true)
    }

    pub fn set_generated_title(
        &mut self,
        title: impl Into<String>,
        recorded_at_millis: u64,
    ) -> Result<bool, WorkspaceConversationStateError> {
        if self.manual_title().is_some() {
            return Ok(false);
        }

        let title = ConversationThreadTitle::new(
            title,
            ConversationThreadTitleSource::FirstCompletedTurn,
            recorded_at_millis,
        )
        .ok_or(WorkspaceConversationStateError::EmptyThreadTitle)?;
        if self.gui_title.as_ref() == Some(&title) {
            return Ok(false);
        }

        self.gui_title = Some(title);
        Ok(true)
    }

    fn manual_title(&self) -> Option<&str> {
        self.title_from_gui_source(ConversationThreadTitleSource::Manual)
    }

    fn generated_title(&self) -> Option<&str> {
        self.title_from_gui_source(ConversationThreadTitleSource::FirstCompletedTurn)
    }

    fn title_from_gui_source(&self, source: ConversationThreadTitleSource) -> Option<&str> {
        let title = self.gui_title.as_ref()?;
        (title.source() == source).then_some(title.text())
    }
}

fn deserialize_thread_automatic_title_generation_state<'de, D>(
    deserializer: D,
) -> Result<ThreadAutomaticTitleGenerationState, D::Error>
where
    D: serde::Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum WireState {
        State(ThreadAutomaticTitleGenerationState),
        LegacyAttempted(bool),
    }

    match WireState::deserialize(deserializer)? {
        WireState::State(ThreadAutomaticTitleGenerationState::InFlight) => {
            Ok(ThreadAutomaticTitleGenerationState::Abandoned)
        }
        WireState::State(state) => Ok(state),
        WireState::LegacyAttempted(false) => Ok(ThreadAutomaticTitleGenerationState::NotStarted),
        WireState::LegacyAttempted(true) => Ok(ThreadAutomaticTitleGenerationState::Abandoned),
    }
}
