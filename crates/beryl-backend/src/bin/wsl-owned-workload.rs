#[cfg(target_os = "linux")]
mod workload {
    use std::{
        io::{BufRead, BufReader, Read, Write},
        net::TcpListener,
        process::{Command, Stdio},
    };

    unsafe extern "C" {
        fn fork() -> i32;
        fn setsid() -> i32;
        fn geteuid() -> u32;
        fn getegid() -> u32;
        fn getgroups(size: i32, list: *mut u32) -> i32;
    }

    pub fn run() -> Result<(), Box<dyn std::error::Error>> {
        let arguments: Vec<String> = std::env::args().skip(1).collect();
        match arguments.first().map(String::as_str) {
            Some("--identity") => {
                println!("{}", identity()?);
                std::io::stdout().flush()?;
                let mut control = [0; 1];
                if std::io::stdin().read(&mut control)? != 0 {
                    return Err("unexpected identity control byte".into());
                }
                println!("BERYL_IDENTITY_CLOSED");
                Ok(())
            }
            Some("--detached-listener") => {
                // The fixture starts single-threaded and carries no application locks across fork.
                let forked = unsafe { fork() };
                if forked < 0 {
                    return Err(std::io::Error::last_os_error().into());
                }
                if forked > 0 {
                    return Ok(());
                }
                if unsafe { setsid() } < 0 {
                    return Err(std::io::Error::last_os_error().into());
                }
                let forked = unsafe { fork() };
                if forked < 0 {
                    return Err(std::io::Error::last_os_error().into());
                }
                if forked > 0 {
                    return Ok(());
                }
                let port = spawn_listener("--nested-listener")?;
                serve("127.0.0.1:0", Some(port), true)
            }
            Some("--nested-listener") | Some("--independent-listener") => {
                serve("127.0.0.1:0", None, true)
            }
            Some("app-server") => {
                let listen = argument(&arguments, "--listen")?
                    .strip_prefix("ws://")
                    .ok_or("missing loopback listener")?;
                let token_path = argument(&arguments, "--ws-token-file")?;
                let mut token = std::fs::read(token_path)?;
                if token.len() != 64 {
                    return Err("unexpected bearer file".into());
                }
                token.fill(0);
                drop(token);
                let detached = spawn_listener("--detached-listener")?;
                serve(listen, Some(detached), false)
            }
            _ => Err("unsupported fixed fixture invocation".into()),
        }
    }

    fn identity() -> Result<String, Box<dyn std::error::Error>> {
        let count = unsafe { getgroups(0, std::ptr::null_mut()) };
        if !(0..=256).contains(&count) {
            return Err("supplementary groups exceed fixed identity bound".into());
        }
        let mut groups = vec![0; count as usize];
        if unsafe { getgroups(count, groups.as_mut_ptr()) } != count {
            return Err(std::io::Error::last_os_error().into());
        }
        let groups = groups
            .iter()
            .map(u32::to_string)
            .collect::<Vec<_>>()
            .join(",");
        Ok(format!(
            "BERYL_IDENTITY_V1|{}|{}|{}|{}|{}",
            unsafe { geteuid() },
            unsafe { getegid() },
            groups,
            environment_field("HOME", 4096)?,
            environment_field("USER", 256)?
        ))
    }

    fn environment_field(name: &str, bound: usize) -> Result<String, Box<dyn std::error::Error>> {
        use std::os::unix::ffi::OsStrExt;
        match std::env::var_os(name) {
            Some(value) if value.as_bytes().len() <= bound => Ok(hex::encode(value.as_bytes())),
            Some(_) => Err("allowlisted identity field exceeds its fixed bound".into()),
            None => Ok("-".into()),
        }
    }

    fn argument<'a>(
        arguments: &'a [String],
        name: &str,
    ) -> Result<&'a str, Box<dyn std::error::Error>> {
        arguments
            .windows(2)
            .find(|pair| pair[0] == name)
            .map(|pair| pair[1].as_str())
            .ok_or_else(|| "missing fixture argument".into())
    }

    fn spawn_listener(mode: &str) -> Result<u16, Box<dyn std::error::Error>> {
        let mut child = Command::new(std::env::current_exe()?)
            .arg(mode)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()?;
        let mut port = String::new();
        BufReader::new(child.stdout.take().ok_or("missing fixture output")?)
            .take(64)
            .read_line(&mut port)?;
        Ok(port.trim().parse()?)
    }

    fn serve(
        address: &str,
        descendant: Option<u16>,
        announce: bool,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let listener = TcpListener::bind(address)?;
        if announce {
            println!("{}", listener.local_addr()?.port());
            std::io::stdout().flush()?;
        }
        for stream in listener.incoming() {
            let mut stream = stream?;
            let mut command = String::new();
            BufReader::new(stream.try_clone()?)
                .take(64)
                .read_line(&mut command)?;
            match command.trim() {
                "descendant" => writeln!(stream, "{}", descendant.unwrap_or(0))?,
                "ping" => writeln!(stream, "alive")?,
                "identity" => writeln!(stream, "{}", identity()?)?,
                "quit" if announce && descendant.is_none() => break,
                _ => writeln!(stream, "unsupported")?,
            }
        }
        Ok(())
    }
}

#[cfg(target_os = "linux")]
fn main() {
    if let Err(error) = workload::run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}

#[cfg(not(target_os = "linux"))]
fn main() {
    eprintln!("Linux ownership fixture requires Linux");
    std::process::exit(1);
}
