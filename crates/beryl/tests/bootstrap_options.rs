#[path = "../src/options.rs"]
mod options;

use clap::Parser;
use options::Options;

#[test]
fn ordinary_startup_uses_user_home() {
    let user_home = std::env::current_dir().unwrap().join("bootstrap-user");
    let configuration = Options::try_parse_from(["beryl"])
        .unwrap()
        .resolve(|| Some(user_home.clone()))
        .unwrap();
    assert_eq!(configuration.home, user_home.join(".beryl"));
    assert!(!configuration.diagnostic_target_stdio);
}

#[test]
fn explicit_relative_home_is_fixed_without_consulting_user_home() {
    let configuration = Options::try_parse_from(["beryl", "--beryl-home-dir", "isolated-home"])
        .unwrap()
        .resolve(|| panic!("explicit selection must not consult the user home"))
        .unwrap();
    assert_eq!(
        configuration.home,
        std::env::current_dir().unwrap().join("isolated-home")
    );
}

#[test]
fn diagnostic_mode_requires_explicit_home() {
    assert!(Options::try_parse_from(["beryl", "--diagnostic-target-stdio"]).is_err());
    let configuration = Options::try_parse_from([
        "beryl",
        "--diagnostic-target-stdio",
        "--beryl-home-dir",
        "diagnostic-home",
    ])
    .unwrap()
    .resolve(|| panic!("diagnostic mode must not consult the user home"))
    .unwrap();
    assert!(configuration.diagnostic_target_stdio);
    assert!(configuration.home.is_absolute());
}

#[test]
fn missing_user_home_and_empty_explicit_path_are_errors() {
    assert!(
        Options::try_parse_from(["beryl"])
            .unwrap()
            .resolve(|| None)
            .is_err()
    );
    assert!(Options::try_parse_from(["beryl", "--beryl-home-dir", ""]).is_err());
    assert!(
        Options {
            beryl_home_dir: Some(Default::default()),
            diagnostic_target_stdio: false,
        }
        .resolve(|| panic!("empty explicit path must not fall back"))
        .is_err()
    );
}
