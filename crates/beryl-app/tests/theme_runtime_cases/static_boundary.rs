const RUNTIME_SOURCES: [&str; 6] = [
    include_str!("../../src/theme_runtime.rs"),
    include_str!("../../src/theme_runtime/adapter.rs"),
    include_str!("../../src/theme_runtime/coordinator.rs"),
    include_str!("../../src/theme_runtime/error.rs"),
    include_str!("../../src/theme_runtime/identity.rs"),
    include_str!("../../src/theme_runtime/publication.rs"),
];

#[test]
fn coordinator_has_no_gui_storage_document_or_repository_authority() {
    for source in RUNTIME_SOURCES {
        for forbidden in [
            "gpui::",
            "ShellView",
            "std::path",
            "std::fs",
            "tempfile",
            "toml::",
            "ThemeDocument::",
            "ThemeResolver",
            "ThemeRepositoryService",
            "ThemeService",
            "SettingsDraft",
        ] {
            assert!(
                !source.contains(forbidden),
                "theme runtime contains forbidden authority {forbidden}"
            );
        }
    }
}

#[test]
fn crate_entry_point_mounts_only_the_pre_gui_theme_runtime() {
    let entry = include_str!("../../src/lib.rs");
    assert!(entry.contains("pub mod theme_runtime;"));
    assert!(!entry.contains("mod appearance;"));
    assert!(!entry.contains("mod theme_dynamic_tools;"));
    assert!(!entry.contains("pub mod shell;"));
    assert!(!entry.contains("mod shell;"));
}
