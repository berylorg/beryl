#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct QuoteDraftInsertion {
    pub(crate) insertion_offset: usize,
    pub(crate) inserted_text: String,
    pub(crate) next_insertion_offset: usize,
}

pub(crate) fn quote_insertion_for_draft(
    draft: &str,
    insertion_offset: usize,
    selected_text: &str,
) -> Option<QuoteDraftInsertion> {
    let quote_block = quote_block(selected_text)?;
    let insertion_offset = clamp_to_char_boundary(draft, insertion_offset);
    let before = &draft[..insertion_offset];
    let after = &draft[insertion_offset..];
    let leading = leading_spacing(before);
    let trailing = trailing_spacing(after);

    let mut inserted_text =
        String::with_capacity(leading.len() + quote_block.len() + trailing.len());
    inserted_text.push_str(leading);
    inserted_text.push_str(&quote_block);
    inserted_text.push_str(trailing);

    let next_insertion_offset = insertion_offset + inserted_text.len();
    Some(QuoteDraftInsertion {
        insertion_offset,
        inserted_text,
        next_insertion_offset,
    })
}

fn quote_block(selected_text: &str) -> Option<String> {
    let selected_text = normalize_newlines(selected_text);
    if selected_text.is_empty() {
        return None;
    }

    let mut lines = selected_text.lines().peekable();
    lines.peek()?;

    let mut block = String::new();
    for line in lines {
        if !block.is_empty() {
            block.push('\n');
        }
        block.push_str("> ");
        block.push_str(line);
    }
    Some(block)
}

fn leading_spacing(before: &str) -> &'static str {
    if before.is_empty() || before.ends_with("\n\n") {
        ""
    } else if before.ends_with('\n') {
        "\n"
    } else {
        "\n\n"
    }
}

fn trailing_spacing(after: &str) -> &'static str {
    if after.starts_with("\n\n") {
        ""
    } else if after.starts_with('\n') {
        "\n"
    } else {
        "\n\n"
    }
}

fn normalize_newlines(text: &str) -> String {
    text.replace("\r\n", "\n").replace('\r', "\n")
}

fn clamp_to_char_boundary(text: &str, offset: usize) -> usize {
    let mut offset = offset.min(text.len());
    while !text.is_char_boundary(offset) {
        offset -= 1;
    }
    offset
}
