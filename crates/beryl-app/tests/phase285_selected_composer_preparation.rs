#![cfg(feature = "test-faults")]

#[path = "phase141_syndic_composer_host/support.rs"]
mod composer_base;
#[path = "phase186_pending_composer_activation/support.rs"]
mod support;

use std::sync::Arc;

use beryl_app::{
    composer_host::{ComposerHostActivationOutcome, SyndicComposerHost},
    composer_marker_seal::DraftMarkerSealService,
    main_window::{
        MainWindowComposerMarkerMetadataAuthority, MainWindowComposerSelectionIdentity,
        MainWindowComposerSlot, MainWindowComposerSubmissionRequestSource,
        MainWindowConversationComposer, MainWindowConversationComposerConfig,
        MainWindowConversationComposerConfigError, MainWindowConversationComposerConfigurator,
        MainWindowConversationComposerMount, MainWindowConversationComposerPreparedSelection,
        MainWindowConversationComposerService, MainWindowSelectedComposerPreparationTestFault,
    },
};
use beryl_home_store::CommandCancellation;
use gpui::{
    AppContext, Entity, EntityInputHandler, Focusable, IntoElement, ParentElement, Render, Styled,
    div, px,
};
use gpui_text_input::{ClipboardWriteOutcome, ensure_text_input_bindings};
use support::{activation, drive, fixture::Fixture, widget_config};

struct SelectedFixture {
    service: Arc<MainWindowConversationComposerService>,
    marker_seals: DraftMarkerSealService,
    _directory: tempfile::TempDir,
}

impl SelectedFixture {
    fn new(name: &str, seed: u8, populated: bool) -> Self {
        let fixture = Fixture::new(name, seed);
        if populated {
            support::seed_activation_published_draft(&fixture, fixture.selected_thread);
        }
        let mut host = SyndicComposerHost::new(fixture.storage.clone());
        assert!(matches!(
            host.test_activate(
                &fixture.store,
                activation(
                    fixture.selected_thread,
                    21,
                    22,
                    1,
                    if populated {
                        support::ACTIVATION_DRAFT_BYTES
                    } else {
                        0
                    }
                ),
                &CommandCancellation::new(),
            )
            .unwrap(),
            ComposerHostActivationOutcome::Activated { .. }
        ));
        let slot = MainWindowComposerSlot::new(
            fixture.window_id,
            fixture.claims().0,
            host,
            fixture.storage.clone(),
            MainWindowComposerMarkerMetadataAuthority::new(fixture.assets()),
        )
        .unwrap();
        let marker_seals = fixture.marker_seals();
        let (directory, store, _) = fixture.into_store();
        Self {
            service: Arc::new(MainWindowConversationComposerService::new(
                Arc::new(store),
                slot,
            )),
            marker_seals,
            _directory: directory,
        }
    }

    fn selection(&self) -> MainWindowComposerSelectionIdentity {
        self.service.selected_identity().unwrap()
    }

    fn prepare(&self) -> Result<MainWindowConversationComposerPreparedSelection, String> {
        MainWindowConversationComposerMount::prepare_selected(
            self.service.clone(),
            &mut configurator(),
        )
    }
}

fn config(selection: MainWindowComposerSelectionIdentity) -> MainWindowConversationComposerConfig {
    MainWindowConversationComposerConfig::new(
        selection,
        widget_config(
            selection.binding().range_binding(),
            selection.binding().presentation_generation(),
        ),
    )
    .unwrap()
}

fn configurator() -> MainWindowConversationComposerConfigurator {
    Box::new(|selection| Ok(config(selection)))
}

fn submission_source() -> MainWindowComposerSubmissionRequestSource {
    MainWindowComposerSubmissionRequestSource::new(
        beryl_app::cas_projection::ProjectionServiceConfig::try_new(
            1,
            4,
            beryl_home_store::MinimumTurnCaptureReserve::try_new(1).unwrap(),
        )
        .unwrap()
        .turn_start_admission_requirement(),
    )
}

struct MountRoot {
    mount: Entity<MainWindowConversationComposerMount>,
}

impl Render for MountRoot {
    fn render(&mut self, _: &mut gpui::Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        div()
            .w(px(320.))
            .children(self.mount.read(cx).contribution())
    }
}

#[test]
fn selected_preparation_rejects_wrong_identity_without_draining_the_host() {
    let selected = SelectedFixture::new("phase285-exact", 31, true);
    let other = SelectedFixture::new("phase285-other", 41, false);
    let error = MainWindowConversationComposerPreparedSelection::new(
        config(other.selection()),
        selected.service.clone(),
    )
    .err()
    .unwrap();
    assert!(error.contains("StaleSelection"), "{error}");
    let prepared = selected.prepare().unwrap();
    assert_eq!(prepared.selection_identity(), selected.selection());
    assert_eq!(prepared.test_seed_count(), 16);
    assert_eq!(prepared.test_seed_text_bytes(), 8 * 256);
    assert!((prepared.test_seed_text_bytes() as u64) < support::ACTIVATION_DRAFT_BYTES);
}

#[test]
fn configuration_failure_preserves_initial_ranges_and_rejects_sub_scalar_budgets() {
    let fixture = SelectedFixture::new("phase285-config", 51, false);
    let selection = fixture.selection();
    for bytes in 0..4 {
        for platform in [false, true] {
            let mut widget = widget_config(
                selection.binding().range_binding(),
                selection.binding().presentation_generation(),
            );
            if platform {
                widget.limits.platform_bytes = bytes;
            } else {
                widget.limits.page_bytes = bytes;
            }
            assert!(matches!(
                MainWindowConversationComposerConfig::new(selection, widget),
                Err(MainWindowConversationComposerConfigError::InvalidRealizationBudget)
            ));
        }
    }
    let mut fail: MainWindowConversationComposerConfigurator =
        Box::new(|_| Err("configuration unavailable".into()));
    assert_eq!(
        MainWindowConversationComposerMount::prepare_selected(fixture.service.clone(), &mut fail)
            .err()
            .unwrap(),
        "configuration unavailable"
    );
    let prepared = fixture.prepare().unwrap();
    assert_eq!(prepared.test_seed_count(), 2);
    assert_eq!(prepared.test_seed_text_bytes(), 0);
}

#[test]
fn selected_preparation_reports_host_and_seed_boundary_failures() {
    let fixture = SelectedFixture::new("phase285-host-error", 61, false);
    fixture.service.test_fail_selected_preparation(
        MainWindowSelectedComposerPreparationTestFault::HostUnavailable,
    );
    assert_eq!(
        fixture.prepare().err().unwrap(),
        "selected composer host preparation unavailable"
    );
    assert_eq!(fixture.prepare().unwrap().test_seed_count(), 2);

    let malformed = SelectedFixture::new("phase285-malformed-seed", 71, false);
    malformed.service.test_fail_selected_preparation(
        MainWindowSelectedComposerPreparationTestFault::ResponseKind,
    );
    assert_eq!(
        malformed.prepare().err().unwrap(),
        "composer activation response was not activation-legal"
    );

    let wrong_binding = SelectedFixture::new("phase285-wrong-seed", 81, false);
    wrong_binding.service.test_fail_selected_preparation(
        MainWindowSelectedComposerPreparationTestFault::ResponseBinding(
            fixture.selection().binding(),
        ),
    );
    assert_eq!(
        wrong_binding.prepare().err().unwrap(),
        "composer activation seed binding was corrupt"
    );
}

#[gpui::test]
fn empty_selected_mount_becomes_first_presentable_without_focus_or_submission(
    cx: &mut gpui::TestAppContext,
) {
    cx.update(ensure_text_input_bindings);
    let fixture = SelectedFixture::new("phase285-empty", 91, false);
    let prepared = fixture.prepare().unwrap();
    let selection = prepared.selection_identity();
    let (root, cx) = cx.add_window_view(|window, cx| {
        let mount = cx.new(|mount_cx| {
            MainWindowConversationComposerMount::from_prepared(
                prepared,
                configurator(),
                fixture.marker_seals.clone(),
                submission_source(),
                window,
                mount_cx,
            )
            .unwrap()
        });
        MountRoot { mount }
    });
    drive(cx, 32);
    let mount = root.read_with(cx, |root, _| root.mount.clone());
    assert!(mount.read_with(cx, |mount, app| mount.selected_first_presentable(app)));
    let owner = mount.read_with(cx, |mount, _| mount.contribution().unwrap());
    owner.read_with(cx, |owner, app| {
        assert_eq!(owner.selection_identity(), selection);
        assert!(!owner.pending_surface_ready(app));
        assert_eq!(
            owner.gpui_input().read(app).history_frontier(),
            selection.binding().range_history_frontier()
        );
    });
    let input = owner.read_with(cx, |owner, _| owner.gpui_input());
    cx.update(|window, app| {
        input.update(app, |input, app| {
            assert!(!input.focus_handle(app).is_focused(window))
        })
    });
    cx.update(|window, app| {
        input.update(app, |input, cx| {
            input.replace_text_in_range(None, "ready", window, cx)
        })
    });
    drive(cx, 32);
    assert_ne!(fixture.selection(), selection);
    assert!(mount.read_with(cx, |mount, app| mount.selected_first_presentable(app)));
}

#[gpui::test]
fn populated_selected_owners_realize_independently_and_reject_stale_preparation(
    cx: &mut gpui::TestAppContext,
) {
    cx.update(ensure_text_input_bindings);
    let first = SelectedFixture::new("phase285-populated", 101, true);
    let second = SelectedFixture::new("phase285-independent", 111, false);
    let stale = first.prepare().unwrap();
    let prepared = first.prepare().unwrap();
    let bound = prepared.residency_bound();
    let second_prepared = second.prepare().unwrap();
    let second_selection = second.selection();
    let (owner, first_cx) = cx.add_window_view(|window, cx| {
        MainWindowConversationComposer::from_prepared(
            prepared,
            Box::new(|_, _| ClipboardWriteOutcome::Failed),
            window,
            cx,
        )
        .unwrap()
    });
    drive(first_cx, 64);
    assert!(owner.read_with(first_cx, |owner, app| owner.selected_first_presentable(app)));
    owner.read_with(first_cx, |owner, app| {
        assert!(owner.last_error().is_none());
        let diagnostics = owner.realization_diagnostics(app);
        assert!(diagnostics.current.resident_page_bytes <= bound.text_bytes());
    });
    let input = owner.read_with(first_cx, |owner, _| owner.gpui_input());
    first_cx.update(|window, app| {
        input.update(app, |input, cx| {
            input.replace_text_in_range(None, "edited", window, cx)
        })
    });
    drive(first_cx, 64);
    assert_ne!(first.selection(), stale.selection_identity());
    assert_eq!(second.selection(), second_selection);
    let (second_owner, cx) = cx.add_window_view(|window, cx| {
        MainWindowConversationComposer::from_prepared(
            second_prepared,
            Box::new(|_, _| ClipboardWriteOutcome::Failed),
            window,
            cx,
        )
        .unwrap()
    });
    drive(cx, 32);
    assert_ne!(owner.entity_id(), second_owner.entity_id());
    assert_ne!(
        input.entity_id(),
        second_owner
            .read_with(cx, |owner, _| owner.gpui_input())
            .entity_id()
    );
    assert!(second_owner.read_with(cx, |owner, app| owner.selected_first_presentable(app)));
    let error = cx.update(|window, app| {
        stale
            .mount(Box::new(|_, _| ClipboardWriteOutcome::Failed), window, app)
            .err()
            .unwrap()
    });
    assert_eq!(error, "prepared conversation composer selection is stale");
    assert!(owner.read_with(cx, |owner, app| owner.selected_first_presentable(app)));
}

#[gpui::test]
fn minimum_supported_page_and_platform_budgets_construct_empty_and_populated(
    cx: &mut gpui::TestAppContext,
) {
    cx.update(ensure_text_input_bindings);
    for (seed, populated) in [(121, false), (131, true)] {
        let fixture = SelectedFixture::new("phase285-minimum", seed, populated);
        let selection = fixture.selection();
        let mut widget = widget_config(
            selection.binding().range_binding(),
            selection.binding().presentation_generation(),
        );
        widget.limits.page_bytes = 4;
        widget.limits.platform_bytes = 4;
        let config = MainWindowConversationComposerConfig::new(selection, widget).unwrap();
        let prepared =
            MainWindowConversationComposerPreparedSelection::new(config, fixture.service.clone())
                .unwrap();
        let (owner, cx) = cx.add_window_view(|window, cx| {
            MainWindowConversationComposer::from_prepared(
                prepared,
                Box::new(|_, _| ClipboardWriteOutcome::Failed),
                window,
                cx,
            )
            .unwrap()
        });
        drive(cx, 64);
        owner.read_with(cx, |owner, app| {
            assert!(owner.last_error().is_none(), "{:?}", owner.last_error());
            assert!(owner.selected_first_presentable(app));
        });
    }
}
