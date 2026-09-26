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
