//! Reuse immutable per-sheet parsing while rebuilding source order and the page rule budget.
use super::*;
use crate::engine::css::layers::{LayerEvent, LayerRegistry, occurrence_path};
use crate::engine::css::stylesheet::parse_stylesheet_with_layer_events;
#[cfg(test)]
use crate::engine::css::stylesheet::parse_stylesheet_with_rule_budget;
use crate::limits::{MAX_CSS_RULES_PER_STYLESHEET, MAX_PAGE_CSS_RULES};

#[derive(Debug)]
pub(super) struct ParsedSheet {
    input: Rc<SheetInput>,
    rules: Vec<Rule>,
    events: Vec<LayerEvent>,
    // Reaching this budget may have truncated the source; expanding it requires reparsing.
    budget: usize,
}

pub(super) fn assemble(
    inputs: &mut [Rc<SheetInput>],
    environment: MediaEnvironment,
    previous: Option<&CompiledRules>,
) -> (Vec<Rule>, Vec<Rc<ParsedSheet>>) {
    assemble_with_limit(inputs, environment, previous, MAX_PAGE_CSS_RULES)
}

fn assemble_with_limit(
    inputs: &mut [Rc<SheetInput>],
    environment: MediaEnvironment,
    previous: Option<&CompiledRules>,
    page_limit: usize,
) -> (Vec<Rule>, Vec<Rc<ParsedSheet>>) {
    let page_limit = page_limit.min(MAX_PAGE_CSS_RULES);
    // This lookup borrows the lifetime of live compiled cascades, not a global strong cache.
    // Hash collisions still compare the complete source/base/scope key for equality.
    let mut available = HashMap::<Rc<SheetInput>, Rc<ParsedSheet>>::new();
    if let Some(previous) = previous.filter(|set| set.environment == Some(environment)) {
        for sheet in &previous.parsed {
            let entry = available
                .entry(Rc::clone(&sheet.input))
                .or_insert_with(|| Rc::clone(sheet));
            if entry.budget < sheet.budget {
                *entry = Rc::clone(sheet);
            }
        }
    }
    let mut rules = Vec::new();
    let mut parsed = Vec::new();
    let mut registries = HashMap::<RuleScope, LayerRegistry>::new();
    let mut assignments = Vec::new();
    for (occurrence, input) in inputs.iter_mut().enumerate() {
        if let Some(path) = &input.declared_layer {
            registries.entry(input.scope).or_default().declare(path);
            continue;
        }
        let budget = page_limit
            .saturating_sub(rules.len())
            .min(MAX_CSS_RULES_PER_STYLESHEET);
        if budget == 0 {
            break;
        }
        let cached = available
            .get(input)
            .filter(|sheet| sheet.budget >= budget || sheet.rules.len() < sheet.budget);
        let sheet =
            match cached {
                Some(sheet) if sheet.rules.len() <= budget => Rc::clone(sheet),
                Some(sheet) => {
                    let last = sheet.events.iter().position(|event| {
                    matches!(event, LayerEvent::Rule(index) if *index == budget - 1)
                }).expect("parsed rule must have a layer event");
                    Rc::new(ParsedSheet {
                        input: Rc::clone(input),
                        rules: sheet.rules[..budget].to_vec(),
                        events: sheet.events[..=last].to_vec(),
                        budget,
                    })
                }
                None => {
                    let mut rules = Vec::new();
                    let mut events = Vec::new();
                    parse_stylesheet_with_layer_events(
                        &input.source,
                        &input.base_url,
                        environment,
                        &mut 0,
                        &mut rules,
                        input.scope,
                        budget,
                        input.implicit_scope_root,
                        &input.import_scope_prefixes,
                        &mut events,
                    );
                    let sheet = Rc::new(ParsedSheet {
                        input: Rc::clone(input),
                        rules,
                        events,
                        budget,
                    });
                    available.insert(Rc::clone(input), Rc::clone(&sheet));
                    sheet
                }
            };
        // Canonicalize equal source ownership as well as the parsed rules; later cascades do
        // not retain another full CSS-text copy solely for their whole-set equality check.
        *input = Rc::clone(&sheet.input);
        // Order belongs to an occurrence, not the shared parsed payload: insertion, removal,
        // reordering and identical repeated sheets must preserve normal cascade tie breaking.
        let registry = registries.entry(input.scope).or_default();
        for event in &sheet.events {
            match event {
                LayerEvent::Declare(path) => {
                    let path = prefixed_path(&input.layer_prefix, path, occurrence as u32);
                    registry.declare(&path);
                }
                LayerEvent::Rule(index) => {
                    let mut rule = sheet.rules[*index].clone();
                    let path = prefixed_path(
                        &input.layer_prefix,
                        rule.layer.as_deref().unwrap_or(&[]),
                        occurrence as u32,
                    );
                    let layer = (!path.is_empty())
                        .then(|| registry.declare(&path))
                        .flatten();
                    rule.layer = (!path.is_empty()).then_some(path);
                    rule.order = rules.len() as u32;
                    assignments.push(layer.map(|id| (input.scope, id)));
                    rules.push(rule);
                }
            }
        }
        parsed.push(sheet);
    }
    let ranks = registries
        .into_iter()
        .map(|(scope, registry)| (scope, registry.ranks()))
        .collect::<HashMap<_, _>>();
    for (rule, assignment) in rules.iter_mut().zip(assignments) {
        if let Some((scope, layer)) = assignment {
            rule.layer_rank = ranks[&scope][layer];
        }
    }
    (rules, parsed)
}

fn prefixed_path(
    prefix: &[crate::engine::css::layers::LayerSegment],
    local: &[crate::engine::css::layers::LayerSegment],
    occurrence: u32,
) -> crate::engine::css::layers::LayerPath {
    let mut path = prefix.to_vec();
    path.extend(occurrence_path(local, occurrence));
    path
}

#[cfg(test)]
mod tests;
