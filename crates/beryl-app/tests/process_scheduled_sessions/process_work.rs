use super::*;
use beryl_app::{
    LifecycleYieldOutcome,
    cas_projection::{
        ProcessWorkPageLimits, ProjectionCancellationToken, ScheduledOrdinaryAdmissionResult,
    },
    lifecycle_attention::{LifecycleAttentionAdmission, ProcessLifecycleAttentionPool},
};
use beryl_model::SyndicTurnId;

fn page(
    fixture: &syndic::Fixture,
    sessions: &ScheduledExecutionSessions,
    attention: &ProcessLifecycleAttentionPool,
) -> beryl_app::cas_projection::ProcessWorkPage {
    let inventory = fixture.store.process_work_inventory(sessions, attention);
    let revision = inventory.revision().unwrap();
    inventory
        .page(
            &revision,
            None,
            ProcessWorkPageLimits::new(256, 65_536).unwrap(),
            &ProjectionCancellationToken::new(),
        )
        .unwrap()
}

#[test]
fn inventory_available_session_is_idle_and_checkout_attention_share_one_row() {
    let (fixture, sessions) = fixture(221);
    let attention = ProcessLifecycleAttentionPool::new();
    let server = NormalTerminalServer::spawn_admission_only();
    let registration = install_session(
        &fixture,
        &sessions,
        fixture.thread,
        server.endpoint(),
        74_001,
    );
    server.wait_for_admission();
    assert_eq!(page(&fixture, &sessions, &attention).total_threads(), 0);
    let lease = match fixture
        .store
        .checkout_scheduled_session_for_test(fixture.thread, syndic::execution_binding())
        .unwrap()
    {
        ScheduledOrdinaryAdmissionResult::Issued(lease) => lease,
        _ => panic!("available session did not check out"),
    };
    let executing = page(&fixture, &sessions, &attention);
    assert_eq!(executing.total_threads(), 1);
    assert!(executing.records()[0].facts.executing);
    let attempt = attention
        .track_accepted_yield(
            fixture.home().home_id(),
            fixture.thread,
            SyndicTurnId::from_bytes([1; 16]),
            LifecycleYieldOutcome::PhaseNeedsReview,
        )
        .unwrap();
    assert!(matches!(
        attention.report_terminal(&attempt),
        LifecycleAttentionAdmission::Admitted(_)
    ));
    let combined = page(&fixture, &sessions, &attention);
    assert_eq!(combined.total_threads(), 1);
    assert!(combined.records()[0].facts.executing);
    assert_eq!(combined.records()[0].attention.len(), 1);
    drop(lease);
    let attention_only = page(&fixture, &sessions, &attention);
    assert_eq!(attention_only.total_threads(), 1);
    assert!(!attention_only.records()[0].facts.executing);
    assert!(attention.acknowledge(attention_only.records()[0].attention[0].token()));
    assert_eq!(page(&fixture, &sessions, &attention).total_threads(), 0);
    assert!(sessions.retire(registration));
    wait_until("inventory session retirement", || {
        (sessions.diagnostics().retained == 0).then_some(())
    });
    let (directory, service) = fixture.into_service();
    let _ = service.close().unwrap();
    server.join();
    drop(directory);
}

#[test]
fn inventory_merges_durable_queues_pending_and_attention_with_current_unpublished_title() {
    let (mut fixture, sessions) = fixture(223);
    let attention = ProcessLifecycleAttentionPool::new();
    let queued = seed_runtime_next_input_without_wake(&mut fixture, 223);
    let pending = fixture.create_ordinary_pending(225, " inventory pending title");
    let idle = fixture.create_ordinary(227);
    let attempt = attention
        .track_accepted_yield(
            fixture.home().home_id(),
            pending,
            SyndicTurnId::from_bytes([1; 16]),
            LifecycleYieldOutcome::PhaseNeedsReview,
        )
        .unwrap();
    assert!(matches!(
        attention.report_terminal(&attempt),
        LifecycleAttentionAdmission::Admitted(_)
    ));
    let before = fixture.home().home_revision().unwrap();
    assert!(matches!(
        fixture
            .storage
            .prepare_thread_catalog_summary(&fixture.home(), pending)
            .unwrap(),
        Some(syndic_storage::ThreadCatalogSummaryPreparation::PreparedReplacement(_))
    ));
    let observed = page(&fixture, &sessions, &attention);
    assert_eq!(observed.total_threads(), 2);
    assert!(observed.records().iter().all(|row| row.thread_id != idle));
    assert!(
        observed
            .records()
            .iter()
            .find(|row| row.thread_id == queued.thread)
            .unwrap()
            .facts
            .queued
    );
    let pending_row = observed
        .records()
        .iter()
        .find(|row| row.thread_id == pending)
        .unwrap();
    assert!(pending_row.facts.pending);
    assert_eq!(pending_row.attention.len(), 1);
    assert!(
        pending_row
            .title
            .as_ref()
            .unwrap()
            .text()
            .contains("inventory pending title")
    );
    assert_eq!(fixture.home().home_revision().unwrap(), before);
    for row in observed.records() {
        let home = fixture.home();
        let canonical = fixture
            .storage
            .prepare_thread_catalog_summary(&home, row.thread_id)
            .unwrap()
            .unwrap();
        let summary = match &canonical {
            syndic_storage::ThreadCatalogSummaryPreparation::ExactCurrent(current) => {
                current.summary()
            }
            syndic_storage::ThreadCatalogSummaryPreparation::PreparedReplacement(prepared) => {
                prepared.replacement()
            }
        };
        assert_eq!(row.title.as_ref(), summary.title());
        assert_eq!(&row.execution, summary.execution());
        assert_eq!(row.last_activity_at, summary.last_activity_at());
    }
    assert_eq!(fixture.home().home_revision().unwrap(), before);
    let (directory, service) = fixture.into_service();
    let _ = service.close().unwrap();
    drop(directory);
}
