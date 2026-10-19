#![cfg(feature = "test-faults")]

use beryl_app::activity_panel_test_support::*;
use beryl_model::SyndicItemId;
use std::sync::Arc;

fn id(value: u64) -> SyndicItemId {
    let mut bytes = [73; 16];
    bytes[..8].copy_from_slice(&value.to_le_bytes());
    SyndicItemId::from_bytes(bytes)
}

fn page(start: u64, values: impl IntoIterator<Item = u64>) -> Arc<Page> {
    Arc::new(Page {
        start,
        rows: values
            .into_iter()
            .map(|value| Row {
                id: id(value),
                status: Status::Running,
                agent: "Main".into(),
                value: "bounded display".into(),
                tooltip: "bounded tooltip".into(),
            })
            .collect(),
    })
}

fn query(total: u64) -> Query {
    Query {
        owner: 1,
        period: 1,
        revision: 1,
        total,
    }
}

#[test]
fn large_logical_extent_keeps_one_demand_and_small_realization() {
    let mut model = Model::new(query(1_000_000));
    model.inert = false;
    model.set_viewport(84.);
    model
        .replace(query(1_000_000), vec![page(0, 0..64)])
        .unwrap();
    assert_eq!(model.visible(), 0..3);
    assert_eq!(model.realized_range(), 0..7);
    assert!(model.request().is_none());
    assert_eq!(model.content_height(), 28_000_000.);
    assert!(model.move_to_end(true));
    assert_eq!(model.visible(), 999_997..1_000_000);
    let demand = model.request().unwrap();
    assert_eq!(demand.range, 999_993..1_000_000);
    assert!(model.request().is_none());
    assert!(model.scroll_by(-2800.));
    let replacement = model.request().unwrap();
    assert_ne!(replacement, demand);
    assert_eq!(model.pending.as_ref(), Some(&replacement));
    model
        .replace(query(1_000_000), vec![page(999_893, 999_893..999_957)])
        .unwrap();
    assert!(model.request().is_none());
    assert!(model.pending.is_none());
    assert_eq!(model.pages.len(), 1);
}

#[test]
fn revision_reordering_preserves_manual_anchor_and_period_change_reattaches_top() {
    let mut model = Model::new(query(20));
    model.inert = false;
    model.set_viewport(84.);
    model.replace(query(20), vec![page(0, 0..20)]).unwrap();
    assert!(model.scroll_by(150.));
    assert_eq!((model.first, model.fraction), (5, 10.));
    let revised = Query {
        revision: 2,
        ..query(20)
    };
    let reordered = [
        19, 18, 17, 16, 15, 14, 13, 12, 5, 0, 1, 2, 3, 4, 6, 7, 8, 9, 10, 11,
    ];
    model.replace(revised, vec![page(0, reordered)]).unwrap();
    assert_eq!((model.first, model.fraction), (8, 10.));
    assert_eq!(model.row(model.first).unwrap().id, id(5));
    model
        .replace(
            Query {
                period: 2,
                ..revised
            },
            vec![page(0, 0..20)],
        )
        .unwrap();
    assert_eq!((model.first, model.fraction), (0, 0.));
    assert!(model.top_attached);
    model.scroll_by(140.);
    model
        .set_feedback(Some(Feedback {
            identity: 9,
            explanation: "query feedback".into(),
            pending: false,
            retry_available: true,
        }))
        .unwrap();
    model
        .replace(
            Query {
                owner: 2,
                period: 2,
                ..revised
            },
            vec![page(0, 0..20)],
        )
        .unwrap();
    assert!(model.top_attached);
    assert_eq!((model.first, model.fraction), (0, 0.));
    assert!(model.feedback.is_none());
}

#[test]
fn a_missing_anchor_can_arrive_in_a_later_bounded_page() {
    let mut model = Model::new(query(100));
    model.inert = false;
    model.set_viewport(56.);
    model.replace(query(100), vec![page(0, 0..16)]).unwrap();
    model.scroll_by(150.);
    let revised = Query {
        revision: 2,
        ..query(100)
    };
    model.replace(revised, vec![page(16, 16..32)]).unwrap();
    assert_eq!((model.first, model.fraction), (5, 10.));
    model
        .replace(revised, vec![page(40, [40, 41, 5, 43])])
        .unwrap();
    assert_eq!((model.first, model.fraction), (42, 10.));
}

#[test]
fn fractional_viewports_reach_exact_end_and_wheel_boundaries_propagate() {
    let mut model = Model::new(query(3));
    model.inert = false;
    model.set_viewport(60.);
    assert!(!model.scroll_by(-10.));
    assert!(model.move_to_end(true));
    assert_eq!((model.first, model.fraction), (0, 24.));
    assert_eq!(model.visible(), 0..3);
    assert!(!model.scroll_by(10.));
    assert!(model.move_to_end(false));
    assert!(model.top_attached);
    model.inert = true;
    assert!(!model.scroll_by(10.));
    assert!(!model.move_to_end(true));
    assert!(model.request().is_none());
}

#[test]
fn malformed_or_excessive_owner_pages_never_replace_the_coherent_model() {
    let mut model = Model::new(query(400));
    model.replace(query(400), vec![page(0, 0..64)]).unwrap();
    for pages in [
        vec![page(0, 0..65)],
        vec![page(0, 0..32), page(16, 32..64)],
        vec![page(0, 0..32), page(32, 0..32)],
        vec![page(399, 0..2)],
        (0..5)
            .map(|index| page(index * 64, index * 64..index * 64 + 64))
            .collect(),
    ] {
        assert!(
            model
                .replace(
                    Query {
                        revision: 2,
                        ..query(400)
                    },
                    pages
                )
                .is_err()
        );
        assert_eq!(model.query, query(400));
        assert_eq!(model.pages.len(), 1);
        assert_eq!(model.row(0).unwrap().id, id(0));
    }
    assert!(
        model
            .set_feedback(Some(Feedback {
                identity: 1,
                explanation: "x".repeat(513).into(),
                pending: false,
                retry_available: true,
            }))
            .is_err()
    );
    assert!(model.feedback.is_none());
}
