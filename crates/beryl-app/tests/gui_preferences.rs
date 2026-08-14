#[path = "support/tempdir.rs"]
mod tempdir_support;

use std::fs;

use beryl_app::{
    AgentPreferences, ContextCompactionTimeoutError, DEFAULT_CONTEXT_COMPACTION_TIMEOUT_SECONDS,
    GuiPreferences, GuiPreferencesStore, MAX_CONTEXT_COMPACTION_TIMEOUT_SECONDS,
    MIN_CONTEXT_COMPACTION_TIMEOUT_SECONDS, NotificationPreferences, NotificationSoundPathError,
    OperationPreferences, normalize_developer_instructions_text,
    parse_context_compaction_timeout_seconds_text, parse_notification_sound_path_text,
};

#[test]
fn gui_preferences_default_has_no_end_turn_sound() {
    let root = unique_temp_dir();
    let store = GuiPreferencesStore::new(&root);

    let preferences = store.load_or_default().unwrap();

    assert_eq!(preferences.notifications.end_turn_sound_path, None);
    assert_eq!(preferences.agent.developer_instructions, None);
    assert_eq!(
        preferences.operations.context_compaction_timeout_seconds,
        DEFAULT_CONTEXT_COMPACTION_TIMEOUT_SECONDS
    );
    cleanup_temp_dir(root);
}

#[test]
fn gui_preferences_roundtrip_through_preferences_toml() {
    let root = unique_temp_dir();
    let store = GuiPreferencesStore::new(&root);
    let sound_path = root.join("Done.WAV");
    let preferences = GuiPreferences {
        notifications: NotificationPreferences::with_end_turn_sound_path(Some(sound_path.clone()))
            .unwrap(),
        agent: AgentPreferences::with_developer_instructions(Some(
            "Use subagents for independent reviews.".to_string(),
        )),
        operations: OperationPreferences::with_context_compaction_timeout_seconds(240).unwrap(),
    };

    store.save(&preferences).unwrap();

    let loaded = store.load_or_default().unwrap();
    assert_eq!(
        loaded.notifications.end_turn_sound_path.as_deref(),
        Some(sound_path.as_path())
    );
    assert_eq!(
        loaded.agent.developer_instructions.as_deref(),
        Some("Use subagents for independent reviews.")
    );
    assert_eq!(loaded.operations.context_compaction_timeout_seconds, 240);
    assert!(store.preferences_path().exists());
    cleanup_temp_dir(root);
}

#[test]
fn gui_preferences_invalid_persisted_operation_timeout_preserves_other_preferences() {
    assert_invalid_persisted_operation_timeout_preserves_other_preferences(
        "context_compaction_timeout_seconds = 0",
    );
    assert_invalid_persisted_operation_timeout_preserves_other_preferences(
        "context_compaction_timeout_seconds = \"abc\"",
    );
}

fn assert_invalid_persisted_operation_timeout_preserves_other_preferences(replacement: &str) {
    let root = unique_temp_dir();
    let store = GuiPreferencesStore::new(&root);
    let sound_path = root.join("Done.WAV");
    let preferences = GuiPreferences {
        notifications: NotificationPreferences::with_end_turn_sound_path(Some(sound_path.clone()))
            .unwrap(),
        agent: AgentPreferences::with_developer_instructions(Some(
            "Keep valid preferences when operation timeout is invalid.".to_string(),
        )),
        operations: OperationPreferences::with_context_compaction_timeout_seconds(240).unwrap(),
    };

    store.save(&preferences).unwrap();
    let text = fs::read_to_string(store.preferences_path()).unwrap();
    fs::write(
        store.preferences_path(),
        text.replace("context_compaction_timeout_seconds = 240", replacement),
    )
    .unwrap();

    let loaded = store.load_or_default().unwrap();
    assert_eq!(
        loaded.notifications.end_turn_sound_path.as_deref(),
        Some(sound_path.as_path())
    );
    assert_eq!(
        loaded.agent.developer_instructions.as_deref(),
        Some("Keep valid preferences when operation timeout is invalid.")
    );
    assert_eq!(
        loaded.operations.context_compaction_timeout_seconds,
        DEFAULT_CONTEXT_COMPACTION_TIMEOUT_SECONDS
    );
    cleanup_temp_dir(root);
}

#[cfg(windows)]
#[test]
fn gui_preferences_failed_persist_preserves_existing_preferences_file() {
    let root = unique_temp_dir();
    let store = GuiPreferencesStore::new(&root);
    let original = GuiPreferences {
        notifications: NotificationPreferences::with_end_turn_sound_path(Some(
            root.join("OldSound.WAV"),
        ))
        .unwrap(),
        agent: AgentPreferences::with_developer_instructions(Some(
            "Keep the old persisted instructions.".to_string(),
        )),
        operations: OperationPreferences::with_context_compaction_timeout_seconds(240).unwrap(),
    };
    let replacement = GuiPreferences {
        notifications: NotificationPreferences::with_end_turn_sound_path(Some(
            root.join("NewSound.WAV"),
        ))
        .unwrap(),
        agent: AgentPreferences::with_developer_instructions(Some(
            "This write should fail before becoming authoritative.".to_string(),
        )),
        operations: OperationPreferences::with_context_compaction_timeout_seconds(300).unwrap(),
    };

    store.save(&original).unwrap();
    let original_text = fs::read_to_string(store.preferences_path()).unwrap();
    let lock = tempdir_support::lock_file_against_replacement(&store.preferences_path()).unwrap();

    assert!(store.save(&replacement).is_err());
    drop(lock);

    assert_eq!(store.load_or_default().unwrap(), original);
    assert_eq!(
        fs::read_to_string(store.preferences_path()).unwrap(),
        original_text
    );
    cleanup_temp_dir(root);
}

#[test]
fn gui_preferences_rejects_relative_or_non_wav_sound_paths() {
    assert_eq!(
        parse_notification_sound_path_text("sounds/done.wav").unwrap_err(),
        NotificationSoundPathError::NotAbsolute
    );

    let root = unique_temp_dir();
    let store = GuiPreferencesStore::new(&root);
    let preferences = GuiPreferences {
        notifications: NotificationPreferences {
            end_turn_sound_path: Some(root.join("done.mp3")),
        },
        ..GuiPreferences::default()
    };

    assert!(store.save(&preferences).is_err());
    assert!(!store.preferences_path().exists());
    cleanup_temp_dir(root);
}

#[test]
fn empty_notification_sound_text_disables_sound() {
    assert_eq!(parse_notification_sound_path_text("   ").unwrap(), None);
}

#[test]
fn empty_developer_instructions_text_disables_setting() {
    assert_eq!(normalize_developer_instructions_text(" \n\t "), None);
    assert_eq!(
        AgentPreferences::with_developer_instructions(Some(" \n\t ".to_string()))
            .developer_instructions,
        None
    );
}

#[test]
fn developer_instructions_normalization_preserves_non_empty_text() {
    let text = "Use subagents when the work can run independently.\nKeep changes scoped.";
    assert_eq!(
        normalize_developer_instructions_text(text).as_deref(),
        Some(text)
    );
}

#[test]
fn context_compaction_timeout_text_parses_whole_seconds_in_range() {
    assert_eq!(
        parse_context_compaction_timeout_seconds_text(" 240 ").unwrap(),
        240
    );
}

#[test]
fn context_compaction_timeout_text_rejects_invalid_values() {
    assert_eq!(
        parse_context_compaction_timeout_seconds_text("").unwrap_err(),
        ContextCompactionTimeoutError::NotInteger
    );
    assert_eq!(
        parse_context_compaction_timeout_seconds_text("0").unwrap_err(),
        ContextCompactionTimeoutError::TooSmall {
            min: MIN_CONTEXT_COMPACTION_TIMEOUT_SECONDS
        }
    );
    assert_eq!(
        parse_context_compaction_timeout_seconds_text(
            &(MAX_CONTEXT_COMPACTION_TIMEOUT_SECONDS + 1).to_string()
        )
        .unwrap_err(),
        ContextCompactionTimeoutError::TooLarge {
            max: MAX_CONTEXT_COMPACTION_TIMEOUT_SECONDS
        }
    );
}

fn unique_temp_dir() -> tempdir_support::TestTempDir {
    tempdir_support::temp_dir("beryl-gui-preferences-test-")
}

fn cleanup_temp_dir(root: tempdir_support::TestTempDir) {
    root.close().unwrap();
}
