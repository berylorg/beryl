use beryl_model::workspace::{
    BerylWorkspaceId, BerylWorkspaceIdError, BerylWorkspaceManifest, BerylWorkspaceTitleError,
    BerylWorkspaceTitleSource, derive_workspace_slug,
};

#[test]
fn legacy_scratchpad_manifest_uses_fixed_identity() {
    let manifest = BerylWorkspaceManifest::scratchpad(1234);

    assert_eq!(manifest.id(), &BerylWorkspaceId::scratchpad());
    assert!(manifest.is_scratchpad());
    assert_eq!(manifest.title(), "Scratchpad");
    assert_eq!(
        manifest.title_source(),
        Some(BerylWorkspaceTitleSource::Manual)
    );
    assert_eq!(manifest.last_updated_at_millis(), 1234);
}

#[test]
fn untitled_manifest_uses_sequence_identity_and_label() {
    let manifest = BerylWorkspaceManifest::untitled(7, 1234);

    assert_eq!(manifest.id(), &BerylWorkspaceId::untitled(7));
    assert!(manifest.is_untitled());
    assert_eq!(manifest.title(), "Untitled 7");
    assert_eq!(manifest.title_source(), None);
    assert_eq!(manifest.last_updated_at_millis(), 1234);
}

#[test]
fn named_workspace_ids_reject_invalid_characters() {
    let error = BerylWorkspaceId::new("graphics learning").unwrap_err();

    assert_eq!(error, BerylWorkspaceIdError::InvalidCharacter { ch: ' ' });
}

#[test]
fn named_workspace_ids_reject_reserved_filesystem_names() {
    let error = BerylWorkspaceId::new("con").unwrap_err();

    assert_eq!(
        error,
        BerylWorkspaceIdError::ReservedFilesystemName {
            name: "con".to_string()
        }
    );
}

#[test]
fn workspace_title_derives_ascii_slug_from_accents() {
    let slug = derive_workspace_slug("Crème Brûlée").unwrap();

    assert_eq!(slug.as_str(), "creme-brulee");
}

#[test]
fn workspace_title_derives_ascii_slug_from_cyrillic() {
    let slug = derive_workspace_slug("Привет мир").unwrap();

    assert_eq!(slug.as_str(), "privet-mir");
}

#[test]
fn workspace_title_slug_normalizes_case_punctuation_and_spacing() {
    let slug = derive_workspace_slug("  My   Project__Plan!!  ").unwrap();

    assert_eq!(slug.as_str(), "my-project-plan");
}

#[test]
fn workspace_title_rejects_empty_derived_slug() {
    assert_eq!(
        derive_workspace_slug(" -- __ !! ").unwrap_err(),
        BerylWorkspaceTitleError::EmptyDerivedSlug
    );
}

#[test]
fn workspace_title_rejects_filesystem_unsafe_slug() {
    assert_eq!(
        derive_workspace_slug("CON").unwrap_err(),
        BerylWorkspaceTitleError::InvalidDerivedSlug {
            source: BerylWorkspaceIdError::ReservedFilesystemName {
                name: "con".to_string()
            }
        }
    );
}

#[test]
fn named_workspace_manifest_roundtrips_metadata() {
    let workspace_id = BerylWorkspaceId::new("graphics_learning").unwrap();
    let manifest =
        BerylWorkspaceManifest::named(workspace_id.clone(), "Graphics Learning", 987_654);

    assert_eq!(manifest.id(), &workspace_id);
    assert_eq!(manifest.title(), "Graphics Learning");
    assert_eq!(
        manifest.title_source(),
        Some(BerylWorkspaceTitleSource::Manual)
    );
    assert_eq!(manifest.last_updated_at_millis(), 987_654);
    assert!(!manifest.is_scratchpad());
    assert!(!manifest.is_untitled());
}

#[test]
fn generated_workspace_title_only_applies_to_untitled_manifest() {
    let mut manifest = BerylWorkspaceManifest::untitled(9, 1234);

    assert!(
        manifest
            .set_generated_title_if_untitled(" Renderer notes ")
            .unwrap()
    );
    assert_eq!(manifest.id().as_str(), "renderer-notes");
    assert_eq!(manifest.title(), "Renderer notes");
    assert_eq!(
        manifest.title_source(),
        Some(BerylWorkspaceTitleSource::FirstCompletedTurn)
    );
    assert!(!manifest.is_untitled());
    assert!(
        !manifest
            .set_generated_title_if_untitled("Second title")
            .unwrap()
    );
    assert_eq!(manifest.id().as_str(), "renderer-notes");
    assert_eq!(manifest.title(), "Renderer notes");
}

#[test]
fn manual_workspace_title_overrides_generated_title() {
    let mut manifest = BerylWorkspaceManifest::untitled(3, 1234);

    manifest
        .set_generated_title_if_untitled("Generated notes")
        .unwrap();
    assert!(manifest.set_manual_title("Manual notes").unwrap());
    assert_eq!(manifest.id().as_str(), "manual-notes");
    assert_eq!(manifest.title(), "Manual notes");
    assert_eq!(
        manifest.title_source(),
        Some(BerylWorkspaceTitleSource::Manual)
    );
    assert!(!manifest.set_manual_title("Manual notes").unwrap());
}

#[test]
fn manual_workspace_title_same_slug_updates_display_title_only() {
    let workspace_id = BerylWorkspaceId::new("my-project").unwrap();
    let mut manifest = BerylWorkspaceManifest::named(workspace_id.clone(), "My Project", 1234);

    assert!(manifest.set_manual_title("My  Project!!").unwrap());
    assert_eq!(manifest.id(), &workspace_id);
    assert_eq!(manifest.title(), "My  Project!!");
    assert!(!manifest.set_manual_title("My  Project!!").unwrap());
}

#[test]
fn manual_workspace_title_repairs_legacy_mismatched_id() {
    let mut manifest =
        BerylWorkspaceManifest::named(BerylWorkspaceId::new("legacy-name").unwrap(), "Beryl", 1234);

    assert!(manifest.set_manual_title("Beryl").unwrap());
    assert_eq!(manifest.id().as_str(), "beryl");
    assert_eq!(manifest.title(), "Beryl");
}

#[test]
fn workspace_title_rejects_empty_text() {
    let mut manifest = BerylWorkspaceManifest::untitled(1, 1234);

    assert_eq!(
        manifest.set_manual_title("   ").unwrap_err(),
        BerylWorkspaceTitleError::Empty
    );
    assert_eq!(
        manifest.set_generated_title_if_untitled("   ").unwrap_err(),
        BerylWorkspaceTitleError::Empty
    );
    assert!(manifest.is_untitled());
}
