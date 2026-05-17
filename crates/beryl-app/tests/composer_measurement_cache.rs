mod shell {
    #[allow(dead_code)]
    #[path = "../../src/shell/layout.rs"]
    pub(crate) mod layout;

    #[allow(dead_code)]
    #[path = "../../src/shell/composer_measurement.rs"]
    pub(crate) mod composer_measurement;
}

use gpui::{Bounds, point, px, size};
use gpui_text_input::{TextInputScrollLimits, TextInputVerticalReveal};
use shell::composer_measurement::{ComposerInputMeasurementCache, ComposerInputMeasurementKey};
use shell::layout;

#[test]
fn composer_measurement_cache_reuses_matching_layout_key() {
    let mut cache = ComposerInputMeasurementCache::default();
    let key = key(1, 1, px(360.0), px(500.0), px(760.0), 1.0, 3, true, false);
    let mut calls = 0usize;

    let first = cache.measure_or_insert_with(key.clone(), || {
        calls += 1;
        measurement(px(80.0))
    });
    let second = cache.measure_or_insert_with(key, || {
        calls += 1;
        measurement(px(120.0))
    });

    assert_eq!(calls, 1);
    assert_eq!(first.composer_height, px(80.0));
    assert_eq!(second.composer_height, px(80.0));
}

#[test]
fn composer_measurement_cache_invalidates_all_visible_layout_inputs() {
    let base = key(1, 1, px(360.0), px(500.0), px(760.0), 1.0, 3, true, false);
    let variants = [
        key(2, 1, px(360.0), px(500.0), px(760.0), 1.0, 3, true, false),
        key(1, 2, px(360.0), px(500.0), px(760.0), 1.0, 3, true, false),
        key(1, 1, px(320.0), px(500.0), px(760.0), 1.0, 3, true, false),
        key(1, 1, px(360.0), px(420.0), px(760.0), 1.0, 3, true, false),
        key(1, 1, px(360.0), px(500.0), px(640.0), 1.0, 3, true, false),
        key(1, 1, px(360.0), px(500.0), px(760.0), 1.25, 3, true, false),
        key(1, 1, px(360.0), px(500.0), px(760.0), 1.0, 4, true, false),
        key(1, 1, px(360.0), px(500.0), px(760.0), 1.0, 3, false, false),
        key(1, 1, px(360.0), px(500.0), px(760.0), 1.0, 3, true, true),
    ];

    for (index, variant) in variants.into_iter().enumerate() {
        let mut cache = ComposerInputMeasurementCache::default();
        let mut calls = 0usize;
        let _ = cache.measure_or_insert_with(base.clone(), || {
            calls += 1;
            measurement(px(80.0))
        });
        let next = cache.measure_or_insert_with(variant, || {
            calls += 1;
            measurement(px(120.0 + index as f32))
        });

        assert_eq!(calls, 2);
        assert!(next.composer_height > px(80.0));
    }
}

fn key(
    input_revision: u64,
    image_atom_revision: u64,
    conversation_column_width: gpui::Pixels,
    main_region_height: gpui::Pixels,
    viewport_height: gpui::Pixels,
    scale_factor: f32,
    theme_revision: u64,
    enabled: bool,
    thread_edit_mode_active: bool,
) -> ComposerInputMeasurementKey {
    ComposerInputMeasurementKey::new(
        input_revision,
        image_atom_revision,
        conversation_column_width,
        main_region_height,
        viewport_height,
        scale_factor,
        theme_revision,
        enabled,
        thread_edit_mode_active,
    )
}

fn measurement(composer_height: gpui::Pixels) -> layout::ComposerInputMeasurement {
    layout::ComposerInputMeasurement {
        input_bounds: Bounds::new(point(px(0.0), px(0.0)), size(px(320.0), px(20.0))),
        composer_height,
        visible_input_height: px(56.0),
        visible_text_height: px(42.0),
        text_content_height: px(20.0),
        input_render_height: px(20.0),
        text_top_padding: px(11.0),
        visual_line_count: 1,
        scroll_limits: TextInputScrollLimits {
            max_x: px(0.0),
            max_y: px(0.0),
        },
        vertical_reveal: Some(TextInputVerticalReveal {
            scroll_y: px(0.0),
            max_scroll_y: px(0.0),
        }),
    }
}
