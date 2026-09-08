use super::*;

#[test]
fn height_three_binary_assignment_deletes_source_then_continues_target_consumption() {
    let mut fixture = height_three_binary(2, 2, false);
    let assignment = fixture
        .state
        .assign_next(DraftMarkerAdmissionCommandIdV1::from_bytes([30; 16]), 1)
        .unwrap();
    assert_eq!(assignment.source_root().height(), 2);
    assert_eq!(assignment.source_root().count(), 3);
    assert_eq!(assignment.target_root().height(), 3);
    assert_eq!(assignment.target_root().count(), 4);
    let expected = fixture
        .source_first_descent
        .iter()
        .copied()
        .chain(std::iter::once(fixture.source_right_sibling))
        .chain(fixture.target_first_descent.iter().copied())
        .collect::<Vec<_>>();
    assert_eq!(assignment.retained_predecessor_nodes(), expected);
    assert_eq!(
        assignment.added_charge().encoded_bytes(),
        assignment.write_bytes()
    );
    assert_eq!(
        assignment.removed_charge().encoded_bytes(),
        assignment.delete_bytes()
    );

    let consumed = fixture
        .state
        .consume_target(consume_marker(0), consume_identity(0))
        .unwrap();
    assert_eq!(consumed.target_root().height(), 2);
    assert_eq!(consumed.target_root().count(), 3);
    assert!(consumed.deletions().contains(&fixture.target_right_sibling));
    assert_eq!(consumed.superseded_nodes(), consumed.deletions());
    let continued = fixture
        .state
        .assign_next(DraftMarkerAdmissionCommandIdV1::from_bytes([31; 16]), 2)
        .unwrap();
    assert_eq!(continued.source_root().count(), 2);
    assert_eq!(continued.target_root().count(), 3);
    let consumed_again = fixture
        .state
        .consume_target(consume_marker(1), consume_identity(1))
        .unwrap();
    assert_eq!(consumed_again.target_root().count(), 2);
}

#[test]
fn target_consumption_prefers_right_then_left_siblings() {
    let mut right = height_three_binary(2, 2, true);
    let first = right
        .state
        .consume_target(consume_marker(0), consume_identity(0))
        .unwrap();
    assert_eq!(first.deletions().last(), Some(&right.target_right_sibling));

    let mut left = height_three_binary(2, 2, true);
    let second = left
        .state
        .consume_target(consume_marker(2), consume_identity(2))
        .unwrap();
    assert_eq!(second.deletions().last(), Some(&left.target_left_sibling));
}

#[test]
fn retained_predecessors_have_exact_membership_charge_and_no_collateral_deletion() {
    let mut fixture = height_three_binary(2, 2, false);
    let first = fixture
        .state
        .assign_next(DraftMarkerAdmissionCommandIdV1::from_bytes([32; 16]), 1)
        .unwrap();
    let expected = fixture
        .source_first_descent
        .iter()
        .copied()
        .chain(std::iter::once(fixture.source_right_sibling))
        .chain(fixture.target_first_descent.iter().copied())
        .collect::<Vec<_>>();
    assert_eq!(first.retained_predecessor_nodes(), expected);
    assert!(first.deletions().is_empty());
    assert_eq!(first.removed_charge().encoded_bytes(), 0);
    assert!(
        fixture
            .state
            .nodes()
            .iter()
            .any(|node| node.key() == fixture.source_right_sibling)
    );

    let second = fixture
        .state
        .assign_next(DraftMarkerAdmissionCommandIdV1::from_bytes([33; 16]), 2)
        .unwrap();
    assert_eq!(second.deletions(), expected);
    assert_eq!(
        second.removed_charge().encoded_bytes(),
        second.delete_bytes()
    );
}

#[test]
fn redistribution_at_128_preserves_the_canonical_midpoint_split() {
    let mut fixture = height_three_binary(2, 128, true);
    let step = fixture
        .state
        .consume_target(consume_marker(0), consume_identity(0))
        .unwrap();
    assert_eq!(step.target_root().height(), 3);
    assert_eq!(step.target_root().count(), 129);
    assert_eq!(
        root_child_fanouts(&fixture.state, step.target_root()),
        vec![64, 65]
    );
    assert_eq!(step.superseded_nodes(), step.deletions());
    assert_eq!(step.added_charge().encoded_bytes(), step.write_bytes());
    assert_eq!(step.removed_charge().encoded_bytes(), step.delete_bytes());
    assert!(step.command_bytes() <= DRAFT_MARKER_ADMISSION_COMMAND_MAX_ENCODED_BYTES);
}

#[test]
fn selected_unary_root_collapses_to_internal_leaf_and_empty() {
    let mut fixture = height_three_binary(3, 2, true);
    let target_root = fixture.state.target_root();
    let root_node = fixture
        .state
        .nodes()
        .iter()
        .find(|node| Some(node.key()) == target_root.node())
        .unwrap()
        .clone();
    let DraftMarkerAdmissionNodePayloadV1::Internal { children, .. } = root_node.payload() else {
        panic!("expected internal root");
    };
    let unary_root = internal(
        owner_for_test(),
        DraftMarkerAdmissionTreeV1::TargetId,
        3,
        99,
        vec![children[0]],
    );
    fixture
        .state
        .set_roots_for_test(fixture.state.source_root(), root(&unary_root));
    let mut nodes = fixture.state.nodes().to_vec();
    nodes.push(unary_root);
    fixture.state = DraftMarkerAdmissionIndexTestStateV1::from_parts(
        owner_for_test(),
        fixture.state.source_root(),
        root(&nodes.last().unwrap()),
        nodes,
        Vec::new(),
    );

    let internal = fixture
        .state
        .consume_target(consume_marker(0), consume_identity(0))
        .unwrap();
    assert_eq!(internal.target_root().height(), 2);
    assert_eq!(internal.target_root().count(), 2);
    let leaf = fixture
        .state
        .consume_target(consume_marker(1), consume_identity(1))
        .unwrap();
    assert_eq!(leaf.target_root().height(), 1);
    assert_eq!(leaf.target_root().count(), 1);
    let empty = fixture
        .state
        .consume_target(consume_marker(2), consume_identity(2))
        .unwrap();
    assert_eq!(empty.target_root().height(), 0);
    assert_eq!(empty.target_root().count(), 0);
    assert!(empty.target_root().node().is_none());
}

#[test]
fn target_only_height_eighteen_unary_root_collapses_after_successive_consumption() {
    let mut state = target_only_unary_height_eighteen();
    assert_eq!(state.target_root().height(), 18);
    assert_eq!(state.target_root().count(), 65_536);

    let first = state
        .consume_target(consume_marker(0), consume_identity(0))
        .unwrap();
    assert_eq!(first.target_root().height(), 16);
    assert_eq!(first.target_root().count(), 65_535);
    assert_eq!(first.superseded_nodes(), first.deletions());
    assert!(first.command_bytes() <= DRAFT_MARKER_ADMISSION_COMMAND_MAX_ENCODED_BYTES);

    let second = state
        .consume_target(consume_marker(1), consume_identity(1))
        .unwrap();
    assert_eq!(second.target_root().height(), 16);
    assert_eq!(second.target_root().count(), 65_534);
    assert_eq!(second.superseded_nodes(), second.deletions());
    assert!(second.command_bytes() <= DRAFT_MARKER_ADMISSION_COMMAND_MAX_ENCODED_BYTES);
}

#[test]
fn tall_dual_tree_assignment_authenticates_and_reclaims_a_real_predecessor_closure() {
    let mut state = dual_tree_height_seventeen();
    assert_eq!(state.source_root().height(), 17);
    assert_eq!(state.target_root().height(), 17);
    assert_eq!(state.source_root().count(), 32_768);
    assert_eq!(state.target_root().count(), 32_768);
    assert_eq!(state.nodes().len(), 131_072);
    assert!(state.nodes().len() <= 250_000);

    let source_before = state.source_root();
    let target_before = state.target_root();
    let first_command = DraftMarkerAdmissionCommandIdV1::from_bytes([232; 16]);
    let first = state.assign_next(first_command, 0).unwrap();
    assert_eq!(first.source_root().count(), 32_767);
    assert_eq!(first.target_root().count(), 32_768);
    assert_eq!(first.retained_predecessor_nodes().len(), 48);
    assert_eq!(first.deletions().len(), 0);
    assert!(first.command_bytes() <= DRAFT_MARKER_ADMISSION_COMMAND_MAX_ENCODED_BYTES);

    let receipt = assignment_receipt(
        &state,
        first_command,
        source_before,
        first.source_root(),
        target_before,
        first.target_root(),
        first.retained_predecessor_nodes(),
    );
    let second = state
        .assign_next_with_receipt(
            &receipt,
            DraftMarkerAdmissionCommandIdV1::from_bytes([233; 16]),
            1,
        )
        .unwrap();
    assert_eq!(second.source_root().count(), 32_766);
    assert_eq!(second.target_root().count(), 32_768);
    assert_eq!(second.deletions(), first.retained_predecessor_nodes());
    assert_eq!(second.retained_predecessor_nodes().len(), 31);
    assert!(second.command_bytes() <= DRAFT_MARKER_ADMISSION_COMMAND_MAX_ENCODED_BYTES);
    assert_eq!(
        (
            first.point_attempts(),
            first.stored_node_acquisitions(),
            first.stored_node_emissions(),
            first.command_bytes(),
            first.peak_reserved_bytes(),
        ),
        (78, 48, 30, 40_141, 105_232)
    );
    assert_eq!(
        (
            second.point_attempts(),
            second.stored_node_acquisitions(),
            second.stored_node_emissions(),
            second.command_bytes(),
            second.peak_reserved_bytes(),
        ),
        (109, 79, 30, 78_596, 121_550)
    );
}

#[test]
fn full_high_fanout_dual_tree_assignment_preserves_the_complete_ledger_profile() {
    let mut state = dual_tree_high_fanout();
    assert_eq!(state.source_root().height(), 3);
    assert_eq!(state.target_root().height(), 3);
    assert_eq!(state.source_root().count(), 16_384);
    assert_eq!(state.target_root().count(), 16_384);
    assert_eq!(state.nodes().len(), 33_026);

    let source_before = state.source_root();
    let target_before = state.target_root();
    let first_command = DraftMarkerAdmissionCommandIdV1::from_bytes([234; 16]);
    let first = state.assign_next(first_command, 0).unwrap();
    assert_eq!(first.retained_predecessor_nodes().len(), 6);
    assert_eq!(first.deletions().len(), 0);
    assert_eq!(
        root_child_fanouts(&state, first.target_root()),
        vec![128; 128]
    );

    let receipt = assignment_receipt(
        &state,
        first_command,
        source_before,
        first.source_root(),
        target_before,
        first.target_root(),
        first.retained_predecessor_nodes(),
    );
    let second = state
        .assign_next_with_receipt(
            &receipt,
            DraftMarkerAdmissionCommandIdV1::from_bytes([235; 16]),
            1,
        )
        .unwrap();
    assert_eq!(second.deletions(), first.retained_predecessor_nodes());
    assert_eq!(second.retained_predecessor_nodes().len(), 6);
    assert!(first.command_bytes() <= DRAFT_MARKER_ADMISSION_COMMAND_MAX_ENCODED_BYTES);
    assert!(second.command_bytes() <= DRAFT_MARKER_ADMISSION_COMMAND_MAX_ENCODED_BYTES);
    assert_eq!(
        (
            first.point_attempts(),
            first.stored_node_acquisitions(),
            first.stored_node_emissions(),
            first.command_bytes(),
            first.peak_reserved_bytes(),
        ),
        (11, 6, 5, 153_391, 201_094)
    );
    assert_eq!(
        (
            second.point_attempts(),
            second.stored_node_acquisitions(),
            second.stored_node_emissions(),
            second.command_bytes(),
            second.peak_reserved_bytes(),
        ),
        (17, 12, 5, 306_721, 306_721)
    );
}

#[test]
fn weighted_source_and_dense_target_assignment_stay_within_the_complete_ledger_profile() {
    let mut state = weighted_source_with_dense_target();
    assert_eq!(state.source_root().height(), 12);
    assert_eq!(state.target_root().height(), 4);
    assert_eq!(state.source_root().count(), 65_410);
    assert_eq!(state.target_root().count(), 65_410);
    assert_eq!(
        selected_path_child_references(&state, state.source_root()),
        1_155
    );
    assert_eq!(
        selected_path_child_references(&state, state.target_root()),
        260
    );
    assert!(state.nodes().len() < 250_000);

    let source_before = state.source_root();
    let target_before = state.target_root();
    let first_command = DraftMarkerAdmissionCommandIdV1::from_bytes([236; 16]);
    let first = state.assign_next(first_command, 0).unwrap();
    assert_eq!(first.source_root().count(), 65_409);
    assert_eq!(first.target_root().count(), 65_410);
    assert_eq!(first.retained_predecessor_nodes().len(), 16);
    assert_eq!(first.deletions().len(), 0);
    assert!(first.command_bytes() <= DRAFT_MARKER_ADMISSION_COMMAND_MAX_ENCODED_BYTES);

    let receipt = assignment_receipt(
        &state,
        first_command,
        source_before,
        first.source_root(),
        target_before,
        first.target_root(),
        first.retained_predecessor_nodes(),
    );
    let second = state
        .assign_next_with_receipt(
            &receipt,
            DraftMarkerAdmissionCommandIdV1::from_bytes([237; 16]),
            1,
        )
        .unwrap();
    assert_eq!(second.source_root().count(), 65_408);
    assert_eq!(second.target_root().count(), 65_410);
    assert_eq!(second.deletions(), first.retained_predecessor_nodes());
    assert_eq!(second.retained_predecessor_nodes().len(), 15);
    assert!(second.command_bytes() <= DRAFT_MARKER_ADMISSION_COMMAND_MAX_ENCODED_BYTES);
    assert_eq!(
        (
            first.point_attempts(),
            first.stored_node_acquisitions(),
            first.stored_node_emissions(),
            first.command_bytes(),
            first.peak_reserved_bytes(),
        ),
        (30, 16, 14, 438_623, 503_438)
    );
    assert_eq!(
        (
            second.point_attempts(),
            second.stored_node_acquisitions(),
            second.stored_node_emissions(),
            second.command_bytes(),
            second.peak_reserved_bytes(),
        ),
        (45, 31, 14, 876_600, 876_600)
    );
}

#[test]
fn selected_assignment_receipt_accepts_its_exact_live_predecessor_closure() {
    let (mut fixture, receipt, source_before, target_before) = assigned_receipt_fixture();

    assert_eq!(receipt.source_before(), source_before);
    assert_eq!(receipt.target_before(), target_before);
    assert!(
        fixture
            .state
            .assign_next_with_receipt(
                &receipt,
                DraftMarkerAdmissionCommandIdV1::from_bytes([182; 16]),
                1,
            )
            .is_ok()
    );
}

#[test]
fn selected_assignment_receipt_rejects_a_live_reachable_offpath_target_node() {
    let (mut fixture, receipt, _, _) = assigned_receipt_fixture();
    let offpath = live_right_target_subtree(&fixture.state);
    let mut retained = receipt.retained_predecessor_nodes().to_vec();
    let target_position = retained
        .iter()
        .position(|expected| {
            fixture.state.nodes().iter().any(|node| {
                node.key() == expected.key() && node.tree() == DraftMarkerAdmissionTreeV1::TargetId
            })
        })
        .unwrap();
    retained[target_position] = offpath;
    let substituted = replacement_receipt(
        &receipt,
        receipt.source_before(),
        receipt.target_before(),
        receipt.source_after(),
        receipt.target_after(),
        retained,
    );
    assert_ne!(substituted.digest(), receipt.digest());

    let source_before = fixture.state.source_root();
    let target_before = fixture.state.target_root();
    assert!(
        fixture
            .state
            .assign_next_with_receipt(
                &substituted,
                DraftMarkerAdmissionCommandIdV1::from_bytes([183; 16]),
                1,
            )
            .is_err()
    );
    assert_eq!(fixture.state.source_root(), source_before);
    assert_eq!(fixture.state.target_root(), target_before);
}

#[test]
fn selected_assignment_receipt_rejects_substituted_before_root_without_mutation() {
    let (mut fixture, receipt, _, _) = assigned_receipt_fixture();
    let substituted = replacement_receipt(
        &receipt,
        receipt.source_after(),
        receipt.target_before(),
        receipt.source_after(),
        receipt.target_after(),
        receipt.retained_predecessor_nodes().to_vec(),
    );
    assert_ne!(substituted.digest(), receipt.digest());

    let source_before = fixture.state.source_root();
    let target_before = fixture.state.target_root();
    assert!(
        fixture
            .state
            .assign_next_with_receipt(
                &substituted,
                DraftMarkerAdmissionCommandIdV1::from_bytes([184; 16]),
                1,
            )
            .is_err()
    );
    assert_eq!(fixture.state.source_root(), source_before);
    assert_eq!(fixture.state.target_root(), target_before);
}

#[test]
fn missing_or_substituted_siblings_and_retention_fail_closed() {
    let mut missing = height_three_binary(2, 2, false);
    assert!(
        missing
            .state
            .remove_node_for_test(missing.source_right_sibling)
    );
    assert!(matches!(
        missing
            .state
            .assign_next(DraftMarkerAdmissionCommandIdV1::from_bytes([40; 16]), 1),
        Err(DraftMarkerAdmissionIndexTestErrorV1::MissingNode)
    ));

    let mut substituted = height_three_binary(2, 2, true);
    assert!(
        substituted
            .state
            .corrupt_node_digest_for_test(substituted.target_right_sibling)
    );
    assert!(
        substituted
            .state
            .consume_target(consume_marker(0), consume_identity(0))
            .is_err()
    );

    let mut retained = height_three_binary(2, 2, false);
    let unrelated = retained.state.nodes()[0].clone();
    retained
        .state
        .set_prior_replay_nodes_for_test(vec![child(&unrelated)]);
    assert!(
        retained
            .state
            .assign_next(DraftMarkerAdmissionCommandIdV1::from_bytes([41; 16]), 1)
            .is_err()
    );
}
