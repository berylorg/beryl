use gpui::Pixels;

use super::layout;

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ComposerInputMeasurementKey {
    input_revision: u64,
    image_atom_revision: u64,
    conversation_column_width: Pixels,
    main_region_height: Pixels,
    viewport_height: Pixels,
    scale_factor_bits: u32,
    theme_revision: u64,
    enabled: bool,
    thread_edit_mode_active: bool,
}

#[derive(Default)]
pub(crate) struct ComposerInputMeasurementCache {
    entry: Option<ComposerInputMeasurementCacheEntry>,
}

struct ComposerInputMeasurementCacheEntry {
    key: ComposerInputMeasurementKey,
    measurement: layout::ComposerInputMeasurement,
}

impl ComposerInputMeasurementKey {
    pub(crate) fn new(
        input_revision: u64,
        image_atom_revision: u64,
        conversation_column_width: Pixels,
        main_region_height: Pixels,
        viewport_height: Pixels,
        scale_factor: f32,
        theme_revision: u64,
        enabled: bool,
        thread_edit_mode_active: bool,
    ) -> Self {
        Self {
            input_revision,
            image_atom_revision,
            conversation_column_width,
            main_region_height,
            viewport_height,
            scale_factor_bits: scale_factor.to_bits(),
            theme_revision,
            enabled,
            thread_edit_mode_active,
        }
    }
}

impl ComposerInputMeasurementCache {
    pub(crate) fn measure_or_insert_with(
        &mut self,
        key: ComposerInputMeasurementKey,
        measure: impl FnOnce() -> layout::ComposerInputMeasurement,
    ) -> layout::ComposerInputMeasurement {
        if let Some(entry) = &self.entry
            && entry.key == key
        {
            return entry.measurement.clone();
        }

        let measurement = measure();
        self.entry = Some(ComposerInputMeasurementCacheEntry {
            key,
            measurement: measurement.clone(),
        });
        measurement
    }
}
