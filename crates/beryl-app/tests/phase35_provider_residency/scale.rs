use beryl_app::cas_projection::test_faults::provider_broker_snapshot;

use super::{
    fixture::{LiveHarness, assert_broker_idle_and_bounded, ingress_snapshot},
    server::ObservationSpec,
};

const MEDIUM_PATTERNS: u64 = 80_000;
const VERY_LARGE_PATTERNS: u64 = 800_000;
const SEQUENTIAL_OBSERVATIONS: u64 = 32;

pub(super) fn prove_scale_repetition_and_release() {
    let harness = LiveHarness::new(101);
    let medium = ObservationSpec::new(1, MEDIUM_PATTERNS);
    let medium_report = harness.send(medium, 1);
    let medium_ingress = ingress_snapshot(&harness, medium_report);
    harness.wait_for_broker_idle();
    let medium_broker = provider_broker_snapshot(harness.session());
    assert_broker_idle_and_bounded(medium_broker);
    let saturated_pages = harness.session().provider_page_diagnostics();
    assert_eq!(saturated_pages.high_water, 1);
    assert_eq!(saturated_pages.leased, 0);

    let very_large = ObservationSpec::new(2, VERY_LARGE_PATTERNS);
    let large_report = harness.send(very_large, 2);
    let large_ingress = ingress_snapshot(&harness, large_report);
    harness.wait_for_broker_idle();
    let large_broker = provider_broker_snapshot(harness.session());
    assert_broker_idle_and_bounded(large_broker);
    assert_eq!(
        large_ingress.maximum_transport_chunk_bytes(),
        medium_ingress.maximum_transport_chunk_bytes()
    );
    assert_eq!(
        large_ingress.maximum_parser_buffer_bytes(),
        medium_ingress.maximum_parser_buffer_bytes()
    );
    assert_eq!(
        large_broker.in_flight().high_water(),
        medium_broker.in_flight().high_water()
    );
    assert_eq!(
        large_broker.staged_fragments().high_water(),
        medium_broker.staged_fragments().high_water()
    );
    assert!(large_broker.submitted() > medium_broker.submitted());
    assert!(large_broker.staged_fragment_batches() > medium_broker.staged_fragment_batches());
    assert_eq!(
        harness.session().provider_page_diagnostics().high_water,
        saturated_pages.high_water
    );

    for sequence in 3..3 + SEQUENTIAL_OBSERVATIONS {
        let _ = harness.send(ObservationSpec::new(sequence, 2_000), sequence);
    }
    let final_frontier = 2 + SEQUENTIAL_OBSERVATIONS;
    harness.wait_for_broker_idle();
    let final_broker = provider_broker_snapshot(harness.session());
    assert_broker_idle_and_bounded(final_broker);
    assert_eq!(
        final_broker.in_flight().high_water(),
        medium_broker.in_flight().high_water()
    );
    assert_eq!(
        final_broker.staged_fragments().high_water(),
        medium_broker.staged_fragments().high_water()
    );
    assert!(final_broker.submitted() > large_broker.submitted());
    assert!(final_broker.staged_fragment_batches() > large_broker.staged_fragment_batches());
    harness.assert_frontier(final_frontier);
    harness.assert_digest(medium);
    harness.assert_digest(very_large);
    harness.assert_digest(ObservationSpec::new(final_frontier, 2_000));
    harness.close();
}
