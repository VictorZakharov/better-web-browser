//! Style-set construction, cascade ordering, and presentational hints.

use super::*;

#[derive(Debug, Default)]
pub struct StyleSet {
    pub styles: HashMap<NodeId, ComputedStyle>,
    rules: Vec<Rule>,
    document_base_url: String,
}

impl StyleSet {
    pub fn from_dom(dom: &Dom, external_stylesheets: &[String], viewport_width: f32) -> Self {
        let sources = external_stylesheets
            .iter()
            .map(|stylesheet| (String::new(), stylesheet.clone()))
            .collect::<Vec<_>>();
        Self::from_document(&dom.document, "", &sources, viewport_width)
    }

    pub(crate) fn from_sources(
        dom: &Dom,
        document_base_url: &str,
        external_stylesheets: &[(String, String)],
        viewport_width: f32,
    ) -> Self {
        Self::from_document(
            &dom.document,
            document_base_url,
            external_stylesheets,
            viewport_width,
        )
    }

    pub(crate) fn from_document(
        document: &NodeRef,
        document_base_url: &str,
        external_stylesheets: &[(String, String)],
        viewport_width: f32,
    ) -> Self {
        let mut rules = Vec::new();
        let mut next_order = 0_u32;
        for style_element in
            dom::Node::descendants(document).filter(|node| node.tag_name() == Some("style"))
        {
            parse_stylesheet(
                &style_element.text_content(),
                document_base_url,
                viewport_width,
                &mut next_order,
                &mut rules,
            );
        }
        for (source_url, stylesheet) in external_stylesheets {
            parse_stylesheet(
                stylesheet,
                source_url,
                viewport_width,
                &mut next_order,
                &mut rules,
            );
        }
        let mut set = Self {
            styles: HashMap::new(),
            rules,
            document_base_url: document_base_url.to_string(),
        };
        set.compute_subtree(document, None);
        set
    }

    pub fn get(&self, node: &NodeRef) -> &ComputedStyle {
        self.styles
            .get(&node_id(node))
            .expect("style should exist for every DOM node")
    }

    fn compute_subtree(&mut self, node: &NodeRef, parent: Option<&ComputedStyle>) {
        let style = self.compute_style(node, parent);
        self.styles.insert(node_id(node), style.clone());
        for child in node.children.borrow().iter() {
            self.compute_subtree(child, Some(&style));
        }
    }

    fn compute_style(&self, node: &NodeRef, parent: Option<&ComputedStyle>) -> ComputedStyle {
        let mut style = ComputedStyle::inherit_from(parent);
        apply_user_agent_defaults(node, &mut style);

        let mut matching = self
            .rules
            .iter()
            .filter(|rule| selector_matches(&rule.selector, node))
            .collect::<Vec<_>>();
        matching.sort_by(|left, right| {
            left.selector
                .specificity
                .cmp(&right.selector.specificity)
                .then_with(|| left.order.cmp(&right.order))
        });
        let inline_declarations = node
            .attr("style")
            .map(|inline| parse_declarations(&inline))
            .unwrap_or_default();

        // CSS Cascade places every important author declaration above every normal author
        // declaration. Inline declarations retain their higher specificity within each group.
        // https://drafts.csswg.org/css-cascade/#importance
        let mut cascaded = Vec::new();
        for important in [false, true] {
            for rule in &matching {
                cascaded.extend(
                    rule.declarations
                        .iter()
                        .filter(|declaration| declaration.important == important)
                        .map(|declaration| (declaration, rule.base_url.as_str())),
                );
            }
            cascaded.extend(
                inline_declarations
                    .iter()
                    .filter(|declaration| declaration.important == important)
                    .map(|declaration| (declaration, self.document_base_url.as_str())),
            );
        }

        for &(declaration, _) in &cascaded {
            apply_custom_properties(&mut style, std::slice::from_ref(declaration), parent);
        }
        for &(declaration, base_url) in &cascaded {
            if declaration.name != "line-height" {
                apply_resolved_declaration(&mut style, declaration, parent, base_url);
            }
        }
        // line-height depends on the winning font-size, independent of declaration source order.
        for &(declaration, base_url) in &cascaded {
            if declaration.name == "line-height" {
                apply_resolved_declaration(&mut style, declaration, parent, base_url);
            }
        }
        apply_presentational_hints(node, &mut style);
        if node.attr("hidden").is_some() || is_hidden_by_html_rendering(node) {
            style.display = Display::None;
        }
        style.line_height = style.line_height.max(style.font_size);
        style
    }
}

fn apply_presentational_hints(node: &NodeRef, style: &mut ComputedStyle) {
    if let Some(align) = node.attr("align") {
        style.text_align = match align.to_ascii_lowercase().as_str() {
            "center" | "middle" => TextAlign::Center,
            "right" => TextAlign::End,
            _ => TextAlign::Start,
        };
    }
    if node.attr("nowrap").is_some() {
        style.white_space = WhiteSpace::NoWrap;
    }
    if style.width == Length::Auto
        && let Some(width) = node
            .attr("width")
            .and_then(|value| parse_html_length(&value))
    {
        style.width = width;
    }
    if style.height == Length::Auto
        && let Some(height) = node
            .attr("height")
            .and_then(|value| parse_html_length(&value))
    {
        style.height = height;
    }
    if let Some(color) = node.attr("color").and_then(|value| parse_color(&value)) {
        style.color = color;
    }
    if let Some(background) = node.attr("bgcolor").and_then(|value| parse_color(&value)) {
        style.background_color = background;
    }
    if node.tag_name() == Some("font") {
        if let Some(face) = node.attr("face") {
            style.font_family = first_font_family(&face);
        }
        if let Some(size) = node
            .attr("size")
            .and_then(|value| value.parse::<i32>().ok())
        {
            const LEGACY_SIZES: [f32; 7] = [10.0, 13.0, 16.0, 18.0, 24.0, 32.0, 48.0];
            style.font_size = LEGACY_SIZES[(size.clamp(1, 7) - 1) as usize];
            style.line_height = style.font_size * 1.2;
        }
    }
}

fn parse_html_length(value: &str) -> Option<Length> {
    let value = value.trim();
    if let Some(percent) = value.strip_suffix('%') {
        percent.parse::<f32>().ok().map(Length::Percent)
    } else {
        value
            .trim_end_matches("px")
            .parse::<f32>()
            .ok()
            .map(Length::Px)
    }
}

fn node_id(node: &NodeRef) -> NodeId {
    node.id()
}
