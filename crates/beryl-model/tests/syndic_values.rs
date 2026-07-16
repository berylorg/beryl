use std::any::TypeId;

use beryl_model::{
    AcceptedInputRevision, CasConversationToolProfile, CasConversationToolProfileVersion,
    CasGenerationError, CasItemId, CasLoadedSessionGeneration, CasLoadedThreadGeneration,
    CasNativeTurnCount, CasNativeTurnCountError, CasProcessGeneration, DiscussionContextDigest,
    DiscussionContextOwnerId, ProjectionRevision, RecoveryItemSequenceDigest, SyndicDraftId,
    SyndicExecutionSnapshotId, SyndicPathDigest, SyndicResourceId,
};

#[test]
fn new_syndic_identities_remain_type_and_text_distinct() {
    let bytes = [4; 16];
    let resource = SyndicResourceId::from_bytes(bytes);
    let execution = SyndicExecutionSnapshotId::from_bytes(bytes);

    assert!(resource.to_string().starts_with("syndic_resource_"));
    assert!(execution.to_string().starts_with("syndic_execution_"));
    assert_ne!(resource.to_string(), execution.to_string());
    assert_ne!(
        TypeId::of::<SyndicResourceId>(),
        TypeId::of::<SyndicExecutionSnapshotId>()
    );
}

#[test]
fn context_owner_preserves_draft_or_submitted_turn_identity() {
    let draft = SyndicDraftId::from_bytes([1; 16]);
    let turn = draft.submitted_turn_id();
    let accepted = draft.accepted_input_id();

    assert_eq!(turn.as_bytes(), draft.as_bytes());
    assert_eq!(accepted.as_bytes(), draft.as_bytes());
    assert_ne!(accepted.to_string(), turn.to_string());
    assert_eq!(
        DiscussionContextOwnerId::submitted_from_draft(draft),
        DiscussionContextOwnerId::SubmittedTurn(turn)
    );
}

#[test]
fn digest_domains_cannot_be_mixed_by_type() {
    assert_ne!(
        TypeId::of::<DiscussionContextDigest>(),
        TypeId::of::<SyndicPathDigest>()
    );
    assert_ne!(
        TypeId::of::<SyndicPathDigest>(),
        TypeId::of::<RecoveryItemSequenceDigest>()
    );
}

#[test]
fn cas_generations_and_provider_item_ids_are_bounded() {
    assert_eq!(CasProcessGeneration::new(0), Err(CasGenerationError::Zero));
    let process = CasProcessGeneration::new(7).unwrap();
    let thread = CasLoadedThreadGeneration::new(9).unwrap();
    let loaded = CasLoadedSessionGeneration::new(process, thread);
    assert_eq!(loaded.process(), process);
    assert_eq!(loaded.thread(), thread);
    assert!(CasItemId::new("i".repeat(256)).is_ok());
    assert!(CasItemId::new("i".repeat(257)).is_err());
}

#[test]
fn syndic_revision_domains_remain_distinct() {
    assert_ne!(
        TypeId::of::<AcceptedInputRevision>(),
        TypeId::of::<ProjectionRevision>()
    );
}

#[test]
fn native_cas_turn_count_is_zero_capable_and_checked() {
    let zero = CasNativeTurnCount::ZERO;
    let one = zero.checked_next().unwrap();

    assert_eq!(zero.get(), 0);
    assert_eq!(one.get(), 1);
    assert_eq!(
        CasNativeTurnCount::new(u64::MAX).checked_next(),
        Err(CasNativeTurnCountError::Exhausted)
    );
}

#[test]
fn conversation_tool_profile_preserves_version_and_exact_digest() {
    let profile = CasConversationToolProfile::v1([0x5a; 32]);

    assert_eq!(profile.version(), CasConversationToolProfileVersion::V1);
    assert_eq!(profile.digest(), [0x5a; 32]);
}
