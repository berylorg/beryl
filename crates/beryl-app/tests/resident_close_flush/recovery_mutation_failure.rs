use super::support::{self, drive, drive_until};
use beryl_app::{
    composer_host::ComposerHostMutationAdmissionFailure,
    main_window::{
        MainWindowConversationComposerCloseAdvance as Advance,
        MainWindowConversationComposerCloseTicket as Ticket, MainWindowConversationComposerService,
    },
};
use gpui::TestAppContext;
use std::{fmt, sync::Arc};

struct RetainedServiceError(Arc<MainWindowConversationComposerService>);

impl fmt::Debug for RetainedServiceError {
    fn fmt(&self, _: &mut fmt::Formatter<'_>) -> fmt::Result {
        panic!("recovery must not inspect an opaque failure source");
    }
}

impl fmt::Display for RetainedServiceError {
    fn fmt(&self, _: &mut fmt::Formatter<'_>) -> fmt::Result {
        panic!("recovery must not format an opaque failure source");
    }
}

impl std::error::Error for RetainedServiceError {}

#[gpui::test]
fn recovery_transfers_opaque_failure_custody_once_without_changing_the_editor(
    cx: &mut TestAppContext,
) {
    let (fixture, cx) = support::mounted(cx, "resident-failure-detachment", 183);
    let composer = fixture
        .mount
        .read_with(cx, |mount, _| mount.contribution())
        .unwrap();
    let input = composer.read_with(cx, |composer, _| composer.gpui_input());
    let failure = Arc::new(ComposerHostMutationAdmissionFailure::Storage(
        syndic_storage::DraftMarkerAdmissionStorageErrorV1::Command(
            beryl_home_store::CommandError::ContributorValidation {
                domain: "retained-test-domain",
                source: Box::new(RetainedServiceError(fixture.service.clone())),
            },
        ),
    ));
    composer.update(cx, |composer, _| {
        composer.test_set_mutation_admission_failure(failure.clone());
    });
    let close = cx.update(|window, app| {
        fixture
            .mount
            .update(app, |mount, cx| mount.begin_window_close(window, cx))
            .unwrap()
    });
    assert!(
        composer
            .update(cx, |composer, cx| composer
                .detach_recovery_mutation_failure(close.ticket, cx))
            .is_err()
    );
    drive_until(cx, "failure close becomes ready", |cx| {
        cx.update(|window, app| {
            fixture.mount.update(app, |mount, cx| {
                mount.advance_window_close(close.ticket, window, cx)
            })
        })
        .unwrap()
            == Advance::Ready
    });
    drive_until(cx, "failure editor settles", |cx| {
        input.read_with(cx, |input, _| input.is_quiescent())
    });
    assert!(
        composer
            .update(cx, |composer, cx| composer
                .detach_recovery_mutation_failure(close.ticket, cx))
            .is_err()
    );
    assert_eq!(Arc::strong_count(&failure), 2);
    composer
        .update(cx, |composer, cx| {
            composer.test_set_shutdown_interaction_gated(true, cx)
        })
        .unwrap();
    assert!(
        fixture
            .mount
            .update(cx, |mount, cx| mount
                .fence_interrupted_exit_resident(close.ticket, cx))
            .unwrap()
    );
    let selection = composer.read_with(cx, |composer, _| composer.selection_identity());
    let stale = Ticket::for_test(fixture.mount.entity_id(), 99, selection);
    let before = composer.read_with(cx, |composer, _| {
        composer.recovery_snapshot().unwrap().restoration().clone()
    });
    let feedback = composer.read_with(cx, |composer, _| composer.mutation_feedback());
    let retained = fixture.service.test_with_close_slot_locked(|| {
        composer.update(cx, |resident, cx| {
            assert!(
                resident
                    .detach_recovery_mutation_failure(stale, cx)
                    .is_err()
            );
            assert!(resident.last_mutation_admission_failure().is_some());
            let retained = resident
                .detach_recovery_mutation_failure(close.ticket, cx)
                .unwrap()
                .unwrap();
            assert!(resident.last_mutation_admission_failure().is_none());
            assert!(
                resident
                    .detach_recovery_mutation_failure(close.ticket, cx)
                    .unwrap()
                    .is_none()
            );
            assert!(
                resident
                    .detach_recovery_mutation_failure(stale, cx)
                    .is_err()
            );
            retained
        })
    });
    assert!(Arc::ptr_eq(&retained, &failure));
    assert_eq!(Arc::strong_count(&failure), 2);
    let ComposerHostMutationAdmissionFailure::Storage(
        syndic_storage::DraftMarkerAdmissionStorageErrorV1::Command(
            beryl_home_store::CommandError::ContributorValidation { source, .. },
        ),
    ) = retained.as_ref()
    else {
        panic!("failure custody changed")
    };
    assert!(Arc::ptr_eq(
        &source.downcast_ref::<RetainedServiceError>().unwrap().0,
        &fixture.service
    ));
    drop(failure);
    let service_references = Arc::strong_count(&fixture.service);
    drop(retained);
    assert_eq!(Arc::strong_count(&fixture.service), service_references - 1);
    cx.simulate_keystrokes("x ctrl-z ctrl-v");
    drive(cx, 12);
    composer.read_with(cx, |composer, _| {
        assert!(composer.last_mutation_admission_failure().is_none());
        assert_eq!(composer.mutation_feedback(), feedback);
        assert_eq!(composer.recovery_snapshot().unwrap().restoration(), &before);
        assert_eq!(composer.gpui_input().entity_id(), input.entity_id());
        assert!(!composer.test_widget_released());
    });
    assert_eq!(
        input
            .update(cx, |input, _| input.export_restoration(Some(
                selection.binding().range_history_frontier()
            )))
            .unwrap(),
        before
    );
    assert!(!input.read_with(cx, |input, _| input.is_enabled()));
    assert!(fixture.service.test_window_close_is_current(close.ticket));
    drop((input, composer));
    support::finish(fixture, cx);
}
