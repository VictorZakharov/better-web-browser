//! CSS Nesting Level 1: ordered declarations and nested style/group rules.
use super::*;
use crate::engine::css::selector_parser::parse_style_rule_selector_with_parent;

#[derive(Clone)]
pub(super) struct StyleSelector {
    selector: Selector,
    pseudo: Option<PseudoElement>,
    host_condition: Option<Selector>,
    scope: RuleScope,
}

pub(super) struct Context<'a> {
    pub(super) base_url: &'a str,
    pub(super) environment: MediaEnvironment,
    pub(super) next_order: &'a mut u32,
    pub(super) output: &'a mut Vec<Rule>,
    pub(super) scope: RuleScope,
    pub(super) rule_limit: usize,
    pub(super) next_anonymous: &'a mut u64,
    pub(super) events: &'a mut Vec<LayerEvent>,
}

#[derive(Clone, Copy)]
enum BlockItem<'a> {
    Declarations(&'a str),
    Rule { prelude: &'a str, body: &'a str },
    Statement(&'a str),
}

#[allow(clippy::too_many_arguments)]
pub(super) fn parse_style_rule(
    prelude: &str,
    body: &str,
    depth: usize,
    layer: &[LayerSegment],
    parent: Option<&[StyleSelector]>,
    context: &mut Context<'_>,
) {
    if depth >= MAX_CSS_NESTING_DEPTH || context.output.len() >= context.rule_limit {
        return;
    }
    let Some(selectors) = parse_style_selectors(prelude, parent, context.scope) else {
        return;
    };
    parse_style_contents(body, depth, layer, &selectors, true, context);
}

fn parse_style_selectors(
    prelude: &str,
    parent: Option<&[StyleSelector]>,
    scope: RuleScope,
) -> Option<Vec<StyleSelector>> {
    let parent_selectors = parent.map(|parents| {
        parents
            .iter()
            .filter(|parent| parent.pseudo.is_none())
            .map(|parent| parent.selector.clone())
            .collect::<Vec<_>>()
    });
    let parent_specificity = parent.and_then(|parents| {
        parents
            .iter()
            .map(|parent| parent.selector.specificity)
            .max()
    });
    let mut selectors = Vec::new();
    for member in split_css_top_level(prelude, ',') {
        let member = member.trim();
        if member.is_empty() {
            return None;
        }
        if let Some(parents) = parent {
            // Nested selectors in host/slotted rules require a composed-tree selector model.
            // Keep their declarations, but do not leak unsupported nested rules across roots.
            if parents
                .iter()
                .any(|parent| matches!(parent.scope, RuleScope::Host(_) | RuleScope::Slotted(_)))
            {
                return None;
            }
            let (selector, pseudo) = parse_style_rule_selector_with_parent(
                member,
                parent_selectors.as_deref(),
                parent_specificity,
            )?;
            selectors.push(StyleSelector {
                selector,
                pseudo,
                host_condition: None,
                scope,
            });
        } else {
            let (source, rule_scope, host_condition) = scoped_selector(member, scope)?;
            let host_condition = host_condition.map(parse_selector).transpose_option()?;
            let (mut selector, pseudo) = parse_style_rule_selector(source)?;
            if let Some(condition) = host_condition.as_ref() {
                selector.specificity.ids = selector
                    .specificity
                    .ids
                    .saturating_add(condition.specificity.ids);
                selector.specificity.classes = selector
                    .specificity
                    .classes
                    .saturating_add(condition.specificity.classes);
                selector.specificity.tags = selector
                    .specificity
                    .tags
                    .saturating_add(condition.specificity.tags);
            }
            selectors.push(StyleSelector {
                selector,
                pseudo,
                host_condition,
                scope: rule_scope,
            });
        }
    }
    (!selectors.is_empty()).then_some(selectors)
}

trait TransposeOption<T> {
    fn transpose_option(self) -> Option<Option<T>>;
}

impl<T> TransposeOption<T> for Option<Option<T>> {
    fn transpose_option(self) -> Option<Option<T>> {
        match self {
            Some(Some(value)) => Some(Some(value)),
            Some(None) => None,
            None => Some(None),
        }
    }
}

fn parse_style_contents(
    body: &str,
    depth: usize,
    layer: &[LayerSegment],
    selectors: &[StyleSelector],
    emit_empty_initial: bool,
    context: &mut Context<'_>,
) {
    if depth >= MAX_CSS_NESTING_DEPTH {
        return;
    }
    // Most author rules contain only declarations. Keep that path at one parse pass.
    if !body.as_bytes().contains(&b'{') && !body.as_bytes().contains(&b'@') {
        let declarations = parse_declarations(body);
        if !declarations.is_empty() || emit_empty_initial {
            emit_rules(selectors, declarations, layer, context);
        }
        return;
    }
    let mut first = emit_empty_initial;
    for item in block_items(body) {
        if context.output.len() >= context.rule_limit {
            break;
        }
        match item {
            BlockItem::Declarations(text) => {
                let declarations = parse_declarations(text);
                if !declarations.is_empty() || first {
                    emit_rules(selectors, declarations, layer, context);
                }
                first = false;
            }
            BlockItem::Statement(text) => {
                if first {
                    emit_rules(selectors, Vec::new(), layer, context);
                    first = false;
                }
                if let Some(paths) = parse_layer_statement(text) {
                    for path in paths {
                        let mut complete = layer.to_vec();
                        complete.extend(path);
                        context.events.push(LayerEvent::Declare(complete));
                    }
                }
            }
            BlockItem::Rule { prelude, body } => {
                if first {
                    emit_rules(selectors, Vec::new(), layer, context);
                    first = false;
                }
                if at_rule_prelude(prelude, "media").is_some() {
                    if media::media_matches_for_environment(prelude, context.environment) {
                        parse_style_contents(body, depth + 1, layer, selectors, false, context);
                    }
                } else if at_rule_prelude(prelude, "supports").is_some() {
                    if supports::supports_matches(prelude) {
                        parse_style_contents(body, depth + 1, layer, selectors, false, context);
                    }
                } else if let Some(name) = layer_prelude(prelude) {
                    let mut path = layer.to_vec();
                    if name.is_empty() {
                        path.push(LayerSegment::Anonymous(*context.next_anonymous));
                        *context.next_anonymous = context.next_anonymous.saturating_add(1);
                    } else if let Some(parsed) = parse_layer_name(name) {
                        path.extend(parsed);
                    } else {
                        continue;
                    }
                    context.events.push(LayerEvent::Declare(path.clone()));
                    parse_style_contents(body, depth + 1, &path, selectors, false, context);
                } else if !prelude.starts_with('@') {
                    parse_style_rule(prelude, body, depth + 1, layer, Some(selectors), context);
                }
            }
        }
    }
    if first && context.output.len() < context.rule_limit {
        emit_rules(selectors, Vec::new(), layer, context);
    }
}

fn emit_rules(
    selectors: &[StyleSelector],
    declarations: Vec<Declaration>,
    layer: &[LayerSegment],
    context: &mut Context<'_>,
) {
    for parsed in selectors {
        if context.output.len() >= context.rule_limit {
            break;
        }
        context.output.push(Rule {
            order: *context.next_order,
            layer: (!layer.is_empty()).then(|| layer.to_vec()),
            layer_rank: u32::MAX,
            data: Rc::new(RuleData {
                selector: parsed.selector.clone(),
                pseudo: parsed.pseudo,
                host_condition: parsed.host_condition.clone(),
                declarations: declarations.clone(),
                base_url: context.base_url.to_string(),
                scope: parsed.scope,
            }),
        });
        context
            .events
            .push(LayerEvent::Rule(context.output.len() - 1));
        *context.next_order = context.next_order.wrapping_add(1);
    }
}

fn block_items(input: &str) -> Vec<BlockItem<'_>> {
    let mut items = Vec::new();
    let mut cursor = 0;
    let mut declarations_start = 0;
    let mut item_start = 0;
    let mut quote = None;
    let mut escaped = false;
    let mut parentheses = 0_u32;
    let mut brackets = 0_u32;
    while cursor < input.len() {
        let character = input[cursor..].chars().next().unwrap();
        let width = character.len_utf8();
        if let Some(active) = quote {
            if escaped {
                escaped = false;
            } else if character == '\\' {
                escaped = true;
            } else if character == active {
                quote = None;
            }
            cursor += width;
            continue;
        }
        match character {
            '\'' | '"' => quote = Some(character),
            '(' => parentheses += 1,
            ')' => parentheses = parentheses.saturating_sub(1),
            '[' => brackets += 1,
            ']' => brackets = brackets.saturating_sub(1),
            ';' if parentheses == 0 && brackets == 0 => {
                let statement = input[item_start..cursor].trim();
                if statement.starts_with('@') {
                    if declarations_start < item_start {
                        items.push(BlockItem::Declarations(
                            &input[declarations_start..item_start],
                        ));
                    }
                    items.push(BlockItem::Statement(statement));
                    declarations_start = cursor + 1;
                }
                item_start = cursor + 1;
            }
            '{' if parentheses == 0 && brackets == 0 => {
                let Some(close) = find_matching_brace(input, cursor) else {
                    break;
                };
                let prelude = input[item_start..cursor].trim();
                // A custom property may contain balanced braces as part of its value.
                if prelude.starts_with("--") && split_css_once(prelude, ':').is_some() {
                    cursor = close + 1;
                    continue;
                }
                if declarations_start < item_start {
                    items.push(BlockItem::Declarations(
                        &input[declarations_start..item_start],
                    ));
                }
                items.push(BlockItem::Rule {
                    prelude,
                    body: &input[cursor + 1..close],
                });
                cursor = close + 1;
                item_start = cursor;
                declarations_start = cursor;
                continue;
            }
            _ => {}
        }
        cursor += width;
    }
    if declarations_start < input.len() {
        items.push(BlockItem::Declarations(&input[declarations_start..]));
    }
    items
}
