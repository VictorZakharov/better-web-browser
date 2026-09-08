//! Reuse immutable per-sheet parsing while rebuilding source order and the page rule budget.
use super::*;
use crate::engine::css::stylesheet::parse_stylesheet_with_rule_budget;
use crate::limits::{MAX_CSS_RULES_PER_STYLESHEET, MAX_PAGE_CSS_RULES};

#[derive(Debug)]
pub(super) struct ParsedSheet {
    input: Rc<SheetInput>,
    rules: Vec<Rule>,
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
    for input in inputs {
        let budget = page_limit
            .saturating_sub(rules.len())
            .min(MAX_CSS_RULES_PER_STYLESHEET);
        if budget == 0 {
            break;
        }
        let cached = available
            .get(input)
            .filter(|sheet| sheet.budget >= budget || sheet.rules.len() < sheet.budget);
        let sheet = match cached {
            Some(sheet) if sheet.rules.len() <= budget => Rc::clone(sheet),
            Some(sheet) => Rc::new(ParsedSheet {
                input: Rc::clone(input),
                rules: sheet.rules[..budget].to_vec(),
                budget,
            }),
            None => {
                let mut rules = Vec::new();
                parse_stylesheet_with_rule_budget(
                    &input.source,
                    &input.base_url,
                    environment,
                    &mut 0,
                    &mut rules,
                    input.scope,
                    budget,
                );
                let sheet = Rc::new(ParsedSheet {
                    input: Rc::clone(input),
                    rules,
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
        for mut rule in sheet.rules.iter().cloned() {
            rule.order = rules.len() as u32;
            rules.push(rule);
        }
        parsed.push(sheet);
    }
    (rules, parsed)
}

#[cfg(test)]
mod tests;
