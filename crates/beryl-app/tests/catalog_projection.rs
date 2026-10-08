use beryl_app::catalog_projection::{
    CatalogProjectionBuildError, ThreadCatalogProjectionPreparation,
    prepare_thread_catalog_projection,
};
mod catalog_projection_support;
use beryl_home_store::{CommandOutcome, HomeCommand};
use beryl_model::SyndicThreadId;
use beryl_state::{CatalogClaimKind, CatalogPointReadLimit, CatalogTitleSource};
use catalog_projection_support::*;
#[test]
fn projection_publishes_once_then_converges_to_an_exact_no_op() {
    let fixture = Fixture::new(r"C:\Work\Beryl");
    let window_id = fixture.claim_thread();
    let missing = prepare_thread_catalog_projection(
        &fixture.store,
        &fixture.syndic,
        &fixture.state,
        SyndicThreadId::from_bytes([9; 16]),
    )
    .expect("prepare missing thread");
    assert!(matches!(
        missing,
        ThreadCatalogProjectionPreparation::ThreadMissing
    ));

    let command = match prepare_thread_catalog_projection(
        &fixture.store,
        &fixture.syndic,
        &fixture.state,
        fixture.thread_id,
    )
    .expect("prepare initial projection")
    {
        ThreadCatalogProjectionPreparation::Publish(command) => command,
        ThreadCatalogProjectionPreparation::ThreadMissing => panic!("thread unexpectedly missing"),
        ThreadCatalogProjectionPreparation::ExactCurrent => {
            panic!("catalog unexpectedly current before its first publication")
        }
    };
    match fixture.store.execute(command) {
        CommandOutcome::Committed {
            later_failure: None,
            ..
        } => {}
        CommandOutcome::NotCommitted { evidence } => {
            panic!("publish catalog row unexpectedly not committed: {evidence:?}")
        }
        outcome @ CommandOutcome::Committed {
            later_failure: Some(_),
            ..
        } => panic!("publish catalog row committed with later failure: {outcome:?}"),
        outcome @ CommandOutcome::Indeterminate { .. } => {
            panic!("publish catalog row indeterminate: {outcome:?}")
        }
    }

    let row = fixture
        .state
        .catalog()
        .row(
            &fixture.store,
            fixture.thread_id,
            CatalogPointReadLimit::schema_maximum(),
        )
        .expect("read catalog row")
        .expect("published catalog row");
    assert_eq!(row.title_source(), CatalogTitleSource::Absent);
    assert_eq!(row.facts().execution().runtime_id(), fixture.runtime_id);
    assert_eq!(row.facts().execution().root_id(), fixture.root_id);
    assert_eq!(row.facts().search().title(), "");
    assert_eq!(row.facts().search().full_root_path(), r"c:\work\beryl");
    assert!(row.facts().complete());
    assert_eq!(row.facts().claim().window_id(), Some(window_id));
    assert_eq!(row.facts().claim().kind(), Some(CatalogClaimKind::Active));
    assert!(row.sources().claim().is_some());
    let exact = prepare_thread_catalog_projection(
        &fixture.store,
        &fixture.syndic,
        &fixture.state,
        fixture.thread_id,
    )
    .expect("prepare current projection");
    assert!(matches!(
        exact,
        ThreadCatalogProjectionPreparation::ExactCurrent
    ));
}

#[test]
fn projection_rejects_a_syndic_binding_that_disagrees_with_the_root_authority() {
    let fixture = Fixture::new(r"C:\Work\Elsewhere");
    let error = prepare_thread_catalog_projection(
        &fixture.store,
        &fixture.syndic,
        &fixture.state,
        fixture.thread_id,
    )
    .err()
    .expect("mismatched binding must fail");
    assert!(matches!(
        error,
        CatalogProjectionBuildError::ExecutionBindingMismatch
    ));
}
