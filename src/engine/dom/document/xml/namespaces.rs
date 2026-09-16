//! Preserve explicit xmlns attributes, including redundant declarations. The semantic
//! XML reader resolves namespaces but exposes only the in-scope map, not declarations.
use xml::common::TextPosition;

pub(super) struct Declarations<'a> {
    input: &'a str,
    lines: Vec<usize>,
    tags: std::collections::BTreeMap<usize, (usize, Vec<String>)>,
}

impl<'a> Declarations<'a> {
    pub(super) fn new(input: &'a str) -> Self {
        let mut lines = vec![0];
        for (offset, byte) in input.bytes().enumerate() {
            if byte == b'\n' || (byte == b'\r' && input.as_bytes().get(offset + 1) != Some(&b'\n'))
            {
                lines.push(offset + 1);
            }
        }
        let mut tags = std::collections::BTreeMap::new();
        let mut start = None;
        let mut declared = Vec::new();
        for token in xmlparser::Tokenizer::from(input) {
            if tags.len() >= crate::limits::MAX_DOM_NODES {
                break;
            }
            match token {
                Ok(xmlparser::Token::ElementStart { span, .. }) => {
                    start = Some(span.start());
                    declared.clear();
                }
                Ok(xmlparser::Token::Attribute { prefix, local, .. })
                    if prefix.as_str() == "xmlns" =>
                {
                    declared.push(local.as_str().into())
                }
                Ok(xmlparser::Token::Attribute { prefix, local, .. })
                    if prefix.is_empty() && local.as_str() == "xmlns" =>
                {
                    declared.push(String::new())
                }
                Ok(xmlparser::Token::ElementEnd { span, .. }) => {
                    if let Some(start) = start.take() {
                        tags.insert(start, (span.end(), std::mem::take(&mut declared)));
                    }
                }
                Err(_) => break,
                _ => {}
            }
        }
        Self { input, lines, tags }
    }

    pub(super) fn at(&self, position: TextPosition) -> Option<Vec<String>> {
        let line = *self.lines.get(position.row as usize)?;
        let column = self.input[line..]
            .char_indices()
            .nth(position.column as usize)
            .map_or(self.input.len() - line, |(offset, _)| offset);
        let offset = line + column;
        let (_, (end, declarations)) = self.tags.range(..=offset).next_back()?;
        (offset <= *end).then(|| declarations.clone())
    }
}
