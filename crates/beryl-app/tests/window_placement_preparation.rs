use beryl_app::main_window::{
    WindowPlacementMonitor, WindowPlacementRect, resolve_window_placement,
};
use beryl_model::{
    MonitorHint, MonitorId, VirtualDesktopId, WindowBounds, WindowDisplayState, WindowId,
    WindowPlacement,
};

fn window() -> WindowId {
    WindowId::from_bytes([31; 16])
}

fn rect(x: f64, y: f64, width: f64, height: f64) -> WindowPlacementRect {
    WindowPlacementRect {
        x,
        y,
        width,
        height,
    }
}

fn monitor(id: &str, work_area: WindowPlacementRect) -> WindowPlacementMonitor {
    WindowPlacementMonitor {
        id: MonitorId::new(id).unwrap(),
        work_area,
    }
}

fn saved(
    x: i32,
    y: i32,
    width: u32,
    height: u32,
    hint: Option<(&str, WindowBounds)>,
) -> WindowPlacement {
    WindowPlacement::new(
        WindowBounds::new(x, y, width, height).unwrap(),
        WindowDisplayState::Maximized,
        hint.map(|(id, work)| MonitorHint::new(MonitorId::new(id).unwrap(), work)),
        Some(VirtualDesktopId::from_bytes([77; 16])),
    )
}

#[test]
fn unchanged_placement_keeps_geometry_state_desktop_and_binding() {
    let original = saved(120, 90, 800, 600, None);
    let resolved = resolve_window_placement(
        window(),
        original.clone(),
        [monitor("a", rect(0., 0., 1920., 1080.))],
    )
    .unwrap();
    assert_eq!(resolved.bounds(), rect(120., 90., 800., 600.));
    assert_eq!(resolved.saved_placement(), &original);
    assert_eq!(resolved.window_id(), window());
    assert_eq!(resolved.monitor_id().as_str(), "a");
    resolved.validate_binding(window(), &original).unwrap();
    assert!(
        resolved
            .validate_binding(WindowId::from_bytes([32; 16]), &original)
            .is_err()
    );
    assert!(
        resolved
            .validate_binding(window(), &saved(121, 90, 800, 600, None))
            .is_err()
    );
    let normal = WindowPlacement::new(original.bounds(), WindowDisplayState::Normal, None, None);
    let normal_result = resolve_window_placement(
        window(),
        normal.clone(),
        [monitor("a", rect(0., 0., 1920., 1080.))],
    )
    .unwrap();
    assert_eq!(normal_result.saved_placement(), &normal);
}

#[test]
fn saved_monitor_identity_wins_and_translates_old_work_area_offset() {
    let original = saved(
        100,
        80,
        600,
        400,
        Some(("saved", WindowBounds::new(0, 0, 1920, 1080).unwrap())),
    );
    let resolved = resolve_window_placement(
        window(),
        original,
        [
            monitor("overlap", rect(0., 0., 1920., 1080.)),
            monitor("saved", rect(-1920., 40., 1920., 1040.)),
        ],
    )
    .unwrap();
    assert_eq!(resolved.monitor_id().as_str(), "saved");
    assert_eq!(resolved.bounds(), rect(-1820., 120., 600., 400.));
}

#[test]
fn missing_monitor_uses_greatest_intersection_before_distance() {
    let original = saved(
        800,
        100,
        400,
        500,
        Some(("gone", WindowBounds::new(0, 0, 1000, 800).unwrap())),
    );
    let resolved = resolve_window_placement(
        window(),
        original,
        [
            monitor("smaller", rect(0., 0., 950., 800.)),
            monitor("larger", rect(950., 0., 8000., 800.)),
        ],
    )
    .unwrap();
    assert_eq!(resolved.monitor_id().as_str(), "larger");
    assert_eq!(resolved.bounds(), rect(950., 100., 400., 500.));
}

#[test]
fn offscreen_windows_choose_nearest_center_and_clamp_reachable() {
    let resolved = resolve_window_placement(
        window(),
        saved(4000, 100, 600, 400, None),
        [
            monitor("far", rect(-1920., 0., 1920., 1080.)),
            monitor("near", rect(0., 0., 1920., 1080.)),
        ],
    )
    .unwrap();
    assert_eq!(resolved.monitor_id().as_str(), "near");
    assert_eq!(resolved.bounds(), rect(1320., 100., 600., 400.));
}

#[test]
fn monitor_ties_do_not_depend_on_enumeration_order() {
    for ids in [["z", "a"], ["a", "z"]] {
        let resolved = resolve_window_placement(
            window(),
            saved(20, 20, 200, 100, None),
            ids.into_iter()
                .map(|id| monitor(id, rect(0., 0., 800., 600.))),
        )
        .unwrap();
        assert_eq!(resolved.monitor_id().as_str(), "a");
    }
}

#[test]
fn oversized_extreme_saved_rectangle_is_bounded_by_fractional_work_area() {
    let work = rect(-1536.4, -864.8, 1500.2, 830.6);
    let resolved = resolve_window_placement(
        window(),
        saved(i32::MAX, i32::MIN, u32::MAX, u32::MAX, None),
        [monitor("fractional", work)],
    )
    .unwrap();
    assert_eq!(resolved.bounds(), work);
}

#[test]
fn moved_and_shrunk_saved_monitor_clamps_both_edges() {
    let original = saved(
        1500,
        800,
        400,
        240,
        Some(("a", WindowBounds::new(0, 0, 1920, 1080).unwrap())),
    );
    let resolved = resolve_window_placement(
        window(),
        original,
        [monitor("a", rect(-800., -600., 800., 600.))],
    )
    .unwrap();
    assert_eq!(resolved.bounds(), rect(-400., -240., 400., 240.));
}

#[test]
fn no_monitors_and_invalid_monitor_facts_fail_explicitly() {
    assert!(resolve_window_placement(window(), saved(0, 0, 200, 100, None), []).is_err());
    for work in [
        rect(f64::NAN, 0., 800., 600.),
        rect(0., f64::INFINITY, 800., 600.),
        rect(0., 0., 0., 600.),
        rect(0., 0., 800., -1.),
        rect(f64::MAX, 0., f64::MAX, 600.),
    ] {
        assert!(
            resolve_window_placement(
                window(),
                saved(0, 0, 200, 100, None),
                [
                    monitor("valid", rect(0., 0., 800., 600.)),
                    monitor("invalid", work)
                ]
            )
            .is_err()
        );
    }
}

#[cfg(target_os = "windows")]
#[test]
fn worker_preparation_binds_actual_native_monitor_to_resolved_geometry() {
    let original = saved(100, 100, 800, 600, None);
    let saved_for_worker = original.clone();
    let prepared = std::thread::spawn(move || {
        beryl_app::main_window::prepare_windows_window_placement(window(), saved_for_worker)
    })
    .join()
    .unwrap()
    .unwrap();
    prepared.validate_binding(window(), &original).unwrap();
    assert_eq!(
        prepared.resolved().monitor_id().as_str(),
        prepared.monitor().uuid().to_string()
    );
    assert_eq!(prepared.resolved().saved_placement(), &original);
    let bounds = prepared.resolved().bounds();
    let work = prepared.monitor().work_area();
    let scale = f64::from(prepared.monitor().scale_factor());
    assert!(bounds.x >= f64::from(work.origin.x.0) / scale);
    assert!(bounds.y >= f64::from(work.origin.y.0) / scale);
    assert!(
        bounds.x + bounds.width <= f64::from(work.origin.x.0 + work.size.width.0) / scale + 1e-8
    );
    assert!(
        bounds.y + bounds.height <= f64::from(work.origin.y.0 + work.size.height.0) / scale + 1e-8
    );
}
