use std::path::Path;

use beryl_model::workspace::WorkspaceId;

#[test]
fn host_openable_path_keeps_host_windows_paths_verbatim() {
    let workspace = WorkspaceId::host_windows(r"C:\work\beryl");

    assert_eq!(
        workspace.host_openable_path(Path::new(r"C:\work\beryl\output\image.png")),
        Path::new(r"C:\work\beryl\output\image.png")
    );
}

#[test]
fn host_openable_path_converts_wsl_paths_to_unc_paths() {
    let workspace = WorkspaceId::wsl_linux("Debian", "/work/beryl");

    assert_eq!(
        workspace.host_openable_path(Path::new("/work/beryl/output/image.png")),
        Path::new(r"\\wsl.localhost\Debian\work\beryl\output\image.png")
    );
}
