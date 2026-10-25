//! Positive rightmost-compound keys for conservative selector candidate selection.
use super::*;

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
enum Key<'a> {
    Id(&'a str),
    Class(&'a str),
    Tag(&'a str),
    Attribute(&'a str),
}

fn keys(target: &CompoundSelector) -> Vec<Key<'_>> {
    let mut keys = Vec::new();
    keys.extend(target.id.as_deref().map(Key::Id));
    keys.extend(target.classes.iter().map(|value| Key::Class(value)));
    keys.extend(target.tag.as_deref().map(Key::Tag));
    keys.extend(
        target
            .attributes
            .iter()
            .map(|value| Key::Attribute(&value.name)),
    );
    if target.requires_link {
        keys.push(Key::Tag("a"));
    }
    // Alternatives and negations cannot supply a necessary positive key.
    keys
}

#[derive(Debug, Default)]
pub(super) struct RuleIndex {
    targets: [CandidateIndex; 4],
}

#[derive(Debug, Default)]
struct CandidateIndex {
    universal: Vec<usize>,
    by_id: HashMap<String, Vec<usize>>,
    by_class: HashMap<String, Vec<usize>>,
    by_tag: HashMap<String, Vec<usize>>,
    by_attribute: HashMap<String, Vec<usize>>,
}

impl RuleIndex {
    pub(super) fn new(rules: &[Rule]) -> Self {
        Self::from_rules(rules.iter().enumerate())
    }

    /// Keep original rule identities while indexing a responsibility-specific subset.
    pub(super) fn for_indices(rules: &[Rule], indices: &[usize]) -> Self {
        Self::from_rules(
            indices
                .iter()
                .filter_map(|&index| rules.get(index).map(|rule| (index, rule))),
        )
    }

    fn from_rules<'a>(rules: impl Iterator<Item = (usize, &'a Rule)> + Clone) -> Self {
        let mut frequency = HashMap::<Key<'_>, usize>::new();
        for target in rules
            .clone()
            .filter_map(|(_, rule)| rule.selector.compounds.last())
        {
            for key in keys(target) {
                *frequency.entry(key).or_default() += 1;
            }
        }
        let mut index = Self::default();
        for (rule_index, rule) in rules {
            let bucket = &mut index.targets[target_index(rule.pseudo)];
            let Some(target) = rule.selector.compounds.last() else {
                continue;
            };
            // Store each rule exactly once, under its least frequent necessary key. The full
            // selector matcher still decides applicability, including scope and attribute values.
            let selected = keys(target).into_iter().min_by_key(|key| frequency[key]);
            let (map, value) = match selected {
                Some(Key::Id(value)) => (&mut bucket.by_id, value),
                Some(Key::Class(value)) => (&mut bucket.by_class, value),
                Some(Key::Tag(value)) => (&mut bucket.by_tag, value),
                Some(Key::Attribute(value)) => (&mut bucket.by_attribute, value),
                None => {
                    bucket.universal.push(rule_index);
                    continue;
                }
            };
            map.entry(value.to_string()).or_default().push(rule_index);
        }
        index
    }

    pub(super) fn candidates(&self, node: &NodeRef, pseudo: Option<PseudoElement>) -> Vec<usize> {
        self.targets[target_index(pseudo)].candidates(node)
    }

    /// Whether any candidate rule for `node` satisfies `test`, without collecting a set.
    pub(super) fn any_candidate(
        &self,
        node: &NodeRef,
        pseudo: Option<PseudoElement>,
        mut test: impl FnMut(usize) -> bool,
    ) -> bool {
        self.targets[target_index(pseudo)].visit(node, |rules| rules.iter().any(|&rule| test(rule)))
    }
}

fn target_index(pseudo: Option<PseudoElement>) -> usize {
    match pseudo {
        None => 0,
        Some(PseudoElement::Before) => 1,
        Some(PseudoElement::After) => 2,
        Some(PseudoElement::Placeholder) => 3,
    }
}

impl CandidateIndex {
    fn candidates(&self, node: &NodeRef) -> Vec<usize> {
        let mut candidates = Vec::new();
        self.visit(node, |rules| {
            candidates.extend(rules);
            false
        });
        candidates.sort_unstable();
        candidates.dedup();
        candidates
    }

    /// Calls `visit` with each bucket that can hold rules for `node`, until it returns true.
    /// Buckets may repeat a rule; callers that need a set sort and deduplicate.
    fn visit(&self, node: &NodeRef, mut visit: impl FnMut(&[usize]) -> bool) -> bool {
        let Some(element) = node.element() else {
            return false;
        };
        if visit(&self.universal) {
            return true;
        }
        if let Some(rules) = self.by_tag.get(element.name.local.as_ref())
            && visit(rules)
        {
            return true;
        }
        let attributes = element.attrs.borrow();
        if !self.by_attribute.is_empty() {
            for attribute in attributes.iter() {
                // Match the existing DOM attribute lookup's ASCII-insensitive local-name contract.
                let name = attribute.name.local.as_ref();
                let normalized;
                let name = if name.bytes().any(|byte| byte.is_ascii_uppercase()) {
                    normalized = name.to_ascii_lowercase();
                    normalized.as_str()
                } else {
                    name
                };
                if let Some(rules) = self.by_attribute.get(name)
                    && visit(rules)
                {
                    return true;
                }
            }
        }
        // DOM attribute lookup uses the first matching local name, including namespace aliases.
        if !self.by_id.is_empty()
            && let Some(id) = attributes
                .iter()
                .find(|a| a.name.local.as_ref().eq_ignore_ascii_case("id"))
            && let Some(rules) = self.by_id.get(id.value.as_ref())
            && visit(rules)
        {
            return true;
        }
        drop(attributes);
        let mut found = false;
        if !self.by_class.is_empty() {
            node.for_each_class(|class| {
                if !found && let Some(rules) = self.by_class.get(class) {
                    found = visit(rules);
                }
            });
        }
        found
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::css::media::MediaEnvironment;
    use crate::engine::css::stylesheet::parse_stylesheet;

    #[test]
    fn candidate_index_preserves_full_scan_matches_across_mutations() {
        let css = "*{} [hidden]{} [data-state=on]{} [DATA-STATE]{} .shared.component{} div.shared{} #target{} :is(.x,[hidden]){} :not([hidden]){} main > .shared{} .before + div{} a:link{} .shared::before{} #target::after{} [hidden]::placeholder{}";
        let mut rules = Vec::new();
        parse_stylesheet(
            css,
            "",
            MediaEnvironment::new(800.0, 600.0, 1.0, false),
            &mut 0,
            &mut rules,
            RuleScope::Document,
        );
        let index = RuleIndex::new(&rules);
        let dom = dom::parse(
            "<main><i class=before></i><div id=target class='shared shared component'></div><a href='/'>x</a></main>",
        );
        let target = dom.elements_named("div").next().unwrap();
        for step in 0..4 {
            match step {
                1 => {
                    target.set_attr("hidden", "");
                }
                2 => {
                    target.set_attr("data-state", "on");
                }
                3 => {
                    target.set_attr("class", "x");
                }
                _ => {}
            }
            for node in Node::descendants(&dom.document) {
                for pseudo in [
                    None,
                    Some(PseudoElement::Before),
                    Some(PseudoElement::After),
                    Some(PseudoElement::Placeholder),
                ] {
                    let expected = rules
                        .iter()
                        .enumerate()
                        .filter(|(_, rule)| {
                            rule.pseudo == pseudo
                                && selector_match::selector_matches(&rule.selector, &node)
                        })
                        .map(|(i, _)| i)
                        .collect::<Vec<_>>();
                    let actual = index
                        .candidates(&node, pseudo)
                        .into_iter()
                        .filter(|i| selector_match::selector_matches(&rules[*i].selector, &node))
                        .collect::<Vec<_>>();
                    assert_eq!(actual, expected, "step {step}, node {:?}", node.id());
                    // The short-circuit visitor finds a match exactly when the collected set has one.
                    let any = index.any_candidate(&node, pseudo, |i| {
                        selector_match::selector_matches(&rules[i].selector, &node)
                    });
                    assert_eq!(
                        any,
                        !expected.is_empty(),
                        "step {step}, node {:?}",
                        node.id()
                    );
                }
            }
        }
    }

    #[test]
    fn attribute_rules_do_not_enter_every_elements_candidate_set() {
        let css = (0..200)
            .map(|i| format!("[data-flag-{i}] {{color:red}} .shared.component-{i} {{color:blue}}"))
            .collect::<String>();
        let mut rules = Vec::new();
        parse_stylesheet(
            &css,
            "",
            MediaEnvironment::new(800.0, 600.0, 1.0, false),
            &mut 0,
            &mut rules,
            RuleScope::Document,
        );
        let index = RuleIndex::new(&rules);
        let dom = dom::parse("<div class='shared component-17' data-flag-17></div>");
        let node = dom.elements_named("div").next().unwrap();
        assert_eq!(rules.len(), 400);
        assert_eq!(index.candidates(&node, None).len(), 2);
        assert!(
            index
                .candidates(&node, Some(PseudoElement::Before))
                .is_empty()
        );
    }
}
