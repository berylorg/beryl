#[path = "src/build_identity.rs"]
mod build_identity;

use std::{
    env,
    path::{Path, PathBuf},
    process::Command,
};

fn main() {
    println!("cargo:rerun-if-env-changed=BERYL_BUILD_GIT_COMMIT");

    let manifest_dir =
        PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").expect("Cargo sets manifest dir"));
    let injected_commit_value = env::var_os("BERYL_BUILD_GIT_COMMIT");
    let injected_commit = match injected_commit_value.as_deref() {
        None => build_identity::InjectedCommit::Missing,
        Some(value) => match value.to_str() {
            Some(value) => build_identity::InjectedCommit::Present(value),
            None => build_identity::InjectedCommit::NonUnicode,
        },
    };
    let discovered = LocalGitMetadata::discover(&manifest_dir);
    let build_id = build_identity::resolve_build_identity(
        injected_commit,
        discovered.as_ref().map(|metadata| metadata.commit.as_str()),
        discovered.as_ref().is_some_and(|metadata| metadata.dirty),
    );

    println!("cargo:rustc-env=BERYL_BUILD_ID={build_id}");
}

struct LocalGitMetadata {
    commit: String,
    dirty: bool,
}

impl LocalGitMetadata {
    fn discover(manifest_dir: &Path) -> Option<Self> {
        let repository = git_output(manifest_dir, &["rev-parse", "--show-toplevel"])?;
        let repository = PathBuf::from(repository);
        emit_git_rerun_inputs(&repository);

        let commit = git_output(&repository, &["rev-parse", "--verify", "HEAD"])?;
        let status = git_output(
            &repository,
            &["status", "--porcelain", "--untracked-files=no"],
        )?;
        let dirty = !status.is_empty();

        Some(Self { commit, dirty })
    }
}

fn emit_git_rerun_inputs(repository: &Path) {
    for git_path in ["HEAD", "index", "packed-refs"] {
        if let Some(path) = git_output(repository, &["rev-parse", "--git-path", git_path]) {
            emit_rerun_if_changed(repository, &path);
        }
    }

    if let Some(reference) = git_output(repository, &["symbolic-ref", "-q", "HEAD"])
        && let Some(path) = git_output(repository, &["rev-parse", "--git-path", &reference])
    {
        emit_rerun_if_changed(repository, &path);
    }

    if let Some(files) = git_output(repository, &["ls-files", "-z"]) {
        for file in files.split('\0').filter(|file| !file.is_empty()) {
            emit_rerun_if_changed(repository, file);
        }
    }
}

fn emit_rerun_if_changed(repository: &Path, path: &str) {
    let path = PathBuf::from(path);
    let path = if path.is_absolute() {
        path
    } else {
        repository.join(path)
    };
    if path.exists() {
        println!("cargo:rerun-if-changed={}", path.display());
    }
}

fn git_output(repository: &Path, arguments: &[&str]) -> Option<String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(repository)
        .args(arguments)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }

    String::from_utf8(output.stdout)
        .ok()
        .map(|output| output.trim().to_owned())
}
