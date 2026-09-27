//! CSS Nesting Level 1: ordered declarations and nested style/group rules.
use super::style_selectors::{StyleSelector, parse_style_selectors};
use super::*;

pub(super) struct Context<'a> {
    pub(super) base_url: &'a str,
    pub(super) environment: MediaEnvironment,
    pub(super) next_order: &'a mut u32,
    pub(super) output: &'a mut Vec<Rule>,
    pub(super) scope: RuleScope,
    pub(super) css_scopes: &'a [CssScope],
    pub(super) relative_scope_selectors: bool,
    pub(super) implicit_scope_root: Option<NodeId>,
    pub(super) rule_limit: usize,
    pub(super) next_anonymous: &'a mut u64,
    pub(super) events: &'a mut Vec<LayerEvent>,
}

#[derive(Clone, Copy)]
pub(super) enum BlockItem<'a> {
    Declarations(&'a str),
    Rule {
        prelude: &'a str,
        body: &'a str,
        source: &'a str,
    },
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
    let Some(selectors) = parse_style_selectors(
        prelude,
        parent,
        context.scope,
        context.relative_scope_selectors,
    ) else {
        return;
    };
    parse_style_contents(body, depth, layer, &selectors, true, context);
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
            BlockItem::Rule { prelude, body, .. } => {
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
                } else if let Some(boundaries) = at_rule_prelude(prelude, "scope") {
                    let parents = selectors
                        .iter()
                        .filter(|selector| selector.pseudo.is_none())
                        .map(|selector| selector.selector.clone())
                        .collect::<Vec<_>>();
                    if let Some(boundary) = scope::parse_scope_prelude_in_style(
                        boundaries,
                        &parents,
                        context.implicit_scope_root,
                    ) {
                        let mut nested = context.css_scopes.to_vec();
                        nested.push(boundary);
                        scope::parse_scope_contents(
                            body,
                            depth + 1,
                            layer,
                            &mut Context {
                                base_url: context.base_url,
                                environment: context.environment,
                                next_order: &mut *context.next_order,
                                output: &mut *context.output,
                                scope: context.scope,
                                css_scopes: &nested,
                                relative_scope_selectors: true,
                                implicit_scope_root: context.implicit_scope_root,
                                rule_limit: context.rule_limit,
                                next_anonymous: &mut *context.next_anonymous,
                                events: &mut *context.events,
                            },
                        );
                    }
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
                host_context: parsed.host_context,
                slotted_origin: parsed.slotted_origin.clone(),
                slotted_host_child: parsed.slotted_host_child,
                part: parsed.part.clone(),
                declarations: declarations.clone(),
                base_url: context.base_url.to_string(),
                scope: parsed.scope,
                css_scopes: context.css_scopes.to_vec(),
            }),
        });
        context
            .events
            .push(LayerEvent::Rule(context.output.len() - 1));
        *context.next_order = context.next_order.wrapping_add(1);
    }
}

/// CSS Cascade 6 wraps each contiguous @scope declaration run in a zero-specificity rule.
pub(super) fn emit_scoped_declarations(
    text: &str,
    layer: &[LayerSegment],
    context: &mut Context<'_>,
) {
    let declarations = parse_declarations(text);
    if declarations.is_empty() {
        return;
    }
    let selector = parse_selector(":where(:scope)").expect("fixed scoped declaration selector");
    emit_rules(
        &[StyleSelector {
            selector,
            pseudo: None,
            host_condition: None,
            host_context: false,
            slotted_origin: None,
            slotted_host_child: false,
            part: None,
            scope: context.scope,
        }],
        declarations,
        layer,
        context,
    );
}

pub(super) fn block_items(input: &str) -> Vec<BlockItem<'_>> {
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
                    source: &input[item_start..close + 1],
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
