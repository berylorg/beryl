use beryl_app::cas_projection::RuntimeTokenDirectory;
use beryl_model::{AdmittedHostPath, PathFlavor, RuntimeMode};

#[test]
fn host_keeps_the_admitted_path() {
    let host =
        AdmittedHostPath::from_admitted(PathFlavor::Windows, r"C:\Users\operator\Temp").unwrap();
    let directory = RuntimeTokenDirectory::from_admitted(host.clone());
    let runtime = directory.runtime_path(&RuntimeMode::Host).unwrap();
    assert_eq!(directory.host(), &host);
    assert_eq!(runtime.as_str(), host.as_str());
    assert_eq!(runtime.mode(), &RuntimeMode::Host);
}

#[test]
fn wsl_projects_canonical_drive_path_into_each_exact_distribution() {
    let directory = RuntimeTokenDirectory::from_admitted(
        AdmittedHostPath::from_admitted(PathFlavor::Windows, r"\\?\C:\Users\operator\Temp")
            .unwrap(),
    );
    for distribution in ["Ubuntu", "Debian"] {
        let mode = RuntimeMode::wsl(distribution).unwrap();
        let runtime = directory.runtime_path(&mode).unwrap();
        assert_eq!(runtime.as_str(), "/mnt/c/Users/operator/Temp");
        assert_eq!(runtime.mode(), &mode);
    }
}

#[test]
fn unmappable_paths_have_no_wsl_fallback() {
    let mode = RuntimeMode::wsl("Ubuntu").unwrap();
    for path in [
        r"\\server\share\tokens",
        r"\\?\UNC\server\share\tokens",
        r"C:\tokens\..\other",
    ] {
        let directory = RuntimeTokenDirectory::from_admitted(
            AdmittedHostPath::from_admitted(PathFlavor::Windows, path).unwrap(),
        );
        assert!(directory.runtime_path(&mode).is_none());
        assert!(directory.runtime_path(&RuntimeMode::Host).is_some());
    }
}
