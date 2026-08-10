mod projection_candidate_reauthentication {
    use super::*;

    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/unit/projection_candidate_reauthentication/support.rs"
    ));
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/unit/projection_candidate_reauthentication/support_helpers.rs"
    ));

    mod candidate_outcomes {
        use super::*;
        include!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/unit/projection_candidate_reauthentication/candidate_outcomes.rs"
        ));
    }

    mod authenticated_facts {
        use super::*;
        include!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/unit/projection_candidate_reauthentication/authenticated_facts.rs"
        ));
    }

    mod owner_facts {
        use super::*;
        include!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/unit/projection_candidate_reauthentication/owner_facts.rs"
        ));
    }

    mod pending_binding_facts {
        use super::*;
        include!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/unit/projection_candidate_reauthentication/pending_binding_facts.rs"
        ));
    }

    mod retry_and_revocation {
        use super::*;
        include!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/unit/projection_candidate_reauthentication/retry_and_revocation.rs"
        ));
    }

    mod retirement {
        use super::*;
        include!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/unit/projection_candidate_reauthentication/retirement.rs"
        ));
    }

    mod shared_authority {
        use super::*;
        include!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/unit/projection_candidate_reauthentication/shared_authority.rs"
        ));
    }

    mod stable_read {
        use super::*;
        include!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/unit/projection_candidate_reauthentication/stable_read.rs"
        ));
    }
}
