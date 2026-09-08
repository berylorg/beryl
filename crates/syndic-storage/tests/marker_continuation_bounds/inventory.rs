use syndic_storage::DraftPieceBuildWorkV1;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct Ceiling {
    pub structures: u64,
    pub points: u64,
    pub charged: u64,
    pub peak: u64,
}

impl Ceiling {
    fn new(structures: u64, points: u64, charged: u64) -> Self {
        Self {
            structures,
            points,
            charged,
            peak: charged + 75_056,
        }
    }

    pub fn assert_contains(self, work: DraftPieceBuildWorkV1) {
        assert!(
            work.stored_structure_records() <= self.structures,
            "{work:?} exceeds {self:?}"
        );
        assert!(
            work.point_attempts() <= self.points,
            "{work:?} exceeds {self:?}"
        );
        assert!(
            work.encoded_bytes() <= self.charged,
            "{work:?} exceeds {self:?}"
        );
        assert!(
            work.peak_encoded_bytes() <= self.peak,
            "{work:?} exceeds {self:?}"
        );
    }
}

fn common() -> u64 {
    let constructor_observations = 197_178;
    let mutable_submission_fences = 164_089;
    let build_receipt_session_emissions = 32_921;
    let fragments = 4 * 75_056;
    let active_copies = 7 * 272;
    let off_path_roots = 8 * 32_801;
    constructor_observations
        + mutable_submission_fences
        + build_receipt_session_emissions
        + fragments
        + active_copies
        + off_path_roots
}

pub(super) fn ceilings() -> [Ceiling; 10] {
    let height = 64;
    let sequence_node = 105 + 186 * 128;
    let identity_node = 108 + 88 * 128;
    let order_node = 107 + 64 * 128;
    let sequence_half = height * sequence_node + 32_893;
    let mutation = |reads: u64, emits: u64, primitive: u64| {
        Ceiling::new(
            8 + reads + emits,
            20 + 8 + reads + emits,
            common() + primitive + 33 * emits,
        )
    };
    let predecessor_combined_root = 49 + 65_536;
    let original_source_closure = predecessor_combined_root + 3 * 32_801 + 32_801;
    [
        Ceiling::new(9, 29, common() + 32_801),
        Ceiling::new(77, 98, common() + sequence_half + original_source_closure),
        Ceiling::new(73, 93, common() + sequence_half),
        mutation(128, 65, 2 * sequence_half),
        mutation(65, 132, 3_146_733),
        mutation(128, 65, (128 + 65) * identity_node),
        mutation(65, 130, (65 + 130) * identity_node),
        mutation(128, 65, (128 + 65) * order_node),
        mutation(65, 130, (65 + 130) * order_node),
        Ceiling::new(61, 83, common() + 32_801 + 714_332 + 3 * 65_537 + 65_584),
    ]
}

#[test]
fn full_command_inventory_derives_each_published_branch_ceiling() {
    assert_eq!(common(), 958_724);
    let expected = [
        (9, 29, 991_525, 1_066_581),
        (77, 98, 2_718_838, 2_793_894),
        (73, 93, 2_522_049, 2_597_105),
        (201, 221, 4_087_519, 4_162_575),
        (205, 225, 4_109_813, 4_184_869),
        (201, 221, 3_155_665, 3_230_721),
        (203, 223, 3_180_554, 3_255_610),
        (201, 221, 2_562_576, 2_637_632),
        (203, 223, 2_581_319, 2_656_375),
        (61, 83, 1_968_052, 2_043_108),
    ];
    for (actual, (structures, points, charged, peak)) in ceilings().into_iter().zip(expected) {
        assert_eq!(
            actual,
            Ceiling {
                structures,
                points,
                charged,
                peak
            }
        );
        assert!(actual.structures + 1 <= 256);
        assert!(actual.points <= 512);
        assert!(actual.peak <= 4_194_304);
    }
    assert_eq!(4_194_304 - ceilings()[4].peak, 9_435);
}

#[test]
fn u64_population_limits_every_legal_height_and_sibling_reference_inventory() {
    for height in 1..=64_u32 {
        let mut references = 0_u128;
        for level in 1..=height {
            let descendants_per_child = 1_u128 << (level - 1);
            let population_limit = u128::from(u64::MAX) / descendants_per_child;
            let same_height_group_limit = if level == height { 128 } else { 130 };
            references += population_limit.min(same_height_group_limit);
        }
        assert!(references <= 8_064);
        let internal_records = 2 * u128::from(height) - 1;
        let acquired_bytes = internal_records * 105 + references * 186 + 32_893;
        assert!(acquired_bytes <= 64 * (105 + 128 * 186) + 32_893);
    }
    assert_eq!(1_u64.checked_shl(63), Some(9_223_372_036_854_775_808));
    assert!(1_u64.checked_shl(64).is_none());
    assert!(u64::MAX.checked_add(1).is_none());
}

#[test]
fn sequence_split_inventory_includes_both_text_outputs_and_all_internal_constructors() {
    let height = 64_u64;
    let acquisitions = height * (105 + 186 * 128) + 32_893;
    let first_split_level = 2 * 105 + 186 * 130;
    let remaining_split_levels = (height - 1) * (2 * 105 + 186 * 129);
    let two_text_leaves = 32_768 + 2 * 125;
    let marker_leaf = 194;
    let new_root = 105 + 2 * 186;
    let actual_inventory = acquisitions
        + first_split_level
        + remaining_split_levels
        + two_text_leaves
        + marker_leaf
        + new_root;
    assert!(actual_inventory <= 3_146_733);
    assert_eq!(height + 1, 65);
    assert_eq!(2 * height + 3 + 1, 132);
    assert_eq!(common() + 3_146_733 + 132 * 33, ceilings()[4].charged);
}
