use super::*;

#[test]
fn diagnostic_window_requires_sole_shell_or_exact_active_registered_shell() {
    assert_eq!(registered_window_index(0, None), None);
    assert_eq!(registered_window_index(1, None), Some(0));
    assert_eq!(registered_window_index(1, Some(7)), Some(0));
    assert_eq!(registered_window_index(2, None), None);
    assert_eq!(registered_window_index(2, Some(0)), Some(0));
    assert_eq!(registered_window_index(2, Some(1)), Some(1));
    assert_eq!(registered_window_index(2, Some(7)), None);
}
