use std::collections::HashMap;

use beryl_model::workspace::BerylWorkspaceId;
use gpui_settings_window::{
    SettingsFieldId, SettingsFieldKind, SettingsRow, SettingsSection, SettingsSectionId,
};

use crate::{WorkspaceGraphUpkeepPolicy, normalize_graph_upkeep_instructions_text};

const GRAPH_SECTION: &str = "graph";
const GRAPH_UPKEEP_INSTRUCTIONS_FIELD: &str = "graph.graph_upkeep_instructions";
const NO_WORKSPACE_MESSAGE: &str = "Select a workspace before editing graph-upkeep instructions.";
const PERSISTENCE_UNAVAILABLE_MESSAGE: &str =
    "Workspace settings storage is unavailable for graph-upkeep instructions.";

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum GraphSettingsTarget {
    Unavailable {
        reason: String,
    },
    Workspace {
        workspace_id: BerylWorkspaceId,
        policy: WorkspaceGraphUpkeepPolicy,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct GraphSettingsDraft {
    target: GraphSettingsTarget,
    instructions: String,
}

impl Default for GraphSettingsTarget {
    fn default() -> Self {
        Self::Unavailable {
            reason: NO_WORKSPACE_MESSAGE.to_string(),
        }
    }
}

impl GraphSettingsTarget {
    pub(crate) fn no_workspace() -> Self {
        Self::Unavailable {
            reason: NO_WORKSPACE_MESSAGE.to_string(),
        }
    }

    pub(crate) fn persistence_unavailable() -> Self {
        Self::Unavailable {
            reason: PERSISTENCE_UNAVAILABLE_MESSAGE.to_string(),
        }
    }

    pub(crate) fn workspace(
        workspace_id: BerylWorkspaceId,
        policy: WorkspaceGraphUpkeepPolicy,
    ) -> Self {
        Self::Workspace {
            workspace_id,
            policy,
        }
    }

    fn active_policy(&self) -> Option<&WorkspaceGraphUpkeepPolicy> {
        match self {
            Self::Workspace { policy, .. } => Some(policy),
            Self::Unavailable { .. } => None,
        }
    }

    fn workspace_id(&self) -> Option<&BerylWorkspaceId> {
        match self {
            Self::Workspace { workspace_id, .. } => Some(workspace_id),
            Self::Unavailable { .. } => None,
        }
    }

    fn unavailable_reason(&self) -> Option<&str> {
        match self {
            Self::Unavailable { reason } => Some(reason.as_str()),
            Self::Workspace { .. } => None,
        }
    }
}

impl GraphSettingsDraft {
    pub(crate) fn from_target(target: GraphSettingsTarget) -> Self {
        let instructions = target
            .active_policy()
            .and_then(WorkspaceGraphUpkeepPolicy::instructions)
            .map(str::to_string)
            .unwrap_or_default();
        Self {
            target,
            instructions,
        }
    }

    pub(crate) fn set_target(&mut self, target: GraphSettingsTarget) {
        *self = Self::from_target(target);
    }

    pub(crate) fn rebind_workspace_id(
        &mut self,
        old_workspace_id: &BerylWorkspaceId,
        new_workspace_id: BerylWorkspaceId,
    ) -> bool {
        match &mut self.target {
            GraphSettingsTarget::Workspace { workspace_id, .. }
                if workspace_id == old_workspace_id =>
            {
                *workspace_id = new_workspace_id;
                true
            }
            GraphSettingsTarget::Workspace { .. } | GraphSettingsTarget::Unavailable { .. } => {
                false
            }
        }
    }

    pub(crate) fn set_field_value(&mut self, field_id: &SettingsFieldId, value: String) -> bool {
        if *field_id != graph_upkeep_instructions_field_id()
            || self.target.active_policy().is_none()
        {
            return false;
        }
        self.instructions = value;
        true
    }

    #[allow(dead_code)]
    pub(crate) fn set_instructions(&mut self, value: String) {
        if self.target.active_policy().is_some() {
            self.instructions = value;
        }
    }

    #[allow(dead_code)]
    pub(crate) fn instructions_value(&self) -> &str {
        &self.instructions
    }

    #[allow(dead_code)]
    pub(crate) fn active_policy_snapshot(&self) -> Option<WorkspaceGraphUpkeepPolicy> {
        self.target.active_policy().cloned()
    }

    pub(crate) fn active_policy_snapshot_for_workspace(
        &self,
        workspace_id: &BerylWorkspaceId,
    ) -> Option<WorkspaceGraphUpkeepPolicy> {
        match &self.target {
            GraphSettingsTarget::Workspace {
                workspace_id: target_workspace_id,
                policy,
            } if target_workspace_id == workspace_id => Some(policy.clone()),
            GraphSettingsTarget::Workspace { .. } | GraphSettingsTarget::Unavailable { .. } => None,
        }
    }

    pub(crate) fn target_workspace_id(&self) -> Option<BerylWorkspaceId> {
        self.target.workspace_id().cloned()
    }

    pub(crate) fn pending_policy(
        &self,
    ) -> Result<
        Option<(BerylWorkspaceId, WorkspaceGraphUpkeepPolicy)>,
        HashMap<SettingsFieldId, String>,
    > {
        if !self.is_modified() {
            return Ok(None);
        }

        let Some(workspace_id) = self.target.workspace_id().cloned() else {
            let mut errors = HashMap::new();
            errors.insert(
                graph_upkeep_instructions_field_id(),
                self.target
                    .unavailable_reason()
                    .unwrap_or(PERSISTENCE_UNAVAILABLE_MESSAGE)
                    .to_string(),
            );
            return Err(errors);
        };

        Ok(Some((workspace_id, self.policy_from_draft())))
    }

    pub(crate) fn record_saved_policy(
        &mut self,
        workspace_id: &BerylWorkspaceId,
        saved_policy: WorkspaceGraphUpkeepPolicy,
    ) -> bool {
        let current_policy = self.policy_from_draft();
        let Some(active_policy) = self.target_policy_mut(workspace_id) else {
            return false;
        };
        *active_policy = saved_policy.clone();
        if current_policy == saved_policy {
            self.instructions = saved_policy
                .instructions()
                .map(str::to_string)
                .unwrap_or_default();
        }
        true
    }

    pub(crate) fn is_modified(&self) -> bool {
        self.target
            .active_policy()
            .is_some_and(|policy| self.policy_from_draft() != *policy)
    }

    fn policy_from_draft(&self) -> WorkspaceGraphUpkeepPolicy {
        WorkspaceGraphUpkeepPolicy::with_instructions(normalize_graph_upkeep_instructions_text(
            &self.instructions,
        ))
    }

    fn target_policy_mut(
        &mut self,
        workspace_id: &BerylWorkspaceId,
    ) -> Option<&mut WorkspaceGraphUpkeepPolicy> {
        match &mut self.target {
            GraphSettingsTarget::Workspace {
                workspace_id: target_workspace_id,
                policy,
            } if target_workspace_id == workspace_id => Some(policy),
            GraphSettingsTarget::Workspace { .. } | GraphSettingsTarget::Unavailable { .. } => None,
        }
    }
}

pub(crate) fn settings_section(
    draft: &GraphSettingsDraft,
    errors: &HashMap<SettingsFieldId, String>,
) -> SettingsSection {
    let field_id = graph_upkeep_instructions_field_id();
    let subtext = draft
        .target
        .unavailable_reason()
        .unwrap_or(
            "Workspace-scoped. Guides AI graph upkeep without overriding graph invariants or tool schemas.",
        );
    let row = SettingsRow::new(
        field_id.clone(),
        "Graph Upkeep Instructions",
        draft.instructions_value(),
        SettingsFieldKind::MultilineText,
    )
    .with_subtext(subtext)
    .with_modified(draft.is_modified());

    let row = match errors
        .get(&field_id)
        .map(String::as_str)
        .or_else(|| draft.target.unavailable_reason())
    {
        Some(error) => row.with_error(error.to_string()),
        None => row,
    };

    SettingsSection::new(graph_section_id(), "Graph").with_row(row)
}

pub(crate) fn graph_section_id() -> SettingsSectionId {
    SettingsSectionId::from(GRAPH_SECTION)
}

pub(crate) fn has_section_id(section_id: &SettingsSectionId) -> bool {
    *section_id == graph_section_id()
}

pub(crate) fn graph_upkeep_instructions_field_id() -> SettingsFieldId {
    SettingsFieldId::from(GRAPH_UPKEEP_INSTRUCTIONS_FIELD)
}
