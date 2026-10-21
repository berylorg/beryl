use std::{sync::atomic::Ordering, time::{SystemTime, UNIX_EPOCH}};

use beryl_home_store::{CommandOutcome, HomeCommand};
use beryl_model::{GeneratedMediaUnavailableReason as Reason, SyndicResourceId};
use beryl_state::{GeneratedMediaAdmission, GeneratedMediaError, GeneratedMediaFacts, GeneratedMediaTarget};
use syndic_storage::{GeneratedMediaSourceProof, SyndicTimestamp};

use crate::cas_projection::GeneratedSourceEligibility;
use super::{coordinator::{Services, Result, failure, source_read_failure, admission_read_failure}, source::{self, Admission}};

fn target(proof: &GeneratedMediaSourceProof) -> GeneratedMediaTarget {
    GeneratedMediaTarget::new(proof.resource_id(), proof.thread_id(), proof.turn_id(), proof.item_id(), proof.source_fingerprint())
}

fn dimensions(facts: &GeneratedMediaFacts) -> Option<[u32; 2]> {
    facts.dimensions().map(|dimensions| [
        u32::try_from(dimensions.width().get()).expect("generated width bound"),
        u32::try_from(dimensions.height().get()).expect("generated height bound"),
    ])
}

fn timestamp() -> SyndicTimestamp {
    SyndicTimestamp::from_unix_millis(SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis().min(u128::from(u64::MAX)) as u64)
}

fn command(services: &Services) -> Result<HomeCommand> {
    Ok(HomeCommand::new(services.home.home_revision().map_err(failure)?).with_cancellation(services.shared.command_cancel.clone()))
}

fn execute(services: &Services, command: HomeCommand) -> bool {
    match services.home.execute(command) {
        CommandOutcome::Committed { .. } => true,
        CommandOutcome::NotCommitted { .. } => false,
        CommandOutcome::Indeterminate { reconciliation, .. } => {
            reconciliation.install();
            false
        }
    }
}

pub(super) fn discoverable(services: &Services, resource: SyndicResourceId) -> Result<bool> {
    let Some(proof) = services.syndic.generated_media_source(&services.home, resource).map_err(source_read_failure)? else { return Ok(false); };
    if proof.saved_path().is_some() { return Ok(true); }
    match services.authority.authorize(&proof) {
        Err(GeneratedSourceEligibility::LostSourceAuthority) => Ok(true),
        Err(GeneratedSourceEligibility::Internal(error)) => Err(failure(error)),
        _ => Ok(false),
    }
}

pub(super) fn finalize(services: &Services, resource: SyndicResourceId) -> Result<bool> {
    if services.shared.cancelled.load(Ordering::Acquire) { return Ok(false); }
    let Some(proof) = services.syndic.generated_media_source(&services.home, resource).map_err(source_read_failure)? else { return Ok(false); };
    if let Some(admission) = services.assets.generated_media_admission(&services.home, target(&proof)).map_err(admission_read_failure)? {
        return publish(services, proof, &admission);
    }
    let authorized = match services.authority.authorize(&proof) {
        Ok(authorized) => authorized,
        Err(GeneratedSourceEligibility::Busy) => return Ok(false),
        Err(GeneratedSourceEligibility::LostSourceAuthority) => return unavailable(services, proof, Reason::LostSourceAuthority),
        Err(GeneratedSourceEligibility::Backend(error)) => return match source::classify(error) {
            Some(reason) if proof.saved_path().is_some() => unavailable(services, proof, reason),
            _ => Ok(false),
        },
        Err(GeneratedSourceEligibility::Internal(error)) => return Err(failure(error)),
    };
    match source::admit(services, &proof, authorized)? {
        Admission::Pending => Ok(false),
        Admission::Unavailable(reason) => unavailable(services, proof, reason),
        Admission::Prepared(sidecar, facts) => {
            if services.shared.cancelled.load(Ordering::Acquire) { return Ok(false); }
            let original_target = target(&proof);
            let Some(proof) = services.syndic.generated_media_source(&services.home, resource).map_err(source_read_failure)? else { return Ok(false); };
            if target(&proof) != original_target { return Ok(false); }
            let revision = services.assets.revision(&services.home).map_err(failure)?;
            let preparation = match services.assets.prepare_generated_media(&services.home, revision, sidecar, target(&proof), facts) {
                Ok(preparation) => preparation,
                Err(GeneratedMediaError::NoEffect) => return resume(services, resource),
                Err(GeneratedMediaError::Rejected) => return Ok(false),
                Err(error) => return Err(admission_read_failure(error)),
            };
            let effective = preparation.facts();
            let syndic = services.syndic.generated_media_preparation(proof, effective.asset(), effective.media_type().as_str(), dimensions(effective)).map_err(failure)?;
            let mut command = command(services)?;
            preparation.add_to(&mut command).map_err(failure)?;
            command.add_validation(syndic).map_err(failure)?;
            if execute(services, command) {
                #[cfg(feature = "test-faults")]
                services.shared.preparation_checkpoint();
                if services.shared.cancelled.load(Ordering::Acquire) { return Ok(false); }
                resume(services, resource)
            } else { Ok(false) }
        }
    }
}

fn resume(services: &Services, resource: SyndicResourceId) -> Result<bool> {
    let Some(proof) = services.syndic.generated_media_source(&services.home, resource).map_err(source_read_failure)? else { return Ok(true); };
    let Some(admission) = services.assets.generated_media_admission(&services.home, target(&proof)).map_err(admission_read_failure)? else { return Ok(false); };
    publish(services, proof, &admission)
}

fn publish(services: &Services, proof: GeneratedMediaSourceProof, admission: &GeneratedMediaAdmission) -> Result<bool> {
    let revision = services.assets.revision(&services.home).map_err(failure)?;
    let asset = match services.assets.publish_generated_media(&services.home, revision, admission) {
        Ok(asset) => asset,
        Err(GeneratedMediaError::NoEffect | GeneratedMediaError::Rejected) => return Ok(false),
        Err(error) => return Err(admission_read_failure(error)),
    };
    let facts = admission.facts();
    let syndic = services.syndic.publish_generated_media(proof, facts.asset(), facts.media_type().as_str(), dimensions(facts), timestamp()).map_err(failure)?;
    let mut command = command(services)?;
    command.add(asset).map_err(failure)?;
    command.add(syndic).map_err(failure)?;
    Ok(execute(services, command))
}

fn unavailable(services: &Services, proof: GeneratedMediaSourceProof, reason: Reason) -> Result<bool> {
    let revision = services.assets.revision(&services.home).map_err(failure)?;
    let asset = services.assets.validate_generated_media_absence(revision, target(&proof), reason);
    let syndic = services.syndic.unavailable_generated_media(proof, reason, timestamp()).map_err(failure)?;
    let mut command = command(services)?;
    command.add_validation(asset).map_err(failure)?;
    command.add(syndic).map_err(failure)?;
    Ok(execute(services, command))
}
