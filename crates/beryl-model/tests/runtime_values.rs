use beryl_model::{
    AdmittedHostPath, ExecutionBinding, PathFlavor, RootId, RuntimeId, RuntimeMode,
    RuntimeNativePath, ValueError, WslDistributionName,
};

#[test]
fn runtime_modes_preserve_exact_validated_wsl_identity() {
    let mode = RuntimeMode::wsl("Ubuntu-24.04").unwrap();

    assert_eq!(
        mode,
        RuntimeMode::Wsl(WslDistributionName::new("Ubuntu-24.04").unwrap())
    );
    assert!(matches!(
        RuntimeMode::wsl(""),
        Err(ValueError::Empty {
            kind: "WSL distribution name"
        })
    ));
    assert!(RuntimeMode::wsl(" Ubuntu").is_err());
    assert!(RuntimeMode::wsl("Ubuntu/other").is_err());
    assert!(RuntimeMode::wsl("x".repeat(257)).is_err());
}

#[test]
fn admitted_paths_are_absolute_and_environment_typed() {
    let wsl = RuntimeMode::wsl("OL9").unwrap();
    let path =
        RuntimeNativePath::from_admitted(wsl.clone(), PathFlavor::Posix, "/home/operator/p/beryl")
            .unwrap();

    assert_eq!(path.mode(), &wsl);
    assert_eq!(path.flavor(), PathFlavor::Posix);
    assert_eq!(path.as_str(), "/home/operator/p/beryl");
    assert!(matches!(
        RuntimeNativePath::from_admitted(wsl, PathFlavor::Windows, r"C:\Users\operator\p\beryl"),
        Err(ValueError::RuntimePathMismatch)
    ));
    assert!(
        RuntimeNativePath::from_admitted(RuntimeMode::host(), PathFlavor::Posix, "relative/path")
            .is_err()
    );
}

#[test]
fn host_paths_support_windows_unc_and_posix_hosts() {
    assert!(
        AdmittedHostPath::from_admitted(PathFlavor::Windows, r"C:\Users\operator\codex.exe")
            .is_ok()
    );
    assert!(
        AdmittedHostPath::from_admitted(
            PathFlavor::Windows,
            r"\\wsl.localhost\OL9\home\operator\codex"
        )
        .is_ok()
    );
    assert!(AdmittedHostPath::from_admitted(PathFlavor::Posix, "/usr/bin/codex").is_ok());
}

#[test]
fn execution_binding_retains_root_identity_and_path_without_probing() {
    let runtime_id = RuntimeId::from_bytes([1; 16]);
    let root_id = RootId::from_bytes([2; 16]);
    let path = RuntimeNativePath::from_admitted(
        RuntimeMode::host(),
        PathFlavor::Windows,
        r"C:\Users\operator\p\beryl",
    )
    .unwrap();
    let binding = ExecutionBinding::new(runtime_id, root_id, path);

    assert_eq!(binding.runtime_id(), runtime_id);
    assert_eq!(binding.root_id(), root_id);
    assert_eq!(binding.root_path().as_str(), r"C:\Users\operator\p\beryl");
}

#[test]
fn serde_revalidates_runtime_path_invariants() {
    let invalid = r#"{
        "mode": {"Wsl": "OL9"},
        "flavor": "Windows",
        "value": "C:\\Users\\operator"
    }"#;

    assert!(serde_json::from_str::<RuntimeNativePath>(invalid).is_err());
}
