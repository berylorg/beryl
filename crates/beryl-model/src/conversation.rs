mod state;
mod thread_metadata;
mod token_usage;

use std::path::PathBuf;
use std::{error::Error, fmt};

use serde::{Deserialize, Serialize};

use crate::workspace::{
    RuntimeMode, WorkspaceId, WorkspaceMember, WorkspaceMemberAvailability, WorkspaceMemberId,
};

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
pub struct ConversationTurnId(String);

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RegisteredConversationThread {
    thread_id: ConversationThreadId,
    execution_target: WorkspaceId,
    preview: String,
    #[serde(default, alias = "title")]
    backend_name: Option<String>,
    #[serde(default)]
    ignored_backend_name_for_automatic_title: Option<String>,
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
pub enum ThreadAutomaticTitleGenerationState {
    #[default]
    NotStarted,
    InFlight,
    Abandoned,
    Applied,
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

#[derive(Deserialize)]
struct WorkspaceConversationStateWire {
    #[serde(default, alias = "selected_runtime")]
    default_runtime: Option<RuntimeMode>,
    #[serde(default)]
    explicit_members: Vec<WorkspaceMemberWire>,
    #[serde(default)]
    primary_explicit_member_id: Option<WorkspaceMemberId>,
    #[serde(default)]
    next_member_number: u64,
    #[serde(default)]
    threads: Vec<RegisteredConversationThread>,
    #[serde(default)]
    active_thread: Option<ConversationThreadId>,
}

#[derive(Deserialize)]
struct WorkspaceMemberWire {
    id: WorkspaceMemberId,
    #[serde(default)]
    runtime_mode: Option<RuntimeMode>,
    canonical_path: PathBuf,
    #[serde(default)]
    availability: Option<WorkspaceMemberAvailability>,
    #[serde(default)]
    available: Option<bool>,
}

impl ConversationThreadId {
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

impl<'de> Deserialize<'de> for WorkspaceConversationState {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let wire = WorkspaceConversationStateWire::deserialize(deserializer)?;
        let default_runtime = wire.default_runtime;
        let explicit_members = wire
            .explicit_members
            .into_iter()
            .map(|member| {
                let runtime_mode = member
                    .runtime_mode
                    .or_else(|| default_runtime.clone())
                    .ok_or_else(|| {
                        serde::de::Error::custom(format!(
                            "workspace member {} at {} is missing a runtime mode",
                            member.id.as_str(),
                            member.canonical_path.display()
                        ))
                    })?;
                let availability = member.availability.unwrap_or_else(|| {
                    if member.available == Some(false) {
                        WorkspaceMemberAvailability::PathNotFound
                    } else {
                        WorkspaceMemberAvailability::Available
                    }
                });

                Ok(WorkspaceMember::new_with_availability(
                    member.id,
                    runtime_mode,
                    member.canonical_path,
                    availability,
                ))
            })
            .collect::<Result<Vec<_>, D::Error>>()?;

        let mut state = Self {
            default_runtime,
            explicit_members,
            primary_explicit_member_id: wire.primary_explicit_member_id,
            next_member_number: wire.next_member_number,
            threads: wire.threads,
            active_thread: wire.active_thread,
        };
        state.normalize_unavailable_primary_after_deserialize();
        Ok(state)
    }
}

impl WorkspaceConversationState {
    fn normalize_unavailable_primary_after_deserialize(&mut self) {
        let Some(primary_id) = self.primary_explicit_member_id.as_ref() else {
            return;
        };
        let primary_available = self
            .explicit_members
            .iter()
            .any(|member| member.id() == primary_id && member.is_available());
        if primary_available {
            return;
        }

        self.primary_explicit_member_id = self
            .explicit_members
            .iter()
            .find(|member| member.is_available())
            .map(|member| member.id().clone());
    }
}

impl RegisteredConversationThread {
    pub fn new(
        thread_id: ConversationThreadId,
        execution_target: WorkspaceId,
        preview: impl Into<String>,
        backend_name: Option<String>,
        created_at_millis: i64,
        updated_at_millis: i64,
    ) -> Self {
        Self {
            thread_id,
            execution_target,
            preview: preview.into(),
            backend_name: normalize_optional_title(backend_name),
            ignored_backend_name_for_automatic_title: None,
            gui_title: None,
            member_binding: None,
            rebind_required: None,
            token_usage_snapshot: None,
            beryl_created: false,
            automatic_title_generation_state: ThreadAutomaticTitleGenerationState::NotStarted,
            created_at_millis,
            updated_at_millis,
        }
    }

    pub fn thread_id(&self) -> &ConversationThreadId {
        &self.thread_id
    }

    pub fn execution_target(&self) -> &WorkspaceId {
        &self.execution_target
    }

    pub fn preview(&self) -> &str {
        &self.preview
    }

    pub fn backend_name(&self) -> Option<&str> {
        non_empty_trimmed(self.backend_name.as_deref())
    }

    pub fn title(&self) -> Option<&str> {
        self.manual_title()
            .or_else(|| self.backend_name())
            .or_else(|| self.generated_title())
            .or_else(|| self.legacy_backend_metadata_title())
    }

    pub fn title_with_backend_name<'a>(&'a self, backend_name: Option<&'a str>) -> Option<&'a str> {
        let backend_name = self.unsuppressed_backend_name(backend_name);
        self.manual_title()
            .or(backend_name)
            .or_else(|| self.backend_name())
            .or_else(|| self.generated_title())
            .or_else(|| self.legacy_backend_metadata_title())
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

    pub fn ignored_backend_name_for_automatic_title(&self) -> Option<&str> {
        non_empty_trimmed(self.ignored_backend_name_for_automatic_title.as_deref())
    }

    pub fn ignores_backend_name_for_automatic_title(&self, backend_name: Option<&str>) -> bool {
        self.ignored_backend_name_for_automatic_title()
            .is_some_and(|ignored| non_empty_trimmed(backend_name) == Some(ignored))
    }

    pub fn with_ignored_backend_name_for_automatic_title(
        mut self,
        backend_name: Option<String>,
    ) -> Self {
        self.ignored_backend_name_for_automatic_title = normalize_optional_title(backend_name);
        self
    }

    pub fn with_beryl_created(mut self) -> Self {
        self.beryl_created = true;
        self.apply_existing_backend_name_to_title_generation_state();
        self
    }

    pub fn with_member_binding(mut self, binding: ConversationThreadMemberBinding) -> Self {
        self.member_binding = Some(binding);
        self
    }

    pub fn mark_beryl_created(&mut self) -> bool {
        if self.beryl_created {
            return false;
        }

        self.beryl_created = true;
        self.apply_existing_backend_name_to_title_generation_state();
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
        if self.automatic_title_generation_state == ThreadAutomaticTitleGenerationState::Applied
            && self.ignored_backend_name_for_automatic_title.is_none()
        {
            return false;
        }

        self.automatic_title_generation_state = ThreadAutomaticTitleGenerationState::Applied;
        self.ignored_backend_name_for_automatic_title = None;
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

    pub fn set_backend_name(&mut self, backend_name: Option<String>) -> bool {
        self.set_backend_name_from_source(backend_name, false)
    }

    pub fn set_authoritative_backend_name(&mut self, backend_name: Option<String>) -> bool {
        self.set_backend_name_from_source(backend_name, true)
    }

    fn set_backend_name_from_source(
        &mut self,
        backend_name: Option<String>,
        authoritative: bool,
    ) -> bool {
        let mut backend_name = normalize_optional_title(backend_name);
        if !authoritative && self.ignores_backend_name_for_automatic_title(backend_name.as_deref())
        {
            backend_name = None;
        }
        let backend_name_changed = self.backend_name != backend_name;
        if backend_name_changed {
            self.backend_name = backend_name;
        }

        let title_generation_state_changed = if self.backend_name().is_some() {
            self.mark_automatic_title_generation_applied()
        } else {
            false
        };
        if !backend_name_changed && !title_generation_state_changed {
            return false;
        }
        if self
            .gui_title
            .as_ref()
            .is_some_and(|title| title.source() == ConversationThreadTitleSource::BackendMetadata)
        {
            self.gui_title = None;
        }
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
        self.ignored_backend_name_for_automatic_title = None;
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
        self.ignored_backend_name_for_automatic_title = None;
        Ok(true)
    }

    fn manual_title(&self) -> Option<&str> {
        self.title_from_gui_source(ConversationThreadTitleSource::Manual)
    }

    fn generated_title(&self) -> Option<&str> {
        self.title_from_gui_source(ConversationThreadTitleSource::FirstCompletedTurn)
    }

    fn legacy_backend_metadata_title(&self) -> Option<&str> {
        self.title_from_gui_source(ConversationThreadTitleSource::BackendMetadata)
    }

    fn title_from_gui_source(&self, source: ConversationThreadTitleSource) -> Option<&str> {
        let title = self.gui_title.as_ref()?;
        (title.source() == source).then_some(title.text())
    }

    fn apply_existing_backend_name_to_title_generation_state(&mut self) {
        if self.backend_name().is_some() {
            self.mark_automatic_title_generation_applied();
        }
    }

    fn unsuppressed_backend_name<'a>(&self, backend_name: Option<&'a str>) -> Option<&'a str> {
        if self.ignores_backend_name_for_automatic_title(backend_name) {
            None
        } else {
            non_empty_trimmed(backend_name)
        }
    }
}

impl fmt::Display for WorkspaceConversationStateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::RuntimeEnvironmentLocked => write!(
                f,
                "the workspace runtime environment cannot change while explicit workspace members are attached"
            ),
            Self::RuntimeEnvironmentNotSelected => {
                write!(
                    f,
                    "select a workspace runtime environment before attaching members"
                )
            }
            Self::MissingWorkspaceMember { member_id } => {
                write!(f, "workspace member {} is not attached", member_id.as_str())
            }
            Self::UnavailableWorkspaceMember { member_id } => {
                write!(
                    f,
                    "workspace member {} is unavailable and cannot be primary",
                    member_id.as_str()
                )
            }
            Self::MissingThread { thread_id } => {
                write!(
                    f,
                    "conversation thread {} is not registered",
                    thread_id.as_str()
                )
            }
            Self::EmptyThreadTitle => write!(f, "conversation thread title must not be empty"),
            Self::EmptyRebindRequirement => write!(f, "thread rebind detail must not be empty"),
            Self::WorkspaceMemberOverlap {
                existing_member_id,
                existing_path,
                candidate_path,
            } => write!(
                f,
                "workspace member {candidate_path} overlaps attached member {} at {existing_path}",
                existing_member_id.as_str()
            ),
        }
    }
}

impl Error for WorkspaceConversationStateError {}

fn normalize_optional_title(value: Option<String>) -> Option<String> {
    value.and_then(|value| {
        let value = value.trim().to_string();
        (!value.is_empty()).then_some(value)
    })
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

fn non_empty_trimmed(value: Option<&str>) -> Option<&str> {
    let value = value?.trim();
    (!value.is_empty()).then_some(value)
}
