fn main() {
    #[cfg(target_os = "linux")]
    {
        let mut arguments = std::env::args_os().skip(1);
        let mode = arguments.next();
        if arguments.next().is_some() {
            std::process::exit(2);
        }
        let result = match mode.as_deref().and_then(|value| value.to_str()) {
            Some("context-broker") => beryl_wsl_supervisor::linux::context_broker(),
            Some("supervise") => beryl_wsl_supervisor::linux::supervise(),
            _ => Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "unsupported role",
            )),
        };
        if result.is_err() {
            std::process::exit(1);
        }
    }
    #[cfg(not(target_os = "linux"))]
    {
        eprintln!("Linux supervision is unsupported on this platform");
        std::process::exit(1);
    }
}
