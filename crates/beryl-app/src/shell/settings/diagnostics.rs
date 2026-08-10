use std::collections::HashMap;

use gpui_settings_window::{
    SettingsChoiceOption, SettingsFieldId, SettingsFieldKind, SettingsRow, SettingsSection,
    SettingsSectionId,
};

use crate::{
    ActivityDiagnosticCaptureErrorCategory, ActivityDiagnosticCaptureRuntimeState,
    ActivityDiagnosticCaptureStatus, DiagnosticPreferences,
};

const DIAGNOSTICS_SECTION: &str = "diagnostics";
const ACTIVITY_DIAGNOSTIC_CAPTURE_FIELD: &str = "diagnostics.activity_diagnostic_capture";
const DISABLED_VALUE: &str = "disabled";
const ENABLED_VALUE: &str = "enabled";

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct DiagnosticsSettingsDraft {
    activity_diagnostic_capture: String,
}

impl DiagnosticsSettingsDraft {
    pub(crate) fn from_preferences(preferences: &DiagnosticPreferences) -> Self {
        Self {
            activity_diagnostic_capture: if preferences.activity_diagnostic_capture_enabled {
                ENABLED_VALUE.to_string()
            } else {
                DISABLED_VALUE.to_string()
            },
        }
    }

    pub(crate) fn set_field_value(&mut self, field_id: &SettingsFieldId, value: String) -> bool {
        if *field_id != activity_diagnostic_capture_field_id() {
            return false;
        }
        self.activity_diagnostic_capture = value;
        true
    }

    pub(crate) fn to_preferences(
        &self,
    ) -> Result<DiagnosticPreferences, HashMap<SettingsFieldId, String>> {
        let activity_diagnostic_capture_enabled = match self.activity_diagnostic_capture.as_str() {
            DISABLED_VALUE => false,
            ENABLED_VALUE => true,
            _ => {
                let mut errors = HashMap::new();
                errors.insert(
                    activity_diagnostic_capture_field_id(),
                    "Activity diagnostic capture must be Disabled or Enabled.".to_string(),
                );
                return Err(errors);
            }
        };
        Ok(DiagnosticPreferences {
            activity_diagnostic_capture_enabled,
        })
    }
}

pub(crate) fn settings_section(
    draft: &DiagnosticsSettingsDraft,
    status: &ActivityDiagnosticCaptureStatus,
    errors: &HashMap<SettingsFieldId, String>,
) -> SettingsSection {
    let field_id = activity_diagnostic_capture_field_id();
    let row = SettingsRow::new(
        field_id.clone(),
        "Activity diagnostic capture",
        &draft.activity_diagnostic_capture,
        SettingsFieldKind::Choice,
    )
    .with_choice(SettingsChoiceOption::new(DISABLED_VALUE, "Disabled"))
    .with_choice(SettingsChoiceOption::new(ENABLED_VALUE, "Enabled"))
    .with_subtext("Captures bounded content-free Activity diagnostic evidence.")
    .with_error(activity_diagnostic_capture_status_message(status));

    SettingsSection::new(diagnostics_section_id(), "Diagnostics").with_row(
        match errors.get(&field_id) {
            Some(error) => row.with_error(error.clone()),
            None => row,
        },
    )
}

pub(crate) fn diagnostics_section_id() -> SettingsSectionId {
    SettingsSectionId::from(DIAGNOSTICS_SECTION)
}

pub(crate) fn has_section_id(section_id: &SettingsSectionId) -> bool {
    *section_id == diagnostics_section_id()
}

pub(crate) fn activity_diagnostic_capture_field_id() -> SettingsFieldId {
    SettingsFieldId::from(ACTIVITY_DIAGNOSTIC_CAPTURE_FIELD)
}

fn activity_diagnostic_capture_status_message(status: &ActivityDiagnosticCaptureStatus) -> String {
    let configured = if status.configured {
        "Enabled"
    } else {
        "Disabled"
    };
    let runtime = match status.runtime_state {
        ActivityDiagnosticCaptureRuntimeState::Disabled => "disabled",
        ActivityDiagnosticCaptureRuntimeState::Starting => "starting",
        ActivityDiagnosticCaptureRuntimeState::Active => "active",
        ActivityDiagnosticCaptureRuntimeState::Stopping => "stopping",
        ActivityDiagnosticCaptureRuntimeState::Unavailable => "unavailable",
        ActivityDiagnosticCaptureRuntimeState::Failed => "failed",
    };
    let error = status
        .error_category
        .map(activity_diagnostic_capture_error_category)
        .map(|category| format!(" Last error: {category}."))
        .unwrap_or_default();

    format!(
        "Configured: {configured}. Runtime: {runtime}. Written: {}. Dropped: {} (queue full: {}, disconnected: {}, rejected: {}). Oversized: {}. Repairs: {}. Rotations: {}.{error}",
        status.written_record_count,
        status.dropped_record_count,
        status.queue_full_drop_count,
        status.queue_disconnected_drop_count,
        status.schema_rejection_drop_count,
        status.oversized_record_count,
        status.repair_count,
        status.rotation_count,
    )
}

fn activity_diagnostic_capture_error_category(
    category: ActivityDiagnosticCaptureErrorCategory,
) -> &'static str {
    match category {
        ActivityDiagnosticCaptureErrorCategory::LockUnavailable => "lock unavailable",
        ActivityDiagnosticCaptureErrorCategory::Directory => "capture directory failure",
        ActivityDiagnosticCaptureErrorCategory::Lock => "capture lock failure",
        ActivityDiagnosticCaptureErrorCategory::Recovery => "capture recovery failure",
        ActivityDiagnosticCaptureErrorCategory::Rotation => "capture rotation failure",
        ActivityDiagnosticCaptureErrorCategory::Serialization => "capture serialization failure",
        ActivityDiagnosticCaptureErrorCategory::Write => "capture write failure",
        ActivityDiagnosticCaptureErrorCategory::WriterDisconnected => "capture writer unavailable",
    }
}
