use std::num::NonZeroU64;

use beryl_model::{AssetId, AssetIdentityVersion};

#[test]
fn asset_identity_retains_version_digest_and_exact_length() {
    let digest = [0xab; 32];
    let length = NonZeroU64::new(42).unwrap();
    let asset = AssetId::sha256_v1(digest, length);

    assert_eq!(asset.version(), AssetIdentityVersion::Sha256V1);
    assert_eq!(asset.digest(), digest);
    assert_eq!(asset.length(), length);
}

#[test]
fn asset_identity_round_trips_through_serde() {
    let asset = AssetId::sha256_v1([7; 32], NonZeroU64::new(1_024).unwrap());
    let encoded = serde_json::to_string(&asset).unwrap();

    assert_eq!(serde_json::from_str::<AssetId>(&encoded).unwrap(), asset);
}
