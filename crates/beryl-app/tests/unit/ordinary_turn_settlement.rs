#[test]
fn ordinary_typed_result_is_not_overridden_by_unrelated_health() {
    assert_eq!(
        ordinary_typed_settlement(false, false),
        OrdinaryTurnSettlement::Settled
    );
    assert_eq!(
        ordinary_typed_settlement(false, true),
        OrdinaryTurnSettlement::PersistentHomeFailure
    );
}

#[test]
fn typed_verification_pending_stays_nonterminal() {
    assert_eq!(
        ordinary_typed_settlement(true, false),
        OrdinaryTurnSettlement::VerificationPending
    );
    assert_eq!(
        ordinary_typed_settlement(true, true),
        OrdinaryTurnSettlement::VerificationPending
    );
}
