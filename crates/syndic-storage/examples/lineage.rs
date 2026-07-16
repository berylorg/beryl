use beryl_model::{CasNativeTurnCount, SyndicPathDigest, SyndicTurnId, ThreadRevision};
use syndic_storage::{
    CasLineageProof, CasRepresentedPrefixProof, ConversationParent, NativeCasLineage,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let tail = SyndicTurnId::from_bytes([7; 16]);
    let represented_prefix = CasRepresentedPrefixProof::new(
        Some(tail),
        ThreadRevision::new(3)?,
        SyndicPathDigest::from_bytes([9; 32]),
    );
    let lineage = CasLineageProof::native(NativeCasLineage::Continuation, represented_prefix)?;

    assert_eq!(ConversationParent::Turn(tail).turn(), Some(tail));
    assert_eq!(lineage.established_prefix(), represented_prefix);
    assert_eq!(
        CasNativeTurnCount::new(3).checked_next()?,
        CasNativeTurnCount::new(4)
    );
    Ok(())
}
