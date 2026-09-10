use std::{
    ffi::OsString,
    io,
    path::Path,
    sync::atomic::{AtomicBool, Ordering},
};

mod record;
#[cfg(target_os = "windows")]
mod windows;

pub const REPORTER_ARGUMENT: &str = "--beryl-crash-reporter";

static INSTALLED: AtomicBool = AtomicBool::new(false);

pub fn install(executable: &Path) -> io::Result<()> {
    if INSTALLED.swap(true, Ordering::Relaxed) {
        return Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "fatal panic handling is already installed",
        ));
    }
    install_abort_hook();
    #[cfg(target_os = "windows")]
    {
        let sender = windows::start(executable)?;
        std::panic::set_hook(Box::new(move |panic| {
            sender.publish(panic);
            std::process::abort();
        }));
        Ok(())
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = executable;
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "crash reporting is unavailable on this platform",
        ))
    }
}

pub fn run_reporter(arguments: &[OsString], present: impl FnOnce(String)) -> ! {
    install_abort_hook();
    #[cfg(target_os = "windows")]
    let result = windows::receive(arguments);
    #[cfg(not(target_os = "windows"))]
    let result: io::Result<Option<String>> = {
        let _ = arguments;
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "crash reporting is unavailable on this platform",
        ))
    };
    match result {
        Ok(Some(report)) => present(report),
        Ok(None) => {}
        Err(_) => std::process::exit(1),
    }
    std::process::exit(0);
}

fn install_abort_hook() {
    std::panic::set_hook(Box::new(|_| std::process::abort()));
}
