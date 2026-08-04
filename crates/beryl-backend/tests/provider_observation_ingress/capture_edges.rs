use beryl_backend::{
    ManagedBackendError, OrderedTurnStreamProgress, ProviderField, ProviderObservationControl,
    ProviderObservationError, ProviderObservationSchemaError, ProviderScalar, ProviderValueContext,
};

use super::support::{
    SinkOptions, agent_started, drive, drive_fragmented, fragments_for, lifecycle, mcp_item,
};

const ITEM_ID: ProviderValueContext = ProviderValueContext::Field(ProviderField::ItemId);
const AGENT_TEXT: ProviderValueContext =
    ProviderValueContext::Field(ProviderField::AgentMessageText);

#[test]
fn decoded_text_exchanges_short_page_suffixes_without_splitting_utf8() {
    for capacity in 4..=7 {
        for prefix in ["", "a", "ab", "abc"] {
            let expected = format!("{prefix}é🙂tail");
            let message = escaped_unicode(agent_started(&expected));
            let case = drive(message, SinkOptions::with_page_capacity(capacity));

            assert_eq!(case.outcome.unwrap(), OrderedTurnStreamProgress::Progress);
            assert_eq!(fragments_for(&case.trace, AGENT_TEXT), expected.as_bytes());
            assert_utf8_fragments(&case.trace.fragments);
            assert_eq!(case.pool.high_water, 1);
            assert_eq!(case.pool.leased, 0);
        }
    }
}

#[test]
fn fixed_item_identity_emission_uses_utf8_safe_page_cuts() {
    let item_id = "item_é🙂";
    let item = format!(
        "{{\"type\":\"agentMessage\",\"id\":{},\"text\":\"ok\"}}",
        serde_json::to_string(item_id).unwrap()
    );
    for capacity in 4..=7 {
        let case = drive(
            escaped_unicode(lifecycle(&item, false)),
            SinkOptions::with_page_capacity(capacity),
        );

        assert_eq!(case.outcome.unwrap(), OrderedTurnStreamProgress::Progress);
        assert_eq!(fragments_for(&case.trace, ITEM_ID), item_id.as_bytes());
        assert_utf8_fragments(&case.trace.fragments);
    }
}

#[test]
fn non_ascii_mcp_key_and_type_probes_preserve_valid_fragments() {
    let result = r#"{"content":[{"type":"té🙂xt","clé🙂":"väl🙂"}]}"#;
    let item = mcp_item("{}", Some(result));
    for capacity in 4..=7 {
        let case = drive(
            escaped_unicode(lifecycle(&item, true)),
            SinkOptions::with_page_capacity(capacity),
        );

        assert_eq!(case.outcome.unwrap(), OrderedTurnStreamProgress::Progress);
        assert_utf8_fragments(&case.trace.fragments);
        let emitted: Vec<u8> = case
            .trace
            .fragments
            .iter()
            .flat_map(|(_, bytes)| bytes.iter().copied())
            .collect();
        for expected in ["té🙂xt", "clé🙂", "väl🙂"] {
            assert!(
                emitted
                    .windows(expected.len())
                    .any(|window| window == expected.as_bytes()),
                "missing exact MCP text {expected:?} at capacity {capacity}"
            );
        }
    }
}

#[test]
fn structured_float_conversion_is_exact_across_input_splits() {
    let midpoint = "1.00000000000000011102230246251565404236316680908203125";
    let [subnormal_below, subnormal_tie, subnormal_above] = subnormal_midpoint_cases();
    assert_eq!(subnormal_below.parse::<f64>().unwrap().to_bits(), 0);
    assert_eq!(subnormal_tie.parse::<f64>().unwrap().to_bits(), 0);
    assert_eq!(subnormal_above.parse::<f64>().unwrap().to_bits(), 1);
    let spellings = vec![
        "0.12328035397235139830".to_string(),
        "0.12345678901234567890123456789012345678901234567890".to_string(),
        "123456789012345678901234567890123456789e-100".to_string(),
        format!("1.{}", "0".repeat(2_000)),
        midpoint.to_string(),
        format!("{midpoint}{}1", "0".repeat(2_000)),
        format!("-{midpoint}{}1", "0".repeat(2_000)),
        format!("1{}e-2000", "0".repeat(2_000)),
        format!("0.{}1e2001", "0".repeat(2_000)),
        format!("1e{}3", "0".repeat(2_000)),
        subnormal_below,
        subnormal_tie,
        subnormal_above,
    ];
    let item = mcp_item(&format!("[{}]", spellings.join(",")), None);
    let message = lifecycle(&item, true);

    for split in [1, 2, 3, 7, 31] {
        let case = drive_fragmented(message.clone(), SinkOptions::with_page_capacity(7), split);
        assert_eq!(case.outcome.unwrap(), OrderedTurnStreamProgress::Progress);
        let actual: Vec<u64> = case
            .trace
            .controls
            .iter()
            .filter_map(|control| match control {
                ProviderObservationControl::Scalar {
                    value: ProviderScalar::FiniteFloat(value),
                    ..
                } => Some(value.bits()),
                _ => None,
            })
            .collect();
        let expected: Vec<u64> = spellings
            .iter()
            .map(|spelling| spelling.parse::<f64>().unwrap().to_bits())
            .collect();
        assert_eq!(actual, expected, "split {split}");
    }
}

fn subnormal_midpoint_cases() -> [String; 3] {
    const NUMERATOR: &str = concat!(
        "24703282292062327208828439643411068618252990130716238221279284125033775363510437",
        "59326499181808179961898982823477228588654633283551779698981993873980053909390631",
        "50356595155702263922908583924491051844359318028499365361525003193704576782492193",
        "65623669863658480757001585769269903706311928279558551332927834338409351978015531",
        "24659726357957462276646527282722005637400648549997709659947045402082816622623785",
        "73934507363390079677619305775067401763246736009689513405355374585166611342237666",
        "78604162159680461914467291840300530057530849048765391711386591646239524912623653",
        "88187963623937328042389101867234849766823508986338858792562830275599565752445550",
        "72551893136908362547791869486679949683240497058210285131854513962138377228261454",
        "37693412532098591327667236328125",
    );
    let tie = format!("0.{}{NUMERATOR}", "0".repeat(323));
    let mut below_prefix = NUMERATOR.to_string();
    assert_eq!(below_prefix.pop(), Some('5'));
    below_prefix.push('4');
    let below = format!("0.{}{}{}", "0".repeat(323), below_prefix, "9".repeat(1_000));
    let above = format!("{tie}{}1", "0".repeat(1_000));
    [below, tie, above]
}

#[test]
fn nonfinite_structured_numbers_fail_closed_without_a_lexical_cap() {
    let numbers = ["1e999".to_string(), format!("1e{}", "9".repeat(2_000))];
    for number in numbers {
        let case = drive(
            lifecycle(&mcp_item(&format!("[{number}]"), None), true),
            SinkOptions::with_page_capacity(7),
        );
        match case.outcome.unwrap_err() {
            ManagedBackendError::ProviderObservation {
                source: ProviderObservationError::Schema(ProviderObservationSchemaError::WrongType),
                ..
            } => {}
            other => panic!("expected fail-closed structured number, got {other:?}"),
        }
        assert_eq!(case.trace.leased_at_abandon, [0]);
        assert_eq!(case.pool.leased, 0);
    }
}

#[test]
fn structured_integer_spellings_beyond_native_ranges_fall_back_to_finite_f64() {
    let spellings = ["18446744073709551616", "-9223372036854775809"];
    let case = drive(
        lifecycle(&mcp_item(&format!("[{}]", spellings.join(",")), None), true),
        SinkOptions::with_page_capacity(7),
    );
    assert_eq!(case.outcome.unwrap(), OrderedTurnStreamProgress::Progress);
    let actual: Vec<u64> = case
        .trace
        .controls
        .iter()
        .filter_map(|control| match control {
            ProviderObservationControl::Scalar {
                value: ProviderScalar::FiniteFloat(value),
                ..
            } => Some(value.bits()),
            _ => None,
        })
        .collect();
    let expected: Vec<u64> = spellings
        .iter()
        .map(|spelling| spelling.parse::<f64>().unwrap().to_bits())
        .collect();
    assert_eq!(actual, expected);
}

fn escaped_unicode(message: String) -> String {
    message
        .replace('é', "\\u00e9")
        .replace('🙂', "\\ud83d\\ude42")
}

fn assert_utf8_fragments(fragments: &[(ProviderValueContext, Vec<u8>)]) {
    for (_, fragment) in fragments {
        assert!(
            std::str::from_utf8(fragment).is_ok(),
            "provider fragment split one UTF-8 scalar: {fragment:?}"
        );
    }
}
