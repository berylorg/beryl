use beryl_backend::{
    BackendConfigDefaults, BoundedResponseResult, BoundedResponseTextError, CompatibilityError,
    CompatibilityProbe, CompatibilityProbeResult, CompatibilityProbeSet, ConfigReadResponse,
    DefaultReasoningEffort, InitializePlatform, InitializeResponse, MODEL_CURSOR_MAX_BYTES,
    MODEL_DISPLAY_NAME_MAX_BYTES, MODEL_PAGE_MAX_RECORDS, ModelDisplayName, ModelPage,
    ModelPageCursor, ModelRecord, PROTOCOL_IDENTITY_MAX_BYTES, ProtocolIdentity, ReasoningEffort,
    SupportedReasoningEfforts, ThreadUnsubscribeResponse, ThreadUnsubscribeStatus,
};

use std::mem::size_of;

fn model_record(index: usize) -> ModelRecord {
    let id = format!("id-{index}");
    let model = format!("model-{index}");
    let mut supported = SupportedReasoningEfforts::empty();
    supported.insert(ReasoningEffort::Low);
    supported.insert(ReasoningEffort::High);
    ModelRecord::new(
        ProtocolIdentity::try_new(&id).unwrap(),
        ProtocolIdentity::try_new(&model).unwrap(),
        ModelDisplayName::try_new("Model").unwrap(),
        index % 2 == 0,
        index == 0,
        supported,
        DefaultReasoningEffort::High,
    )
}

#[test]
fn bounded_text_facts_enforce_exact_utf8_byte_caps() {
    let identity = "i".repeat(PROTOCOL_IDENTITY_MAX_BYTES);
    assert_eq!(
        ProtocolIdentity::try_new(&identity).unwrap().as_str(),
        identity
    );
    assert!(matches!(
        ProtocolIdentity::try_new(&(identity + "i")),
        Err(BoundedResponseTextError::TooLong { .. })
    ));

    let display = "d".repeat(MODEL_DISPLAY_NAME_MAX_BYTES);
    assert_eq!(
        ModelDisplayName::try_new(&display).unwrap().as_str(),
        display
    );
    let cursor = "c".repeat(MODEL_CURSOR_MAX_BYTES);
    assert_eq!(ModelPageCursor::try_new(&cursor).unwrap().as_str(), cursor);
    assert!(matches!(
        ModelPageCursor::try_new(""),
        Err(BoundedResponseTextError::Empty)
    ));
}

#[test]
fn initialize_retains_only_the_product_token_and_closed_platform() {
    let initialize =
        InitializeResponse::try_new("beryl/0.146.0", InitializePlatform::HostWindows).unwrap();
    assert_eq!(initialize.user_agent_product(), "beryl/0.146.0");
    assert_eq!(initialize.platform(), InitializePlatform::HostWindows);
    initialize.validate_required_app_server_version().unwrap();
    assert_eq!(
        InitializePlatform::from_wire_pair("unix", "linux"),
        Some(InitializePlatform::WslLinux)
    );
    assert_eq!(InitializePlatform::from_wire_pair("unix", "macos"), None);
    assert!(matches!(
        InitializeResponse::try_new("beryl/0.146.0 trailing", InitializePlatform::HostWindows),
        Err(BoundedResponseTextError::InvalidUserAgentProduct)
    ));
    let wrong_product =
        InitializeResponse::try_new("codex-cli/0.146.0", InitializePlatform::HostWindows).unwrap();
    assert!(matches!(
        wrong_product.validate_required_app_server_version(),
        Err(CompatibilityError::AppServerVersionUnrecognized { .. })
    ));
    let wrong_version =
        InitializeResponse::try_new("beryl/0.145.0", InitializePlatform::HostWindows).unwrap();
    assert!(matches!(
        wrong_version.validate_required_app_server_version(),
        Err(CompatibilityError::AppServerVersionMismatch { .. })
    ));
}

#[test]
fn config_defaults_retain_only_optional_bounded_identities() {
    let defaults = BackendConfigDefaults::new(
        Some(ProtocolIdentity::try_new("gpt-model").unwrap()),
        Some(ProtocolIdentity::try_new("medium").unwrap()),
        true,
        true,
    );
    let response = ConfigReadResponse::new(defaults);
    assert_eq!(response.defaults().model(), Some("gpt-model"));
    assert_eq!(response.defaults().model_reasoning_effort(), Some("medium"));
    let defaults = response.into_defaults();
    assert_eq!(defaults.model(), Some("gpt-model"));
    assert!(defaults.proves_spawn_agent_model_overrides());
}

#[test]
fn reasoning_efforts_are_closed_facts_without_retained_custom_text() {
    let mut supported = SupportedReasoningEfforts::empty();
    assert!(supported.insert_wire("none"));
    assert!(supported.insert_wire("ultra"));
    assert!(!supported.insert_wire("focused"));
    assert!(supported.contains(ReasoningEffort::None));
    assert!(supported.contains(ReasoningEffort::Ultra));
    assert!(!supported.contains(ReasoningEffort::Medium));

    assert_eq!(
        DefaultReasoningEffort::from_wire("xhigh"),
        Some(DefaultReasoningEffort::XHigh)
    );
    assert_eq!(
        DefaultReasoningEffort::from_wire("focused"),
        Some(DefaultReasoningEffort::Other)
    );
    assert_eq!(DefaultReasoningEffort::from_wire(""), None);
}

#[test]
fn model_page_owns_exactly_sixty_four_fixed_slots_and_one_cursor() {
    let mut page = ModelPage::new();
    for index in 0..MODEL_PAGE_MAX_RECORDS {
        page.try_push(model_record(index)).unwrap();
    }
    let cursor = ModelPageCursor::try_new("64").unwrap();
    page.set_next_cursor(Some(cursor));

    assert_eq!(page.len(), MODEL_PAGE_MAX_RECORDS);
    assert_eq!(page.records().next().unwrap().id(), "id-0");
    assert_eq!(page.records().next_back().unwrap().model(), "model-63");
    assert_eq!(page.next_cursor(), Some("64"));
    assert_eq!(
        page.try_push(model_record(MODEL_PAGE_MAX_RECORDS))
            .unwrap_err()
            .maximum,
        MODEL_PAGE_MAX_RECORDS
    );
}

#[test]
fn model_page_result_indirection_keeps_enclosing_result_abis_compact() {
    const MAX_COMPACT_RESULT_BYTES: usize = 1_024;

    assert!(size_of::<ModelPage>() > MAX_COMPACT_RESULT_BYTES);
    assert_eq!(size_of::<Box<ModelPage>>(), size_of::<usize>());
    assert!(size_of::<CompatibilityProbeResult>() <= MAX_COMPACT_RESULT_BYTES);
    assert!(size_of::<BoundedResponseResult>() <= MAX_COMPACT_RESULT_BYTES);
}

#[test]
fn compatibility_probe_facts_use_one_exact_u16_set() {
    let mut facts = CompatibilityProbeSet::empty();
    for probe in CompatibilityProbe::ALL {
        assert!(!facts.contains(probe));
        facts.insert(probe);
        assert!(facts.contains(probe));
    }
    assert!(facts.is_complete());
    assert_eq!(facts.bits().count_ones(), 11);

    assert!(matches!(
        CompatibilityProbeResult::unexpected_mutating_success(CompatibilityProbe::ThreadRollback),
        Some(CompatibilityProbeResult::UnexpectedMutatingSuccess(
            CompatibilityProbe::ThreadRollback
        ))
    ));
    assert!(
        CompatibilityProbeResult::unexpected_mutating_success(CompatibilityProbe::ConfigRead)
            .is_none()
    );
}

#[test]
fn unsubscribe_status_is_closed_and_constructible_without_json_values() {
    let status = ThreadUnsubscribeStatus::from_wire("notLoaded").unwrap();
    let response = ThreadUnsubscribeResponse::new(status);
    assert_eq!(response.status, ThreadUnsubscribeStatus::NotLoaded);
    assert!(ThreadUnsubscribeStatus::from_wire("deleted").is_none());
}
