use std::process::ExitCode;
#[cfg(target_os = "windows")]
use std::{env, ffi::OsString, path::Path, process::Command};

#[cfg(target_os = "windows")]
fn linux_drive_path(path: &Path) -> Result<String, String> {
    let path = path.canonicalize().map_err(|error| error.to_string())?;
    let text = path.to_str().ok_or("test path is not Unicode")?;
    let text = text.strip_prefix(r"\\?\").unwrap_or(text);
    let bytes = text.as_bytes();
    if bytes.len() < 3 || !bytes[0].is_ascii_alphabetic() || bytes[1] != b':' || bytes[2] != b'\\' {
        return Err("test binary must be on an absolute Windows drive path".into());
    }
    Ok(format!(
        "/mnt/{}/{}",
        (bytes[0] as char).to_ascii_lowercase(),
        text[3..].replace('\\', "/")
    ))
}

#[cfg(target_os = "windows")]
fn run() -> Result<i32, String> {
    let mut args = env::args_os().skip(1);
    let distro: OsString = args.next().ok_or("missing distribution")?;
    let name = distro.to_str().ok_or("distribution name is not Unicode")?;
    if name.is_empty() || name.len() > 256 || name.chars().any(char::is_control) {
        return Err("invalid distribution name".into());
    }
    let mut binary = args.next().ok_or("missing test binary")?;
    let root = binary == "--root";
    if root {
        binary = args.next().ok_or("missing test binary")?;
    }
    let path = linux_drive_path(Path::new(&binary))?;
    let cwd = env::current_dir().map_err(|error| error.to_string())?;
    let mut command = Command::new("wsl.exe");
    command
        .arg("--distribution")
        .arg(distro)
        .arg("--cd")
        .arg(cwd);
    if root {
        command.arg("--user").arg("root");
    }
    let status = command
        .arg("--exec")
        .arg(path)
        .args(args)
        .status()
        .map_err(|error| error.to_string())?;
    Ok(status.code().unwrap_or(1))
}

#[cfg(not(target_os = "windows"))]
fn run() -> Result<i32, String> {
    Err("the WSL target runner requires Windows".into())
}

fn main() -> ExitCode {
    match run() {
        Ok(code) => ExitCode::from(u8::try_from(code).unwrap_or(1)),
        Err(error) => {
            eprintln!("WSL test runner: {error}");
            ExitCode::FAILURE
        }
    }
}
