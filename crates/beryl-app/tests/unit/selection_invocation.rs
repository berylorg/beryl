use super::*;
use gpui::AppContext;

#[gpui::test]
fn published_invocation_requires_exactly_one_original_root(cx: &mut gpui::TestAppContext) {
    let original_entity = cx.new(|_| ());
    let foreign_entity = cx.new(|_| ());
    let original = original_entity.entity_id();
    let foreign = foreign_entity.entity_id();
    let invocation = MainWindowSelectionInvocation {
        root: original,
        window: beryl_model::WindowId::from_bytes([13; 16]),
    };
    invocation
        .qualify_published_roots([original, foreign].into_iter())
        .unwrap();
    assert!(
        invocation
            .qualify_published_roots([foreign].into_iter())
            .is_err()
    );
    assert!(
        invocation
            .qualify_published_roots(std::iter::empty())
            .is_err()
    );
    assert!(
        invocation
            .qualify_published_roots([original, original].into_iter())
            .is_err()
    );
    assert!(
        invocation
            .qualify_published_roots(std::iter::repeat_n(
                original,
                beryl_state::MAX_RESTORABLE_WINDOWS + 1
            ))
            .is_err()
    );
}
