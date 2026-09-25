use beryl_app::main_window::GpuiMainWindowShellHost;
use beryl_model::{WindowId, WindowPlacement};

#[cfg(target_os = "windows")]
pub type Prepared = beryl_app::main_window::PreparedWindowsWindowPlacement;
#[cfg(not(target_os = "windows"))]
pub type Prepared = ();

pub fn prepare(window: WindowId, saved: &WindowPlacement) -> Prepared {
    #[cfg(target_os = "windows")]
    {
        let saved = saved.clone();
        std::thread::spawn(move || {
            beryl_app::main_window::prepare_windows_window_placement(window, saved)
        })
        .join()
        .unwrap()
        .unwrap()
    }
    #[cfg(not(target_os = "windows"))]
    let _ = (window, saved);
}

pub fn attach(
    host: GpuiMainWindowShellHost<'_>,
    prepared: Prepared,
) -> GpuiMainWindowShellHost<'_> {
    #[cfg(target_os = "windows")]
    {
        host.with_prepared_placement(prepared)
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = prepared;
        host
    }
}
