#![cfg(feature = "test-faults")]

use syndic_storage::test_faults::{
    draft_build_mapping_maximum_for_test, draft_build_mapping_receipt_cases_for_test,
    draft_build_mapping_reencode_for_test, draft_build_mapping_stage_encodings_for_test,
    draft_build_mapping_versions_for_test,
};

fn boundary(rank: u64, inner: u64) -> Vec<u8> {
    [rank.to_be_bytes(), inner.to_be_bytes()].concat()
}

fn unit(n: u128) -> Vec<u8> {
    n.to_be_bytes().to_vec()
}

fn expected_stages() -> Vec<Vec<u8>> {
    let boundary = boundary(2, 3);
    let end_boundary = self::boundary(5, 6);
    let start = [boundary.clone(), unit(4)].concat();
    let end = [end_boundary.clone(), unit(7)].concat();
    let proof = [vec![1, 1], 8_u64.to_be_bytes().to_vec()].concat();
    let root = [vec![1], unit(7)].concat();
    let splice = [
        vec![0],
        unit(4),
        unit(2),
        unit(0),
        vec![1],
        vec![9; 16],
        vec![10; 32],
        2_u64.to_be_bytes().to_vec(),
        3_u64.to_be_bytes().to_vec(),
        5_u64.to_be_bytes().to_vec(),
    ]
    .concat();
    vec![
        vec![],
        proof.clone(),
        [start.clone(), proof.clone()].concat(),
        [start.clone(), end.clone(), proof.clone()].concat(),
        [start.clone(), end.clone(), boundary.clone(), proof].concat(),
        [start, end].concat(),
        [boundary.clone(), end_boundary.clone(), unit(4)].concat(),
        [boundary.clone(), end_boundary.clone(), unit(4), unit(7)].concat(),
        [
            boundary.clone(),
            end_boundary,
            unit(7),
            boundary.clone(),
            8_u64.to_be_bytes().to_vec(),
        ]
        .concat(),
        [vec![1], unit(4)].concat(),
        unit(4),
        unit(4),
        unit(4),
        vec![],
        unit(4),
        boundary.clone(),
        vec![],
        vec![],
        [splice.clone(), root.clone(), unit(6)].concat(),
        splice.clone(),
        [splice, root.clone()].concat(),
        [vec![0], unit(4), unit(2), unit(0), root].concat(),
        vec![],
        unit(4),
        [boundary, 3_u64.to_be_bytes().to_vec()].concat(),
    ]
}

#[test]
fn every_mapping_stage_has_the_exact_ordered_v6_payload() {
    let actual = draft_build_mapping_stage_encodings_for_test();
    let prefix = [vec![1, 1], unit(9), unit(1), vec![1], unit(7)].concat();
    assert_eq!(actual.len(), 25);
    for (tag, (actual, payload)) in actual.iter().zip(expected_stages()).enumerate() {
        let expected = [prefix.clone(), vec![u8::try_from(tag).unwrap()], payload].concat();
        assert_eq!(*actual, expected, "mapping stage {tag}");
        assert_eq!(
            draft_build_mapping_reencode_for_test(actual).as_ref(),
            Some(actual)
        );
    }
}

#[test]
fn maximum_mapping_block_is_337_bytes_and_ready_discards_proof_scratch() {
    let maximum = draft_build_mapping_maximum_for_test();
    assert_eq!(maximum.len(), 337);
    assert_eq!(
        draft_build_mapping_reencode_for_test(&maximum),
        Some(maximum)
    );
    let encodings = draft_build_mapping_stage_encodings_for_test();
    assert_eq!(encodings[20].len() - encodings[21].len(), 73);
}

#[test]
fn mapping_decoder_rejects_absence_invalid_tags_scratch_and_trailing_fields() {
    assert!(draft_build_mapping_reencode_for_test(&[0]).is_none());
    assert!(draft_build_mapping_reencode_for_test(&[2]).is_none());
    let cases = draft_build_mapping_stage_encodings_for_test();
    let mut unknown_stage = cases[0].clone();
    *unknown_stage.last_mut().unwrap() = 25;
    assert!(draft_build_mapping_reencode_for_test(&unknown_stage).is_none());
    let mut trailing = cases[0].clone();
    trailing.push(0);
    assert!(draft_build_mapping_reencode_for_test(&trailing).is_none());
    let mut invalid_root = cases[0].clone();
    invalid_root[1] = 3;
    assert!(draft_build_mapping_reencode_for_test(&invalid_root).is_none());
    let mut empty_identity = cases[0].clone();
    empty_identity[2..18].fill(0);
    assert!(draft_build_mapping_reencode_for_test(&empty_identity).is_none());
    let mut scratch = cases[1].clone();
    scratch[52] = 0;
    assert!(draft_build_mapping_reencode_for_test(&scratch).is_none());
    for value in cases {
        for length in 0..value.len() {
            assert!(
                draft_build_mapping_reencode_for_test(&value[..length]).is_none(),
                "truncated mapping accepted at {length}"
            );
        }
    }
}

#[test]
fn all_three_endpoint_families_use_v6() {
    assert_eq!(draft_build_mapping_versions_for_test(), [6, 6, 6]);
}

#[test]
fn closed_receipts_validate_splice_deltas_and_preserve_terminal_and_refresh_state() {
    for (name, expected, actual) in draft_build_mapping_receipt_cases_for_test() {
        assert_eq!(actual, expected, "{name}");
    }
}
