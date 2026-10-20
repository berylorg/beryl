use std::{cell::RefCell, ops::Range, rc::Rc};

use beryl_app::settings_window::theme_editor::{
    THEME_ROLE_OVERSCAN, THEME_ROLE_ROW_STRIDE, ThemeRoleColumn, ThemeRoleNavigationKey as Key,
    ThemeRoleNavigatorState, ThemeRoleProjection, ThemeRoleRow, ThemeRoleSource,
};
use beryl_state::{ThemeRoleId, canonical_theme_schema};

struct Source {
    roots: Vec<ThemeRoleId>,
    parent: Option<ThemeRoleId>,
    children: Vec<ThemeRoleId>,
    requests: RefCell<Vec<Range<usize>>>,
}

impl ThemeRoleSource for Source {
    fn page(&self, parent: Option<&ThemeRoleId>, range: Range<usize>) -> Vec<ThemeRoleRow> {
        self.requests.borrow_mut().push(range.clone());
        self.ids(parent)[range]
            .iter()
            .map(|id| ThemeRoleRow {
                id: id.clone(),
                label: id.as_str().to_owned(),
                has_children: self.parent.as_ref() == Some(id),
                supported_property_count: 1,
            })
            .collect()
    }

    fn index_of(&self, parent: Option<&ThemeRoleId>, role: &ThemeRoleId) -> Option<usize> {
        self.ids(parent).iter().position(|id| id == role)
    }

    fn total_role_count(&self) -> usize {
        self.roots.len() + self.children.len()
    }
    fn max_column_count(&self) -> usize {
        2
    }
}

impl Source {
    fn ids(&self, parent: Option<&ThemeRoleId>) -> &[ThemeRoleId] {
        if parent.is_none() {
            &self.roots
        } else if parent == self.parent.as_ref() {
            &self.children
        } else {
            &[]
        }
    }
}

fn flat(count: usize) -> (Rc<Source>, ThemeRoleProjection) {
    let source = Rc::new(Source {
        roots: canonical_theme_schema()
            .roles()
            .keys()
            .take(count)
            .cloned()
            .collect(),
        parent: None,
        children: Vec::new(),
        requests: RefCell::new(Vec::new()),
    });
    assert_eq!(source.roots.len(), count);
    let projection = ThemeRoleProjection {
        revision: 1,
        selected: None,
        columns: vec![ThemeRoleColumn {
            parent: None,
            selected: None,
            logical_item_count: count,
            header: "Roles".into(),
        }],
        source: source.clone(),
        property_row_count: 0,
    };
    (source, projection)
}

fn nested() -> (Rc<Source>, ThemeRoleProjection) {
    let ids: Vec<_> = canonical_theme_schema()
        .roles()
        .keys()
        .take(60)
        .cloned()
        .collect();
    let source = Rc::new(Source {
        roots: ids[..30].to_vec(),
        parent: Some(ids[10].clone()),
        children: ids[30..].to_vec(),
        requests: RefCell::new(Vec::new()),
    });
    let projection = ThemeRoleProjection {
        revision: 1,
        selected: Some(source.children[10].clone()),
        columns: vec![
            ThemeRoleColumn {
                parent: None,
                selected: source.parent.clone(),
                logical_item_count: source.roots.len(),
                header: "Roles".into(),
            },
            ThemeRoleColumn {
                parent: source.parent.clone(),
                selected: Some(source.children[10].clone()),
                logical_item_count: source.children.len(),
                header: "Children".into(),
            },
        ],
        source: source.clone(),
        property_row_count: 1,
    };
    (source, projection)
}

#[test]
fn realization_requests_only_a_fixed_window_as_role_counts_grow() {
    for count in [20, canonical_theme_schema().roles().len()] {
        let (source, projection) = flat(count);
        let mut state = ThemeRoleNavigatorState::new(projection).unwrap();
        for offset in [0., 76., count as f32 * THEME_ROLE_ROW_STRIDE] {
            state.set_offset(0, offset);
            let range = state.realized_range(0);
            assert!(range.len() <= 4 + 2 * THEME_ROLE_OVERSCAN);
            assert_eq!(state.realized_page(0).len(), range.len());
            assert!(state.visible_range(0).start >= range.start);
            assert!(state.visible_range(0).end <= range.end);
        }
        assert!(
            source
                .requests
                .borrow()
                .iter()
                .all(|range| range.len() <= 8)
        );
    }
}

#[test]
fn keyboard_reveals_logical_order_without_selecting_or_publishing_children() {
    let (source, projection) = flat(30);
    let mut state = ThemeRoleNavigatorState::new(projection).unwrap();
    assert_eq!(state.enter().unwrap().role, source.roots[0]);
    assert_eq!(
        state.navigate(Key::End, true).unwrap().role,
        source.roots[29]
    );
    assert!(state.focus_is_realized());
    assert!(state.offset(0) > 0.);
    assert_eq!(
        state.navigate(Key::Up, true).unwrap().role,
        source.roots[28]
    );
    assert!(state.navigate(Key::Right, true).is_none());
    assert_eq!(
        state.navigate(Key::Home, true).unwrap().role,
        source.roots[0]
    );
    assert_eq!(
        state.navigate(Key::Down, true).unwrap().role,
        source.roots[1]
    );
    assert!(state.selected_role().is_none());
}

#[test]
fn selected_child_and_parent_navigation_uses_only_the_published_trail() {
    let (source, projection) = nested();
    let mut state = ThemeRoleNavigatorState::new(projection.clone()).unwrap();
    assert_eq!(state.enter().unwrap().role, source.children[10]);
    assert_eq!(
        state.navigate(Key::Left, true).unwrap().role,
        source.parent.clone().unwrap()
    );
    assert_eq!(
        state.navigate(Key::Right, true).unwrap().role,
        source.children[10]
    );
    state.navigate(Key::Left, true);
    state.navigate(Key::Down, true);
    assert!(state.navigate(Key::Right, true).is_none());
    assert_eq!(state.selected_role(), projection.selected.as_ref());
}

#[test]
fn scrolling_retires_realized_focus_but_retains_the_compact_identity() {
    let (source, projection) = flat(40);
    let mut state = ThemeRoleNavigatorState::new(projection).unwrap();
    state.enter();
    state.set_offset(0, 1000.);
    assert!(!state.focus_is_realized());
    assert_eq!(state.logical_focus().unwrap().role, source.roots[0]);
    state.set_offset(0, 0.);
    assert!(state.focus_is_realized());
    assert_eq!(state.logical_focus().unwrap().role, source.roots[0]);
}

#[test]
fn projection_refresh_preserves_scroll_and_rebinds_focus_by_stable_parent_and_role() {
    let (source, mut projection) = nested();
    let mut state = ThemeRoleNavigatorState::new(projection.clone()).unwrap();
    state.enter();
    state.set_offset(0, 500.);
    state.set_offset(1, 600.);
    projection.property_row_count = 2;
    state.reconcile(projection.clone()).unwrap();
    assert_eq!(state.offset(0), 500.);
    assert_eq!(state.offset(1), 600.);
    projection.revision = 2;
    state.reconcile(projection).unwrap();
    assert_eq!(state.logical_focus().unwrap().revision, 2);
    assert_eq!(state.logical_focus().unwrap().role, source.children[10]);
    assert_eq!(state.offset(0), 500.);
    assert_eq!(state.offset(1), 600.);
}

#[test]
fn removed_focused_column_falls_back_to_selected_role_without_reusing_its_scroll() {
    let (source, mut projection) = nested();
    let mut state = ThemeRoleNavigatorState::new(projection.clone()).unwrap();
    state.enter();
    state.set_offset(0, 420.);
    projection.revision = 2;
    projection.selected = source.parent.clone();
    projection.columns.truncate(1);
    state.reconcile(projection).unwrap();
    assert_eq!(
        state.logical_focus().unwrap().role,
        source.parent.clone().unwrap()
    );
    assert_eq!(state.logical_focus().unwrap().parent, None);
    assert_eq!(state.offset(0), 420.);
}

#[test]
fn stale_unrealized_and_inert_activation_callbacks_do_not_change_focus_or_selection() {
    let (source, mut projection) = flat(40);
    let mut state = ThemeRoleNavigatorState::new(projection.clone()).unwrap();
    state.enter();
    let original = state.logical_focus().cloned();
    assert!(!state.accept_activation(1, None, source.roots[1].clone(), false));
    assert!(!state.accept_activation(1, None, source.roots[39].clone(), true));
    projection.revision = 2;
    state.reconcile(projection).unwrap();
    assert!(!state.accept_activation(1, None, source.roots[1].clone(), true));
    assert_eq!(state.logical_focus().unwrap().role, original.unwrap().role);
    assert!(state.accept_activation(2, None, source.roots[1].clone(), true));
    assert_eq!(state.logical_focus().unwrap().role, source.roots[1]);
    assert!(state.selected_role().is_none());
}

#[test]
fn inactive_navigator_does_not_admit_keyboard_traversal_or_reveal() {
    let (_, projection) = flat(40);
    let mut state = ThemeRoleNavigatorState::new(projection).unwrap();
    assert!(state.navigate(Key::End, false).is_none());
    assert!(state.logical_focus().is_none());
    assert_eq!(state.offset(0), 0.);
    state.navigate(Key::End, true);
    let focus = state.logical_focus().cloned();
    let offset = state.offset(0);
    assert!(state.navigate(Key::Home, false).is_none());
    assert_eq!(state.logical_focus(), focus.as_ref());
    assert_eq!(state.offset(0), offset);
}

#[test]
fn invalid_and_stale_projections_are_rejected_before_state_changes() {
    let (_, mut projection) = nested();
    let mut state = ThemeRoleNavigatorState::new(projection.clone()).unwrap();
    projection.revision = 2;
    projection.columns.push(projection.columns[1].clone());
    assert!(state.reconcile(projection).is_err());
    let (_, mut flat) = flat(30);
    flat.property_row_count = gpui_settings_window::MAX_PAGE_DETAIL_ROWS + 1;
    assert!(ThemeRoleNavigatorState::new(flat).is_err());
    let (_, mut next) = nested();
    next.revision = 0;
    assert!(state.reconcile(next).is_err());
}

#[test]
fn empty_projection_has_no_fabricated_focus_or_row_and_nonfinite_scroll_is_refused() {
    let (_, mut projection) = flat(0);
    projection.columns.clear();
    let mut empty = ThemeRoleNavigatorState::new(projection).unwrap();
    assert!(empty.enter().is_none());
    assert!(empty.navigate(Key::Down, true).is_none());
    let (_, projection) = flat(30);
    let mut state = ThemeRoleNavigatorState::new(projection).unwrap();
    assert!(!state.set_offset(0, f32::NAN));
    assert!(!state.set_offset(0, f32::INFINITY));
    assert_eq!(state.offset(0), 0.);
}

#[test]
fn wheel_routing_uses_actual_focus_and_one_axis_without_hover_admission() {
    use beryl_app::settings_window::theme_editor::{
        ThemeRoleWheelFocus as Focus, ThemeRoleWheelTarget as Target, theme_role_wheel_target,
    };
    for (x, y, shift) in [(0., -40., false), (-40., 0., false), (0., -40., true)] {
        assert_eq!(
            theme_role_wheel_target(Focus::Outside, x, y, shift),
            Target::Page
        );
    }
    assert_eq!(
        theme_role_wheel_target(Focus::Column(1), 0., -40., false),
        Target::Column(1)
    );
    assert_eq!(
        theme_role_wheel_target(Focus::Column(1), -1., -40., false),
        Target::Column(1)
    );
    assert_eq!(
        theme_role_wheel_target(Focus::Column(1), -40., -1., false),
        Target::Trail
    );
    assert_eq!(
        theme_role_wheel_target(Focus::Column(1), 0., -40., true),
        Target::Trail
    );
    assert_eq!(
        theme_role_wheel_target(Focus::Navigator, -40., 0., false),
        Target::Trail
    );
    assert_eq!(
        theme_role_wheel_target(Focus::Navigator, 0., -40., false),
        Target::Page
    );
    assert_eq!(
        theme_role_wheel_target(Focus::Column(1), f32::NAN, -40., false),
        Target::Page
    );
}

#[path = "settings_window/support.rs"]
mod settings_support;

#[gpui::test]
fn mounted_focus_rebinds_without_stealing_property_focus_and_observes_live_panel_admission(
    cx: &mut gpui::TestAppContext,
) {
    use beryl_app::settings_window::theme_editor::ThemeRoleNavigator;
    use gpui::{AppContext as _, IntoElement as _, Render as _};
    use gpui_settings_window::{
        SettingsInteractionAdmission, SettingsInteractionGate, SettingsPage,
        SettingsPageBodyRenderer, SettingsPageCustomBody, SettingsPageSplitWorkReceiver,
        SettingsPanel, SettingsSection, SettingsWindowModel, SettingsWindowOptions,
    };

    let wheel = |x, y, shift| gpui::ScrollWheelEvent {
        position: gpui::point(gpui::px(0.), gpui::px(0.)),
        delta: gpui::ScrollDelta::Pixels(gpui::point(gpui::px(x), gpui::px(y))),
        modifiers: gpui::Modifiers {
            shift,
            ..Default::default()
        },
        touch_phase: gpui::TouchPhase::Moved,
    };

    let fixture = settings_support::on_worker(cx, settings_support::Fixture::new);
    let generation = fixture.runtime.current().unwrap();
    let admission = Rc::new(RefCell::new(None::<SettingsInteractionAdmission>));
    let target = admission.clone();
    let model = SettingsWindowModel::new(vec![
        SettingsSection::new("themes", "Themes").with_root_page(
            SettingsPage::new("themes", "Themes")
                .with_stacked_custom_body(SettingsPageCustomBody::new("navigator", 156)),
        ),
    ])
    .unwrap();
    let options = SettingsWindowOptions::default().with_page_body_renderer(
        SettingsPageBodyRenderer::new(move |_, current| {
            *target.borrow_mut() = Some(current);
            Some(gpui::div().into_any_element())
        }),
    );
    let (panel, visual) = cx.add_window_view(|window, cx| {
        SettingsPanel::new_with_options(
            model.clone(),
            options,
            SettingsPageSplitWorkReceiver::new(),
            window,
            cx,
        )
    });
    visual.update(|window, app| {
        panel.update(app, |panel, cx| {
            drop(panel.render(window, cx));
        })
    });
    assert!(admission.borrow().as_ref().unwrap().is_open());
    let (source, projection) = flat(40);
    let widget = visual.update(|_, app| {
        ThemeRoleNavigator::mount(
            projection.clone(),
            generation.clone(),
            admission.clone(),
            app,
        )
        .unwrap()
    });
    visual.update(|window, app| {
        widget.update(app, |widget, cx| {
            widget.focus_proxy().focus(window);
            drop(widget.render(window, cx));
            assert_eq!(widget.logical_focus().unwrap().role, source.roots[0]);
            let diagnostics = widget.diagnostics(window, 7);
            assert_eq!(diagnostics.focused_target_kind, "row");
            assert!(diagnostics.realized_rows <= 8);
            assert_eq!(diagnostics.total_schema_roles, 40);
        })
    });

    let other_focus = visual.update(|_, app| app.focus_handle());
    visual.update(|window, app| {
        other_focus.focus(window);
        widget.update(app, |widget, cx| {
            let mut refreshed = projection.clone();
            refreshed.revision = 2;
            widget.sync(refreshed, generation.clone(), cx).unwrap();
            drop(widget.render(window, cx));
            assert_eq!(widget.logical_focus().unwrap().revision, 2);
            assert!(other_focus.is_focused(window));
        });
        panel.update(app, |panel, cx| {
            panel.sync_model(
                model
                    .clone()
                    .with_interaction_gate(SettingsInteractionGate::gated("Reconciling")),
                window,
                cx,
            )
        });
        assert!(!admission.borrow().as_ref().unwrap().is_open());
        widget.update(app, |widget, cx| {
            widget.focus_proxy().focus(window);
            drop(widget.render(window, cx));
            assert_eq!(widget.diagnostics(window, 7).focused_target_kind, "row");
            let lease = widget.scroll_lease().unwrap();
            assert!(!widget.activate_role(lease, None, source.roots[0].clone(), window, cx));
            let offset = widget.column_scroll_offset(None).unwrap();
            assert!(widget.scroll_wheel(lease, &wheel(0., -304., false), window, cx));
            assert!(widget.column_scroll_offset(None).unwrap() > offset);
            assert_eq!(
                widget.diagnostics(window, 7).focused_target_kind,
                "viewport-proxy"
            );
            assert_eq!(widget.logical_focus().unwrap().role, source.roots[0]);
            other_focus.focus(window);
            let offset = widget.column_scroll_offset(None).unwrap();
            let horizontal = widget.horizontal_scroll_offset();
            assert!(!widget.scroll_wheel(lease, &wheel(0., -76., false), window, cx));
            assert!(!widget.scroll_wheel(lease, &wheel(-40., 0., false), window, cx));
            assert!(!widget.scroll_wheel(lease, &wheel(0., -40., true), window, cx));
            assert_eq!(widget.column_scroll_offset(None).unwrap(), offset);
            assert_eq!(widget.horizontal_scroll_offset(), horizontal);
            widget.focus_proxy().focus(window);
            drop(widget.render(window, cx));
            assert!(widget.scroll_wheel(lease, &wheel(0., -40., true), window, cx));
            assert!(widget.scroll_wheel(lease, &wheel(-40., 0., false), window, cx));
            assert!(widget.scroll_wheel(lease, &wheel(0., f32::MAX, false), window, cx));
            assert_eq!(widget.column_scroll_offset(None), Some(0.));
            assert!(widget.scroll_wheel(lease, &wheel(0., 40., false), window, cx));
            assert_eq!(widget.column_scroll_offset(None), Some(0.));
        });
        panel.update(app, |panel, cx| panel.sync_model(model, window, cx));
        assert!(admission.borrow().as_ref().unwrap().is_open());
        widget.update(app, |widget, cx| {
            drop(widget.render(window, cx));
            assert_eq!(widget.diagnostics(window, 7).focused_target_kind, "row");
            let old_lease = widget.scroll_lease().unwrap();
            let panel_admission = admission.borrow().clone().unwrap();
            widget.deactivate(window, cx);
            assert!(widget.scroll_lease().is_none());
            assert!(!widget.scroll_wheel(old_lease, &wheel(0., -76., false), window, cx));
            *admission.borrow_mut() = Some(panel_admission);
            widget.focus_proxy().focus(window);
            drop(widget.render(window, cx));
            let lease = widget.scroll_lease().unwrap();
            assert_ne!(lease, old_lease);
            assert!(!widget.scroll_wheel(old_lease, &wheel(0., -76., false), window, cx));
            assert!(!widget.activate_role(old_lease, None, source.roots[0].clone(), window, cx));
            assert!(widget.activate_role(lease, None, source.roots[0].clone(), window, cx));
            let mut refreshed = projection.clone();
            refreshed.revision = 3;
            widget.sync(refreshed, generation.clone(), cx).unwrap();
            assert!(!widget.scroll_wheel(lease, &wheel(0., -76., false), window, cx));
            drop(widget.render(window, cx));
            let current = widget.scroll_lease().unwrap();
            assert!(widget.scroll_wheel(current, &wheel(0., -76., false), window, cx));
            *admission.borrow_mut() = None;
            assert!(!widget.scroll_wheel(current, &wheel(0., -76., false), window, cx));
        });
    });
    settings_support::on_worker(&mut visual.cx, move || fixture.dispose());
}
