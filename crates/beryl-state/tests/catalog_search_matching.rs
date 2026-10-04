use beryl_model::{AdmittedHostPath, Availability, PathFlavor, RootId, RuntimeId};
use beryl_state::{
    CatalogArchiveSummary, CatalogAvailabilitySummary, CatalogClaimSummary,
    CatalogExecutionSummary, CatalogFacts, CatalogLineageSummary, CatalogNormalizedQuery,
    CatalogResolvedTitle, UnixMillis,
};

#[test]
fn catalog_search_matches_normalized_contiguous_fields_and_preserves_visible_spelling() {
    let execution = CatalogExecutionSummary::new(
        RuntimeId::from_bytes([1; 16]),
        RootId::from_bytes([2; 16]),
        "Host",
        AdmittedHostPath::from_admitted(PathFlavor::Windows, r"C:\Codex\codex.exe").unwrap(),
        AdmittedHostPath::from_admitted(PathFlavor::Windows, r"C:\Work\Straße").unwrap(),
        CatalogAvailabilitySummary::new(Availability::Available, Availability::Available),
    )
    .unwrap();
    let facts = CatalogFacts::new(
        CatalogResolvedTitle::generated("ＦＯＯ Café Straße").unwrap(),
        execution,
        CatalogArchiveSummary::Ordinary,
        UnixMillis::new(1),
        true,
        CatalogClaimSummary::Unclaimed,
        CatalogLineageSummary::TopLevel,
    )
    .unwrap();
    for query in [
        "foo",
        "CAFE\u{301}",
        "STRASSE",
        "HOST",
        "CODEX.EXE",
        r"work\strasse",
        "",
        "\u{200b}",
    ] {
        assert!(
            facts
                .search()
                .matches(&CatalogNormalizedQuery::new(query).unwrap()),
            "{query:?}"
        );
    }
    for query in ["foo strasse", "host codex", "missing"] {
        assert!(
            !facts
                .search()
                .matches(&CatalogNormalizedQuery::new(query).unwrap()),
            "{query:?}"
        );
    }
    assert_eq!(facts.title().text(), Some("ＦＯＯ Café Straße"));
    assert_eq!(
        facts.execution().full_root_path().as_str(),
        r"C:\Work\Straße"
    );
}
