use crate::navigation::resolve_url;

const MAX_RENDERED_TEXT_BYTES: usize = 2 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Document {
    pub title: String,
    pub source_url: String,
    pub blocks: Vec<Block>,
    pub truncated: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Block {
    pub kind: BlockKind,
    pub spans: Vec<Span>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockKind {
    Paragraph,
    Heading(u8),
    ListItem,
    Quote,
    Preformatted,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Span {
    pub text: String,
    pub link: Option<String>,
}

impl Document {
    pub fn plain_text(&self) -> String {
        let mut output = String::new();
        for block in &self.blocks {
            if !output.is_empty() {
                output.push('\n');
            }
            for span in &block.spans {
                output.push_str(&span.text);
            }
        }
        output
    }
}

pub fn parse_html(html: &str, source_url: &str) -> Document {
    Parser::new(source_url).parse(html)
}

struct Parser<'a> {
    source_url: &'a str,
    document: Document,
    current_kind: BlockKind,
    current_spans: Vec<Span>,
    current_link: Option<String>,
    title_buffer: String,
    capturing_title: bool,
    preformatted: bool,
    rendered_bytes: usize,
}

impl<'a> Parser<'a> {
    fn new(source_url: &'a str) -> Self {
        Self {
            source_url,
            document: Document {
                title: String::new(),
                source_url: source_url.to_string(),
                blocks: Vec::new(),
                truncated: false,
            },
            current_kind: BlockKind::Paragraph,
            current_spans: Vec::new(),
            current_link: None,
            title_buffer: String::new(),
            capturing_title: false,
            preformatted: false,
            rendered_bytes: 0,
        }
    }

    fn parse(mut self, html: &str) -> Document {
        let mut cursor = 0;
        while cursor < html.len() && !self.document.truncated {
            let Some(relative_open) = html[cursor..].find('<') else {
                self.handle_text(&html[cursor..]);
                break;
            };
            let open = cursor + relative_open;
            self.handle_text(&html[cursor..open]);

            if html[open..].starts_with("<!--") {
                cursor = html[open + 4..]
                    .find("-->")
                    .map(|offset| open + 4 + offset + 3)
                    .unwrap_or(html.len());
                continue;
            }

            let Some(close) = find_tag_end(html.as_bytes(), open + 1) else {
                self.handle_text(&html[open..]);
                break;
            };
            let tag = &html[open + 1..close];
            let (name, closing) = tag_name(tag);

            if !closing && is_skipped_element(&name) {
                let closing_tag = format!("</{name}");
                if let Some(relative_end) =
                    find_ascii_case_insensitive(&html[close + 1..], &closing_tag)
                {
                    let closing_start = close + 1 + relative_end;
                    cursor = html[closing_start..]
                        .find('>')
                        .map(|offset| closing_start + offset + 1)
                        .unwrap_or(html.len());
                } else {
                    cursor = html.len();
                }
                continue;
            }

            self.handle_tag(tag, &name, closing);
            cursor = close + 1;
        }
        self.flush_block();

        self.document.title = collapse_whitespace(&decode_entities(&self.title_buffer))
            .trim()
            .to_string();
        if self.document.title.is_empty() {
            self.document.title = self
                .document
                .blocks
                .iter()
                .find(|block| matches!(block.kind, BlockKind::Heading(_)))
                .map(block_text)
                .filter(|title| !title.is_empty())
                .unwrap_or_else(|| {
                    crate::navigation::ParsedUrl::parse(self.source_url)
                        .map(|url| url.host)
                        .unwrap_or_else(|_| "Untitled document".into())
                });
        }
        self.document
    }

    fn handle_tag(&mut self, raw_tag: &str, name: &str, closing: bool) {
        match (name, closing) {
            ("title", false) => {
                self.capturing_title = true;
                return;
            }
            ("title", true) => {
                self.capturing_title = false;
                return;
            }
            ("a", false) => {
                self.current_link = attribute(raw_tag, "href")
                    .and_then(|href| resolve_url(self.source_url, &decode_entities(&href)));
                return;
            }
            ("a", true) => {
                self.current_link = None;
                return;
            }
            ("img", false) => {
                if let Some(alt) = attribute(raw_tag, "alt") {
                    let alt = decode_entities(&alt);
                    if !alt.trim().is_empty() {
                        self.add_text(&format!("[{alt}]"));
                    }
                }
                return;
            }
            ("br", _) | ("hr", _) => {
                self.flush_block();
                return;
            }
            ("pre", false) => {
                self.start_block(BlockKind::Preformatted);
                self.preformatted = true;
                return;
            }
            ("pre", true) => {
                self.flush_block();
                self.preformatted = false;
                self.current_kind = BlockKind::Paragraph;
                return;
            }
            _ => {}
        }

        let next_kind = match name {
            "h1" => Some(BlockKind::Heading(1)),
            "h2" => Some(BlockKind::Heading(2)),
            "h3" => Some(BlockKind::Heading(3)),
            "h4" | "h5" | "h6" => Some(BlockKind::Heading(4)),
            "li" => Some(BlockKind::ListItem),
            "blockquote" => Some(BlockKind::Quote),
            "p" | "article" | "section" | "main" | "header" | "footer" | "aside" | "dt" | "dd"
            | "figcaption" => Some(BlockKind::Paragraph),
            _ => None,
        };

        if let Some(kind) = next_kind {
            if closing {
                self.flush_block();
                self.current_kind = BlockKind::Paragraph;
            } else {
                self.start_block(kind);
            }
        }
    }

    fn handle_text(&mut self, text: &str) {
        let decoded = decode_entities(text);
        if self.capturing_title {
            self.title_buffer.push_str(&decoded);
            return;
        }
        self.add_text(&decoded);
    }

    fn add_text(&mut self, text: &str) {
        if text.is_empty() || self.document.truncated {
            return;
        }
        let text = if self.preformatted {
            text.replace("\r\n", "\n")
        } else {
            collapse_whitespace(text)
        };
        if text.is_empty() {
            return;
        }

        let remaining = MAX_RENDERED_TEXT_BYTES.saturating_sub(self.rendered_bytes);
        if remaining == 0 {
            self.document.truncated = true;
            return;
        }
        let original_len = text.len();
        let end = nearest_char_boundary(&text, remaining.min(original_len));
        let text = &text[..end];
        self.rendered_bytes += text.len();
        if end < original_len {
            self.document.truncated = true;
        }

        if let Some(last) = self.current_spans.last_mut()
            && last.link == self.current_link
        {
            last.text.push_str(text);
        } else {
            self.current_spans.push(Span {
                text: text.to_string(),
                link: self.current_link.clone(),
            });
        }
    }

    fn start_block(&mut self, kind: BlockKind) {
        self.flush_block();
        self.current_kind = kind;
    }

    fn flush_block(&mut self) {
        trim_spans(&mut self.current_spans);
        if !self.current_spans.is_empty() {
            self.document.blocks.push(Block {
                kind: self.current_kind,
                spans: std::mem::take(&mut self.current_spans),
            });
        }
    }
}

fn find_tag_end(bytes: &[u8], start: usize) -> Option<usize> {
    let mut quote = None;
    for (index, byte) in bytes.iter().copied().enumerate().skip(start) {
        match (quote, byte) {
            (Some(active), candidate) if active == candidate => quote = None,
            (None, b'\'' | b'"') => quote = Some(byte),
            (None, b'>') => return Some(index),
            _ => {}
        }
    }
    None
}

fn tag_name(tag: &str) -> (String, bool) {
    let tag = tag.trim_start();
    let closing = tag.starts_with('/');
    let tag = tag.strip_prefix('/').unwrap_or(tag).trim_start();
    let name = tag
        .chars()
        .take_while(|character| character.is_ascii_alphanumeric() || *character == '-')
        .collect::<String>()
        .to_ascii_lowercase();
    (name, closing)
}

fn is_skipped_element(name: &str) -> bool {
    matches!(
        name,
        "script" | "style" | "noscript" | "svg" | "canvas" | "template" | "iframe"
    )
}

fn find_ascii_case_insensitive(haystack: &str, needle: &str) -> Option<usize> {
    if needle.is_empty() || haystack.len() < needle.len() {
        return None;
    }
    haystack
        .as_bytes()
        .windows(needle.len())
        .position(|window| window.eq_ignore_ascii_case(needle.as_bytes()))
}

fn attribute(tag: &str, wanted: &str) -> Option<String> {
    let bytes = tag.as_bytes();
    let mut cursor = bytes
        .iter()
        .position(|byte| byte.is_ascii_whitespace())
        .unwrap_or(bytes.len());

    while cursor < bytes.len() {
        while cursor < bytes.len() && (bytes[cursor].is_ascii_whitespace() || bytes[cursor] == b'/')
        {
            cursor += 1;
        }
        let name_start = cursor;
        while cursor < bytes.len()
            && (bytes[cursor].is_ascii_alphanumeric()
                || matches!(bytes[cursor], b'-' | b'_' | b':'))
        {
            cursor += 1;
        }
        if name_start == cursor {
            cursor += 1;
            continue;
        }
        let name = &tag[name_start..cursor];
        while cursor < bytes.len() && bytes[cursor].is_ascii_whitespace() {
            cursor += 1;
        }
        if cursor >= bytes.len() || bytes[cursor] != b'=' {
            continue;
        }
        cursor += 1;
        while cursor < bytes.len() && bytes[cursor].is_ascii_whitespace() {
            cursor += 1;
        }
        let (value_start, value_end) =
            if cursor < bytes.len() && matches!(bytes[cursor], b'\'' | b'"') {
                let quote = bytes[cursor];
                cursor += 1;
                let start = cursor;
                while cursor < bytes.len() && bytes[cursor] != quote {
                    cursor += 1;
                }
                let end = cursor;
                cursor = (cursor + 1).min(bytes.len());
                (start, end)
            } else {
                let start = cursor;
                while cursor < bytes.len()
                    && !bytes[cursor].is_ascii_whitespace()
                    && bytes[cursor] != b'>'
                {
                    cursor += 1;
                }
                (start, cursor)
            };
        if name.eq_ignore_ascii_case(wanted) {
            return Some(tag[value_start..value_end].to_string());
        }
    }
    None
}

fn decode_entities(input: &str) -> String {
    if !input.contains('&') {
        return input.to_string();
    }
    let mut output = String::with_capacity(input.len());
    let mut cursor = 0;
    while let Some(relative_ampersand) = input[cursor..].find('&') {
        let ampersand = cursor + relative_ampersand;
        output.push_str(&input[cursor..ampersand]);
        let Some(relative_semicolon) = input[ampersand + 1..].find(';') else {
            output.push_str(&input[ampersand..]);
            return output;
        };
        let semicolon = ampersand + 1 + relative_semicolon;
        if semicolon - ampersand > 12 {
            output.push('&');
            cursor = ampersand + 1;
            continue;
        }
        let entity = &input[ampersand + 1..semicolon];
        let decoded = match entity {
            "amp" => Some('&'),
            "lt" => Some('<'),
            "gt" => Some('>'),
            "quot" => Some('"'),
            "apos" | "#39" => Some('\''),
            "nbsp" => Some(' '),
            "ndash" => Some('–'),
            "mdash" => Some('—'),
            "hellip" => Some('…'),
            _ if entity.starts_with("#x") || entity.starts_with("#X") => {
                u32::from_str_radix(&entity[2..], 16)
                    .ok()
                    .and_then(char::from_u32)
            }
            _ if entity.starts_with('#') => entity[1..].parse().ok().and_then(char::from_u32),
            _ => None,
        };
        if let Some(character) = decoded {
            output.push(character);
        } else {
            output.push_str(&input[ampersand..=semicolon]);
        }
        cursor = semicolon + 1;
    }
    output.push_str(&input[cursor..]);
    output
}

fn collapse_whitespace(input: &str) -> String {
    let mut output = String::with_capacity(input.len());
    let mut in_whitespace = false;
    for character in input.chars() {
        if character.is_whitespace() {
            if !in_whitespace {
                output.push(' ');
                in_whitespace = true;
            }
        } else {
            output.push(character);
            in_whitespace = false;
        }
    }
    output
}

fn trim_spans(spans: &mut Vec<Span>) {
    while spans
        .first()
        .is_some_and(|span| span.text.trim().is_empty())
    {
        spans.remove(0);
    }
    while spans.last().is_some_and(|span| span.text.trim().is_empty()) {
        spans.pop();
    }
    if let Some(first) = spans.first_mut() {
        first.text = first.text.trim_start().to_string();
    }
    if let Some(last) = spans.last_mut() {
        last.text = last.text.trim_end().to_string();
    }
    spans.retain(|span| !span.text.is_empty());
}

fn nearest_char_boundary(input: &str, mut index: usize) -> usize {
    while index > 0 && !input.is_char_boundary(index) {
        index -= 1;
    }
    index
}

fn block_text(block: &Block) -> String {
    block.spans.iter().map(|span| span.text.as_str()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_structured_content_and_links() {
        let html = r#"
            <html><head><title> A &amp; B </title><style>bad</style></head>
            <body><h1>Hello</h1><p>Read <a href="/next">the next page</a>.</p>
            <script>malicious()</script><ul><li>Fast</li><li>Small</li></ul></body></html>
        "#;
        let document = parse_html(html, "https://example.com/docs/start");
        assert_eq!(document.title, "A & B");
        assert_eq!(document.blocks.len(), 4);
        assert_eq!(document.blocks[0].kind, BlockKind::Heading(1));
        assert!(!document.plain_text().contains("malicious"));
        let link = document.blocks[1]
            .spans
            .iter()
            .find_map(|span| span.link.as_deref());
        assert_eq!(link, Some("https://example.com/next"));
    }

    #[test]
    fn decodes_numeric_entities() {
        assert_eq!(decode_entities("&#9731; &#x1F680;"), "☃ 🚀");
    }

    #[test]
    fn ignores_greater_than_inside_quoted_attributes() {
        let document = parse_html(
            r#"<p><a title="1 > 0" href="next">Works</a></p>"#,
            "https://example.com/a/",
        );
        assert_eq!(document.plain_text(), "Works");
    }
}
