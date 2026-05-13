use std::path::{Path, PathBuf};

use beryl::cli::{BootstrapCli, DEFAULT_PROBE_TIMEOUT_MS, RuntimeTarget};
use clap::error::ErrorKind;

fn parse(args: &[&str]) -> Result<BootstrapCli, clap::Error> {
    BootstrapCli::try_parse_from(std::iter::once("beryl").chain(args.iter().copied()))
}

#[test]
fn default_parse_uses_picker_default_timeout_and_default_home() {
    let cli = parse(&[]).unwrap();

    assert!(matches!(cli.target(), RuntimeTarget::Picker));
    assert_eq!(cli.probe_timeout_ms(), DEFAULT_PROBE_TIMEOUT_MS);
    assert_eq!(cli.beryl_home_dir(), None);
    assert!(!cli.memory_milestones());
    assert!(!cli.diagnostic_target_stdio());
}

#[test]
fn help_lists_beryl_home_options() {
    let error = parse(&["--help"]).unwrap_err();

    assert_eq!(error.kind(), ErrorKind::DisplayHelp);

    let help = error.to_string();
    assert!(help.contains("-H, --beryl-home-dir <PATH>"));
    assert!(help.contains("--host-path <PATH>"));
    assert!(help.contains("--wsl-distro <DISTRO>"));
    assert!(help.contains("--wsl-path <PATH>"));
    assert!(help.contains("--probe-timeout-ms <MS>"));
    assert!(help.contains("--memory-milestones"));
    assert!(help.contains("--diagnostic-target-stdio"));
    assert!(!help.contains("--memory-startup-experiment"));
}

#[test]
fn lowercase_h_remains_help_and_uppercase_h_sets_beryl_home() {
    let help_error = parse(&["-h"]).unwrap_err();
    assert_eq!(help_error.kind(), ErrorKind::DisplayHelp);

    let cli = parse(&["-H", "state-root"]).unwrap();
    assert_eq!(cli.beryl_home_dir(), Some(Path::new("state-root")));
    assert!(matches!(cli.target(), RuntimeTarget::Picker));
}

#[test]
fn host_path_conflicts_with_wsl_target_flags() {
    let conflicts = [
        ["--host-path", r"C:\work", "--wsl-distro", "Ubuntu"],
        ["--host-path", r"C:\work", "--wsl-path", "/work"],
    ];

    for args in conflicts {
        let error = parse(&args).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::ArgumentConflict);
    }
}

#[test]
fn wsl_target_flags_require_each_other() {
    let distro_error = parse(&["--wsl-distro", "Ubuntu"]).unwrap_err();
    assert_eq!(distro_error.kind(), ErrorKind::MissingRequiredArgument);

    let path_error = parse(&["--wsl-path", "/work"]).unwrap_err();
    assert_eq!(path_error.kind(), ErrorKind::MissingRequiredArgument);
}

#[test]
fn zero_probe_timeout_is_rejected() {
    let error = parse(&["--probe-timeout-ms", "0"]).unwrap_err();

    assert_eq!(error.kind(), ErrorKind::ValueValidation);
}

#[test]
fn memory_milestones_flag_is_opt_in() {
    let cli = parse(&["--memory-milestones"]).unwrap();

    assert!(cli.memory_milestones());
    assert!(!cli.diagnostic_target_stdio());
    assert!(matches!(cli.target(), RuntimeTarget::Picker));
}

#[test]
fn diagnostic_target_stdio_requires_explicit_beryl_home() {
    let error = parse(&["--diagnostic-target-stdio"]).unwrap_err();

    assert_eq!(error.kind(), ErrorKind::MissingRequiredArgument);
}

#[test]
fn diagnostic_target_stdio_accepts_explicit_beryl_home() {
    let cli = parse(&[
        "--diagnostic-target-stdio",
        "--beryl-home-dir",
        "child-home",
    ])
    .unwrap();

    assert!(cli.diagnostic_target_stdio());
    assert_eq!(cli.beryl_home_dir(), Some(Path::new("child-home")));
}

#[test]
fn value_flags_reject_missing_values() {
    for flag in [
        "--host-path",
        "--wsl-distro",
        "--wsl-path",
        "--probe-timeout-ms",
        "--beryl-home-dir",
        "-H",
    ] {
        let error = parse(&[flag]).unwrap_err();
        assert_ne!(error.kind(), ErrorKind::DisplayHelp);
    }
}

#[test]
fn long_and_short_beryl_home_options_are_accepted() {
    let long = parse(&["--beryl-home-dir", r"C:\Beryl State"]).unwrap();
    assert_eq!(long.beryl_home_dir(), Some(Path::new(r"C:\Beryl State")));
    assert_eq!(long.probe_timeout_ms(), DEFAULT_PROBE_TIMEOUT_MS);

    let short = parse(&["-H", "relative-state", "--probe-timeout-ms", "1"]).unwrap();
    assert_eq!(short.beryl_home_dir(), Some(Path::new("relative-state")));
    assert_eq!(short.probe_timeout_ms(), 1);
}

#[test]
fn runtime_target_flags_keep_current_meaning_before_resolution() {
    let host = parse(&["--host-path", r"C:\work"]).unwrap();
    assert_eq!(
        host.target(),
        RuntimeTarget::Host {
            path: PathBuf::from(r"C:\work")
        }
    );

    let wsl = parse(&["--wsl-distro", "Ubuntu", "--wsl-path", "/work"]).unwrap();
    assert_eq!(
        wsl.target(),
        RuntimeTarget::Wsl {
            distro: "Ubuntu".to_string(),
            path: PathBuf::from("/work")
        }
    );
}
