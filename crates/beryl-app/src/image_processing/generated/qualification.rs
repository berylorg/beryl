use std::sync::Arc;

use beryl_home_store::HomeServiceReference;
use beryl_state::BerylState;
use syndic_storage::SyndicStorage;

use super::GeneratedMediaCoordinator;
use crate::{
    cas_projection::{ProjectionConnectionService, initial_start::InitialStartOwner},
    image_processing::{ImageMediaProcess, MediaResourceConfiguration, MediaResourceLimits},
};

mod presentation;
pub use presentation::{
    GeneratedPresentation, GeneratedResolution, inspect_generated_presentation,
    inspect_generated_presentation_after_preparation, resolve_generated_presentation,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Snapshot {
    pub queued: usize,
    pub active: usize,
    pub idle: bool,
    pub workers: usize,
    pub path_slots: usize,
    pub verification_slots: usize,
}

pub struct Coordinator {
    coordinator: GeneratedMediaCoordinator,
    process: Arc<ImageMediaProcess>,
}

pub fn domain_configuration() -> gpui::ImageResourceDomainConfig {
    let config = MediaResourceConfiguration::default();
    gpui::ImageResourceDomainConfig {
        staging_buffers: config.upload_staging_buffers,
        staging_bytes: config.upload_staging_bytes,
        texture_entries: config.gpu_texture_entries,
        texture_bytes: config.gpu_texture_bytes as u64,
        window_pin_entries: config.window_pin_entries,
        window_pin_bytes: config.window_pin_gpu_bytes as u64,
    }
}

impl Coordinator {
    pub fn start(
        home: HomeServiceReference,
        syndic: SyndicStorage,
        state: &BerylState,
        service: &ProjectionConnectionService,
        domain: gpui::ImageResourceDomain,
        pause_after_preparation: bool,
    ) -> Result<Self, String> {
        let limits = MediaResourceLimits::new(MediaResourceConfiguration::default())
            .map_err(|error| error.to_string())?;
        let process =
            ImageMediaProcess::start(domain, limits).map_err(|error| error.to_string())?;
        let start = InitialStartOwner::new();
        let coordinator = GeneratedMediaCoordinator::prepare(
            Arc::new(home),
            syndic,
            state.assets(),
            service.generated_source_authority(),
            process.clone(),
            start.gate(),
        )
        .map_err(|error| error.to_string())?;
        if pause_after_preparation {
            coordinator.pause_preparation();
        }
        service.install_generated_media_waker(coordinator.waker());
        assert!(start.release());
        Ok(Self {
            coordinator,
            process,
        })
    }

    pub fn snapshot(&self) -> Snapshot {
        let (queued, active, idle, workers) = self.coordinator.diagnostics();
        let process = self
            .process
            .diagnostics()
            .expect("live qualification process");
        Snapshot {
            queued,
            active,
            idle,
            workers,
            path_slots: process.path_slots,
            verification_slots: process.verification_slots,
        }
    }

    pub fn preparation_reached(&self) -> bool {
        self.coordinator.preparation_reached()
    }
    pub fn release_preparation(&self) {
        self.coordinator.release_preparation();
    }
    pub fn notify(&self) {
        self.coordinator.waker().wake_by_ref();
    }
    pub fn begin_retirement(&self) {
        self.coordinator.begin_retirement();
    }
    pub fn join_retirement(&mut self) -> Result<(), String> {
        self.coordinator
            .join_retirement()
            .map_err(|error| error.to_string())
    }
    pub fn retire_process(&self) -> bool {
        self.process.begin_shutdown() == gpui::ImageResourceDrain::Drained
    }
}
