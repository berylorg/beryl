use super::support::recovery_resources::transfer_resources;
use gpui::TestAppContext;

#[gpui::test]
fn recovery_transfers_all_adapters_together_after_exact_drain(cx: &mut TestAppContext) {
    transfer_resources(cx, false);
}

#[gpui::test]
fn recovery_bundle_preserves_prior_individual_handoff_custody(cx: &mut TestAppContext) {
    transfer_resources(cx, true);
}
