pub fn system_font_appearance(
    state: &beryl_state::BerylState,
) -> beryl_state::PreparedThemeAppearance {
    use beryl_state::*;
    let mut source = String::from("schema = 1\nid = \"native-test\"\nname = \"Native Test\"\n");
    for (_, role) in canonical_theme_schema().roles() {
        if role.property(ThemePropertyId::FontFamily).is_some() {
            source.push_str(&format!(
                "\n[[role]]\nid = \"{}\"\nfont_family = \".SystemUIFont\"\n",
                role.id().as_str()
            ));
        }
    }
    let document =
        ThemeDocument::parse_bytes(source.as_bytes(), ThemeParseMode::StrictCandidate).unwrap();
    let service = state.themes();
    let active = InstalledThemeId::new("native-test").unwrap();
    PreparedThemeAppearance::installed(
        service.settings_identity(beryl_model::DomainRevision::new(1).unwrap(), None),
        &active,
        ThemeDocumentIdentity::new(
            service.manifest(ThemeManifestGeneration::INITIAL),
            active.clone(),
            ThemeDocumentRevision::new(std::num::NonZeroU64::new(1).unwrap()),
            source.len() as u64,
            ThemeDocumentDigest::of_bytes(source.as_bytes()),
        ),
        ThemeResolver::new(document.definition()).unwrap().resolve(),
    )
    .unwrap()
}

pub fn install_native_theme(store: &beryl_home_store::HomeStore, state: &beryl_state::BerylState) {
    use beryl_state::*;
    use std::num::{NonZeroU64, NonZeroUsize};
    struct NoReferences;
    impl ThemeReferenceSnapshotProvider for NoReferences {
        fn current_theme_references(
            &self,
        ) -> Result<ThemeReferenceSnapshot, ThemeReferenceSnapshotUnavailable> {
            Err(ThemeReferenceSnapshotUnavailable)
        }
    }

    let mut source = String::from("schema = 1\nid = \"native-test\"\nname = \"Native Test\"\n");
    for (_, role) in canonical_theme_schema().roles() {
        if role.property(ThemePropertyId::FontFamily).is_some() {
            source.push_str(&format!(
                "\n[[role]]\nid = \"{}\"\nfont_family = \".SystemUIFont\"\n",
                role.id().as_str()
            ));
        }
    }
    let themes = state.themes();
    let max = NonZeroU64::new(1024 * 1024).unwrap();
    let limits = ThemeManifestReadLimits::new(
        NonZeroUsize::new(4096).unwrap(),
        NonZeroUsize::new(16 * 1024).unwrap(),
        NonZeroUsize::new(256 * 1024).unwrap(),
    )
    .unwrap();
    let observation = themes
        .observe_repository(&store, max, limits, None)
        .unwrap();
    let install = ThemeRepositoryCommand::Install(
        InstallTheme::new(
            observation.manifest(),
            InstalledThemeId::new("native-test").unwrap(),
            ThemeName::new("Native Test").unwrap(),
            ThemeDocument::parse_bytes(source.as_bytes(), ThemeParseMode::StrictCandidate).unwrap(),
        )
        .unwrap(),
    );
    assert!(matches!(
        themes
            .execute_command(&store, &observation, &install, max, &NoReferences)
            .unwrap(),
        ThemeRepositoryOperationOutcome::Committed { .. }
    ));
    let settings = state.settings();
    let mut command = beryl_home_store::HomeCommand::new(store.home_revision().unwrap());
    command
        .add(
            settings.apply(
                settings.revision(&store).unwrap(),
                ApplySettings::new(vec![SettingUpdate::new(
                    SettingKey::ActiveThemeId,
                    ExpectedSettingRevision::Absent,
                    SettingValue::active_theme_id("native-test").unwrap(),
                )])
                .unwrap(),
            ),
        )
        .unwrap();
    assert!(matches!(
        store.execute(command),
        beryl_home_store::CommandOutcome::Committed { .. }
    ));
}
