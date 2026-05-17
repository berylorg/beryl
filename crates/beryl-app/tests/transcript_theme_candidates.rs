pub use beryl_app::{
    ActiveThemeProjection, AppearanceSettings, BerylThemeProperty, BerylThemeRole,
    InstalledThemeId, StylePropertyValue, ThemeDefinition, ThemeDocument, ThemeDocumentError,
    ThemeRepositoryError, ThemeRepositorySnapshot, ThemeRepositoryStore, ThemeResolutionContext,
    ThemeResolutionError, ThemeResolver, ThemeValidationDiagnostics, built_in_theme_schema,
};

#[path = "../src/shell/theme_candidates.rs"]
mod theme_candidates;

use theme_candidates::{
    ThemeCandidateFeedbackKind, ThemeCandidatePanelFeedback, ThemeCandidateState,
    ThemeCandidateValidationError,
};

#[test]
fn beryl_theme_language_detection_accepts_candidate_label_variants() {
    assert!(theme_candidates::is_theme_candidate_language(Some(
        "beryl-theme"
    )));
    assert!(theme_candidates::is_theme_candidate_language(Some(
        "BERYL-THEME linenos"
    )));
    assert!(theme_candidates::is_theme_candidate_language(Some(
        "\"beryl-theme\""
    )));
    assert!(theme_candidates::is_theme_candidate_language(Some(
        "`beryl-theme`"
    )));

    assert!(!theme_candidates::is_theme_candidate_language(None));
    assert!(!theme_candidates::is_theme_candidate_language(Some("")));
    assert!(!theme_candidates::is_theme_candidate_language(Some("toml")));
    assert!(!theme_candidates::is_theme_candidate_language(Some(
        "beryl-theme-extra"
    )));
}

#[test]
fn valid_theme_candidate_validates_for_preview_and_install_name() {
    let source = candidate_document(None, Some("Operator Candidate"), "#123456");
    let candidate = theme_candidates::validate_theme_candidate(
        source.as_str(),
        &ThemeRepositorySnapshot::built_in(),
    )
    .unwrap();

    assert_eq!(candidate.install_name().unwrap(), "Operator Candidate");
    assert_eq!(
        projection_foreground(candidate.preview_projection()),
        StylePropertyValue::color("#123456")
    );
}

#[test]
fn duplicate_embedded_candidate_id_is_rejected_before_preview() {
    let source = candidate_document(
        Some(InstalledThemeId::built_in()),
        Some("Built In Copy"),
        "#654321",
    );
    let error = theme_candidates::validate_theme_candidate(
        source.as_str(),
        &ThemeRepositorySnapshot::built_in(),
    )
    .unwrap_err();

    assert!(matches!(
        error,
        ThemeCandidateValidationError::DuplicateEmbeddedId { id } if id.as_str() == "built-in"
    ));
}

#[test]
fn unnamed_theme_candidate_can_preview_but_cannot_install() {
    let source = candidate_document(None, None, "#abcdef");
    let candidate = theme_candidates::validate_theme_candidate(
        source.as_str(),
        &ThemeRepositorySnapshot::built_in(),
    )
    .unwrap();

    assert_eq!(
        projection_foreground(candidate.preview_projection()),
        StylePropertyValue::color("#abcdef")
    );
    assert!(matches!(
        candidate.install_name().unwrap_err(),
        ThemeCandidateValidationError::MissingInstallName
    ));
}

#[test]
fn invalid_theme_candidates_fail_with_validation_errors_without_state_mutation() {
    let mut state = ThemeCandidateState::default();
    let durable = ActiveThemeProjection::built_in();
    state.start_preview(
        "previewed".to_string(),
        Some("thread-a".to_string()),
        durable.clone(),
    );

    for source in [
        "schema = 2\n",
        "schema = 1\n[[role]]\nid = \"app.window\"\nforeground = { value = 12 }\n",
        "schema = 1\n[[role]]\nid = \"not.a.role\"\nforeground = { value = \"#112233\" }\n",
    ] {
        let error = theme_candidates::validate_theme_candidate(
            source,
            &ThemeRepositorySnapshot::built_in(),
        )
        .unwrap_err();
        assert!(matches!(
            error,
            ThemeCandidateValidationError::Document { .. }
                | ThemeCandidateValidationError::InvalidDefinition { .. }
        ));
    }

    assert_eq!(
        state.snapshot().active_preview_panel_id(),
        Some("previewed")
    );
    let restored = state.stop_active_preview().unwrap();
    assert_eq!(restored.style_revision(), durable.style_revision());
}

#[test]
fn preview_state_restores_on_stop_and_thread_switch() {
    let source = candidate_document(None, Some("Thread Preview"), "#334455");
    let candidate = theme_candidates::validate_theme_candidate(
        source.as_str(),
        &ThemeRepositorySnapshot::built_in(),
    )
    .unwrap();
    let durable = ActiveThemeProjection::built_in();
    let preview = candidate.preview_projection().clone();
    assert_ne!(preview.style_revision(), durable.style_revision());

    let mut state = ThemeCandidateState::default();
    state.start_preview(
        "panel-a".to_string(),
        Some("thread-a".to_string()),
        durable.clone(),
    );
    assert_eq!(state.snapshot().active_preview_panel_id(), Some("panel-a"));
    assert_eq!(
        state
            .restore_projection_for_new_preview(preview.clone())
            .style_revision(),
        durable.style_revision()
    );
    assert!(state.restore_if_thread_changed(Some("thread-a")).is_none());
    assert!(state.stop_preview_for_panel("other-panel").is_none());

    let restored = state.stop_preview_for_panel("panel-a").unwrap();
    assert_eq!(restored.style_revision(), durable.style_revision());
    assert!(state.snapshot().active_preview_panel_id().is_none());

    state.start_preview(
        "panel-b".to_string(),
        Some("thread-a".to_string()),
        durable.clone(),
    );
    let restored_after_switch = state.restore_if_thread_changed(Some("thread-b")).unwrap();
    assert_eq!(
        restored_after_switch.style_revision(),
        durable.style_revision()
    );
    assert!(state.snapshot().active_preview_panel_id().is_none());
}

#[test]
fn panel_feedback_is_bounded_and_retained_by_panel_id() {
    let mut state = ThemeCandidateState::default();
    let long_message = "x".repeat(400);
    state.set_feedback(
        "long".to_string(),
        ThemeCandidatePanelFeedback::error(long_message),
    );
    let snapshot = state.snapshot();
    let feedback = snapshot.feedback("long").unwrap();
    assert_eq!(feedback.kind(), ThemeCandidateFeedbackKind::Error);
    assert!(feedback.message().len() <= 240);
    assert!(feedback.message().ends_with("..."));

    for index in 0..130 {
        state.set_feedback(
            format!("panel-{index}"),
            ThemeCandidatePanelFeedback::info("ok"),
        );
    }

    let snapshot = state.snapshot();
    assert!(snapshot.feedback("long").is_none());
    assert!(snapshot.feedback("panel-0").is_none());
    assert!(snapshot.feedback("panel-1").is_none());
    assert!(snapshot.feedback("panel-2").is_some());
    assert_eq!(
        snapshot.feedback("panel-129").unwrap().kind(),
        ThemeCandidateFeedbackKind::Info
    );
}

fn candidate_document(
    id: Option<InstalledThemeId>,
    name: Option<&str>,
    foreground: &str,
) -> String {
    ThemeDocument::new(id, name.map(str::to_string), theme_definition(foreground))
        .unwrap()
        .to_toml_string()
        .unwrap()
}

fn theme_definition(foreground: &str) -> ThemeDefinition {
    let mut settings = AppearanceSettings::default();
    settings.general_ui.foreground = foreground.to_string();
    settings.to_theme_definition().unwrap()
}

fn projection_foreground(projection: &ActiveThemeProjection) -> StylePropertyValue {
    projection
        .resolve_property(
            BerylThemeRole::AppWindow.id(),
            BerylThemeProperty::Foreground.id(),
            &ThemeResolutionContext::new(),
        )
        .unwrap()
}
