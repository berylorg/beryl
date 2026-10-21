use super::ImagePreparationError;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct MediaResourceConfiguration {
    pub(crate) generated_media_queue_items: usize,
    pub(crate) generated_media_worker_slots: usize,
    pub(crate) generated_media_page_bytes: usize,
    pub(crate) paste_admission_queue_items: usize,
    pub(crate) original_command_slots: usize,
    pub(crate) original_clipboard_total_bytes: usize,
    pub(crate) original_clipboard_metadata_bytes: usize,
    pub(crate) path_preparation_slots: usize,
    pub(crate) sidecar_verification_slots: usize,
    pub(crate) sidecar_io_page_bytes: usize,
    pub(crate) decoder_slots: usize,
    pub(crate) decoder_queue_items: usize,
    pub(crate) surface_max_bytes: usize,
    pub(crate) cpu_surface_entries: usize,
    pub(crate) cpu_surface_bytes: usize,
    pub(crate) upload_staging_buffers: usize,
    pub(crate) upload_staging_bytes: usize,
    pub(crate) gpu_texture_entries: usize,
    pub(crate) gpu_texture_bytes: usize,
    pub(crate) window_pin_entries: usize,
    pub(crate) window_pin_gpu_bytes: usize,
    pub(crate) rendition_source_max_bytes: usize,
    pub(crate) rendition_source_max_pixels: usize,
    pub(crate) rendition_source_max_dimension: usize,
    pub(crate) decoder_row_max_bytes: usize,
    pub(crate) decoder_processed_chunk_max_bytes: usize,
    pub(crate) rendition_max_dimension: usize,
}

impl Default for MediaResourceConfiguration {
    fn default() -> Self {
        Self {
            generated_media_queue_items: 8,
            generated_media_worker_slots: 2,
            generated_media_page_bytes: 65_536,
            paste_admission_queue_items: 1,
            original_command_slots: 1,
            original_clipboard_total_bytes: 67_108_864,
            original_clipboard_metadata_bytes: 4_096,
            path_preparation_slots: 2,
            sidecar_verification_slots: 2,
            sidecar_io_page_bytes: 4_096,
            decoder_slots: 2,
            decoder_queue_items: 16,
            surface_max_bytes: 4_194_304,
            cpu_surface_entries: 16,
            cpu_surface_bytes: 67_108_864,
            upload_staging_buffers: 2,
            upload_staging_bytes: 8_388_608,
            gpu_texture_entries: 32,
            gpu_texture_bytes: 134_217_728,
            window_pin_entries: 8,
            window_pin_gpu_bytes: 33_554_432,
            rendition_source_max_bytes: 67_108_864,
            rendition_source_max_pixels: 67_108_864,
            rendition_source_max_dimension: 16_384,
            decoder_row_max_bytes: 262_144,
            decoder_processed_chunk_max_bytes: 1_048_576,
            rendition_max_dimension: 1_024,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct MediaResourceLimits(MediaResourceConfiguration);

impl MediaResourceLimits {
    pub(crate) fn new(config: MediaResourceConfiguration) -> Result<Self, ImagePreparationError> {
        let capacities = [
            config.generated_media_queue_items,
            config.generated_media_worker_slots,
            config.generated_media_page_bytes,
            config.paste_admission_queue_items,
            config.original_command_slots,
            config.original_clipboard_total_bytes,
            config.original_clipboard_metadata_bytes,
            config.path_preparation_slots,
            config.sidecar_verification_slots,
            config.sidecar_io_page_bytes,
            config.decoder_slots,
            config.decoder_queue_items,
            config.surface_max_bytes,
            config.cpu_surface_entries,
            config.cpu_surface_bytes,
            config.upload_staging_buffers,
            config.upload_staging_bytes,
            config.gpu_texture_entries,
            config.gpu_texture_bytes,
            config.window_pin_entries,
            config.window_pin_gpu_bytes,
            config.rendition_source_max_bytes,
            config.rendition_source_max_pixels,
            config.rendition_source_max_dimension,
            config.decoder_row_max_bytes,
            config.decoder_processed_chunk_max_bytes,
            config.rendition_max_dimension,
        ];
        let maximum_surface = config
            .rendition_max_dimension
            .checked_mul(config.rendition_max_dimension)
            .and_then(|pixels| pixels.checked_mul(4))
            .ok_or(ImagePreparationError::Configuration)?;
        let valid = capacities
            .iter()
            .all(|capacity| *capacity > 0 && *capacity <= isize::MAX as usize)
            && config.sidecar_io_page_bytes >= 13
            && config.original_command_slots == 1
            && config.original_clipboard_metadata_bytes >= 24
            && config.original_clipboard_metadata_bytes < config.original_clipboard_total_bytes
            && config.decoder_processed_chunk_max_bytes >= 13
            && config.rendition_source_max_dimension <= u32::MAX as usize
            && config.rendition_max_dimension <= config.rendition_source_max_dimension
            && [
                config.surface_max_bytes,
                config.cpu_surface_bytes,
                config.upload_staging_bytes,
                config.gpu_texture_bytes,
                config.window_pin_gpu_bytes,
            ]
            .iter()
            .all(|capacity| *capacity >= maximum_surface);
        if !valid {
            return Err(ImagePreparationError::Configuration);
        }
        config
            .window_pin_entries
            .checked_mul(beryl_state::MAX_RESTORABLE_WINDOWS)
            .ok_or(ImagePreparationError::Configuration)?;
        config
            .generated_media_worker_slots
            .checked_mul(config.generated_media_page_bytes)
            .and_then(|_| {
                config
                    .decoder_slots
                    .checked_mul(config.decoder_row_max_bytes)
            })
            .and_then(|_| {
                config
                    .paste_admission_queue_items
                    .checked_mul(config.sidecar_io_page_bytes)
            })
            .ok_or(ImagePreparationError::Configuration)?;
        Ok(Self(config))
    }

    pub(crate) fn configuration(self) -> MediaResourceConfiguration {
        self.0
    }

    pub(crate) fn fitted_extent(
        self,
        width: u32,
        height: u32,
    ) -> Result<[u32; 2], ImagePreparationError> {
        if width == 0 || height == 0 {
            return Err(ImagePreparationError::Malformed);
        }
        let maximum = self.0.rendition_max_dimension as u64;
        let major = u64::from(width.max(height));
        if major <= maximum {
            return Ok([width, height]);
        }
        let fit = |dimension: u32| -> Result<u32, ImagePreparationError> {
            let scaled = u64::from(dimension)
                .checked_mul(maximum)
                .ok_or(ImagePreparationError::Capacity)?
                / major;
            u32::try_from(scaled.max(1)).map_err(|_| ImagePreparationError::Capacity)
        };
        Ok([fit(width)?, fit(height)?])
    }
}
