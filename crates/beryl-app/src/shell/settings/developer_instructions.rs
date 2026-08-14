use std::collections::HashMap;

use gpui_settings_window::{
    SettingsFieldId, SettingsFieldKind, SettingsRow, SettingsSection, SettingsSectionId,
};

use crate::{AgentPreferences, normalize_developer_instructions_text};

const AGENT_SECTION: &str = "agent";
const DEVELOPER_INSTRUCTIONS_FIELD: &str = "agent.developer_instructions";

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct AgentSettingsDraft {
    developer_instructions: String,
}

impl AgentSettingsDraft {
    pub(crate) fn from_preferences(preferences: &AgentPreferences) -> Self {
        Self {
            developer_instructions: preferences
                .developer_instructions()
                .map(str::to_string)
                .unwrap_or_default(),
        }
    }

    pub(crate) fn set_field_value(&mut self, field_id: &SettingsFieldId, value: String) -> bool {
        if *field_id != developer_instructions_field_id() {
            return false;
        }
        self.developer_instructions = value;
        true
    }

    #[allow(dead_code)]
    pub(crate) fn set_developer_instructions(&mut self, value: String) {
        self.developer_instructions = value;
    }

    #[allow(dead_code)]
    pub(crate) fn developer_instructions_value(&self) -> &str {
        &self.developer_instructions
    }

    pub(crate) fn to_preferences(
        &self,
    ) -> Result<AgentPreferences, HashMap<SettingsFieldId, String>> {
        Ok(AgentPreferences {
            developer_instructions: normalize_developer_instructions_text(
                &self.developer_instructions,
            ),
        })
    }
}

pub(crate) fn settings_section(
    draft: &AgentSettingsDraft,
    errors: &HashMap<SettingsFieldId, String>,
) -> SettingsSection {
    let field_id = developer_instructions_field_id();
    let row = SettingsRow::new(
        field_id.clone(),
        "Developer Instructions",
        draft.developer_instructions_value(),
        SettingsFieldKind::MultilineText,
    )
    .with_subtext("Sent as developer instructions with every user message.");

    SettingsSection::new(agent_section_id(), "Agent").with_row(match errors.get(&field_id) {
        Some(error) => row.with_error(error.clone()),
        None => row,
    })
}

pub(crate) fn agent_section_id() -> SettingsSectionId {
    SettingsSectionId::from(AGENT_SECTION)
}

pub(crate) fn has_section_id(section_id: &SettingsSectionId) -> bool {
    *section_id == agent_section_id()
}

pub(crate) fn developer_instructions_field_id() -> SettingsFieldId {
    SettingsFieldId::from(DEVELOPER_INSTRUCTIONS_FIELD)
}
