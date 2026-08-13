use super::dom::{Dom, NodeRef};
use cssparser::color::{parse_hash_color, parse_named_color};
use std::cmp::Ordering;
use std::collections::HashMap;
use std::rc::Rc;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Color {
    pub red: u8,
    pub green: u8,
    pub blue: u8,
    pub alpha: u8,
}

impl Color {
    pub const BLACK: Self = Self::rgb(0, 0, 0);
    pub const WHITE: Self = Self::rgb(255, 255, 255);
    pub const TRANSPARENT: Self = Self {
        red: 0,
        green: 0,
        blue: 0,
        alpha: 0,
    };

    pub const fn rgb(red: u8, green: u8, blue: u8) -> Self {
        Self {
            red,
            green,
            blue,
            alpha: 255,
        }
    }

    pub fn to_colorref(self) -> u32 {
        self.red as u32 | ((self.green as u32) << 8) | ((self.blue as u32) << 16)
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Length {
    Auto,
    Px(f32),
    Percent(f32),
    Em(f32),
    Vw(f32),
    Vh(f32),
}

impl Length {
    pub fn resolve(self, basis: f32, font_size: f32) -> Option<f32> {
        match self {
            Self::Auto => None,
            Self::Px(value) => Some(value),
            Self::Percent(value) => Some(basis * value / 100.0),
            Self::Em(value) => Some(font_size * value),
            Self::Vw(value) | Self::Vh(value) => Some(basis * value / 100.0),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Edges {
    pub top: Length,
    pub right: Length,
    pub bottom: Length,
    pub left: Length,
}

impl Edges {
    pub const ZERO: Self = Self {
        top: Length::Px(0.0),
        right: Length::Px(0.0),
        bottom: Length::Px(0.0),
        left: Length::Px(0.0),
    };

    pub fn resolve(self, width: f32, font_size: f32) -> ResolvedEdges {
        ResolvedEdges {
            top: self.top.resolve(width, font_size).unwrap_or(0.0),
            right: self.right.resolve(width, font_size).unwrap_or(0.0),
            bottom: self.bottom.resolve(width, font_size).unwrap_or(0.0),
            left: self.left.resolve(width, font_size).unwrap_or(0.0),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct ResolvedEdges {
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
    pub left: f32,
}

impl ResolvedEdges {
    pub fn horizontal(self) -> f32 {
        self.left + self.right
    }

    pub fn vertical(self) -> f32 {
        self.top + self.bottom
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Display {
    None,
    Block,
    Inline,
    InlineBlock,
    Flex,
    Grid,
    Table,
    TableRow,
    TableCell,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Position {
    Static,
    Relative,
    Absolute,
    Fixed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FlexDirection {
    Row,
    Column,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JustifyContent {
    Start,
    End,
    Center,
    SpaceBetween,
    SpaceAround,
    SpaceEvenly,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AlignItems {
    Stretch,
    Start,
    End,
    Center,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextAlign {
    Start,
    Center,
    End,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WhiteSpace {
    Normal,
    NoWrap,
    Pre,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Float {
    None,
    Left,
    Right,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BoxSizing {
    ContentBox,
    BorderBox,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ComputedStyle {
    pub display: Display,
    pub position: Position,
    pub float: Float,
    pub color: Color,
    pub background_color: Color,
    pub font_size: f32,
    pub font_weight: u16,
    pub italic: bool,
    pub font_family: String,
    pub line_height: f32,
    pub text_align: TextAlign,
    pub white_space: WhiteSpace,
    pub text_decoration_underline: bool,
    pub width: Length,
    pub height: Length,
    pub min_width: Length,
    pub min_height: Length,
    pub max_width: Length,
    pub max_height: Length,
    pub margin: Edges,
    pub padding: Edges,
    pub border_width: Edges,
    pub border_color: Color,
    pub border_radius: Length,
    pub top: Length,
    pub right: Length,
    pub bottom: Length,
    pub left: Length,
    pub visibility: bool,
    pub opacity: f32,
    pub overflow_hidden: bool,
    pub justify_content_end: bool,
    pub align_items_center: bool,
    pub flex_direction: FlexDirection,
    pub justify_content: JustifyContent,
    pub align_items: AlignItems,
    pub flex_wrap: bool,
    pub flex_grow: f32,
    pub flex_shrink: f32,
    pub flex_basis: Length,
    pub box_sizing: BoxSizing,
    pub grid_template_columns: String,
    pub grid_template_rows: String,
    pub grid_column_gap: Length,
    pub grid_row_gap: Length,
    pub grid_column_start: Option<usize>,
    pub grid_column_end: Option<usize>,
    pub grid_row_start: Option<usize>,
    pub grid_row_end: Option<usize>,
}

impl ComputedStyle {
    fn initial() -> Self {
        Self {
            display: Display::Inline,
            position: Position::Static,
            float: Float::None,
            color: Color::BLACK,
            background_color: Color::TRANSPARENT,
            font_size: 16.0,
            font_weight: 400,
            italic: false,
            font_family: "Arial".to_string(),
            line_height: 19.2,
            text_align: TextAlign::Start,
            white_space: WhiteSpace::Normal,
            text_decoration_underline: false,
            width: Length::Auto,
            height: Length::Auto,
            min_width: Length::Auto,
            min_height: Length::Auto,
            max_width: Length::Auto,
            max_height: Length::Auto,
            margin: Edges::ZERO,
            padding: Edges::ZERO,
            border_width: Edges::ZERO,
            border_color: Color::BLACK,
            border_radius: Length::Px(0.0),
            top: Length::Auto,
            right: Length::Auto,
            bottom: Length::Auto,
            left: Length::Auto,
            visibility: true,
            opacity: 1.0,
            overflow_hidden: false,
            justify_content_end: false,
            align_items_center: false,
            flex_direction: FlexDirection::Row,
            justify_content: JustifyContent::Start,
            align_items: AlignItems::Stretch,
            flex_wrap: false,
            flex_grow: 0.0,
            flex_shrink: 1.0,
            flex_basis: Length::Auto,
            box_sizing: BoxSizing::ContentBox,
            grid_template_columns: String::new(),
            grid_template_rows: String::new(),
            grid_column_gap: Length::Px(0.0),
            grid_row_gap: Length::Px(0.0),
            grid_column_start: None,
            grid_column_end: None,
            grid_row_start: None,
            grid_row_end: None,
        }
    }

    fn inherit_from(parent: Option<&Self>) -> Self {
        let mut style = Self::initial();
        if let Some(parent) = parent {
            style.color = parent.color;
            style.font_size = parent.font_size;
            style.font_weight = parent.font_weight;
            style.italic = parent.italic;
            style.font_family.clone_from(&parent.font_family);
            style.line_height = parent.line_height;
            style.text_align = parent.text_align;
            style.white_space = parent.white_space;
            style.visibility = parent.visibility;
        }
        style
    }
}

#[derive(Debug, Default)]
pub struct StyleSet {
    pub styles: HashMap<usize, ComputedStyle>,
    rules: Vec<Rule>,
}

impl StyleSet {
    pub fn from_dom(dom: &Dom, external_stylesheets: &[String], viewport_width: f32) -> Self {
        let mut rules = Vec::new();
        let mut next_order = 0_u32;
        for style_element in dom.elements_named("style") {
            parse_stylesheet(
                &style_element.text_content(),
                viewport_width,
                &mut next_order,
                &mut rules,
            );
        }
        for stylesheet in external_stylesheets {
            parse_stylesheet(stylesheet, viewport_width, &mut next_order, &mut rules);
        }
        let mut set = Self {
            styles: HashMap::new(),
            rules,
        };
        set.compute_subtree(&dom.document, None);
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
        for rule in matching {
            for declaration in &rule.declarations {
                apply_declaration(&mut style, declaration, parent);
            }
        }
        if let Some(inline) = node.attr("style") {
            for declaration in parse_declarations(&inline) {
                apply_declaration(&mut style, &declaration, parent);
            }
        }
        apply_presentational_hints(node, &mut style);
        if node.attr("hidden").is_some() {
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

fn node_id(node: &NodeRef) -> usize {
    Rc::as_ptr(node) as usize
}

#[derive(Debug)]
struct Rule {
    selector: Selector,
    declarations: Vec<Declaration>,
    order: u32,
}

#[derive(Debug, Clone)]
struct Declaration {
    name: String,
    value: String,
}

#[derive(Debug, Clone)]
struct Selector {
    compounds: Vec<CompoundSelector>,
    combinators: Vec<Combinator>,
    specificity: Specificity,
}

#[derive(Debug, Clone, Default)]
struct CompoundSelector {
    tag: Option<String>,
    id: Option<String>,
    classes: Vec<String>,
    attributes: Vec<AttributeSelector>,
    not: Vec<SimpleSelector>,
    requires_link: bool,
    requires_first_child: bool,
    never_matches: bool,
}

#[derive(Debug, Clone)]
struct AttributeSelector {
    name: String,
    operator: AttributeOperator,
    value: String,
    case_insensitive: bool,
}

#[derive(Debug, Clone, Copy)]
enum AttributeOperator {
    Exists,
    Equals,
    Includes,
    DashMatch,
    Prefix,
    Suffix,
    Substring,
}

#[derive(Debug, Clone)]
enum SimpleSelector {
    Tag(String),
    Id(String),
    Class(String),
}

#[derive(Debug, Clone, Copy)]
enum Combinator {
    Descendant,
    Child,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct Specificity {
    ids: u16,
    classes: u16,
    tags: u16,
}

impl Ord for Specificity {
    fn cmp(&self, other: &Self) -> Ordering {
        (self.ids, self.classes, self.tags).cmp(&(other.ids, other.classes, other.tags))
    }
}

impl PartialOrd for Specificity {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

fn parse_stylesheet(css: &str, viewport_width: f32, next_order: &mut u32, output: &mut Vec<Rule>) {
    let css = strip_comments(css);
    parse_rule_list(&css, viewport_width, next_order, output);
}

fn parse_rule_list(css: &str, viewport_width: f32, next_order: &mut u32, output: &mut Vec<Rule>) {
    let mut cursor = 0;
    while cursor < css.len() {
        cursor = skip_css_whitespace(css, cursor);
        if cursor >= css.len() {
            break;
        }
        let Some(open) = find_css_delimiter(css, cursor, '{') else {
            break;
        };
        let prelude = css[cursor..open].trim();
        let Some(close) = find_matching_brace(css, open) else {
            break;
        };
        let body = &css[open + 1..close];
        if prelude.starts_with("@media") {
            if media_matches(prelude, viewport_width) {
                parse_rule_list(body, viewport_width, next_order, output);
            }
        } else if !prelude.starts_with('@') {
            let declarations = parse_declarations(body);
            for selector_text in split_css_top_level(prelude, ',') {
                if let Some(selector) = parse_selector(selector_text.trim()) {
                    output.push(Rule {
                        selector,
                        declarations: declarations.clone(),
                        order: *next_order,
                    });
                    *next_order = next_order.wrapping_add(1);
                }
            }
        }
        cursor = close + 1;
    }
}

fn strip_comments(css: &str) -> String {
    let mut output = String::with_capacity(css.len());
    let mut cursor = 0;
    while let Some(start_offset) = css[cursor..].find("/*") {
        let start = cursor + start_offset;
        output.push_str(&css[cursor..start]);
        let Some(end_offset) = css[start + 2..].find("*/") else {
            return output;
        };
        cursor = start + 2 + end_offset + 2;
    }
    output.push_str(&css[cursor..]);
    output
}

fn parse_declarations(body: &str) -> Vec<Declaration> {
    split_css_top_level(body, ';')
        .filter_map(|declaration| {
            let (name, value) = split_css_once(declaration, ':')?;
            let name = name.trim().to_ascii_lowercase();
            let value = value
                .trim()
                .strip_suffix("!important")
                .unwrap_or(value.trim())
                .trim()
                .to_string();
            (!name.is_empty() && !value.is_empty()).then_some(Declaration { name, value })
        })
        .collect()
}

fn parse_selector(input: &str) -> Option<Selector> {
    if input.is_empty() || input.contains("::") {
        return None;
    }
    let tokens = selector_tokens(input);
    if tokens.is_empty() {
        return None;
    }
    let mut compounds = Vec::new();
    let mut combinators = Vec::new();
    let mut specificity = Specificity::default();
    let mut expect_compound = true;
    for token in tokens {
        match token {
            SelectorToken::Compound(text) => {
                let (compound, compound_specificity) = parse_compound_selector(&text)?;
                if !expect_compound {
                    combinators.push(Combinator::Descendant);
                }
                compounds.push(compound);
                specificity.ids += compound_specificity.ids;
                specificity.classes += compound_specificity.classes;
                specificity.tags += compound_specificity.tags;
                expect_compound = false;
            }
            SelectorToken::Combinator(combinator) if !expect_compound => {
                if combinators.len() < compounds.len() {
                    combinators.push(combinator);
                } else if let Some(last) = combinators.last_mut() {
                    *last = combinator;
                }
                expect_compound = true;
            }
            SelectorToken::Combinator(_) => {}
        }
    }
    if compounds.is_empty() || expect_compound || combinators.len() + 1 != compounds.len() {
        return None;
    }
    Some(Selector {
        compounds,
        combinators,
        specificity,
    })
}

enum SelectorToken {
    Compound(String),
    Combinator(Combinator),
}

fn selector_tokens(input: &str) -> Vec<SelectorToken> {
    let mut tokens = Vec::new();
    let mut start = 0;
    let mut depth = 0_i32;
    let mut in_attribute = 0_i32;
    let mut pending_space = false;
    for (index, character) in input.char_indices() {
        match character {
            '(' => depth += 1,
            ')' => depth = (depth - 1).max(0),
            '[' => in_attribute += 1,
            ']' => in_attribute = (in_attribute - 1).max(0),
            '>' if depth == 0 && in_attribute == 0 => {
                if start < index {
                    let text = input[start..index].trim();
                    if !text.is_empty() {
                        tokens.push(SelectorToken::Compound(text.to_string()));
                    }
                }
                tokens.push(SelectorToken::Combinator(Combinator::Child));
                start = index + character.len_utf8();
                pending_space = false;
            }
            character if character.is_whitespace() && depth == 0 && in_attribute == 0 => {
                if start < index {
                    let text = input[start..index].trim();
                    if !text.is_empty() {
                        tokens.push(SelectorToken::Compound(text.to_string()));
                    }
                }
                start = index + character.len_utf8();
                pending_space = true;
            }
            _ => {
                if pending_space {
                    if !matches!(tokens.last(), Some(SelectorToken::Combinator(_))) {
                        tokens.push(SelectorToken::Combinator(Combinator::Descendant));
                    }
                    pending_space = false;
                }
            }
        }
    }
    if start < input.len() {
        let text = input[start..].trim();
        if !text.is_empty() {
            tokens.push(SelectorToken::Compound(text.to_string()));
        }
    }
    while matches!(tokens.last(), Some(SelectorToken::Combinator(_))) {
        tokens.pop();
    }
    tokens
}

fn parse_compound_selector(input: &str) -> Option<(CompoundSelector, Specificity)> {
    let mut compound = CompoundSelector::default();
    let mut specificity = Specificity::default();
    let bytes = input.as_bytes();
    let mut cursor = 0;
    if bytes.first().is_some_and(|byte| *byte == b'*') {
        cursor = 1;
    } else if bytes
        .first()
        .is_some_and(|byte| byte.is_ascii_alphabetic() || *byte == b'-')
    {
        let end = consume_identifier(bytes, cursor);
        compound.tag = Some(input[cursor..end].to_ascii_lowercase());
        specificity.tags += 1;
        cursor = end;
    }

    while cursor < bytes.len() {
        match bytes[cursor] {
            b'#' => {
                let end = consume_identifier(bytes, cursor + 1);
                if end == cursor + 1 {
                    return None;
                }
                compound.id = Some(input[cursor + 1..end].to_string());
                specificity.ids += 1;
                cursor = end;
            }
            b'.' => {
                let end = consume_identifier(bytes, cursor + 1);
                if end == cursor + 1 {
                    return None;
                }
                compound.classes.push(input[cursor + 1..end].to_string());
                specificity.classes += 1;
                cursor = end;
            }
            b':' => {
                let name_end = consume_identifier(bytes, cursor + 1);
                let name = input[cursor + 1..name_end].to_ascii_lowercase();
                specificity.classes += 1;
                cursor = name_end;
                if cursor < bytes.len() && bytes[cursor] == b'(' {
                    let end = find_matching_parenthesis(input, cursor)?;
                    if name == "not" {
                        let argument = input[cursor + 1..end].trim();
                        if let Some(simple) = parse_simple_selector(argument) {
                            compound.not.push(simple);
                        }
                    }
                    cursor = end + 1;
                } else {
                    match name.as_str() {
                        "link" | "any-link" => compound.requires_link = true,
                        "first-child" => compound.requires_first_child = true,
                        "root" => {}
                        "hover" | "active" | "focus" | "visited" | "focus-visible" => {
                            compound.never_matches = true
                        }
                        _ => compound.never_matches = true,
                    }
                }
            }
            b'[' => {
                let relative_end = input[cursor + 1..].find(']')?;
                let end = cursor + 1 + relative_end;
                let attribute = parse_attribute_selector(&input[cursor + 1..end])?;
                compound.attributes.push(attribute);
                specificity.classes += 1;
                cursor = end + 1;
            }
            _ => return None,
        }
    }
    Some((compound, specificity))
}

fn parse_attribute_selector(input: &str) -> Option<AttributeSelector> {
    let mut expression = input.trim();
    let mut case_insensitive = false;
    if expression.len() >= 2 {
        let suffix = &expression[expression.len() - 2..];
        if suffix.eq_ignore_ascii_case(" i") {
            case_insensitive = true;
            expression = expression[..expression.len() - 2].trim_end();
        } else if suffix.eq_ignore_ascii_case(" s") {
            expression = expression[..expression.len() - 2].trim_end();
        }
    }

    let operators = [
        ("~=", AttributeOperator::Includes),
        ("|=", AttributeOperator::DashMatch),
        ("^=", AttributeOperator::Prefix),
        ("$=", AttributeOperator::Suffix),
        ("*=", AttributeOperator::Substring),
        ("=", AttributeOperator::Equals),
    ];
    for (token, operator) in operators {
        if let Some(index) = expression.find(token) {
            let name = expression[..index].trim().to_ascii_lowercase();
            let value = expression[index + token.len()..]
                .trim()
                .trim_matches(['\'', '"'])
                .to_string();
            return (!name.is_empty()).then_some(AttributeSelector {
                name,
                operator,
                value,
                case_insensitive,
            });
        }
    }

    let name = expression.to_ascii_lowercase();
    (!name.is_empty()).then_some(AttributeSelector {
        name,
        operator: AttributeOperator::Exists,
        value: String::new(),
        case_insensitive,
    })
}

fn parse_simple_selector(input: &str) -> Option<SimpleSelector> {
    if let Some(id) = input.strip_prefix('#') {
        Some(SimpleSelector::Id(id.to_string()))
    } else if let Some(class) = input.strip_prefix('.') {
        Some(SimpleSelector::Class(class.to_string()))
    } else if !input.is_empty() {
        Some(SimpleSelector::Tag(input.to_ascii_lowercase()))
    } else {
        None
    }
}

fn selector_matches(selector: &Selector, node: &NodeRef) -> bool {
    fn matches_at(selector: &Selector, index: usize, node: &NodeRef) -> bool {
        if !compound_matches(&selector.compounds[index], node) {
            return false;
        }
        if index == 0 {
            return true;
        }
        match selector.combinators[index - 1] {
            Combinator::Child => node
                .parent()
                .is_some_and(|parent| matches_at(selector, index - 1, &parent)),
            Combinator::Descendant => {
                let mut ancestor = node.parent();
                while let Some(candidate) = ancestor {
                    if matches_at(selector, index - 1, &candidate) {
                        return true;
                    }
                    ancestor = candidate.parent();
                }
                false
            }
        }
    }

    matches_at(selector, selector.compounds.len() - 1, node)
}

fn compound_matches(selector: &CompoundSelector, node: &NodeRef) -> bool {
    if selector.never_matches || node.element().is_none() {
        return false;
    }
    if selector
        .tag
        .as_deref()
        .is_some_and(|tag| node.tag_name() != Some(tag))
    {
        return false;
    }
    if selector
        .id
        .as_deref()
        .is_some_and(|id| node.attr("id").as_deref() != Some(id))
    {
        return false;
    }
    if selector.classes.iter().any(|class| !node.has_class(class)) {
        return false;
    }
    if selector
        .attributes
        .iter()
        .any(|attribute| !attribute_matches(attribute, node))
    {
        return false;
    }
    if selector.requires_link && node.tag_name() != Some("a") {
        return false;
    }
    if selector.requires_first_child {
        let Some(parent) = node.parent() else {
            return false;
        };
        let is_first = parent
            .children
            .borrow()
            .iter()
            .find(|child| child.element().is_some())
            .is_some_and(|child| Rc::ptr_eq(child, node));
        if !is_first {
            return false;
        }
    }
    !selector.not.iter().any(|simple| match simple {
        SimpleSelector::Tag(tag) => node.tag_name() == Some(tag),
        SimpleSelector::Id(id) => node.attr("id").as_deref() == Some(id),
        SimpleSelector::Class(class) => node.has_class(class),
    })
}

fn attribute_matches(selector: &AttributeSelector, node: &NodeRef) -> bool {
    let Some(actual) = node.attr(&selector.name) else {
        return false;
    };
    if matches!(selector.operator, AttributeOperator::Exists) {
        return true;
    }

    let expected = selector.value.as_str();
    let compare = |left: &str, right: &str| {
        if selector.case_insensitive {
            left.eq_ignore_ascii_case(right)
        } else {
            left == right
        }
    };
    let normalized_actual;
    let normalized_expected;
    let (actual, expected) = if selector.case_insensitive {
        normalized_actual = actual.to_ascii_lowercase();
        normalized_expected = expected.to_ascii_lowercase();
        (normalized_actual.as_str(), normalized_expected.as_str())
    } else {
        (actual.as_str(), expected)
    };

    match selector.operator {
        AttributeOperator::Exists => true,
        AttributeOperator::Equals => compare(actual, expected),
        AttributeOperator::Includes => actual
            .split_ascii_whitespace()
            .any(|value| compare(value, expected)),
        AttributeOperator::DashMatch => {
            compare(actual, expected)
                || actual
                    .strip_prefix(expected)
                    .is_some_and(|suffix| suffix.starts_with('-'))
        }
        AttributeOperator::Prefix => actual.starts_with(expected),
        AttributeOperator::Suffix => actual.ends_with(expected),
        AttributeOperator::Substring => actual.contains(expected),
    }
}

fn apply_user_agent_defaults(node: &NodeRef, style: &mut ComputedStyle) {
    let Some(tag) = node.tag_name() else {
        return;
    };
    style.display = match tag {
        "html" | "body" | "address" | "article" | "aside" | "blockquote" | "center" | "details"
        | "dialog" | "div" | "dl" | "fieldset" | "figcaption" | "figure" | "footer" | "form"
        | "header" | "hgroup" | "hr" | "main" | "nav" | "ol" | "p" | "pre" | "section"
        | "summary" | "ul" | "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => Display::Block,
        "table" => Display::Table,
        "tr" => Display::TableRow,
        "td" | "th" => Display::TableCell,
        "img" | "input" | "button" | "select" | "textarea" | "svg" => Display::InlineBlock,
        "head" | "base" | "link" | "meta" | "title" | "style" | "script" | "template" => {
            Display::None
        }
        _ => Display::Inline,
    };
    match tag {
        "body" => style.margin = uniform_edges(Length::Px(8.0)),
        "p" => {
            style.margin.top = Length::Em(1.0);
            style.margin.bottom = Length::Em(1.0);
        }
        "blockquote" => {
            style.margin.top = Length::Em(1.0);
            style.margin.right = Length::Px(40.0);
            style.margin.bottom = Length::Em(1.0);
            style.margin.left = Length::Px(40.0);
        }
        "h1" => heading_defaults(style, 2.0, 0.67),
        "h2" => heading_defaults(style, 1.5, 0.83),
        "h3" => heading_defaults(style, 1.17, 1.0),
        "h4" => heading_defaults(style, 1.0, 1.33),
        "h5" => heading_defaults(style, 0.83, 1.67),
        "h6" => heading_defaults(style, 0.67, 2.33),
        "pre" => {
            style.font_family = "Cascadia Mono".to_string();
            style.white_space = WhiteSpace::Pre;
            style.margin.top = Length::Em(1.0);
            style.margin.bottom = Length::Em(1.0);
        }
        "b" | "strong" => style.font_weight = 700,
        "i" | "em" => style.italic = true,
        "small" => style.font_size *= 0.833,
        "a" => {
            style.color = Color::rgb(0, 0, 238);
            style.text_decoration_underline = true;
        }
        "input" | "button" | "select" | "textarea" => {
            style.background_color = Color::WHITE;
            style.border_width = uniform_edges(Length::Px(2.0));
            style.border_color = Color::rgb(118, 118, 118);
        }
        "center" => style.text_align = TextAlign::Center,
        "th" => {
            style.font_weight = 700;
            style.text_align = TextAlign::Center;
        }
        "ul" | "ol" => {
            style.margin.top = Length::Em(1.0);
            style.margin.bottom = Length::Em(1.0);
            style.padding.left = Length::Px(40.0);
        }
        "hr" => {
            style.border_width.top = Length::Px(1.0);
            style.margin.top = Length::Em(0.5);
            style.margin.bottom = Length::Em(0.5);
        }
        _ => {}
    }
}

fn heading_defaults(style: &mut ComputedStyle, scale: f32, margin: f32) {
    style.font_size *= scale;
    style.line_height = style.font_size * 1.2;
    style.font_weight = 700;
    style.margin.top = Length::Em(margin);
    style.margin.bottom = Length::Em(margin);
}

fn apply_declaration(
    style: &mut ComputedStyle,
    declaration: &Declaration,
    parent: Option<&ComputedStyle>,
) {
    let value = declaration.value.trim();
    if value.eq_ignore_ascii_case("inherit") {
        let initial = ComputedStyle::initial();
        let inherited = parent.unwrap_or(&initial);
        match declaration.name.as_str() {
            "background" | "background-color" => {
                style.background_color = inherited.background_color;
            }
            "box-sizing" => style.box_sizing = inherited.box_sizing,
            "color" => style.color = inherited.color,
            "font-family" => style.font_family.clone_from(&inherited.font_family),
            "max-width" => style.max_width = inherited.max_width,
            "width" => style.width = inherited.width,
            _ => {}
        }
        return;
    }
    match declaration.name.as_str() {
        "display" => {
            style.display = match value.split_ascii_whitespace().next().unwrap_or("") {
                "none" => Display::None,
                "block" => Display::Block,
                "inline" => Display::Inline,
                "inline-block" | "inline-box" => Display::InlineBlock,
                "flex" | "-webkit-flex" | "-webkit-box" => Display::Flex,
                "grid" | "-ms-grid" => Display::Grid,
                "table" => Display::Table,
                "table-row" => Display::TableRow,
                "table-cell" => Display::TableCell,
                _ => style.display,
            };
        }
        "position" => {
            style.position = match value {
                "relative" => Position::Relative,
                "absolute" => Position::Absolute,
                "fixed" => Position::Fixed,
                _ => Position::Static,
            };
        }
        "float" => {
            style.float = match value {
                "left" => Float::Left,
                "right" => Float::Right,
                _ => Float::None,
            };
        }
        "color" => {
            if let Some(color) = parse_color(value) {
                style.color = color;
            }
        }
        "background-color" => {
            if let Some(color) = parse_color(value) {
                style.background_color = color;
            }
        }
        "background" => {
            if let Some(color) = value.split_ascii_whitespace().find_map(parse_color) {
                style.background_color = color;
            }
        }
        "font-size" => {
            if let Some(size) = parse_font_size(value, style.font_size) {
                style.font_size = size;
                style.line_height = size * 1.2;
            }
        }
        "font-weight" => {
            style.font_weight = match value {
                "normal" => 400,
                "bold" | "bolder" => 700,
                "lighter" => 300,
                _ => value.parse::<u16>().unwrap_or(style.font_weight),
            }
        }
        "font-style" => style.italic = matches!(value, "italic" | "oblique"),
        "font-family" => style.font_family = first_font_family(value),
        "font" => apply_font_shorthand(style, value),
        "line-height" => {
            if let Some(line_height) = parse_line_height(value, style.font_size) {
                style.line_height = line_height;
            }
        }
        "text-align" => {
            style.text_align = match value {
                "center" => TextAlign::Center,
                "right" | "end" => TextAlign::End,
                _ => TextAlign::Start,
            }
        }
        "white-space" => {
            style.white_space = match value {
                "nowrap" => WhiteSpace::NoWrap,
                "pre" | "pre-wrap" => WhiteSpace::Pre,
                _ => WhiteSpace::Normal,
            }
        }
        "text-decoration" | "text-decoration-line" => {
            style.text_decoration_underline = value.contains("underline");
        }
        "width" => assign_length(&mut style.width, value),
        "height" => assign_length(&mut style.height, value),
        "min-width" => assign_length(&mut style.min_width, value),
        "min-height" => assign_length(&mut style.min_height, value),
        "max-width" => assign_length(&mut style.max_width, value),
        "max-height" => assign_length(&mut style.max_height, value),
        "top" => assign_length(&mut style.top, value),
        "right" => assign_length(&mut style.right, value),
        "bottom" => assign_length(&mut style.bottom, value),
        "left" => assign_length(&mut style.left, value),
        "margin" => assign_edges(&mut style.margin, value),
        "margin-top" => assign_length(&mut style.margin.top, value),
        "margin-right" => assign_length(&mut style.margin.right, value),
        "margin-bottom" => assign_length(&mut style.margin.bottom, value),
        "margin-left" => assign_length(&mut style.margin.left, value),
        "padding" => assign_edges(&mut style.padding, value),
        "padding-top" => assign_length(&mut style.padding.top, value),
        "padding-right" => assign_length(&mut style.padding.right, value),
        "padding-bottom" => assign_length(&mut style.padding.bottom, value),
        "padding-left" => assign_length(&mut style.padding.left, value),
        "border-width" => assign_edges(&mut style.border_width, value),
        "border-top-width" => assign_length(&mut style.border_width.top, value),
        "border-right-width" => assign_length(&mut style.border_width.right, value),
        "border-bottom-width" => assign_length(&mut style.border_width.bottom, value),
        "border-left-width" => assign_length(&mut style.border_width.left, value),
        "border-color" => {
            if let Some(color) = value.split_ascii_whitespace().find_map(parse_color) {
                style.border_color = color;
            }
        }
        "border" | "border-top" | "border-right" | "border-bottom" | "border-left" => {
            let width = if value.split_ascii_whitespace().any(|token| token == "none") {
                Length::Px(0.0)
            } else {
                value
                    .split_ascii_whitespace()
                    .find_map(parse_length)
                    .unwrap_or(Length::Px(1.0))
            };
            match declaration.name.as_str() {
                "border-top" => style.border_width.top = width,
                "border-right" => style.border_width.right = width,
                "border-bottom" => style.border_width.bottom = width,
                "border-left" => style.border_width.left = width,
                _ => style.border_width = uniform_edges(width),
            }
            if let Some(color) = value.split_ascii_whitespace().find_map(parse_color) {
                style.border_color = color;
            }
        }
        "border-radius" => {
            if let Some(radius) = value
                .split('/')
                .next()
                .and_then(|value| value.split_ascii_whitespace().next())
                .and_then(parse_length)
            {
                style.border_radius = radius;
            }
        }
        "visibility" => style.visibility = value != "hidden" && value != "collapse",
        "opacity" => {
            style.opacity = value
                .parse::<f32>()
                .unwrap_or(style.opacity)
                .clamp(0.0, 1.0)
        }
        "overflow" | "overflow-x" | "overflow-y" => {
            style.overflow_hidden = matches!(value, "hidden" | "clip")
        }
        "justify-content" | "-webkit-justify-content" | "-webkit-box-pack" => {
            style.justify_content_end = matches!(value, "end" | "flex-end" | "right");
            style.justify_content = match value {
                "end" | "flex-end" | "right" => JustifyContent::End,
                "center" => JustifyContent::Center,
                "space-between" | "justify" => JustifyContent::SpaceBetween,
                "space-around" => JustifyContent::SpaceAround,
                "space-evenly" => JustifyContent::SpaceEvenly,
                _ => JustifyContent::Start,
            };
        }
        "align-items" | "-webkit-align-items" | "-webkit-box-align" => {
            style.align_items_center = value == "center";
            style.align_items = match value {
                "center" => AlignItems::Center,
                "end" | "flex-end" => AlignItems::End,
                "start" | "flex-start" => AlignItems::Start,
                _ => AlignItems::Stretch,
            };
        }
        "flex-direction" | "-webkit-flex-direction" | "-moz-flex-direction" => {
            style.flex_direction = if value.starts_with("column") {
                FlexDirection::Column
            } else {
                FlexDirection::Row
            }
        }
        "flex-wrap" | "-webkit-flex-wrap" | "-moz-flex-wrap" => style.flex_wrap = value != "nowrap",
        "flex-grow" | "-webkit-flex-grow" | "-moz-flex-grow" | "-webkit-box-flex" => {
            style.flex_grow = value.parse::<f32>().unwrap_or(style.flex_grow).max(0.0)
        }
        "flex-shrink" | "-webkit-flex-shrink" | "-moz-flex-shrink" => {
            style.flex_shrink = value.parse::<f32>().unwrap_or(style.flex_shrink).max(0.0)
        }
        "flex-basis" | "-webkit-flex-basis" | "-moz-flex-basis" => {
            assign_length(&mut style.flex_basis, value)
        }
        "flex" | "-webkit-flex" | "-moz-flex" => assign_flex(style, value),
        "box-sizing" | "-webkit-box-sizing" => {
            style.box_sizing = if value == "border-box" {
                BoxSizing::BorderBox
            } else {
                BoxSizing::ContentBox
            }
        }
        "grid-template-columns" | "-ms-grid-columns" => {
            style.grid_template_columns = value.to_string()
        }
        "grid-template-rows" | "-ms-grid-rows" => style.grid_template_rows = value.to_string(),
        "column-gap" | "grid-column-gap" => assign_length(&mut style.grid_column_gap, value),
        "row-gap" | "grid-row-gap" => assign_length(&mut style.grid_row_gap, value),
        "gap" | "grid-gap" => assign_grid_gap(style, value),
        "grid-column-start" | "-ms-grid-column" => style.grid_column_start = parse_grid_line(value),
        "grid-column-end" => style.grid_column_end = parse_grid_line(value),
        "grid-row-start" | "-ms-grid-row" => style.grid_row_start = parse_grid_line(value),
        "grid-row-end" => style.grid_row_end = parse_grid_line(value),
        "grid-column" => assign_grid_axis(
            &mut style.grid_column_start,
            &mut style.grid_column_end,
            value,
        ),
        "grid-row" => assign_grid_axis(&mut style.grid_row_start, &mut style.grid_row_end, value),
        "grid-area" => assign_grid_area(style, value),
        _ => {}
    }
}

fn assign_grid_gap(style: &mut ComputedStyle, value: &str) {
    let values = value
        .split_ascii_whitespace()
        .filter_map(parse_length)
        .collect::<Vec<_>>();
    match values.as_slice() {
        [both] => {
            style.grid_row_gap = *both;
            style.grid_column_gap = *both;
        }
        [row, column, ..] => {
            style.grid_row_gap = *row;
            style.grid_column_gap = *column;
        }
        _ => {}
    }
}

fn assign_flex(style: &mut ComputedStyle, value: &str) {
    if value == "none" {
        style.flex_grow = 0.0;
        style.flex_shrink = 0.0;
        style.flex_basis = Length::Auto;
        return;
    }
    let mut numbers = value
        .split_ascii_whitespace()
        .filter_map(|part| part.parse::<f32>().ok());
    if let Some(grow) = numbers.next() {
        style.flex_grow = grow.max(0.0);
    }
    if let Some(shrink) = numbers.next() {
        style.flex_shrink = shrink.max(0.0);
    }
    if let Some(basis) = value.split_ascii_whitespace().find_map(parse_length) {
        style.flex_basis = basis;
    }
}

fn assign_grid_axis(start: &mut Option<usize>, end: &mut Option<usize>, value: &str) {
    let mut parts = value.split('/').map(str::trim);
    *start = parts.next().and_then(parse_grid_line);
    *end = parts.next().and_then(parse_grid_line);
}

fn assign_grid_area(style: &mut ComputedStyle, value: &str) {
    let parts = value.split('/').map(str::trim).collect::<Vec<_>>();
    if parts.len() == 4 {
        style.grid_row_start = parse_grid_line(parts[0]);
        style.grid_column_start = parse_grid_line(parts[1]);
        style.grid_row_end = parse_grid_line(parts[2]);
        style.grid_column_end = parse_grid_line(parts[3]);
    }
}

fn parse_grid_line(value: &str) -> Option<usize> {
    value
        .split_ascii_whitespace()
        .find_map(|part| part.parse::<usize>().ok())
        .filter(|line| *line > 0)
}

fn apply_font_shorthand(style: &mut ComputedStyle, value: &str) {
    let tokens = value.split_ascii_whitespace().collect::<Vec<_>>();
    let Some(size_index) = tokens.iter().position(|token| {
        token.contains("px")
            || token.contains("pt")
            || token.contains("em")
            || token.contains('%')
            || matches!(*token, "small" | "medium" | "large")
    }) else {
        return;
    };
    for token in &tokens[..size_index] {
        match *token {
            "bold" => style.font_weight = 700,
            "italic" | "oblique" => style.italic = true,
            numeric => {
                if let Ok(weight) = numeric.parse::<u16>() {
                    style.font_weight = weight;
                }
            }
        }
    }
    let size_and_line = tokens[size_index];
    let (size, line_height) = size_and_line
        .split_once('/')
        .map(|(size, line)| (size, Some(line)))
        .unwrap_or((size_and_line, None));
    if let Some(size) = parse_font_size(size, style.font_size) {
        style.font_size = size;
        style.line_height = line_height
            .and_then(|line| parse_line_height(line, size))
            .unwrap_or(size * 1.2);
    }
    if size_index + 1 < tokens.len() {
        style.font_family = first_font_family(&tokens[size_index + 1..].join(" "));
    }
}

fn parse_font_size(value: &str, inherited_size: f32) -> Option<f32> {
    match value.trim() {
        "xx-small" => Some(9.0),
        "x-small" => Some(10.0),
        "small" => Some(13.0),
        "medium" => Some(16.0),
        "large" => Some(18.0),
        "x-large" => Some(24.0),
        "xx-large" => Some(32.0),
        "smaller" => Some(inherited_size * 0.833),
        "larger" => Some(inherited_size * 1.2),
        value => {
            parse_length(value).and_then(|length| length.resolve(inherited_size, inherited_size))
        }
    }
}

fn parse_line_height(value: &str, font_size: f32) -> Option<f32> {
    if value == "normal" {
        return Some(font_size * 1.2);
    }
    if let Ok(multiplier) = value.parse::<f32>() {
        return Some(font_size * multiplier);
    }
    parse_length(value).and_then(|length| length.resolve(font_size, font_size))
}

fn first_font_family(value: &str) -> String {
    value
        .split(',')
        .next()
        .unwrap_or("Arial")
        .trim()
        .trim_matches(['\'', '"'])
        .to_string()
}

fn assign_length(target: &mut Length, value: &str) {
    if let Some(length) = parse_length(value.split_ascii_whitespace().next().unwrap_or(value)) {
        *target = length;
    }
}

fn assign_edges(target: &mut Edges, value: &str) {
    let lengths = value
        .split_ascii_whitespace()
        .filter_map(parse_length)
        .collect::<Vec<_>>();
    match lengths.as_slice() {
        [all] => *target = uniform_edges(*all),
        [vertical, horizontal] => {
            target.top = *vertical;
            target.bottom = *vertical;
            target.left = *horizontal;
            target.right = *horizontal;
        }
        [top, horizontal, bottom] => {
            target.top = *top;
            target.left = *horizontal;
            target.right = *horizontal;
            target.bottom = *bottom;
        }
        [top, right, bottom, left, ..] => {
            target.top = *top;
            target.right = *right;
            target.bottom = *bottom;
            target.left = *left;
        }
        _ => {}
    }
}

fn uniform_edges(value: Length) -> Edges {
    Edges {
        top: value,
        right: value,
        bottom: value,
        left: value,
    }
}

pub(crate) fn parse_length(value: &str) -> Option<Length> {
    let value = value.trim().trim_end_matches("!important").trim();
    if value == "auto" {
        return Some(Length::Auto);
    }
    if value == "0" {
        return Some(Length::Px(0.0));
    }
    for (suffix, constructor) in [
        ("px", Length::Px as fn(f32) -> Length),
        ("pt", |value| Length::Px(value * 96.0 / 72.0)),
        ("em", Length::Em),
        ("rem", |value| Length::Px(value * 16.0)),
        ("vw", Length::Vw),
        ("vh", Length::Vh),
        ("%", Length::Percent),
    ] {
        if let Some(number) = value.strip_suffix(suffix)
            && let Ok(number) = number.trim().parse::<f32>()
        {
            return Some(constructor(number));
        }
    }
    None
}

fn parse_color(value: &str) -> Option<Color> {
    let value = value.trim().trim_end_matches("!important").trim();
    if value.eq_ignore_ascii_case("transparent") {
        return Some(Color::TRANSPARENT);
    }
    if let Some(hex) = value.strip_prefix('#')
        && let Ok((red, green, blue, alpha)) = parse_hash_color(hex.as_bytes())
    {
        return Some(Color {
            red,
            green,
            blue,
            alpha: (alpha * 255.0).round() as u8,
        });
    }
    if let Ok((red, green, blue)) = parse_named_color(value) {
        return Some(Color::rgb(red, green, blue));
    }
    let lower = value.to_ascii_lowercase();
    let function = lower
        .strip_prefix("rgb(")
        .and_then(|value| value.strip_suffix(')'))
        .map(|value| (value, false))
        .or_else(|| {
            lower
                .strip_prefix("rgba(")
                .and_then(|value| value.strip_suffix(')'))
                .map(|value| (value, true))
        })?;
    let components = function
        .0
        .split([',', ' '])
        .filter(|part| !part.is_empty() && *part != "/")
        .collect::<Vec<_>>();
    if components.len() < 3 {
        return None;
    }
    let channel = |component: &str| -> Option<u8> {
        if let Some(percent) = component.strip_suffix('%') {
            Some(
                (percent.parse::<f32>().ok()? * 2.55)
                    .round()
                    .clamp(0.0, 255.0) as u8,
            )
        } else {
            Some(component.parse::<f32>().ok()?.round().clamp(0.0, 255.0) as u8)
        }
    };
    let alpha = if function.1 && components.len() >= 4 {
        if let Some(percent) = components[3].strip_suffix('%') {
            (percent.parse::<f32>().ok()? * 2.55)
                .round()
                .clamp(0.0, 255.0) as u8
        } else {
            (components[3].parse::<f32>().ok()? * 255.0)
                .round()
                .clamp(0.0, 255.0) as u8
        }
    } else {
        255
    };
    Some(Color {
        red: channel(components[0])?,
        green: channel(components[1])?,
        blue: channel(components[2])?,
        alpha,
    })
}

fn consume_identifier(bytes: &[u8], start: usize) -> usize {
    let mut cursor = start;
    while cursor < bytes.len()
        && (bytes[cursor].is_ascii_alphanumeric()
            || matches!(bytes[cursor], b'-' | b'_')
            || bytes[cursor] >= 0x80)
    {
        cursor += 1;
    }
    cursor
}

fn media_matches(prelude: &str, viewport_width: f32) -> bool {
    let queries = prelude
        .trim()
        .strip_prefix("@media")
        .unwrap_or(prelude)
        .trim();
    split_css_top_level(queries, ',').any(|query| media_query_matches(query, viewport_width))
}

fn media_query_matches(query: &str, viewport_width: f32) -> bool {
    let mut query = query.trim().to_ascii_lowercase();
    let negated = query.starts_with("not ");
    if negated {
        query = query["not ".len()..].trim().to_string();
    }
    if let Some(rest) = query.strip_prefix("only ") {
        query = rest.trim().to_string();
    }

    let media_type_matches = if query.starts_with("print") || query.starts_with("speech") {
        false
    } else {
        query.starts_with("screen")
            || query.starts_with("all")
            || query.starts_with('(')
            || query.starts_with("and ")
    };

    let mut conditions_match = true;
    let mut cursor = 0;
    let mut found_condition = false;
    while let Some(relative_open) = query[cursor..].find('(') {
        let open = cursor + relative_open;
        let Some(close) = find_matching_parenthesis(&query, open) else {
            conditions_match = false;
            break;
        };
        found_condition = true;
        let condition = query[open + 1..close].trim();
        if !media_condition_matches(condition, viewport_width) {
            conditions_match = false;
            break;
        }
        cursor = close + 1;
    }

    let mut matches =
        media_type_matches && (!query.contains('(') || found_condition) && conditions_match;
    if negated {
        matches = !matches;
    }
    matches
}

fn media_condition_matches(condition: &str, viewport_width: f32) -> bool {
    let Some((feature, value)) = condition.split_once(':') else {
        return false;
    };
    let feature = feature.trim();
    let value = value.trim();
    match feature {
        "min-width" => parse_length(value)
            .and_then(|length| length.resolve(viewport_width, 16.0))
            .is_some_and(|minimum| viewport_width >= minimum),
        "max-width" => parse_length(value)
            .and_then(|length| length.resolve(viewport_width, 16.0))
            .is_some_and(|maximum| viewport_width <= maximum),
        "width" => parse_length(value)
            .and_then(|length| length.resolve(viewport_width, 16.0))
            .is_some_and(|expected| (viewport_width - expected).abs() < 0.5),
        "hover" | "any-hover" => value == "hover",
        "pointer" | "any-pointer" => value == "fine",
        // Unknown media features are false per CSS media-query evaluation. In particular,
        // vendor-only fallbacks must never leak into the normal standards style set.
        _ => false,
    }
}

fn skip_css_whitespace(input: &str, mut cursor: usize) -> usize {
    while cursor < input.len() && input.as_bytes()[cursor].is_ascii_whitespace() {
        cursor += 1;
    }
    cursor
}

fn find_css_delimiter(input: &str, start: usize, wanted: char) -> Option<usize> {
    let mut quote = None;
    let mut parentheses = 0_i32;
    for (offset, character) in input[start..].char_indices() {
        match (quote, character) {
            (Some(active), candidate) if candidate == active => quote = None,
            (Some(_), _) => {}
            (None, '\'' | '"') => quote = Some(character),
            (None, '(') => parentheses += 1,
            (None, ')') => parentheses = (parentheses - 1).max(0),
            (None, candidate) if candidate == wanted && parentheses == 0 => {
                return Some(start + offset);
            }
            _ => {}
        }
    }
    None
}

fn find_matching_brace(input: &str, open: usize) -> Option<usize> {
    let mut depth = 0_i32;
    let mut quote = None;
    for (offset, character) in input[open..].char_indices() {
        match (quote, character) {
            (Some(active), candidate) if candidate == active => quote = None,
            (Some(_), _) => {}
            (None, '\'' | '"') => quote = Some(character),
            (None, '{') => depth += 1,
            (None, '}') => {
                depth -= 1;
                if depth == 0 {
                    return Some(open + offset);
                }
            }
            _ => {}
        }
    }
    None
}

fn find_matching_parenthesis(input: &str, open: usize) -> Option<usize> {
    let mut depth = 0_i32;
    for (offset, character) in input[open..].char_indices() {
        match character {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    return Some(open + offset);
                }
            }
            _ => {}
        }
    }
    None
}

fn split_css_top_level(input: &str, delimiter: char) -> impl Iterator<Item = &str> {
    let mut parts = Vec::new();
    let mut start = 0;
    let mut parentheses = 0_i32;
    let mut brackets = 0_i32;
    let mut quote = None;
    for (index, character) in input.char_indices() {
        match (quote, character) {
            (Some(active), candidate) if candidate == active => quote = None,
            (Some(_), _) => {}
            (None, '\'' | '"') => quote = Some(character),
            (None, '(') => parentheses += 1,
            (None, ')') => parentheses = (parentheses - 1).max(0),
            (None, '[') => brackets += 1,
            (None, ']') => brackets = (brackets - 1).max(0),
            (None, candidate) if candidate == delimiter && parentheses == 0 && brackets == 0 => {
                parts.push(&input[start..index]);
                start = index + character.len_utf8();
            }
            _ => {}
        }
    }
    parts.push(&input[start..]);
    parts.into_iter()
}

fn split_css_once(input: &str, delimiter: char) -> Option<(&str, &str)> {
    let mut parentheses = 0_i32;
    let mut quote = None;
    for (index, character) in input.char_indices() {
        match (quote, character) {
            (Some(active), candidate) if candidate == active => quote = None,
            (Some(_), _) => {}
            (None, '\'' | '"') => quote = Some(character),
            (None, '(') => parentheses += 1,
            (None, ')') => parentheses = (parentheses - 1).max(0),
            (None, candidate) if candidate == delimiter && parentheses == 0 => {
                return Some((&input[..index], &input[index + character.len_utf8()..]));
            }
            _ => {}
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::dom;

    #[test]
    fn cascades_specificity_and_inline_styles() {
        let dom = dom::parse(
            r#"<style>p { color:red } .note {color:blue} #main {font-size:20px}</style>
               <p id="main" class="note" style="color:#123456">hello</p>"#,
        );
        let styles = StyleSet::from_dom(&dom, &[], 1000.0);
        let paragraph = dom.elements_named("p").next().unwrap();
        let style = styles.get(&paragraph);
        assert_eq!(style.color, Color::rgb(0x12, 0x34, 0x56));
        assert_eq!(style.font_size, 20.0);
    }

    #[test]
    fn matches_descendants_children_compounds_and_not() {
        let dom = dom::parse(
            r#"<style>
                #app > .row a.link { color: rgb(1,2,3); }
                .row:not(.hidden) { background-color: #abcdef; }
               </style><div id="app"><div class="row"><a class="link">x</a></div></div>"#,
        );
        let styles = StyleSet::from_dom(&dom, &[], 1000.0);
        let link = dom.elements_named("a").next().unwrap();
        let row = dom
            .elements_named("div")
            .find(|node| node.has_class("row"))
            .unwrap();
        assert_eq!(styles.get(&link).color, Color::rgb(1, 2, 3));
        assert_eq!(
            styles.get(&row).background_color,
            Color::rgb(0xab, 0xcd, 0xef)
        );
    }

    #[test]
    fn applies_media_width_queries() {
        let dom = dom::parse(
            r#"<style>@media (max-width: 600px) { body { color: green } }</style><p>x</p>"#,
        );
        let narrow = StyleSet::from_dom(&dom, &[], 500.0);
        let wide = StyleSet::from_dom(&dom, &[], 900.0);
        let body = dom.elements_named("body").next().unwrap();
        assert_eq!(narrow.get(&body).color, Color::rgb(0, 128, 0));
        assert_eq!(wide.get(&body).color, Color::BLACK);
    }

    #[test]
    fn matches_attribute_selectors_instead_of_treating_them_as_wildcards() {
        let dom = dom::parse(
            r#"<style>
                .item[data-display="block"] { display: block; color: green; }
                .item[data-display="none"] { display: none; color: red; }
                [data-tags~="featured"] { background-color: #123456; }
               </style>
               <div class="item" data-display="block" data-tags="home featured">visible</div>
               <div class="item" data-display="none">hidden</div>"#,
        );
        let styles = StyleSet::from_dom(&dom, &[], 1000.0);
        let mut items = dom
            .elements_named("div")
            .filter(|node| node.has_class("item"));
        let visible = items.next().unwrap();
        let hidden = items.next().unwrap();
        assert_eq!(styles.get(&visible).display, Display::Block);
        assert_eq!(styles.get(&visible).color, Color::rgb(0, 128, 0));
        assert_eq!(
            styles.get(&visible).background_color,
            Color::rgb(0x12, 0x34, 0x56)
        );
        assert_eq!(styles.get(&hidden).display, Display::None);
        assert_eq!(styles.get(&hidden).color, Color::rgb(255, 0, 0));
    }

    #[test]
    fn rejects_vendor_media_queries_for_other_engines() {
        let dom = dom::parse(
            r#"<style>
                body { color: green; }
                @media screen and (-ms-high-contrast: active),
                       screen and (-ms-high-contrast: none) {
                    body { color: red; }
                }
               </style><p>x</p>"#,
        );
        let styles = StyleSet::from_dom(&dom, &[], 1000.0);
        let body = dom.elements_named("body").next().unwrap();
        assert_eq!(styles.get(&body).color, Color::rgb(0, 128, 0));
    }

    #[test]
    fn honors_explicit_inheritance_after_user_agent_defaults() {
        let dom = dom::parse(
            r#"<style>
                html { box-sizing: border-box; }
                * { box-sizing: inherit; }
                .field { width: 80px; max-width: 90px; background-color: #212121; }
                input { width: inherit; max-width: inherit; background-color: inherit; }
               </style><div class="field"><input></div>"#,
        );
        let styles = StyleSet::from_dom(&dom, &[], 1000.0);
        let input = dom.elements_named("input").next().unwrap();
        let style = styles.get(&input);
        assert_eq!(style.box_sizing, BoxSizing::BorderBox);
        assert_eq!(style.width, Length::Px(80.0));
        assert_eq!(style.max_width, Length::Px(90.0));
        assert_eq!(style.background_color, Color::rgb(0x21, 0x21, 0x21));
    }

    #[test]
    fn parses_css_lengths_and_colors() {
        assert_eq!(parse_length("50%"), Some(Length::Percent(50.0)));
        assert_eq!(parse_length("1.5em"), Some(Length::Em(1.5)));
        assert_eq!(
            parse_color("rgba(10, 20, 30, .5)"),
            Some(Color {
                red: 10,
                green: 20,
                blue: 30,
                alpha: 128,
            })
        );
    }
}
