use std::collections::HashMap;

use gpui_settings_window::{
    SettingsFieldId, SettingsFieldKind, SettingsRow, SettingsSection, SettingsSectionId,
};

use crate::{
    ContextCompactionTimeoutError, OperationPreferences,
    parse_context_compaction_timeout_seconds_text,
};

const OPERATIONS_SECTION: &str = "operations";
const CONTEXT_COMPACTION_TIMEOUT_FIELD: &str = "operations.context_compaction_timeout_seconds";

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct OperationSettingsDraft {
    context_compaction_timeout_seconds: String,
}

impl OperationSettingsDraft {
    pub(crate) fn from_preferences(preferences: &OperationPreferences) -> Self {
        Self {
            context_compaction_timeout_seconds: preferences
                .context_compaction_timeout_seconds
                .to_string(),
        }
    }

    pub(crate) fn set_field_value(&mut self, field_id: &SettingsFieldId, value: String) -> bool {
        if *field_id != context_compaction_timeout_field_id() {
            return false;
        }
        self.context_compaction_timeout_seconds = value;
        true
    }

    pub(crate) fn context_compaction_timeout_seconds_value(&self) -> &str {
        &self.context_compaction_timeout_seconds
    }

    pub(crate) fn to_preferences(
        &self,
    ) -> Result<OperationPreferences, HashMap<SettingsFieldId, String>> {
        match parse_context_compaction_timeout_seconds_text(
            &self.context_compaction_timeout_seconds,
        ) {
            Ok(context_compaction_timeout_seconds) => Ok(OperationPreferences {
                context_compaction_timeout_seconds,
            }),
            Err(error) => {
                let mut errors = HashMap::new();
                errors.insert(
                    context_compaction_timeout_field_id(),
                    context_compaction_timeout_error(error),
                );
                Err(errors)
            }
        }
    }
}

pub(crate) fn settings_section(
    draft: &OperationSettingsDraft,
    errors: &HashMap<SettingsFieldId, String>,
) -> SettingsSection {
    let field_id = context_compaction_timeout_field_id();
    let row = SettingsRow::new(
        field_id.clone(),
        "Context compaction timeout",
        draft.context_compaction_timeout_seconds_value(),
        SettingsFieldKind::Text,
    )
    .with_subtext("Seconds Beryl waits for backend-reported compaction completion.");

    SettingsSection::new(operation_section_id(), "Operations").with_row(
        match errors.get(&field_id) {
            Some(error) => row.with_error(error.clone()),
            None => row,
        },
    )
}

pub(crate) fn operation_section_id() -> SettingsSectionId {
    SettingsSectionId::from(OPERATIONS_SECTION)
}

pub(crate) fn has_section_id(section_id: &SettingsSectionId) -> bool {
    *section_id == operation_section_id()
}

pub(crate) fn context_compaction_timeout_field_id() -> SettingsFieldId {
    SettingsFieldId::from(CONTEXT_COMPACTION_TIMEOUT_FIELD)
}

pub(crate) fn context_compaction_timeout_error(error: ContextCompactionTimeoutError) -> String {
    match error {
        ContextCompactionTimeoutError::NotInteger => {
            "Context compaction timeout must be a whole number of seconds.".to_string()
        }
        ContextCompactionTimeoutError::TooSmall { min } => {
            format!("Context compaction timeout must be at least {min} second.")
        }
        ContextCompactionTimeoutError::TooLarge { max } => {
            format!("Context compaction timeout must be at most {max} seconds.")
        }
    }
}
