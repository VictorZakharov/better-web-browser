//! MIME Sniffing's parsing algorithm, retaining only playback-relevant fields.
//! https://mimesniff.spec.whatwg.org/#parse-a-mime-type
//!
//! Splitting at semicolons is incorrect: an unrelated quoted value may contain
//! `;codecs=...`. Valid duplicate parameter names use their first valid value.

pub(super) struct MediaType {
    pub(super) essence: String,
    pub(super) codecs: Option<String>,
    pub(super) parameters: usize,
}

fn whitespace(character: char) -> bool {
    matches!(character, ' ' | '\t' | '\r' | '\n')
}

fn token(text: &str) -> bool {
    !text.is_empty()
        && text
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~".contains(&byte))
}

fn quoted_value(text: &str) -> bool {
    text.chars()
        .all(|character| matches!(character, '\t' | '\u{20}'..='\u{7e}' | '\u{80}'..='\u{ff}'))
}

pub(super) fn parse(input: &str) -> Option<MediaType> {
    let input = input.trim_matches(whitespace);
    let (essence, parameters) = input.split_once(';').unwrap_or((input, ""));
    let (major, subtype) = essence.split_once('/')?;
    let subtype = subtype.trim_end_matches(whitespace);
    if !token(major) || !token(subtype) {
        return None;
    }
    let mut kind = MediaType {
        essence: format!(
            "{}/{}",
            major.to_ascii_lowercase(),
            subtype.to_ascii_lowercase()
        ),
        codecs: None,
        parameters: 0,
    };
    let mut names = std::collections::HashSet::new();
    let mut remaining = parameters;
    while !remaining.is_empty() {
        remaining = remaining.trim_start_matches(whitespace);
        let delimiter = remaining.find([';', '=']).unwrap_or(remaining.len());
        let name = &remaining[..delimiter];
        remaining = &remaining[delimiter..];
        if !remaining.starts_with('=') {
            remaining = remaining.strip_prefix(';').unwrap_or("");
            continue;
        }
        remaining = &remaining[1..];
        if remaining.is_empty() {
            break;
        }
        let value = if remaining.starts_with('"') {
            let mut characters = remaining[1..].char_indices();
            let mut value = String::new();
            let mut end = remaining.len();
            while let Some((position, character)) = characters.next() {
                if character == '"' {
                    end = position + 2;
                    break;
                }
                if character == '\\' {
                    value.push(characters.next().map_or('\\', |(_, escaped)| escaped));
                } else {
                    value.push(character);
                }
            }
            remaining = &remaining[end..];
            // Garbage after a closing quote belongs to that value, not another
            // parameter; MIME parsing skips it until the next delimiter.
            let end = remaining.find(';').unwrap_or(remaining.len());
            remaining = &remaining[end..];
            value
        } else {
            let end = remaining.find(';').unwrap_or(remaining.len());
            let value = remaining[..end].trim_end_matches(whitespace).to_string();
            remaining = &remaining[end..];
            if value.is_empty() {
                remaining = remaining.strip_prefix(';').unwrap_or("");
                continue;
            }
            value
        };
        if token(name) && quoted_value(&value) && names.insert(name.to_ascii_lowercase()) {
            kind.parameters += 1;
            if name.eq_ignore_ascii_case("codecs") {
                kind.codecs = Some(value);
            }
        }
        remaining = remaining.strip_prefix(';').unwrap_or("");
    }
    Some(kind)
}
