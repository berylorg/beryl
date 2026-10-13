#[allow(dead_code)]
#[path = "../src/reconciliation.rs"]
mod reconciliation;

use beryl_home_store::DomainReconciliation;
use reconciliation::ReconciliationClassification;

fn classify(observations: &[(bool, bool)]) -> DomainReconciliation {
    let mut classification = ReconciliationClassification::new();
    for &(old, new) in observations {
        classification.observe_matches(old, new);
    }
    classification.finish()
}

#[test]
fn matching_unchanged_constraints_preserve_each_changed_side_in_every_order() {
    for (changed, expected) in [
        ((true, false), DomainReconciliation::ExactOld),
        ((false, true), DomainReconciliation::ExactNew),
    ] {
        for observations in [
            vec![changed, (true, true), (true, true)],
            vec![(true, true), changed, (true, true)],
            vec![(true, true), (true, true), changed],
        ] {
            assert_eq!(classify(&observations), expected);
        }
    }
}

#[test]
fn only_nonempty_matching_equal_facts_report_unchanged() {
    assert_eq!(classify(&[]), DomainReconciliation::Collision);
    for count in 1..=4 {
        assert_eq!(
            classify(&vec![(true, true); count]),
            DomainReconciliation::ExactUnchanged,
        );
    }
}

#[test]
fn mismatch_and_incompatible_changed_facts_remain_monotonic_collisions() {
    for observations in [
        vec![(false, false)],
        vec![(true, true), (false, false)],
        vec![(false, false), (true, true), (false, true)],
        vec![(true, false), (false, true)],
        vec![(false, true), (true, true), (true, false)],
        vec![(true, false), (false, false), (true, true)],
    ] {
        assert_eq!(classify(&observations), DomainReconciliation::Collision);
    }
}
