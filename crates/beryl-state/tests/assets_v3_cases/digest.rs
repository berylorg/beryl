use super::*;

#[test]
fn asset_chain_digest_v3_is_content_neutral_and_sensitive_to_asset_set_evidence() {
    let vectors = [
        digest_vector(49, 50, 51, 1, ImageLabelOrdinal::FIRST, b"golden-asset-a"),
        digest_vector(50, 50, 51, 1, ImageLabelOrdinal::FIRST, b"golden-asset-a"),
        digest_vector(49, 50, 52, 1, ImageLabelOrdinal::FIRST, b"golden-asset-a"),
        digest_vector(49, 50, 51, 2, ImageLabelOrdinal::FIRST, b"golden-asset-a"),
        digest_vector(
            49,
            50,
            51,
            1,
            ImageLabelOrdinal::new(2).unwrap(),
            b"golden-asset-a",
        ),
        digest_vector(49, 50, 51, 1, ImageLabelOrdinal::FIRST, b"golden-asset-b"),
    ];
    assert_ne!(vectors[0][0], vectors[1][0]);
    assert_eq!(vectors[0], vectors[2]);
    assert_eq!(vectors[0][0], vectors[5][0]);
    assert!(vectors[3..].iter().all(|vector| vector[1] != vectors[0][1]));
}
