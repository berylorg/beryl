#[cfg(target_os = "windows")]
#[path = "crash_report_fixture/windows.rs"]
mod windows;

fn main() {
    #[cfg(target_os = "windows")]
    windows::run();
    #[cfg(not(target_os = "windows"))]
    std::process::exit(1);
}
