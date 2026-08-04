use beryl_backend::{ImageDetail, StreamedInputSequenceDigestAccumulator, TextSourceProof};

#[test]
fn exported_v1_digest_accumulator_matches_independent_sha256_vector() {
    let mut digest = StreamedInputSequenceDigestAccumulator::new(2);
    digest
        .push_text(1, TextSourceProof::new([0x31; 32]), 3)
        .unwrap();
    digest
        .push_local_image(2, Some(ImageDetail::Original), r"C:\x.png")
        .unwrap();
    assert_eq!(
        digest.finish().unwrap().as_bytes(),
        &[
            0x5d, 0xc1, 0xbf, 0x1d, 0xb3, 0x84, 0x1b, 0x0b, 0x29, 0x47, 0xdc, 0xd9, 0xf9, 0x8b,
            0xb8, 0xad, 0xe0, 0xd3, 0x11, 0x66, 0x5d, 0xf9, 0x95, 0x04, 0x11, 0xb2, 0x1f, 0x95,
            0x7a, 0x9f, 0xc4, 0x4f,
        ]
    );
}
