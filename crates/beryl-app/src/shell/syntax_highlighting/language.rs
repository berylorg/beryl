use super::model::SyntaxLanguage;

pub(crate) fn normalize_syntax_language(syntax_label: Option<&str>) -> Option<SyntaxLanguage> {
    let label = syntax_label?.trim();
    if label.is_empty() {
        return None;
    }

    let first_word = label
        .split_whitespace()
        .next()
        .unwrap_or_default()
        .trim_matches(['`', '\'', '"']);
    if first_word.is_empty() {
        return None;
    }

    match first_word.to_ascii_lowercase().as_str() {
        "markdown" | "md" | "mdown" | "mkd" | "mkdn" | "gfm" => Some(SyntaxLanguage::Markdown),
        "json" => Some(SyntaxLanguage::Json),
        "jsonl" | "ndjson" => Some(SyntaxLanguage::Jsonl),
        "toml" | "beryl-theme" => Some(SyntaxLanguage::Toml),
        "ini" => Some(SyntaxLanguage::WindowsIni),
        _ => None,
    }
}
