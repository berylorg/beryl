use beryl_home_store::RecordCodec;
use beryl_model::{
    SyndicAcceptedInputId, SyndicDraftId, SyndicDraftMarkerId, SyndicItemId, SyndicProjectionId,
    SyndicRetryRecordId,
};

use crate::asset::{AssetReferenceOwner, codec::AssetReferenceCodec};

#[test]
fn exact_owner_codec_retains_marker_cardinality_and_rejects_the_queued_tag() {
    let marker_id = SyndicDraftMarkerId::from_bytes([9; 16]);
    let owners = [
        (
            AssetReferenceOwner::CurrentDraftMarker {
                draft_id: SyndicDraftId::from_bytes([1; 16]),
                marker_id,
            },
            0,
            33,
        ),
        (
            AssetReferenceOwner::AcceptedInputMarker {
                input_id: SyndicAcceptedInputId::from_bytes([2; 16]),
                marker_id,
            },
            1,
            33,
        ),
        (
            AssetReferenceOwner::SubmittedTurnItemMarker {
                item_id: SyndicItemId::from_bytes([3; 16]),
                marker_id,
            },
            2,
            33,
        ),
        (
            AssetReferenceOwner::RetryRecordMarker {
                retry_id: SyndicRetryRecordId::from_bytes([4; 16]),
                marker_id,
            },
            4,
            33,
        ),
        (
            AssetReferenceOwner::TranscriptProjection {
                projection_id: SyndicProjectionId::from_bytes([5; 16]),
            },
            5,
            17,
        ),
    ];

    for (owner, tag, length) in owners {
        let encoded = AssetReferenceCodec::encode_key(&owner).unwrap();
        assert_eq!(encoded[0], tag);
        assert_eq!(encoded.len(), length);
        assert_eq!(AssetReferenceCodec::decode_key(&encoded).unwrap(), owner);
    }

    let mut queued = vec![3];
    queued.extend_from_slice(&[7; 32]);
    assert!(AssetReferenceCodec::decode_key(&queued).is_err());
}

#[test]
fn obsolete_owner_keys_are_not_accepted_by_the_replacement_codec() {
    for tag in [1, 2, 3, 4] {
        let mut obsolete_single_identity = vec![tag];
        obsolete_single_identity.extend_from_slice(&[6; 16]);
        assert!(AssetReferenceCodec::decode_key(&obsolete_single_identity).is_err());
    }
}
