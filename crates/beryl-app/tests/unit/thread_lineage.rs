use super::*;

fn id(value: u128) -> beryl_model::SyndicThreadId {
    beryl_model::SyndicThreadId::from_bytes(value.to_be_bytes())
}
fn query(revision: u64, count: u64) -> LineageQuery {
    LineageQuery {
        revision,
        selected: id(u128::MAX),
        parent_count: count,
    }
}
fn page(request: LineagePageRequest, count: usize) -> LineagePage {
    LineagePage {
        request,
        rows: (request.start..request.start + count as u64)
            .map(|ordinal| LineageBreadcrumb {
                thread: id(ordinal as u128 + 1),
                title: format!("Parent {ordinal}"),
                reason: (ordinal % 3 == 1).then(|| "Open elsewhere".into()),
            })
            .collect(),
        next_ordinal: request.start + count as u64,
    }
}

#[test]
fn far_ordinal_pages_are_bounded_and_partial_pages_resume_without_skipping() {
    let mut model = model::LineageModel::new(query(1, 10_000_000));
    let first = model.request(5_500_021).unwrap();
    assert_eq!(first.start, 5_500_000);
    assert_eq!(first.end, 5_500_032);
    assert!(model.request(5_500_024).is_none());
    let other = model.request(8_900_000).unwrap();
    assert!(model.request(0).is_none());
    assert!(model.settle(first, Some(page(first, 5)), false));
    let continuation = model.request(5_500_021).unwrap();
    assert_eq!(continuation.start, first.start + 5);
    assert_eq!(continuation.end, first.end);
    assert!(model.settle(continuation, Some(page(continuation, 27)), false));
    assert_eq!(model.row(5_500_021).unwrap().thread, id(5_500_022));
    assert!(model.settle(other, Some(page(other, 32)), false));
    for ordinal in (0..1000).step_by(32) {
        let request = model.request(ordinal).unwrap();
        assert!(model.settle(request, Some(page(request, 32)), false));
        assert!(model.resident_count() <= LINEAGE_RESIDENT_PAGES);
    }
    assert!(model.row(5_500_021).is_none());
    let reloaded = model.request(5_500_021).unwrap();
    assert_eq!(reloaded.start, first.start);
}

#[test]
fn stale_or_foreign_results_and_oversize_titles_cannot_replace_resident_identity() {
    let mut model = model::LineageModel::new(query(1, 100));
    let request = model.request(0).unwrap();
    model.replace(query(2, 100));
    assert!(!model.settle(request, Some(page(request, 32)), false));
    let current = model.request(0).unwrap();
    let mut foreign = page(current, 32);
    foreign.request.request_id += 1;
    assert!(!model.settle(current, Some(foreign), false));
    assert!(model.row(0).is_none());
    model.replace(query(3, 100));
    let request = model.request(0).unwrap();
    let mut invalid = page(request, 32);
    invalid.rows[0].title = "x".repeat(513);
    assert!(!model.settle(request, Some(invalid), false));
    assert!(model.failure);
    assert!(model.row(0).is_none());
}

#[test]
fn logical_keyboard_focus_includes_unavailable_and_nonresident_ranges() {
    let mut model = model::LineageModel::new(query(1, 4000));
    let request = model.request(0).unwrap();
    model.settle(request, Some(page(request, 32)), false);
    model.focus_position(0);
    assert_eq!(model.movement(LineageMovement::Right), Some(1));
    assert_eq!(model.focus.unwrap().thread, id(2));
    assert!(model.row(1).unwrap().reason.is_some());
    assert_eq!(model.movement(LineageMovement::End), Some(3999));
    assert_eq!(model.target, Some((3999, None)));
    let request = model.request(3999).unwrap();
    model.settle(request, Some(page(request, 32)), false);
    assert_eq!(model.focus.unwrap().thread, id(4000));
    model.replace(query(2, 4000));
    assert_eq!(model.target, Some((3999, Some(id(4000)))));
    model.replace(query(3, 4000));
    assert_eq!(model.target, Some((3999, Some(id(4000)))));
    let request = model.request(3999).unwrap();
    model.settle(request, Some(page(request, 32)), false);
    assert_eq!(model.focus.unwrap().query, query(3, 4000));
    model.replace(query(4, 3999));
    assert!(model.focus.is_none());
    assert!(model.target.is_none());
}

#[test]
fn pending_focus_refresh_authenticates_current_parent_and_clears_foreign_selection() {
    let mut model = model::LineageModel::new(query(1, 32));
    let request = model.request(0).unwrap();
    model.settle(request, Some(page(request, 32)), false);
    model.focus_position(0);
    model.replace(query(2, 32));
    let stale = model.request(0).unwrap();
    model.replace(query(3, 32));
    assert!(!model.settle(stale, Some(page(stale, 32)), false));
    assert_eq!(model.target, Some((0, Some(id(1)))));
    let request = model.request(0).unwrap();
    let mut changed = page(request, 32);
    changed.rows[0].thread = id(900);
    assert!(model.settle(request, Some(changed), false));
    assert!(model.focus.is_none());
    assert!(model.target.is_none());
    model.focus_position(0);
    model.replace(query(4, 32));
    let mut foreign = query(5, 32);
    foreign.selected = id(901);
    model.replace(foreign);
    assert!(model.focus.is_none());
    assert!(model.target.is_none());
}

#[test]
fn keyless_pending_movement_cannot_restore_prior_settled_parent_on_refresh() {
    let mut model = model::LineageModel::new(query(1, 4000));
    let request = model.request(0).unwrap();
    model.settle(request, Some(page(request, 32)), false);
    model.focus_position(0);
    assert_eq!(model.focus.unwrap().thread, id(1));
    assert_eq!(model.movement(LineageMovement::End), Some(3999));
    assert_eq!(model.target, Some((3999, None)));
    model.replace(query(2, 4000));
    assert!(model.focus.is_none());
    assert!(model.target.is_none());
    let request = model.request(0).unwrap();
    assert!(model.settle(request, Some(page(request, 32)), false));
    assert!(model.focus.is_none());
}

#[path = "thread_lineage/mounted.rs"]
mod mounted;

impl ThreadLineage {
    pub(crate) fn test_readiness(&self) -> String {
        format!(
            "query={:?}, title={:?}, inert={}, gate={:?}, failure={}, paused={}, requests={:?}, resident={}, first_row={:?}, first_ordinal={}, bounds={:?}, handles={}",
            self.model.query,
            self.current_title,
            self.inert,
            self.inert_reason,
            self.model.failure,
            self.model.paused,
            self.model.requests,
            self.model.resident_count(),
            self.model.row(0),
            self.first_ordinal,
            self.bounds,
            self.handles.len(),
        )
    }
    pub(crate) fn test_parent_input(
        &self,
        parent: beryl_model::SyndicThreadId,
        _: &Window,
    ) -> Option<(gpui::Point<gpui::Pixels>, FocusHandle, bool)> {
        let ordinal = self.ranges().0.find(|ordinal| {
            self.model
                .row(*ordinal)
                .is_some_and(|row| row.thread == parent)
        })?;
        let handle = self
            .handles
            .iter()
            .find(|(key, _)| *key == parent)?
            .1
            .clone();
        let point = point(
            self.bounds.origin.x
                + px(
                    (ordinal - self.first_ordinal) as f32 * self.stride() - self.fraction
                        + self.breadcrumb_width() / 2.,
                ),
            self.bounds.center().y,
        );
        Some((
            point,
            handle,
            !self.inert && !self.model.failure && self.model.row(ordinal)?.reason.is_none(),
        ))
    }
    pub(crate) fn test_focus_parent(
        &mut self,
        parent: beryl_model::SyndicThreadId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let ordinal = self
            .ranges()
            .0
            .find(|ordinal| {
                self.model
                    .row(*ordinal)
                    .is_some_and(|row| row.thread == parent)
            })
            .unwrap();
        let (_, handle, _) = self.test_parent_input(parent, window).unwrap();
        self.model.focus_position(ordinal);
        handle.focus(window);
        cx.notify();
    }
    pub(crate) fn test_current_title(&self) -> &str {
        &self.current_title
    }
    pub(crate) fn test_inert(&self) -> bool {
        self.inert
    }
    pub(crate) fn test_logical_focus(
        &self,
        parent: beryl_model::SyndicThreadId,
        window: &Window,
    ) -> bool {
        self.focused_owner(window).is_some()
            && (self
                .model
                .focus
                .is_some_and(|focus| focus.query == self.model.query && focus.thread == parent)
                || self
                    .model
                    .target
                    .is_some_and(|(_, key)| key == Some(parent)))
    }
}
