use super::*;

pub(super) fn layout_document(
    dc: Hdc,
    fonts: &Fonts,
    document: &Document,
    left: i32,
    width: i32,
) -> (Vec<DrawItem>, i32) {
    let mut items = Vec::new();
    let mut y = 28;
    unsafe {
        layout_spans(
            dc,
            fonts,
            &mut items,
            &[Span {
                text: document.title.clone(),
                link: None,
            }],
            FontKind::Heading1,
            left,
            width,
            &mut y,
            43,
            rgb(30, 34, 40),
            "",
        );
        y += 2;
        layout_spans(
            dc,
            fonts,
            &mut items,
            &[Span {
                text: document.source_url.clone(),
                link: Some(document.source_url.clone()),
            }],
            FontKind::Small,
            left,
            width,
            &mut y,
            22,
            rgb(38, 102, 180),
            "",
        );
        y += 25;

        for block in &document.blocks {
            let (font, line_height, color, indent, prefix, spacing) = match block.kind {
                BlockKind::Heading(1) => (FontKind::Heading2, 36, rgb(35, 39, 46), 0, "", 18),
                BlockKind::Heading(2) => (FontKind::Heading2, 36, rgb(35, 39, 46), 0, "", 16),
                BlockKind::Heading(_) => (FontKind::Heading3, 31, rgb(43, 47, 54), 0, "", 13),
                BlockKind::ListItem => (FontKind::Body, 28, rgb(42, 44, 48), 22, "• ", 5),
                BlockKind::Quote => (FontKind::Body, 29, rgb(80, 82, 86), 30, "“ ", 12),
                BlockKind::Preformatted => (FontKind::Mono, 26, rgb(48, 50, 53), 18, "", 13),
                BlockKind::Paragraph => (FontKind::Body, 29, rgb(42, 44, 48), 0, "", 12),
            };
            y += spacing;
            layout_spans(
                dc,
                fonts,
                &mut items,
                &block.spans,
                font,
                left + indent,
                width - indent,
                &mut y,
                line_height,
                color,
                prefix,
            );
        }

        if document.truncated {
            y += 18;
            layout_spans(
                dc,
                fonts,
                &mut items,
                &[Span {
                    text: "Document text was truncated at the 2 MiB safety limit.".into(),
                    link: None,
                }],
                FontKind::Small,
                left,
                width,
                &mut y,
                22,
                rgb(160, 70, 35),
                "",
            );
        }
    }
    (items, y + 48)
}

#[allow(clippy::too_many_arguments)]
unsafe fn layout_spans(
    dc: Hdc,
    fonts: &Fonts,
    output: &mut Vec<DrawItem>,
    spans: &[Span],
    font: FontKind,
    left: i32,
    width: i32,
    y: &mut i32,
    line_height: i32,
    color: u32,
    prefix: &str,
) {
    SelectObject(dc, fonts.get(font));
    let right = left + width;
    let mut x = left;
    let mut line_has_text = false;

    if !prefix.is_empty() {
        let prefix_width = measure_text(dc, prefix).cx;
        output.push(DrawItem {
            x,
            y: *y,
            width: prefix_width,
            height: line_height,
            text: prefix.to_string(),
            link: None,
            font,
            color,
        });
        x += prefix_width;
        line_has_text = true;
    }

    let mut pending_space = false;
    for span in spans {
        for (word, preceded_by_space) in words_with_spacing(&span.text) {
            let needs_space = line_has_text && (pending_space || preceded_by_space);
            let display = if needs_space {
                format!(" {word}")
            } else {
                word.to_string()
            };
            let mut item_width = measure_text(dc, &display).cx;
            if x + item_width > right && line_has_text {
                *y += line_height;
                x = left;
                let display_without_space = word.to_string();
                item_width = measure_text(dc, &display_without_space).cx;
                output.push(DrawItem {
                    x,
                    y: *y,
                    width: item_width,
                    height: line_height,
                    text: display_without_space,
                    link: span.link.clone(),
                    font,
                    color: if span.link.is_some() {
                        rgb(38, 102, 180)
                    } else {
                        color
                    },
                });
            } else {
                output.push(DrawItem {
                    x,
                    y: *y,
                    width: item_width,
                    height: line_height,
                    text: display,
                    link: span.link.clone(),
                    font,
                    color: if span.link.is_some() {
                        rgb(38, 102, 180)
                    } else {
                        color
                    },
                });
            }
            x += item_width;
            line_has_text = true;
            pending_space = false;
        }
        pending_space = span.text.chars().last().is_some_and(char::is_whitespace);
    }
    *y += line_height;
}

pub(super) fn words_with_spacing(text: &str) -> Vec<(&str, bool)> {
    let mut words = Vec::new();
    let mut word_start = None;
    let mut whitespace_before_word = false;
    let mut saw_whitespace = false;
    for (index, character) in text.char_indices() {
        if character.is_whitespace() {
            if let Some(start) = word_start.take() {
                words.push((&text[start..index], whitespace_before_word));
            }
            saw_whitespace = true;
        } else if word_start.is_none() {
            word_start = Some(index);
            whitespace_before_word = saw_whitespace;
            saw_whitespace = false;
        }
    }
    if let Some(start) = word_start {
        words.push((&text[start..], whitespace_before_word));
    }
    words
}
