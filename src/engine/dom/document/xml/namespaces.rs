//! Preserve explicit xmlns attributes, including redundant declarations. The semantic
//! XML reader resolves namespaces but exposes only the in-scope map, not declarations.
use xml::common::TextPosition;

pub(super) struct Declarations<'a> {
    input: &'a str,
    lines: Vec<usize>,
    tags: std::collections::BTreeMap<usize, (usize, Vec<String>)>,
    cursor: Option<(TextPosition, usize)>,
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
        Self {
            input,
            lines,
            tags,
            cursor: None,
        }
    }

    pub(super) fn at(&mut self, position: TextPosition) -> Option<Vec<String>> {
        let line = *self.lines.get(position.row as usize)?;
        // Reader positions advance through the source. Continue from the previous lookup
        // instead of rescanning a potentially huge minified line for every start element.
        let (start, column) = self
            .cursor
            .filter(|(previous, _)| {
                previous.row == position.row && previous.column <= position.column
            })
            .map_or((line, position.column), |(previous, offset)| {
                (offset, position.column - previous.column)
            });
        let column = self.input[start..]
            .char_indices()
            .nth(column as usize)
            .map_or(self.input.len() - start, |(offset, _)| offset);
        let offset = start + column;
        self.cursor = Some((position, offset));
        let (_, (end, declarations)) = self.tags.range(..=offset).next_back()?;
        (offset <= *end).then(|| declarations.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_positions_handle_unicode_repeated_lookups_and_all_xml_newlines() {
        for newline in ["\n", "\r", "\r\n"] {
            let source = format!("<r>é<a xmlns:p='urn:p'/><b/>{newline}<c xmlns:q='urn:q'/></r>");
            let mut declarations = Declarations::new(&source);
            let position = |row, column| TextPosition { row, column };
            assert_eq!(declarations.at(position(0, 4)), Some(vec!["p".into()]));
            assert_eq!(declarations.at(position(0, 4)), Some(vec!["p".into()]));
            let next_column = source[..source.find("<b/>").unwrap()].chars().count() as u64;
            assert_eq!(declarations.at(position(0, next_column)), Some(vec![]));
            assert_eq!(declarations.at(position(1, 0)), Some(vec!["q".into()]));
            assert_eq!(declarations.at(position(0, 4)), Some(vec!["p".into()]));
        }
    }
}
