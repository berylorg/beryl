#[test]
fn ordinary_typed_result_is_not_overridden_by_unrelated_health() {
    assert_eq!(
        ordinary_typed_settlement(false),
        OrdinaryTurnSettlement::Settled
    );
    assert_eq!(
        ordinary_typed_settlement(true),
        OrdinaryTurnSettlement::PersistentHomeFailure
    );
}
