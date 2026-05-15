#![allow(dead_code)]

#[path = "syntax_highlighting/cache.rs"]
mod cache;
#[path = "syntax_highlighting/ini.rs"]
mod ini;
#[path = "syntax_highlighting/json.rs"]
mod json;
#[path = "syntax_highlighting/language.rs"]
mod language;
#[path = "syntax_highlighting/markdown.rs"]
mod markdown;
#[path = "syntax_highlighting/model.rs"]
mod model;
#[path = "syntax_highlighting/toml.rs"]
mod toml;

#[allow(unused_imports)]
pub(crate) use cache::{
    SyntaxHighlightCache, SyntaxHighlightCacheStats, SyntaxHighlightCompletion,
    SyntaxHighlightCompletionResult, SyntaxHighlightLookup, SyntaxHighlightRequest,
};
pub(crate) use language::normalize_syntax_language;
#[allow(unused_imports)]
pub(crate) use model::{SyntaxHighlight, SyntaxLanguage, SyntaxToken, SyntaxTokenRole};

pub(crate) fn highlight_syntax(source: &str, syntax_label: Option<&str>) -> SyntaxHighlight {
    match normalize_syntax_language(syntax_label) {
        Some(language) => highlight_syntax_for_language(source, language),
        None => SyntaxHighlight::plain(),
    }
}

pub(crate) fn highlight_syntax_for_language(
    source: &str,
    language: SyntaxLanguage,
) -> SyntaxHighlight {
    match language {
        SyntaxLanguage::Markdown => markdown::highlight_markdown(source),
        SyntaxLanguage::Json => json::highlight_json(source),
        SyntaxLanguage::Jsonl => json::highlight_jsonl(source),
        SyntaxLanguage::Toml => toml::highlight_toml(source),
        SyntaxLanguage::WindowsIni => ini::highlight_windows_ini(source),
    }
}
