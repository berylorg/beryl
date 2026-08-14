pub(crate) fn derive_short_title_from_turn(
    user_input: &str,
    assistant_text: &str,
) -> Option<String> {
    first_title_line(user_input)
        .or_else(|| first_title_line(assistant_text))
        .map(|title| clamp_title_words(&title, 7, 64))
        .filter(|title| !title.is_empty())
}

fn first_title_line(text: &str) -> Option<String> {
    text.lines()
        .filter_map(normalize_title_line)
        .find(|line| line.chars().any(|ch| ch.is_alphanumeric()))
}

fn normalize_title_line(line: &str) -> Option<String> {
    let trimmed = line.trim();
    if trimmed.is_empty() || trimmed.starts_with("```") {
        return None;
    }

    let stripped = trimmed.trim_start_matches(|ch: char| {
        ch.is_ascii_whitespace()
            || matches!(ch, '#' | '-' | '*' | '+' | '>' | ':' | '.' | ')' | '(')
            || ch.is_ascii_digit()
    });
    let words = stripped
        .split_whitespace()
        .map(|word| {
            word.trim_matches(|ch: char| {
                matches!(
                    ch,
                    '`' | '\''
                        | '"'
                        | ','
                        | ';'
                        | ':'
                        | '.'
                        | '!'
                        | '?'
                        | '('
                        | ')'
                        | '['
                        | ']'
                        | '{'
                        | '}'
                )
            })
        })
        .filter(|word| !word.is_empty())
        .collect::<Vec<_>>();
    if words.is_empty() {
        return None;
    }

    Some(words.join(" "))
}

fn clamp_title_words(title: &str, max_words: usize, max_chars: usize) -> String {
    let mut result = String::new();
    for word in title.split_whitespace().take(max_words) {
        if !result.is_empty() {
            if result.len() + 1 + word.len() > max_chars {
                break;
            }
            result.push(' ');
        }
        if result.len() + word.len() > max_chars {
            let remaining = max_chars.saturating_sub(result.len());
            result.extend(word.chars().take(remaining));
            break;
        }
        result.push_str(word);
    }

    result.trim().to_string()
}
