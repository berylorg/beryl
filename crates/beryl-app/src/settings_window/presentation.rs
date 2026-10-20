use beryl_state::SettingKey;
use gpui_settings_window::{
    SettingsActionAvailability, SettingsFieldAvailability, SettingsFieldValidity,
    SettingsFooterCommand, SettingsInteractionGate, SettingsPage, SettingsRow, SettingsRowAction,
    SettingsSection, SettingsWindowModel,
};

use crate::settings_service::{SettingsDraft, SettingsIssue};

use super::fields::{self, Route};

pub(super) fn appearance_notice() -> SettingsRow {
    SettingsRow::action_only(
        "settings.appearance.failure",
        "The saved appearance could not be applied",
        SettingsRowAction::new("appearance.retry", "Retry"),
    )
    .with_subtext(
        "The saved settings remain committed. Retry applies the current durable appearance.",
    )
}

pub(super) fn issue_text(issue: SettingsIssue) -> String {
    match issue {
        SettingsIssue::InvalidValue => "Enter a valid value for this setting.".into(),
        SettingsIssue::TooLong { maximum, actual } => {
            format!("The value contains {actual} UTF-8 bytes; at most {maximum} bytes can be saved.")
        }
        SettingsIssue::RejectedSaved => {
            "The saved value was rejected. The feature default remains active.".into()
        }
        SettingsIssue::Excluded | SettingsIssue::Unresolved => {
            "The original update could not be proven. This setting is unavailable for this process."
                .into()
        }
        SettingsIssue::CommandRefused => {
            "The complete settings update was not committed. Your draft is preserved.".into()
        }
        SettingsIssue::Stale => {
            "The settings source changed. Discard this draft and reopen Settings to load current values."
                .into()
        }
    }
}

fn row(
    draft: Option<&SettingsDraft>,
    key: SettingKey,
    source_current: bool,
    feedback: Option<&str>,
) -> SettingsRow {
    let (label, kind, cap) = fields::metadata(key);
    let baseline = draft.map(|draft| draft.baseline().row(key));
    let available = source_current && baseline.is_some_and(|row| row.is_available());
    let modified = draft.is_some_and(|draft| draft.is_modified(key));
    let issue = draft
        .and_then(|draft| {
            draft
                .issues()
                .into_iter()
                .find_map(|(candidate, issue)| (key == candidate).then_some(issue))
        })
        .or_else(|| baseline.and_then(|row| row.issue));
    let mut row = SettingsRow::new(
        key.stable_id(),
        label,
        draft.map(|draft| draft.value(key)).unwrap_or(""),
        kind,
    )
    .with_value_byte_limit(Some(cap))
    .with_modified(modified)
    .with_validity(if issue.is_some() {
        SettingsFieldValidity::Invalid
    } else {
        SettingsFieldValidity::Valid
    });
    if !available {
        row = row.with_availability(SettingsFieldAvailability::unavailable(if source_current {
            "This setting has no coherent active value."
        } else {
            "This draft belongs to an unavailable settings source."
        }));
    } else if key == SettingKey::ActiveThemeId {
        row = row.with_availability(SettingsFieldAvailability::unavailable(
            "Select an installed theme and use Activate to stage its identity.",
        ));
    }
    if let Some(feedback) = feedback {
        row = row.with_error(feedback);
    } else if let Some(issue) = issue {
        row = row.with_error(issue_text(issue));
    }
    if modified {
        let reset = SettingsRowAction::new("reset", "Reset");
        row = row.with_action(if available {
            reset
        } else {
            reset.disabled_with_reason("The active value is unavailable.")
        });
    }
    row
}

pub(super) fn model(
    draft: Option<&SettingsDraft>,
    route: Route,
    source_current: bool,
    gate: Option<&str>,
    feedback: &[Option<String>; 5],
    themes: &super::themes::ThemesState,
    window: u64,
    appearance_failed: bool,
    theme_qualified: bool,
    theme_feedback: Option<&str>,
) -> SettingsWindowModel {
    let scalar = |key| {
        let index = fields::KEYS
            .iter()
            .position(|candidate| *candidate == key)
            .unwrap();
        let mut row = row(draft, key, source_current, feedback[index].as_deref());
        if key == SettingKey::ActiveThemeId {
            if let Some(reason) = theme_feedback {
                row = row
                    .with_error(reason)
                    .with_action(SettingsRowAction::new("theme.validate-selection", "Retry"));
            }
            if draft.is_some_and(|draft| draft.value(key).is_empty()) {
                row = row
                    .with_subtext("The durable active theme uses the read-only built-in fallback.");
            }
        }
        row
    };
    let themes = themes.section(
        scalar(SettingKey::ActiveThemeId),
        draft,
        window,
        source_current,
        appearance_failed,
    );
    let notice = |section: SettingsSection| {
        if appearance_failed {
            section.with_row(appearance_notice())
        } else {
            section
        }
    };
    let mut model = SettingsWindowModel::new(vec![
        themes,
        notice(
            SettingsSection::new("operations", "Operations")
                .with_row(scalar(SettingKey::ContextCompactionTimeout))
                .with_row(scalar(SettingKey::DraftAutosaveInterval)),
        ),
        notice(
            SettingsSection::new("notifications", "Notifications")
                .with_row(scalar(SettingKey::EndTurnSound)),
        ),
        notice(
            SettingsSection::new("agent", "Agent")
                .with_row(scalar(SettingKey::DeveloperInstructions)),
        ),
    ])
    .expect("finite settings schema fits the external window model");
    model
        .select_page(route.id())
        .expect("finite settings route");
    let modified = draft.is_some_and(|draft| !draft.modified().is_empty());
    let invalid = draft.is_some_and(|draft| !draft.issues().is_empty());
    let blocked = invalid || (modified && (!source_current || !theme_qualified));
    if !modified || blocked {
        model = model.with_footer_availability(
            SettingsFooterCommand::Apply,
            SettingsActionAvailability::disabled_with_reason(if !modified {
                "There are no modified settings."
            } else {
                "Resolve every unavailable or invalid modified setting before applying."
            }),
        );
    }
    if blocked {
        model = model.with_footer_availability(
            SettingsFooterCommand::Accept,
            SettingsActionAvailability::disabled_with_reason(
                "Resolve every unavailable or invalid modified setting before applying.",
            ),
        );
    }
    if let Some(reason) = gate {
        model = model.with_interaction_gate(SettingsInteractionGate::gated(reason));
    }
    model
}
