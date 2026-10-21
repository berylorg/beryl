use std::panic::{AssertUnwindSafe, catch_unwind};

use gpui::{ImageUploadAdmission, Rgba8ImageLayout};

use super::super::{decode_png_into, preflight_png};
use super::*;

pub(super) fn run(core: Arc<ProcessCore>) {
    loop {
        if let Some((operation, attachment)) = original::claim(&core) {
            original::run(&core, operation, attachment);
            continue;
        }
        let job = {
            let mut state = lock(&core.state);
            while state.queue.is_empty() && !original::has_work(&state) && !state.stopping {
                state = core
                    .work
                    .wait(state)
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
            }
            if state.stopping {
                break;
            }
            if original::has_work(&state) {
                continue;
            }
            let Some(queued) = state.queue.pop_front() else {
                continue;
            };
            let (Some(attachment), Some(interest)) =
                (queued.attachment.upgrade(), queued.interest.upgrade())
            else {
                continue;
            };
            if attachment.retired.load(Ordering::Acquire) || !interest.is_live() {
                continue;
            }
            let Some(source) = lock(&attachment.source).clone() else {
                continue;
            };
            state.active_workers += 1;
            attachment.active_workers.fetch_add(1, Ordering::AcqRel);
            (attachment, interest, source, queued.demand)
        };
        let (attachment, interest, source, demand) = job;
        let lease = WorkerLease {
            core: core.clone(),
            attachment: attachment.clone(),
        };
        let result = catch_unwind(AssertUnwindSafe(|| {
            prepare(&core, &attachment, &interest, &source, demand)
        }));
        if let Err(error) = result.unwrap_or(Err(ImageMediaError(ImageMediaFailure::Worker))) {
            fail_job(&core, &interest, error.0);
        }
        drop(source);
        drop(lease);
    }
    core.event.notify();
}

struct WorkerLease {
    core: Arc<ProcessCore>,
    attachment: Arc<AttachmentState>,
}
impl Drop for WorkerLease {
    fn drop(&mut self) {
        lock(&self.core.state).active_workers -= 1;
        self.attachment
            .active_workers
            .fetch_sub(1, Ordering::AcqRel);
        self.attachment.event.notify();
        self.core.event.notify();
    }
}

pub(super) struct SourceStageLease {
    core: Arc<ProcessCore>,
}
impl SourceStageLease {
    pub(super) fn acquire(core: &Arc<ProcessCore>) -> Result<Self, ImageMediaError> {
        let mut state = lock(&core.state);
        let config = core.limits.configuration();
        if state.stopping
            || state.path_slots >= config.path_preparation_slots
            || state.verification_slots >= config.sidecar_verification_slots
        {
            return Err(ImageMediaError(ImageMediaFailure::Capacity));
        }
        state.path_slots += 1;
        state.verification_slots += 1;
        Ok(Self { core: core.clone() })
    }
}
impl Drop for SourceStageLease {
    fn drop(&mut self) {
        let mut state = lock(&self.core.state);
        state.path_slots -= 1;
        state.verification_slots -= 1;
        drop(state);
        self.core.event.notify();
    }
}

fn prepare(
    core: &Arc<ProcessCore>,
    attachment: &Arc<AttachmentState>,
    interest: &Arc<InterestState>,
    source: &ImageSourceProvider,
    demand: ImageSourceDemand,
) -> Result<(), ImageMediaError> {
    if core.native_progress.is_exhausted() {
        return Err(ImageMediaError(ImageMediaFailure::Capacity));
    }
    let resolved = source.resolve(&demand, &interest.cancellation)?;
    let dimensions = resolved.dimensions();
    let identity = RenditionIdentity {
        attachment: attachment.id,
        asset: resolved.asset_id(),
        extent: core.limits.fitted_extent(dimensions[0], dimensions[1])?,
    };
    let cancellation = {
        let mut state = lock(&core.state);
        if state.stopping || attachment.retired.load(Ordering::Acquire) || !interest.is_live() {
            return Err(ImageMediaError(ImageMediaFailure::Cancelled));
        }
        if let Some(flight) = state.flights.get_mut(&identity) {
            if flight.cancellation.is_cancelled() {
                return Err(ImageMediaError(ImageMediaFailure::Cancelled));
            }
            flight
                .interests
                .insert(interest.id, Arc::downgrade(interest));
            *lock(&interest.status) = if matches!(flight.phase, FlightPhase::Prepared(_)) {
                InterestStatus::Prepared(identity)
            } else {
                InterestStatus::Flight(identity)
            };
            state.joins = state.joins.saturating_add(1);
            drop(state);
            interest.event.notify();
            return Ok(());
        }
        let cancellation = CommandCancellation::new();
        state.flights.insert(
            identity,
            Flight {
                worker: interest.id,
                cancellation: cancellation.clone(),
                upload_cancellation: None,
                interests: BTreeMap::from([(interest.id, Arc::downgrade(interest))]),
                phase: FlightPhase::Working,
            },
        );
        *lock(&interest.status) = InterestStatus::Flight(identity);
        cancellation
    };
    let layout = Rgba8ImageLayout::new(
        identity.extent[0],
        identity.extent[1],
        identity.extent[0]
            .checked_mul(4)
            .ok_or(ImageMediaError(ImageMediaFailure::Capacity))?,
    )?;
    match attachment.consumer.reserve_upload(
        identity.native_key(),
        layout,
        layout.addressed_bytes(),
    )? {
        ImageUploadAdmission::Resident(handle) => {
            core.finish_flight(identity, interest.id, Ok(handle));
            return Ok(());
        }
        ImageUploadAdmission::Upload(mut ticket) => {
            {
                let mut state = lock(&core.state);
                let flight = state
                    .flights
                    .get_mut(&identity)
                    .ok_or(ImageMediaError(ImageMediaFailure::Cancelled))?;
                if flight.worker != interest.id || cancellation.is_cancelled() {
                    return Err(ImageMediaError(ImageMediaFailure::Cancelled));
                }
                flight.upload_cancellation = Some(ticket.cancellation());
            }
            let surface = if let Some(surface) = core.cpu.get(identity)? {
                surface
            } else {
                let stage = SourceStageLease::acquire(core)?;
                let mut reader = source.open_verified(&resolved, cancellation.clone())?;
                let preflight = preflight_png(
                    &mut reader,
                    identity.asset,
                    dimensions,
                    core.limits,
                    &cancellation,
                )?;
                drop(stage);
                let mut output = core.cpu.reserve(identity)?;
                {
                    let mut state = lock(&core.state);
                    if cancellation.is_cancelled() || state.stopping {
                        return Err(ImageMediaError(ImageMediaFailure::Cancelled));
                    }
                    state.decodes = state.decodes.saturating_add(1);
                }
                decode_png_into(
                    reader,
                    preflight,
                    core.limits,
                    &cancellation,
                    output.pixels_mut(),
                )?;
                let state = lock(&core.state);
                if cancellation.is_cancelled()
                    || attachment.retired.load(Ordering::Acquire)
                    || !state.flights.get(&identity).is_some_and(|flight| {
                        flight.worker == interest.id && flight.has_live_interest()
                    })
                {
                    return Err(ImageMediaError(ImageMediaFailure::Cancelled));
                }
                let published = core.cpu.publish(output)?;
                drop(state);
                published
            };
            if surface.identity() != identity {
                return Err(ImageMediaError(ImageMediaFailure::Malformed));
            }
            let copy_page = core.limits.configuration().sidecar_io_page_bytes;
            let native_cancellation = ticket.cancellation();
            for (destination, pixels) in ticket
                .pixels_mut()?
                .chunks_mut(copy_page)
                .zip(surface.pixels().chunks(copy_page))
            {
                if cancellation.is_cancelled() || native_cancellation.is_cancelled() {
                    return Err(ImageMediaError(ImageMediaFailure::Cancelled));
                }
                destination.copy_from_slice(pixels);
            }
            drop(surface);
            let events = {
                let mut state = lock(&core.state);
                if attachment.retired.load(Ordering::Acquire) || cancellation.is_cancelled() {
                    return Err(ImageMediaError(ImageMediaFailure::Cancelled));
                }
                let flight = state
                    .flights
                    .get_mut(&identity)
                    .ok_or(ImageMediaError(ImageMediaFailure::Cancelled))?;
                if flight.worker != interest.id || !flight.has_live_interest() {
                    return Err(ImageMediaError(ImageMediaFailure::Cancelled));
                }
                flight.phase = FlightPhase::Prepared(ticket);
                flight
                    .interests
                    .values()
                    .filter_map(Weak::upgrade)
                    .map(|interest| {
                        if interest.is_live() {
                            *lock(&interest.status) = InterestStatus::Prepared(identity);
                        }
                        interest.event.clone()
                    })
                    .collect::<Vec<_>>()
            };
            for event in events {
                event.notify();
            }
            core.event.notify();
        }
    }
    Ok(())
}

fn fail_job(core: &ProcessCore, interest: &Arc<InterestState>, error: ImageMediaFailure) {
    let flight = {
        let state = lock(&core.state);
        state
            .flights
            .iter()
            .find(|(_, flight)| flight.worker == interest.id)
            .map(|(identity, _)| *identity)
    };
    if let Some(identity) = flight {
        core.finish_flight(identity, interest.id, Err(error));
    }
    if !interest.cancellation.is_cancelled() {
        *lock(&interest.status) = InterestStatus::Failed(error);
        interest.event.notify();
    }
}
