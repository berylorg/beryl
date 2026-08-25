use super::*;

mod submission_fixture {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/unit/submission_fixture.rs"
    ));
}

mod fixture {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/unit/accepted_input_replay/fixture.rs"
    ));
}

mod support {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/unit/accepted_input_replay/support.rs"
    ));
}

mod correlation {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/unit/accepted_input_replay/correlation.rs"
    ));
}

mod marker_free {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/unit/accepted_input_replay/marker_free.rs"
    ));
}

mod marker_aware {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/unit/accepted_input_replay/marker_aware.rs"
    ));
}

mod drift {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/unit/accepted_input_replay/drift.rs"
    ));
}
