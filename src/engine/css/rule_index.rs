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
    universal: Vec<usize>,
    by_id: HashMap<String, Vec<usize>>,
    by_class: HashMap<String, Vec<usize>>,
    by_tag: HashMap<String, Vec<usize>>,
    by_attribute: HashMap<String, Vec<usize>>,
}

impl RuleIndex {
    pub(super) fn new(rules: &[Rule]) -> Self {
        let mut frequency = HashMap::<Key<'_>, usize>::new();
        for target in rules
            .iter()
            .filter_map(|rule| rule.selector.compounds.last())
        {
            for key in keys(target) {
                *frequency.entry(key).or_default() += 1;
            }
        }
        let mut index = Self::default();
        for (rule_index, rule) in rules.iter().enumerate() {
            let Some(target) = rule.selector.compounds.last() else {
                continue;
            };
            // Store each rule exactly once, under its least frequent necessary key. The full
            // selector matcher still decides applicability, including scope and attribute values.
            let selected = keys(target).into_iter().min_by_key(|key| frequency[key]);
            let (map, value) = match selected {
                Some(Key::Id(value)) => (&mut index.by_id, value),
                Some(Key::Class(value)) => (&mut index.by_class, value),
                Some(Key::Tag(value)) => (&mut index.by_tag, value),
                Some(Key::Attribute(value)) => (&mut index.by_attribute, value),
                None => {
                    index.universal.push(rule_index);
                    continue;
                }
            };
            map.entry(value.to_string()).or_default().push(rule_index);
        }
        index
    }

    pub(super) fn candidates(&self, node: &NodeRef) -> Vec<usize> {
        let Some(element) = node.element() else {
            return Vec::new();
        };
        let mut candidates = self.universal.clone();
        if let Some(rules) = self.by_tag.get(element.name.local.as_ref()) {
            candidates.extend(rules);
        }
        let attributes = element.attrs.borrow();
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
            if let Some(rules) = self.by_attribute.get(name) {
                candidates.extend(rules);
            }
        }
        // DOM attribute lookup uses the first matching local name, including namespace aliases.
        if let Some(id) = attributes
            .iter()
            .find(|a| a.name.local.as_ref().eq_ignore_ascii_case("id"))
            && let Some(rules) = self.by_id.get(id.value.as_ref())
        {
            candidates.extend(rules);
        }
        if let Some(classes) = attributes
            .iter()
            .find(|a| a.name.local.as_ref().eq_ignore_ascii_case("class"))
        {
            for class in classes.value.split_ascii_whitespace() {
                if let Some(rules) = self.by_class.get(class) {
                    candidates.extend(rules);
                }
            }
        }
        candidates.sort_unstable();
        candidates.dedup();
        candidates
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::css::media::MediaEnvironment;
    use crate::engine::css::stylesheet::parse_stylesheet;

    #[test]
    fn candidate_index_preserves_full_scan_matches_across_mutations() {
        let css = "*{} [hidden]{} [data-state=on]{} [DATA-STATE]{} .shared.component{} div.shared{} #target{} :is(.x,[hidden]){} :not([hidden]){} main > .shared{} .before + div{} a:link{} .shared::before{}";
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
                let expected = rules
                    .iter()
                    .enumerate()
                    .filter(|(_, rule)| selector_match::selector_matches(&rule.selector, &node))
                    .map(|(i, _)| i)
                    .collect::<Vec<_>>();
                let actual = index
                    .candidates(&node)
                    .into_iter()
                    .filter(|i| selector_match::selector_matches(&rules[*i].selector, &node))
                    .collect::<Vec<_>>();
                assert_eq!(actual, expected, "step {step}, node {:?}", node.id());
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
        assert_eq!(index.candidates(&node).len(), 2);
    }
}
