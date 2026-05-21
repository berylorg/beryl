use super::*;

pub(crate) fn branch_bootstrap_message(input: BranchBootstrapMessageInput<'_>) -> String {
    let parent_title = markdown_link_text(input.parent_thread_title);
    let parent_destination = beryl_thread_link_destination(input.parent_thread_id);
    let mut message =
        format!("Branched from [{parent_title}]({parent_destination}), no response required.");

    if let Some(context) = normalized_branch_context(input.branch_context) {
        message.push_str("\n\n");
        message.push_str(context);
    }

    message
}

pub(crate) fn beryl_thread_link_destination(thread_id: &ConversationThreadId) -> String {
    format!(
        "{BERYL_THREAD_LINK_SCHEME}{}",
        percent_encode_thread_id(thread_id.as_str())
    )
}

pub(crate) fn parse_beryl_thread_link(destination: &str) -> Option<ConversationThreadId> {
    let encoded_thread_id = destination.strip_prefix(BERYL_THREAD_LINK_SCHEME)?;
    let thread_id = percent_decode_thread_id(encoded_thread_id)?;
    let thread_id = thread_id.trim();
    (!thread_id.is_empty()).then(|| ConversationThreadId::new(thread_id.to_string()))
}

fn markdown_link_text(title: Option<&str>) -> String {
    let title = normalized_thread_title(title).unwrap_or(UNTITLED_THREAD_LABEL);
    let mut escaped = String::with_capacity(title.len());
    for ch in title.chars() {
        match ch {
            '\\' | '[' | ']' => {
                escaped.push('\\');
                escaped.push(ch);
            }
            _ => escaped.push(ch),
        }
    }
    escaped
}

fn normalized_thread_title(title: Option<&str>) -> Option<&str> {
    title.and_then(|title| {
        let title = title.trim();
        (!title.is_empty()).then_some(title)
    })
}

fn normalized_branch_context(context: Option<&str>) -> Option<&str> {
    context.and_then(|context| {
        let context = context.trim();
        (!context.is_empty()).then_some(context)
    })
}

fn percent_encode_thread_id(thread_id: &str) -> String {
    let mut encoded = String::new();
    for byte in thread_id.as_bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                encoded.push(*byte as char);
            }
            _ => encoded.push_str(&format!("%{byte:02X}")),
        }
    }
    encoded
}

fn percent_decode_thread_id(encoded: &str) -> Option<String> {
    let mut decoded = Vec::with_capacity(encoded.len());
    let bytes = encoded.as_bytes();
    let mut index = 0usize;
    while index < bytes.len() {
        if bytes[index] != b'%' {
            decoded.push(bytes[index]);
            index += 1;
            continue;
        }
        if index + 2 >= bytes.len() {
            return None;
        }
        let high = hex_value(bytes[index + 1])?;
        let low = hex_value(bytes[index + 2])?;
        decoded.push((high << 4) | low);
        index += 3;
    }

    String::from_utf8(decoded).ok()
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}
