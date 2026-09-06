use syndic_storage::test_faults::awaiting_terminal_codec_tags;

#[test]
fn awaiting_terminal_values_use_their_exact_canonical_tags_and_round_trip() {
    let tags = awaiting_terminal_codec_tags().expect("awaiting-terminal codecs must round-trip");
    assert_eq!(tags.next_turn_reason(), 7);
    assert_eq!(tags.input_gate_state(), 7);
    assert_eq!(tags.route_target(), 4);
    assert_eq!(tags.lost_target(), 2);
}
