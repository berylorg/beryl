use std::error::Error;

use beryl_backend::JsonRpcError;

const USER_DETAIL_LIMIT: usize = 1200;

pub(crate) fn non_empty_user_text(value: impl AsRef<str>, fallback: &str) -> String {
    let value = value.as_ref().trim();
    if value.is_empty() {
        fallback.to_string()
    } else {
        value.to_string()
    }
}

pub(crate) fn source_chain_detail(
    message: impl AsRef<str>,
    error: &(dyn Error + 'static),
) -> String {
    let mut detail = non_empty_user_text(message, "The operation failed without an error message.");
    let mut source = Some(error);

    while let Some(error) = source {
        let source_text = error.to_string();
        let source_text = source_text.trim();
        if !source_text.is_empty() && !detail.contains(source_text) {
            if !detail.ends_with('.') && !detail.ends_with(':') {
                detail.push(':');
            }
            detail.push(' ');
            detail.push_str(source_text);
        }
        source = error.source();
    }

    truncate_user_detail(&detail)
}

pub(crate) fn json_rpc_error_detail(error: &JsonRpcError) -> String {
    let message = non_empty_user_text(
        &error.message,
        "The backend returned a JSON-RPC error without a message.",
    );
    let mut detail = format!("JSON-RPC code {}: {message}", error.code);

    if let Some(data) = error.data.as_ref() {
        let data_text = data.to_string();
        if !data_text.trim().is_empty() && data_text.trim() != "null" {
            detail.push_str(". Data: ");
            detail.push_str(&truncate_text(data_text.trim(), USER_DETAIL_LIMIT / 2));
        }
    }

    truncate_user_detail(&detail)
}

pub(crate) fn truncate_user_detail(value: &str) -> String {
    truncate_text(value, USER_DETAIL_LIMIT)
}

fn truncate_text(value: &str, limit: usize) -> String {
    if value.chars().count() <= limit {
        return value.to_string();
    }

    let mut truncated: String = value.chars().take(limit).collect();
    truncated.push_str("...");
    truncated
}
