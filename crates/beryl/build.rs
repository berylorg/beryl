#[path = "src/wsl_supervisor_artifact/identity.rs"]
mod identity;

use std::{env, fs, path::PathBuf};

fn main() {
    println!("cargo:rerun-if-env-changed=BERYL_WSL_SUPERVISOR_ARTIFACT");
    println!("cargo:rerun-if-changed=src/wsl_supervisor_artifact/identity.rs");
    let descriptor = env::var_os("BERYL_WSL_SUPERVISOR_ARTIFACT").map(|input| {
        let path = PathBuf::from(input);
        println!("cargo:rerun-if-changed={}", path.display());
        let mut artifact = fs::File::open(&path).expect("open build-owned WSL supervisor artifact");
        identity::digest(&mut artifact)
            .expect("validate static Linux x86_64 WSL supervisor artifact")
    });
    let contents = match descriptor {
        Some(digest) => format!(
            "const BUNDLED_ARTIFACT: Option<([u8; 32], u16)> = Some(({digest:?}, {}));\n",
            beryl_wsl_supervisor::PROTOCOL_VERSION
        ),
        None => "const BUNDLED_ARTIFACT: Option<([u8; 32], u16)> = None;\n".to_owned(),
    };
    let output = PathBuf::from(env::var_os("OUT_DIR").expect("Cargo build output"));
    fs::write(output.join("wsl_supervisor_release.rs"), contents)
        .expect("write immutable WSL supervisor release identity");
}
