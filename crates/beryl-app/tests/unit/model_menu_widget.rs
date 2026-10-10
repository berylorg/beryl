use super::*;

struct Rows(Vec<MenuRow>);
impl MenuCollection for Rows {
    fn len(&self) -> usize {
        self.0.len()
    }
    fn row(&self, index: usize) -> MenuRow {
        let mut row = self.0[index].clone();
        row.index = index;
        row
    }
    fn index_of(&self, id: &str) -> Option<usize> {
        self.0.iter().position(|row| row.id.as_deref() == Some(id))
    }
}

fn row(id: Option<&str>, kind: MenuRowKind, enabled: bool) -> MenuRow {
    MenuRow {
        index: 0,
        id: id.map(Arc::from),
        label: "bounded row".into(),
        kind,
        selected: false,
        enabled,
        disabled_reason: (!enabled).then(|| "Pending".into()),
        selector: "bounded-row".into(),
    }
}

struct Host {
    menu: Option<AnchoredContextMenu>,
    prior: FocusHandle,
}
impl Render for Host {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().track_focus(&self.prior)
    }
}

fn host(cx: &mut gpui::TestAppContext) -> gpui::WindowHandle<Host> {
    cx.update(|app| {
        app.open_window(gpui::WindowOptions::default(), |_, app| {
            app.new(|cx| Host {
                menu: None,
                prior: cx.focus_handle(),
            })
        })
        .unwrap()
    })
}

fn key(key: &str, held: bool) -> MenuEvent {
    MenuEvent::Key {
        invocation: 1,
        key: key.to_owned(),
        held,
    }
}

fn realize(
    menu: &mut AnchoredContextMenu,
    rows: &Rows,
    range: std::ops::Range<usize>,
    window: &Window,
) {
    let facts: Vec<_> = range.clone().map(|index| rows.row(index)).collect();
    menu.realize(&facts, range, window);
}

#[gpui::test]
fn canonical_rows_skip_headers_suppress_held_activation_and_preserve_stable_focus_on_reorder(
    cx: &mut gpui::TestAppContext,
) {
    let window = host(cx);
    window.update(cx, |host, window, cx| {
        let mut menu = AnchoredContextMenu::open(1, window, cx);
        let mut rows = Rows(vec![row(None, MenuRowKind::Header, false), row(Some("a"), MenuRowKind::Selection, true), row(Some("b"), MenuRowKind::Selection, true)]);
        menu.input(key("home", false), &rows);
        assert_eq!(menu.focused_id(), Some("a"));
        assert!(menu.input(key("enter", true), &rows).is_empty());
        assert!(matches!(&menu.input(key("space", false), &rows)[..], [MenuIntent::Focus(1)]));
        assert!(menu.take_pending_activation(&rows).is_none());
        realize(&mut menu, &rows, 0..3, window);
        let activation = menu.take_pending_activation(&rows).unwrap();
        assert!(matches!(&menu.input(activation, &rows)[..], [MenuIntent::Focus(1), MenuIntent::Activate { index: 1, id }] if id.as_ref() == "a"));
        rows.0.swap(1, 2);
        menu.reconcile(&rows);
        assert_eq!(menu.focused_index(), Some(2));
        assert_eq!(menu.focused_id(), Some("a"));
        assert!(menu.input(MenuEvent::Pointer { invocation: 1, index: 1, id: Arc::from("a") }, &rows).is_empty());
        assert!(menu.input(MenuEvent::Pointer { invocation: 2, index: 2, id: Arc::from("a") }, &rows).is_empty());
        menu.input(key("home", false), &rows);
        assert_eq!(menu.focused_id(), Some("b"));
        host.menu = Some(menu);
    }).unwrap();
}

#[gpui::test]
fn unresolved_logical_row_requires_real_identity_before_activation_and_disabled_retry_stays_focusable(
    cx: &mut gpui::TestAppContext,
) {
    let window = host(cx);
    window.update(cx, |_, window, cx| {
        let mut menu = AnchoredContextMenu::open(1, window, cx);
        let mut rows = Rows(vec![row(None, MenuRowKind::PendingSelection, false), row(Some("retry"), MenuRowKind::Command, false)]);
        menu.input(key("home", false), &rows);
        assert_eq!(menu.focused_index(), Some(0));
        assert!(matches!(&menu.input(key("enter", false), &rows)[..], [MenuIntent::Focus(0)]));
        assert!(menu.take_pending_activation(&rows).is_none());
        rows.0[0] = row(Some("real"), MenuRowKind::Selection, true);
        menu.focus_row(0, &rows, true);
        assert!(menu.pending_activation.is_some());
        menu.reconcile(&rows);
        realize(&mut menu, &rows, 0..2, window);
        let activation = menu.take_pending_activation(&rows).unwrap();
        assert!(matches!(&menu.input(activation, &rows)[..], [MenuIntent::Focus(0), MenuIntent::Activate { index: 0, id }] if id.as_ref() == "real"));
        menu.input(key("end", false), &rows);
        assert_eq!(menu.focused_id(), Some("retry"));
        assert!(menu.input(key("enter", false), &rows).is_empty());
        rows.0[1].enabled = true;
        assert!(matches!(&menu.input(key("space", false), &rows)[..], [MenuIntent::Activate { id, .. }] if id.as_ref() == "retry"));
    }).unwrap();
}

#[gpui::test]
fn empty_collection_navigation_is_bounded_and_dismissal_restores_prior_focus(
    cx: &mut gpui::TestAppContext,
) {
    let window = host(cx);
    window
        .update(cx, |host, window, cx| {
            window.focus(&host.prior);
            let mut menu = AnchoredContextMenu::open(1, window, cx);
            assert!(menu.focus.is_focused(window));
            let rows = Rows(Vec::new());
            let pending = Rows(vec![row(None, MenuRowKind::PendingSelection, false)]);
            menu.input(key("home", false), &pending);
            assert!(matches!(
                &menu.input(key("enter", false), &pending)[..],
                [MenuIntent::Focus(0)]
            ));
            assert!(menu.pending_activation.is_some());
            for name in ["home", "end", "up", "down", "enter", "space"] {
                assert!(menu.input(key(name, false), &rows).is_empty());
            }
            assert!(menu.pending_activation.is_none());
            assert!(matches!(
                &menu.input(key("escape", false), &rows)[..],
                [MenuIntent::Dismiss]
            ));
            menu.dismiss(window, &host.prior);
            assert!(host.prior.is_focused(window));
            let anchor = Bounds::new(point(px(795.), px(590.)), size(px(5.), px(10.)));
            menu.content_width = px(480.);
            let bounds = menu.bounds(anchor, 100_000, size(px(800.), px(600.)));
            assert!(bounds.left() >= px(0.) && bounds.top() >= px(0.));
            assert!(bounds.right() <= px(800.) && bounds.bottom() <= px(600.));
            assert_eq!(bounds.size.height, px(320.));
            assert_eq!(OVERSCAN, 0);
        })
        .unwrap();
}
