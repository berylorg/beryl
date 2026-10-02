use super::*;
use beryl_home_store::{HomeCandidateRecoveryAccess, HomeCommand};

pub(in crate::composer_host) fn prepare_asset_plan_candidate(
    access: &HomeCandidateRecoveryAccess<'_>,
    assets: &AssetState,
    candidate: DraftRootHistoryPairV1,
    evidence: DraftEditorCandidatePublicationEvidenceV1,
) -> Result<PublicationAssetPlan, ComposerHostError> {
    let current = assets.owner_head_candidate(
        access,
        AssetOwner::CurrentDraft(candidate.root().key().draft_id()),
    )?;
    match evidence {
        DraftEditorCandidatePublicationEvidenceV1::ChangedNonempty { asset_proof, .. }
        | DraftEditorCandidatePublicationEvidenceV1::UnchangedNonempty { asset_proof } => {
            assets.sealed_reference_set_manifest_candidate(access, asset_proof)?;
        }
        _ => {}
    }
    prepare_asset_plan_from_head(candidate, evidence, current)
}

pub(in crate::composer_host) fn add_asset_participant_candidate(
    command: &mut HomeCommand,
    access: &HomeCandidateRecoveryAccess<'_>,
    assets: &AssetState,
    plan: PublicationAssetPlan,
) -> Result<(), ComposerHostError> {
    let revision = assets.revision_candidate(access)?;
    execution::append_asset_participant(command, assets, revision, plan)
}
