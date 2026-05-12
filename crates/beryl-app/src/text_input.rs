use gpui::App;

pub(crate) use gpui_text_input::{
    Copy as SharedTextInputCopy, Cut as SharedTextInputCut, Enter as SharedTextInputEnter,
    Paste as SharedTextInputPaste, TextInput as SingleLineInput, TextInputAtomClipboardPolicy,
    TextInputEnterKey, TextInputEvent, TextInputOptions, TextInputRetainedCounts,
    TextInputRichPastePolicy, TextInputSelectionAtom, TextInputSelectionExport,
    wrapped_visual_line_count_for_width,
};

pub(crate) fn bind_keys(cx: &mut App) {
    gpui_text_input::ensure_text_input_bindings(cx);
}
