#![cfg(feature = "test-faults")]

use beryl_app::activity_display_test_support::*;
#[path = "../../syndic-storage/tests/support/mod.rs"]
mod support;
use beryl_model::{ExecutionBinding, PathFlavor, RuntimeMode, RuntimeNativePath};
use syndic_storage::*;
#[path = "../../syndic-storage/tests/activity_display/fixture.rs"]
mod fixture;
use fixture::*;

#[test]
fn powershell_normalization_replaces_only_exact_first_launcher_tokens() {
    for launcher in [
        r"C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe",
        r"d:\WINDOWS.old\System32\WindowsPowerShell\v1.0\PowerShell.EXE",
        r"C:\\Windows\\System32\\WindowsPowerShell\\v1.0\\powershell.exe",
    ] {
        for quote in ["", "\"", "'"] {
            let command = format!("{quote}{launcher}{quote} -NoProfile -Command \"exact rest\"");
            assert_eq!(
                normalize_powershell(&command),
                "powershell.exe -NoProfile -Command \"exact rest\""
            );
        }
    }
    for command in [
        r"echo C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe",
        r"C:\other\powershell.exe x",
        r"C:\Windows.old.other\System32\WindowsPowerShell\v1.0\powershell.exe",
        r"\\host\Windows\System32\WindowsPowerShell\v1.0\powershell.exe",
    ] {
        assert_eq!(normalize_powershell(command), command);
    }
}

#[test]
fn exact_metadata_decorates_in_place_without_ids_defaults_or_provider_as_model() {
    let f = Fixture::new();
    f.complete(
        command_value("echo visible", ProviderCommandStatusV1::InProgress),
        command_value("echo visible", ProviderCommandStatusV1::Completed),
    );
    let entry = f.entry();
    let parent = support::id(99);
    let (source, _) = f.extract(&entry);
    let mut row = format_row(parent, None, &entry, source, None);
    assert_eq!(row.agent(), "");
    let identity = row.source().clone();
    let value = row.value().to_owned();
    let status = row.status();
    let metadata = ActivityAgentMetadata::new(
        entry.source().cas_item().turn().thread_id().clone(),
        "Scout",
        Some("exact-model"),
        Some("high"),
    )
    .unwrap();
    row.decorate_agent(parent, Some(&metadata));
    assert_eq!(row.agent(), "Scout (exact-model/high)");
    assert_eq!(row.source(), &identity);
    assert_eq!(row.value(), value);
    assert_eq!(row.status(), status);
    assert!(row.tooltip().contains("Scout (exact-model/high)"));
    row.decorate_agent(entry.source().thread_id(), Some(&metadata));
    assert_eq!(row.agent(), "Main");
    assert!(
        ActivityAgentMetadata::new(metadata.thread_id().clone(), &"x".repeat(1025), None, None)
            .is_none()
    );
    assert!(
        ActivityAgentMetadata::new(metadata.thread_id().clone(), "line\nname", None, None)
            .is_none()
    );
    let nickname_only =
        ActivityAgentMetadata::new(metadata.thread_id().clone(), "Scout", None, Some("high"))
            .unwrap();
    row.decorate_agent(parent, Some(&nickname_only));
    assert_eq!(row.agent(), "Scout");
    let wrong = ActivityAgentMetadata::new(
        beryl_model::CasThreadId::new("different").unwrap(),
        "Wrong",
        Some("model"),
        None,
    )
    .unwrap();
    row.decorate_agent(parent, Some(&wrong));
    assert_eq!(row.agent(), "");
}

fn binding(flavor: PathFlavor, path: &str) -> ExecutionBinding {
    let template = support::exact_cas::execution_binding();
    ExecutionBinding::new(
        template.runtime_id(),
        template.root_id(),
        RuntimeNativePath::from_admitted(RuntimeMode::host(), flavor, path).unwrap(),
    )
}

#[test]
fn file_paths_require_a_complete_sole_path_and_exact_lexical_execution_root() {
    for (path, root, expected) in [
        ("src/lib.rs", None, "Patching src/lib.rs, +1 -1"),
        ("../secret", None, "Patching 1 file(s), +1 -1"),
        (
            r"C:\work\src\lib.rs",
            Some(binding(PathFlavor::Windows, r"C:\work")),
            "Patching src/lib.rs, +1 -1",
        ),
        (
            r"C:\work-other\secret",
            Some(binding(PathFlavor::Windows, r"C:\work")),
            "Patching 1 file(s), +1 -1",
        ),
        (
            r"\server\share\root\secret",
            Some(binding(PathFlavor::Windows, r"\\server\share\root")),
            "Patching 1 file(s), +1 -1",
        ),
        (
            r"/server/share/root/secret",
            Some(binding(PathFlavor::Windows, r"\\server\share\root")),
            "Patching 1 file(s), +1 -1",
        ),
        (
            r"\\server\share\root\visible",
            Some(binding(PathFlavor::Windows, r"\\server\share\root")),
            "Patching visible, +1 -1",
        ),
        (
            r"\\\server\share\root\secret",
            Some(binding(PathFlavor::Windows, r"\\server\share\root")),
            "Patching 1 file(s), +1 -1",
        ),
        (
            r"\work\secret",
            Some(binding(PathFlavor::Windows, r"C:\work")),
            "Patching 1 file(s), +1 -1",
        ),
        (
            r"\\?\C:\work\secret",
            Some(binding(PathFlavor::Windows, r"\\?\C:\work")),
            "Patching 1 file(s), +1 -1",
        ),
        (
            "/work/src/lib.rs",
            Some(binding(PathFlavor::Posix, "/work")),
            "Patching src/lib.rs, +1 -1",
        ),
        (
            "/work/../secret",
            Some(binding(PathFlavor::Posix, "/work")),
            "Patching 1 file(s), +1 -1",
        ),
        (
            "/other/secret",
            Some(binding(PathFlavor::Posix, "/work")),
            "Patching 1 file(s), +1 -1",
        ),
    ] {
        let f = Fixture::new();
        let changes = vec![change(path, "+added\n-deleted")];
        f.complete(
            ProviderItemV1::FileChange(ProviderFileChangeV1 {
                status: ProviderPatchStatusV1::InProgress,
                changes: changes.clone(),
            }),
            ProviderItemV1::FileChange(ProviderFileChangeV1 {
                status: ProviderPatchStatusV1::Completed,
                changes,
            }),
        );
        let entry = f.entry();
        let (source, _) = f.extract(&entry);
        let row = format_row(
            entry.source().thread_id(),
            root.as_ref(),
            &entry,
            source,
            None,
        );
        assert_eq!(row.value(), expected);
        assert!(!row.tooltip().contains("secret"));
    }
}

#[test]
fn handoff_uses_only_compact_byte_count_and_long_values_have_bounded_tooltips() {
    let f = Fixture::new();
    f.complete(
        command_value(&"界".repeat(2000), ProviderCommandStatusV1::InProgress),
        command_value(&"界".repeat(2000), ProviderCommandStatusV1::Completed),
    );
    let entry = f.entry();
    let (source, _) = f.extract(&entry);
    let metadata = ActivityAgentMetadata::new(
        entry.source().cas_item().turn().thread_id().clone(),
        &"n".repeat(1024),
        Some(&"m".repeat(512)),
        Some(&"r".repeat(128)),
    )
    .unwrap();
    let row = format_row(support::id(99), None, &entry, source, Some(&metadata));
    assert!(row.truncated());
    assert!(row.value().len() <= 1152);
    assert!(row.tooltip().len() <= 3072);
    assert!(row.value().ends_with('…'));
    let handoff = ActivityQueryEntryRecord::new(
        entry.thread_id(),
        entry.work_period(),
        entry.order(),
        entry.source().clone(),
        entry.source_event(),
        ProviderItemKind::AgentMessage,
        ProviderItemLifecycle::Completed,
        Some(ActivityCompactFact::ChildHandoff(
            ActivityChildHandoffFact::new(
                entry.source().thread_id(),
                ProjectionSourceRange::new(0, 31).unwrap(),
            ),
        )),
    )
    .unwrap();
    let row = format_handoff(support::id(99), &handoff, None).unwrap();
    assert_eq!(row.value(), "handoff: 31 bytes");
    assert_eq!(row.status(), ActivityDisplayStatus::FinishedOk);
    assert_eq!(row.item_id(), entry.item_id());
}
