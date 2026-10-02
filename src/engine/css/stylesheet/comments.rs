//! CSS comments are recognized outside strings and do not consume quoted content.
pub(in crate::engine::css) fn strip_comments(css: &str) -> String {
    let mut output = String::with_capacity(css.len());
    let mut quote = None;
    let mut escaped = false;
    let mut cursor = 0;
    while cursor < css.len() {
        let ch = css[cursor..].chars().next().unwrap();
        if escaped {
            output.push(ch);
            escaped = false;
        } else if ch == '\\' {
            output.push(ch);
            escaped = true;
        } else if let Some(active) = quote {
            output.push(ch);
            if ch == active {
                quote = None;
            }
        } else if ch == '\'' || ch == '"' {
            output.push(ch);
            quote = Some(ch);
        } else if css[cursor..].starts_with("/*") {
            let Some(end) = css[cursor + 2..].find("*/") else {
                break;
            };
            cursor += end + 4;
            continue;
        } else {
            output.push(ch);
        }
        cursor += ch.len_utf8();
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn quoted_comment_delimiters_and_escapes_are_not_comments() {
        assert_eq!(
            strip_comments(r#"a/*gone*/b "/*kept*/" '/*also kept*/'"#),
            r#"ab "/*kept*/" '/*also kept*/'"#
        );
        assert_eq!(
            strip_comments(r#""escaped\"/*kept*/"/*gone*/"#),
            r#""escaped\"/*kept*/""#
        );
        assert_eq!(strip_comments("p{color:red}/*unterminated"), "p{color:red}");
    }
}
