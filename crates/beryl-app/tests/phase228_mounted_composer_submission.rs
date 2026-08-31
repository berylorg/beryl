#![cfg(feature = "test-faults")]

#[path = "phase177_main_window_composer_slot/support.rs"]
mod support;

use std::{sync::Arc, time::Duration};

use beryl_app::{
    cas_projection::ProjectionServiceConfig,
    composer_host::{ComposerHostAutosaveInterval, SyndicComposerHost},
    main_window::{
        MainWindowComposerMarkerMetadataAuthority, MainWindowComposerSlot,
        MainWindowComposerSubmissionRequestSource, MainWindowComposerSubmissionTestAdvance,
        MainWindowConversationComposerAutosavePhase, MainWindowConversationComposerConfig,
        MainWindowConversationComposerMount, MainWindowConversationComposerService,
        MainWindowConversationComposerSubmissionStatus,
    },
};
use beryl_home_store::{
    CommandCancellation, FreeSpaceOutcome, MinimumTurnCaptureReserve,
    test_faults::FreeSpaceTestObservation,
};
use gpui::{
    AppContext, Entity, EntityInputHandler, IntoElement, ParentElement, Render, SharedString,
    StreamingLayoutBinding, StreamingLayoutLimits, StreamingLayoutPosition, TextRun, black, div,
    font, px,
};
use gpui_scrollbar::ScrollbarStyle;
use gpui_text_input::{
    ClipboardLimits, ExactGeometryLimits, MutationLimits, ObjectResidencyLimits,
    PresentationGeneration, RangeSettlementCoordinator, RangeTextInputConfig, RangeTextInputLimits,
    ResidencyLimits, SegmentationLimits, StreamingGeometryStyle, StreamingOversizePresentation,
    TextInputAtomClipboardPolicy, TextInputEnterKey, TextInputRichPastePolicy, TextInputTheme,
    ensure_text_input_bindings,
};
use syndic_storage::SyndicPointReadLimit;

use support::{Fixture, activation};

struct MountRoot {
    mount: Option<Entity<MainWindowConversationComposerMount>>,
}

struct MountedSubmissionFixture {
    _directory: tempfile::TempDir,
    root: Entity<MountRoot>,
    mount: Entity<MainWindowConversationComposerMount>,
    service: Arc<MainWindowConversationComposerService>,
}

impl Render for MountRoot {
    fn render(
        &mut self,
        _window: &mut gpui::Window,
        cx: &mut gpui::Context<Self>,
    ) -> impl IntoElement {
        div().children(
            self.mount
                .as_ref()
                .and_then(|mount| mount.read(cx).contribution()),
        )
    }
}

#[gpui::test]
fn mounted_enter_flushes_the_dirty_binding_once_then_opens_the_editable_successor(
    cx: &mut gpui::TestAppContext,
) {
    cx.update(ensure_text_input_bindings);
    let fixture = Fixture::new("phase228-mounted-success", 228);
    let claim = fixture.claims().0;
    let window_id = fixture.window_id;
    let thread = fixture.selected_thread;
    let marker_authority = MainWindowComposerMarkerMetadataAuthority::new(fixture.assets());
    let marker_seals = fixture.marker_seals();
    let (_directory, store, storage) = fixture.into_store();
    let mut host = SyndicComposerHost::new(storage.clone());
    assert!(
        host.test_activate(
            &store,
            activation(thread, 1, 2, 1),
            &CommandCancellation::new(),
        )
        .is_ok()
    );
    let slot =
        MainWindowComposerSlot::new(window_id, claim, host, storage.clone(), marker_authority)
            .unwrap();
    let store = Arc::new(store);
    let service = Arc::new(MainWindowConversationComposerService::new(
        store.clone(),
        slot,
    ));
    let mounted_service = service.clone();
    let (root, cx) = cx.add_window_view(|window, cx| {
        let mount = cx.new(|mount_cx| {
            MainWindowConversationComposerMount::new(
                mounted_service,
                Box::new(configure),
                marker_seals,
                submission_source(),
                window,
                mount_cx,
            )
            .unwrap()
        });
        MountRoot { mount: Some(mount) }
    });
    drive(cx, 16);

    let mount = root.read_with(cx, |root, _| root.mount.clone().unwrap());
    let predecessor = mount
        .read_with(cx, |mount, _| mount.contribution())
        .unwrap();
    let predecessor_id = predecessor.entity_id();
    let pristine = service.selected_identity().unwrap();
    let pristine_published = storage
        .current_draft(&store, thread, SyndicPointReadLimit::new(65_536).unwrap())
        .unwrap()
        .unwrap()
        .draft()
        .root_history();
    let input = predecessor.read_with(cx, |composer, _| composer.gpui_input());
    cx.update(|window, app| {
        input.update(app, |input, input_cx| {
            input.focus(window);
            input.replace_and_mark_text_in_range(None, "mounted submit", None, window, input_cx)
        })
    });
    drive_until(cx, 256, "dirty editor binding", |_| {
        service
            .selected_identity()
            .is_some_and(|selection| selection.binding() != pristine.binding())
    });
    let dirty = service.selected_identity().unwrap();
    assert_ne!(dirty.binding(), pristine.binding());
    let durable_before_enter = storage
        .current_draft(&store, thread, SyndicPointReadLimit::new(65_536).unwrap())
        .unwrap()
        .unwrap()
        .draft()
        .root_history();
    assert_eq!(durable_before_enter.root(), pristine_published.root());
    assert_eq!(durable_before_enter.history(), pristine_published.history());
    assert!(
        durable_before_enter.root() != dirty.binding().root()
            || durable_before_enter.history() != dirty.binding().history(),
        "the dirty candidate must remain unflushed until Enter starts submission"
    );

    cx.simulate_keystrokes("enter");
    cx.simulate_keystrokes("enter");
    let mut submitted = false;
    for _ in 0..2_048 {
        if service.selected_identity().is_some_and(|selection| {
            selection.binding().presentation_generation().get() == 2
                && mount.read_with(cx, |mount, _| mount.submission_status())
                    == MainWindowConversationComposerSubmissionStatus::Idle
        }) {
            submitted = true;
            break;
        }
        drive(cx, 1);
    }
    let submission_status = mount.read_with(cx, |mount, _| mount.submission_status());
    assert!(
        submitted,
        "mounted dirty immediate Enter did not settle; status was {submission_status:?}"
    );

    let successor = service.selected_identity().unwrap();
    assert_eq!(successor.claim(), claim);
    assert_eq!(successor.binding().presentation_generation().get(), 2);
    let successor_contribution = mount
        .read_with(cx, |mount, _| mount.contribution())
        .unwrap();
    assert_ne!(successor_contribution.entity_id(), predecessor_id);
    assert_eq!(
        storage
            .current_draft(&store, thread, SyndicPointReadLimit::new(65_536).unwrap())
            .unwrap()
            .unwrap()
            .draft()
            .id(),
        successor.binding().candidate().draft_id()
    );

    let successor_input = successor_contribution.read_with(cx, |composer, _| composer.gpui_input());
    cx.update(|window, app| successor_input.update(app, |input, _| input.focus(window)));
    let successor_before_newline = successor;
    cx.simulate_keystrokes("shift-enter");
    drive_until(cx, 256, "mounted Shift+Enter newline", |_| {
        service.selected_identity().is_some_and(|selection| {
            selection.binding().logical_extent().logical_utf8_bytes()
                == successor_before_newline
                    .binding()
                    .logical_extent()
                    .logical_utf8_bytes()
                    + 1
        })
    });
    assert_eq!(
        mount.read_with(cx, |mount, _| mount.submission_status()),
        MainWindowConversationComposerSubmissionStatus::Idle
    );
}

#[gpui::test]
fn mounted_direct_admission_denial_preserves_the_coherent_draft_and_releases_the_ticket(
    cx: &mut gpui::TestAppContext,
) {
    cx.update(ensure_text_input_bindings);
    let fixture = Fixture::new("phase228-mounted-denial", 229);
    let claim = fixture.claims().0;
    let window_id = fixture.window_id;
    let thread = fixture.selected_thread;
    let faults = fixture.faults.clone();
    let marker_authority = MainWindowComposerMarkerMetadataAuthority::new(fixture.assets());
    let marker_seals = fixture.marker_seals();
    let (_directory, store, storage) = fixture.into_store();
    let mut host = SyndicComposerHost::new(storage.clone());
    assert!(
        host.test_activate(
            &store,
            activation(thread, 3, 4, 1),
            &CommandCancellation::new(),
        )
        .is_ok()
    );
    let slot =
        MainWindowComposerSlot::new(window_id, claim, host, storage, marker_authority).unwrap();
    let service = Arc::new(MainWindowConversationComposerService::new(
        Arc::new(store),
        slot,
    ));
    let mounted_service = service.clone();
    let (root, cx) = cx.add_window_view(|window, cx| {
        let mount = cx.new(|mount_cx| {
            MainWindowConversationComposerMount::new(
                mounted_service,
                Box::new(configure),
                marker_seals,
                submission_source(),
                window,
                mount_cx,
            )
            .unwrap()
        });
        MountRoot { mount: Some(mount) }
    });
    drive(cx, 16);

    let mount = root.read_with(cx, |root, _| root.mount.clone().unwrap());
    let contribution = mount
        .read_with(cx, |mount, _| mount.contribution())
        .unwrap();
    let contribution_id = contribution.entity_id();
    let input = contribution.read_with(cx, |composer, _| composer.gpui_input());
    cx.update(|window, app| {
        input.update(app, |input, input_cx| {
            input.focus(window);
            input.replace_and_mark_text_in_range(None, "denied", None, window, input_cx)
        })
    });
    drive(cx, 32);
    cx.update(|window, app| {
        mount.update(app, |mount, mount_cx| {
            mount.publish_autosave_interval(
                1,
                ComposerHostAutosaveInterval::new(5).unwrap(),
                window,
                mount_cx,
            )
        })
    })
    .unwrap();
    cx.executor().advance_clock(Duration::from_secs(5));
    drive_until(cx, 512, "dirty denial autosave", |cx| {
        mount.read_with(cx, |mount, _| mount.autosave_diagnostics().phase())
            == MainWindowConversationComposerAutosavePhase::Idle
    });
    let retained = service.selected_identity().unwrap();
    faults.push_free_space_observation(FreeSpaceTestObservation::Observed {
        available_bytes: 0,
        total_free_bytes: 0,
        total_bytes: u64::MAX,
    });

    cx.simulate_keystrokes("enter");
    drive_until(cx, 2_048, "direct admission observation", |_| {
        faults.free_space_observation_count() == 1
    });
    for _ in 0..2_048 {
        if matches!(
            mount.read_with(cx, |mount, _| mount.submission_status()),
            MainWindowConversationComposerSubmissionStatus::DirectAdmissionDenied(
                FreeSpaceOutcome::BelowReserve { .. }
            )
        ) {
            break;
        }
        drive(cx, 1);
    }
    let denial_status = mount.read_with(cx, |mount, _| mount.submission_status());
    assert!(
        matches!(
            denial_status,
            MainWindowConversationComposerSubmissionStatus::DirectAdmissionDenied(
                FreeSpaceOutcome::BelowReserve { .. }
            )
        ),
        "mounted direct-admission result was {denial_status:?}"
    );
    assert_eq!(faults.free_space_observation_count(), 1);
    assert_eq!(service.selected_identity(), Some(retained));
    assert_eq!(
        mount
            .read_with(cx, |mount, _| mount.contribution())
            .unwrap()
            .entity_id(),
        contribution_id
    );
    assert!(input.read_with(cx, |input, _| input.is_surface_current_and_interactive()));
}

#[gpui::test]
fn mounted_empty_submission_preserves_the_coherent_editor_without_retaining_work(
    cx: &mut gpui::TestAppContext,
) {
    cx.update(ensure_text_input_bindings);
    let fixture = Fixture::new("phase228-mounted-empty", 230);
    let claim = fixture.claims().0;
    let window_id = fixture.window_id;
    let thread = fixture.selected_thread;
    let marker_authority = MainWindowComposerMarkerMetadataAuthority::new(fixture.assets());
    let marker_seals = fixture.marker_seals();
    let (_directory, store, storage) = fixture.into_store();
    let mut host = SyndicComposerHost::new(storage.clone());
    assert!(
        host.test_activate(
            &store,
            activation(thread, 5, 6, 1),
            &CommandCancellation::new(),
        )
        .is_ok()
    );
    let slot =
        MainWindowComposerSlot::new(window_id, claim, host, storage, marker_authority).unwrap();
    let service = Arc::new(MainWindowConversationComposerService::new(
        Arc::new(store),
        slot,
    ));
    let mounted_service = service.clone();
    let (root, cx) = cx.add_window_view(|window, cx| {
        let mount = cx.new(|mount_cx| {
            MainWindowConversationComposerMount::new(
                mounted_service,
                Box::new(configure),
                marker_seals,
                submission_source(),
                window,
                mount_cx,
            )
            .unwrap()
        });
        MountRoot { mount: Some(mount) }
    });
    drive(cx, 16);

    let mount = root.read_with(cx, |root, _| root.mount.clone().unwrap());
    let contribution = mount
        .read_with(cx, |mount, _| mount.contribution())
        .unwrap();
    let contribution_id = contribution.entity_id();
    let retained = service.selected_identity().unwrap();
    let input = contribution.read_with(cx, |composer, _| composer.gpui_input());
    cx.update(|window, app| input.update(app, |input, _| input.focus(window)));

    cx.simulate_keystrokes("enter");
    drive_until(cx, 512, "empty mounted submission", |cx| {
        mount.read_with(cx, |mount, _| mount.submission_status())
            == MainWindowConversationComposerSubmissionStatus::NotCommitted
    });
    assert_eq!(service.selected_identity(), Some(retained));
    assert_eq!(
        mount
            .read_with(cx, |mount, _| mount.contribution())
            .unwrap()
            .entity_id(),
        contribution_id
    );
    assert!(input.read_with(cx, |input, _| input.is_surface_current_and_interactive()));
}

#[gpui::test]
fn mounted_held_submission_suppresses_duplicate_enter_then_drop_drains_without_retargeting(
    cx: &mut gpui::TestAppContext,
) {
    cx.update(ensure_text_input_bindings);
    let (fixture, cx) = mounted_submission_fixture(cx, "phase228-mounted-held", 231);
    let (predecessor, retained) = prepare_flushed_submission(
        cx,
        &fixture.mount,
        &fixture.service,
        "held mounted submission",
    );
    let gate = fixture
        .mount
        .update(cx, |mount, _| mount.test_block_next_submission_advance());

    cx.simulate_keystrokes("enter");
    drive_until(cx, 512, "held mounted submission", |_| gate.is_blocked());
    let token = fixture
        .mount
        .read_with(cx, |mount, _| mount.test_submission_advance_token())
        .expect("held mounted submission has a ticket");
    assert!(fixture.mount.read_with(cx, |mount, _| {
        mount.test_submission_diagnostics().active_ticket()
    }));
    assert!(fixture.mount.read_with(cx, |mount, _| {
        mount.test_submission_diagnostics().active_task()
    }));

    cx.simulate_keystrokes("enter");
    drive(cx, 32);
    assert!(gate.is_blocked());
    assert_eq!(
        fixture
            .mount
            .read_with(cx, |mount, _| mount.test_submission_advance_token()),
        Some(token)
    );
    assert!(fixture.mount.read_with(cx, |mount, _| {
        mount.test_submission_diagnostics().active_task()
    }));

    let MountedSubmissionFixture {
        _directory,
        root,
        mount,
        service,
    } = fixture;
    root.update(cx, |root, _| root.mount = None);
    drop(mount);
    gate.release();
    drive_until(cx, 2_048, "detached mounted submission drain", |_| {
        let diagnostics = service.test_submission_diagnostics().unwrap();
        service.selected_identity() == Some(retained)
            && service.pending_receipt().is_none()
            && diagnostics.selected_submission().is_some_and(|submission| {
                !submission.pending()
                    && submission.retained_roots() == 0
                    && submission.retained_materializations() == 0
            })
            && !diagnostics.submission_successor_reserved()
            && !diagnostics.pending_activation_reserved()
    });
    assert_eq!(service.selected_identity(), Some(retained));
    assert!(service.pending_receipt().is_none());
    let diagnostics = service.test_submission_diagnostics().unwrap();
    let submission = diagnostics
        .selected_submission()
        .expect("detached drain retains the selected host");
    assert!(!submission.pending());
    assert_eq!(submission.retained_roots(), 0);
    assert_eq!(submission.retained_materializations(), 0);
    assert!(!diagnostics.submission_successor_reserved());
    assert!(!diagnostics.pending_activation_reserved());
    assert_eq!(
        predecessor.read_with(cx, |composer, _| composer.selection_identity()),
        retained
    );
}

#[gpui::test]
fn mounted_reconciliation_suppresses_duplicate_enter_until_explicit_exact_resume(
    cx: &mut gpui::TestAppContext,
) {
    cx.update(ensure_text_input_bindings);
    let (fixture, cx) = mounted_submission_fixture(cx, "phase228-mounted-reconciling", 232);
    prepare_flushed_submission(
        cx,
        &fixture.mount,
        &fixture.service,
        "reconciling mounted submission",
    );
    fixture.mount.update(cx, |mount, _| {
        mount.test_inject_next_submission_advance(
            MainWindowComposerSubmissionTestAdvance::ReconciliationPending,
        )
    });

    cx.simulate_keystrokes("enter");
    drive_until(cx, 2_048, "mounted reconciliation", |cx| {
        fixture
            .mount
            .read_with(cx, |mount, _| mount.submission_status())
            == MainWindowConversationComposerSubmissionStatus::Reconciling
    });
    let token = fixture
        .mount
        .read_with(cx, |mount, _| mount.test_submission_advance_token())
        .expect("reconciling mounted submission has a ticket");

    cx.simulate_keystrokes("enter");
    drive(cx, 32);
    assert_eq!(
        fixture
            .mount
            .read_with(cx, |mount, _| mount.submission_status()),
        MainWindowConversationComposerSubmissionStatus::Reconciling
    );
    assert_eq!(
        fixture
            .mount
            .read_with(cx, |mount, _| mount.test_submission_advance_token()),
        Some(token)
    );

    cx.update(|window, app| {
        fixture.mount.update(app, |mount, mount_cx| {
            mount.test_resume_submission_after_reconciliation(window, mount_cx)
        })
    })
    .expect("mounted reconciliation resumes");
    drive_until(
        cx,
        2_048,
        "mounted reconciliation exact classification",
        |cx| {
            fixture
                .service
                .selected_identity()
                .is_some_and(|selection| selection.binding().presentation_generation().get() == 2)
                && fixture
                    .mount
                    .read_with(cx, |mount, _| mount.submission_status())
                    == MainWindowConversationComposerSubmissionStatus::Idle
        },
    );
    let diagnostics = fixture
        .mount
        .read_with(cx, |mount, _| mount.test_submission_diagnostics());
    assert!(!diagnostics.active_ticket());
    assert!(!diagnostics.active_task());
}

#[gpui::test]
fn mounted_collision_sets_unavailable_only_after_host_ticket_settles_without_custody(
    cx: &mut gpui::TestAppContext,
) {
    cx.update(ensure_text_input_bindings);
    let (fixture, cx) = mounted_submission_fixture(cx, "phase228-mounted-collision", 233);
    prepare_flushed_submission(
        cx,
        &fixture.mount,
        &fixture.service,
        "colliding mounted submission",
    );
    fixture.mount.update(cx, |mount, _| {
        mount
            .test_inject_next_submission_advance(MainWindowComposerSubmissionTestAdvance::Collision)
    });

    cx.simulate_keystrokes("enter");
    drive_until(cx, 2_048, "mounted collision", |cx| {
        let diagnostics = fixture
            .mount
            .read_with(cx, |mount, _| mount.test_submission_diagnostics());
        diagnostics.status() == MainWindowConversationComposerSubmissionStatus::Unavailable
            && !diagnostics.active_ticket()
            && !diagnostics.active_task()
            && !diagnostics.successor()
    });
}

#[gpui::test]
fn mounted_successor_readiness_interruption_promotes_the_same_successor_without_resubmission(
    cx: &mut gpui::TestAppContext,
) {
    cx.update(ensure_text_input_bindings);
    let (fixture, cx) = mounted_submission_fixture(cx, "phase228-mounted-successor-failure", 235);
    let (predecessor, predecessor_selection) = prepare_flushed_submission(
        cx,
        &fixture.mount,
        &fixture.service,
        "successor readiness failure",
    );
    let predecessor_entity = predecessor.entity_id();
    let gate = fixture
        .mount
        .update(cx, |mount, _| mount.test_block_next_submission_advance());
    fixture.mount.update(cx, |mount, _| {
        mount.test_fail_submission_successor_after_readiness_once()
    });

    cx.simulate_keystrokes("enter");
    drive_until(cx, 512, "held successor-readiness submission", |_| {
        gate.is_blocked()
    });
    let token = fixture
        .mount
        .read_with(cx, |mount, _| mount.test_submission_advance_token())
        .expect("held successor-readiness submission has a ticket");
    gate.release();
    drive_until(
        cx,
        2_048,
        "resumed mounted successor-readiness promotion",
        |cx| {
            let mount_diagnostics = fixture
                .mount
                .read_with(cx, |mount, _| mount.test_submission_diagnostics());
            let service_diagnostics = fixture.service.test_submission_diagnostics().unwrap();
            fixture
                .mount
                .read_with(cx, |mount, _| mount.submission_status())
                == MainWindowConversationComposerSubmissionStatus::Idle
                && !mount_diagnostics.active_ticket()
                && !mount_diagnostics.active_task()
                && !mount_diagnostics.successor()
                && fixture.service.pending_receipt().is_none()
                && fixture
                    .mount
                    .read_with(cx, |mount, _| mount.test_pending_contribution())
                    .is_none()
                && service_diagnostics
                    .selected_submission()
                    .is_some_and(|submission| {
                        !submission.pending()
                            && submission.retained_roots() == 0
                            && submission.retained_materializations() == 0
                    })
                && !service_diagnostics.submission_successor_reserved()
                && !service_diagnostics.pending_activation_reserved()
        },
    );
    let successor = fixture.service.selected_identity().unwrap();
    assert_eq!(successor.claim(), predecessor_selection.claim());
    assert_eq!(successor.binding().presentation_generation().get(), 2);
    let successor_entity = fixture
        .mount
        .read_with(cx, |mount, _| mount.contribution())
        .unwrap()
        .entity_id();
    assert_ne!(successor_entity, predecessor_entity);

    cx.update(|window, app| {
        fixture.mount.update(app, |mount, mount_cx| {
            mount.test_apply_late_submission_advance(
                token,
                MainWindowComposerSubmissionTestAdvance::Collision,
                window,
                mount_cx,
            )
        })
    })
    .expect("resumed successor promotion fences the late advance");
    drive(cx, 32);
    assert_eq!(fixture.service.selected_identity(), Some(successor));
    assert_eq!(
        fixture
            .mount
            .read_with(cx, |mount, _| mount.contribution())
            .unwrap()
            .entity_id(),
        successor_entity
    );
    assert_eq!(
        fixture
            .mount
            .read_with(cx, |mount, _| mount.submission_status()),
        MainWindowConversationComposerSubmissionStatus::Idle
    );
}

#[gpui::test]
fn mounted_late_predecessor_advances_cannot_mutate_the_promoted_successor(
    cx: &mut gpui::TestAppContext,
) {
    cx.update(ensure_text_input_bindings);
    let (fixture, cx) = mounted_submission_fixture(cx, "phase228-mounted-late", 234);
    prepare_flushed_submission(
        cx,
        &fixture.mount,
        &fixture.service,
        "late predecessor mounted submission",
    );
    let gate = fixture
        .mount
        .update(cx, |mount, _| mount.test_block_next_submission_advance());

    cx.simulate_keystrokes("enter");
    drive_until(cx, 512, "held predecessor submission", |_| {
        gate.is_blocked()
    });
    let predecessor_token = fixture
        .mount
        .read_with(cx, |mount, _| mount.test_submission_advance_token())
        .expect("held predecessor submission has a ticket");
    gate.release();
    drive_until(cx, 2_048, "promoted mounted successor", |cx| {
        fixture
            .service
            .selected_identity()
            .is_some_and(|selection| selection.binding().presentation_generation().get() == 2)
            && fixture
                .mount
                .read_with(cx, |mount, _| mount.submission_status())
                == MainWindowConversationComposerSubmissionStatus::Idle
    });
    let successor = fixture.service.selected_identity().unwrap();
    let successor_entity = fixture
        .mount
        .read_with(cx, |mount, _| mount.contribution())
        .unwrap()
        .entity_id();

    for advance in [
        MainWindowComposerSubmissionTestAdvance::Collision,
        MainWindowComposerSubmissionTestAdvance::ReconciliationPending,
    ] {
        cx.update(|window, app| {
            fixture.mount.update(app, |mount, mount_cx| {
                mount.test_apply_late_submission_advance(
                    predecessor_token,
                    advance,
                    window,
                    mount_cx,
                )
            })
        })
        .expect("late predecessor advance is fenced");
        drive(cx, 32);
        assert_eq!(fixture.service.selected_identity(), Some(successor));
        assert_eq!(
            fixture
                .mount
                .read_with(cx, |mount, _| mount.contribution())
                .unwrap()
                .entity_id(),
            successor_entity
        );
        assert_eq!(
            fixture
                .mount
                .read_with(cx, |mount, _| mount.submission_status()),
            MainWindowConversationComposerSubmissionStatus::Idle
        );
    }
}

fn mounted_submission_fixture<'a>(
    cx: &'a mut gpui::TestAppContext,
    name: &str,
    seed: u8,
) -> (MountedSubmissionFixture, &'a mut gpui::VisualTestContext) {
    let fixture = Fixture::new(name, seed);
    let claim = fixture.claims().0;
    let marker_authority = MainWindowComposerMarkerMetadataAuthority::new(fixture.assets());
    let marker_seals = fixture.marker_seals();
    let window_id = fixture.window_id;
    let thread = fixture.selected_thread;
    let (directory, store, storage) = fixture.into_store();
    let mut host = SyndicComposerHost::new(storage.clone());
    assert!(
        host.test_activate(
            &store,
            activation(thread, seed.wrapping_add(1), seed.wrapping_add(2), 1),
            &CommandCancellation::new(),
        )
        .is_ok()
    );
    let slot = MainWindowComposerSlot::new(window_id, claim, host, storage, marker_authority)
        .expect("mounted fixture slot");
    let service = Arc::new(MainWindowConversationComposerService::new(
        Arc::new(store),
        slot,
    ));
    let mounted_service = service.clone();
    let (root, cx) = cx.add_window_view(|window, cx| {
        let mount = cx.new(|mount_cx| {
            MainWindowConversationComposerMount::new(
                mounted_service,
                Box::new(configure),
                marker_seals,
                submission_source(),
                window,
                mount_cx,
            )
            .expect("mounted fixture")
        });
        MountRoot { mount: Some(mount) }
    });
    let mount = root.read_with(cx, |root, _| root.mount.clone().unwrap());
    drive(cx, 16);
    (
        MountedSubmissionFixture {
            _directory: directory,
            root,
            mount,
            service,
        },
        cx,
    )
}

fn prepare_flushed_submission(
    cx: &mut gpui::VisualTestContext,
    mount: &Entity<MainWindowConversationComposerMount>,
    service: &Arc<MainWindowConversationComposerService>,
    text: &str,
) -> (
    Entity<beryl_app::main_window::MainWindowConversationComposer>,
    beryl_app::main_window::MainWindowComposerSelectionIdentity,
) {
    drive(cx, 16);
    let contribution = mount
        .read_with(cx, |mount, _| mount.contribution())
        .expect("mounted fixture contribution");
    let baseline = service
        .selected_identity()
        .expect("mounted fixture selection");
    let input = contribution.read_with(cx, |composer, _| composer.gpui_input());
    cx.update(|window, app| {
        input.update(app, |input, input_cx| {
            input.focus(window);
            input.replace_and_mark_text_in_range(None, text, None, window, input_cx)
        })
    });
    drive_until(cx, 256, "dirty mounted fixture binding", |_| {
        service
            .selected_identity()
            .is_some_and(|selection| selection.binding() != baseline.binding())
    });
    cx.update(|window, app| {
        mount.update(app, |mount, mount_cx| {
            mount.publish_autosave_interval(
                1,
                ComposerHostAutosaveInterval::new(5).unwrap(),
                window,
                mount_cx,
            )
        })
    })
    .expect("mounted fixture autosave interval");
    cx.executor().advance_clock(Duration::from_secs(5));
    drive_until(cx, 512, "mounted fixture autosave", |cx| {
        mount.read_with(cx, |mount, _| mount.autosave_diagnostics().phase())
            == MainWindowConversationComposerAutosavePhase::Idle
    });
    let flushed = service
        .selected_identity()
        .expect("flushed mounted fixture selection");
    assert_ne!(flushed.binding(), baseline.binding());
    (contribution, flushed)
}

fn configure(
    selection: beryl_app::main_window::MainWindowComposerSelectionIdentity,
) -> Result<MainWindowConversationComposerConfig, String> {
    MainWindowConversationComposerConfig::new(
        selection,
        widget_config(
            selection.binding().range_binding(),
            selection.binding().presentation_generation(),
        ),
    )
    .map_err(|error| error.to_string())
}

fn widget_config(
    binding: gpui_text_input::RangeBinding,
    presentation: std::num::NonZeroU64,
) -> RangeTextInputConfig {
    let layout = StreamingLayoutBinding {
        input_id: 22_800,
        segment_policy_id: 22_801,
        start_position: StreamingLayoutPosition::at(0),
        wrap_width: px(320.),
        font_size: px(12.),
        line_height: px(16.),
        limits: StreamingLayoutLimits {
            segment_bytes: 256,
            runs: 8,
            decorations: 8,
            glyphs: 4096,
            wraps: 256,
            maps: 4097,
            fragments: 4,
            retained_items: 32_768,
            retained_bytes: 2 * 1024 * 1024,
        },
    };
    let run = TextRun {
        len: 0,
        font: font(".SystemUIFont"),
        color: black(),
        background_color: None,
        underline: None,
        strikethrough: None,
    };
    RangeTextInputConfig {
        binding,
        presentation_generation: PresentationGeneration::new(presentation.get()),
        enter_key: TextInputEnterKey::Propagate,
        atom_clipboard_policy: TextInputAtomClipboardPolicy::Propagate,
        rich_paste_policy: TextInputRichPastePolicy::Propagate,
        layout,
        style: StreamingGeometryStyle::new(
            run,
            StreamingOversizePresentation::new(
                SharedString::new_static(""),
                vec![],
                px(12.),
                px(16.),
                px(12.),
                None,
            ),
        ),
        geometry_limits: ExactGeometryLimits::new(256, 16, 2 * 1024 * 1024, 32_768).unwrap(),
        residency_limits: ResidencyLimits::new(9, 384 * 1024, 6, 384 * 1024).unwrap(),
        object_residency_limits: ObjectResidencyLimits::new(9, 48, 65_536, 65_536, 6, 48, 65_536)
            .unwrap(),
        mutation_limits: MutationLimits::new(64, 65_536).unwrap(),
        clipboard_limits: ClipboardLimits::new(64 * 1024, 256).unwrap(),
        segmentation_limits: SegmentationLimits::new(256, 4096).unwrap(),
        limits: RangeTextInputLimits::new(8 * 1024 * 1024, 131_072, 64, px(64.), 256, 256, px(16.))
            .unwrap(),
        settlement_coordinator: RangeSettlementCoordinator::new(4).unwrap(),
        viewport_extent: px(640.),
        overscan: px(32.),
        placeholder: SharedString::new_static("Message"),
        theme: TextInputTheme::default(),
        scrollbar_style: ScrollbarStyle::default(),
    }
}

fn drive(cx: &mut gpui::VisualTestContext, rounds: usize) {
    for _ in 0..rounds {
        cx.run_until_parked();
        cx.update(|window, app| window.draw(app).clear());
    }
}

fn submission_source() -> MainWindowComposerSubmissionRequestSource {
    MainWindowComposerSubmissionRequestSource::new(
        ProjectionServiceConfig::try_new(1, 4, MinimumTurnCaptureReserve::try_new(1).unwrap())
            .unwrap()
            .turn_start_admission_requirement(),
    )
}

fn drive_until(
    cx: &mut gpui::VisualTestContext,
    rounds: usize,
    operation: &str,
    mut complete: impl FnMut(&mut gpui::VisualTestContext) -> bool,
) {
    for _ in 0..rounds {
        if complete(cx) {
            return;
        }
        drive(cx, 1);
    }
    assert!(
        complete(cx),
        "mounted operation did not settle: {operation}"
    );
}
