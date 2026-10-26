//! Generated `::before`/`::after` styles for elements that share a candidate's style.
//
// A sharing element reaches the same pseudo-element rule-index buckets as its source, and rules
// whose selectors read only compared inputs match both alike. When the remaining rules also
// agree, the same rules match both elements over the same originating style, so the source's
// pseudo-element style applies unchanged. Its `content` text is still resolved per element.
use super::*;

/// A share source's generated pseudo-element: the rules another element must revalidate, with
/// their results for the source, and the style the source's matching rules produced.
pub(in crate::engine::css::cascade) struct SourcePseudo {
    revalidation: Vec<(usize, bool)>,
    style: Option<Arc<ComputedStyle>>,
}

impl StyleSet {
    /// `origin`'s `pseudo` style when `origin` shares its style with `source`, which was styled
    /// earlier in this traversal: `Some(None)` when no rule generates it. `None` when it must
    /// be computed.
    pub(in crate::engine::css::cascade) fn shared_pseudo_style(
        &self,
        cache: &mut SharingCache,
        origin: &NodeRef,
        source: &NodeRef,
        pseudo: PseudoElement,
    ) -> Option<Option<Arc<ComputedStyle>>> {
        let shared = cache
            .pseudos
            .entry((source.id(), pseudo))
            .or_insert_with(|| self.source_pseudo(source, pseudo))
            .as_ref()?;
        let tree_root = Node::tree_root(origin);
        let ancestors = OnceCell::new();
        let same = shared.revalidation.iter().all(|&(rule, result)| {
            let rule = &self.compiled.rules[rule];
            self.rule_matches(rule, origin, Some(pseudo), &tree_root, &ancestors) == result
        });
        #[cfg(test)]
        {
            cache.stats.pseudo_hits += usize::from(same);
        }
        same.then(|| shared.style.clone())
    }

    fn source_pseudo(&self, source: &NodeRef, pseudo: PseudoElement) -> Option<SourcePseudo> {
        let sharing = self
            .compiled
            .sharing
            .rules
            .get_or_init(|| classify_rules(&self.compiled.rules));
        let tree_root = Node::tree_root(source);
        let ancestors = OnceCell::new();
        let mut revalidation = Vec::new();
        let mut matched = false;
        for index in self.compiled.index.candidates(source, Some(pseudo)) {
            let rule = self.compiled.rules.get(index)?;
            let result = self.rule_matches(rule, source, Some(pseudo), &tree_root, &ancestors);
            matched |= result;
            match sharing.get(index).copied()? {
                RuleSharing::KeyDetermined => {}
                RuleSharing::Unshareable => return None,
                RuleSharing::Revalidate => revalidation.push((index, result)),
            }
        }
        // The source synchronized its generated content when it was styled. Without a matching
        // rule its entry, if any, came from a CSSOM query and generates nothing.
        let style = if matched {
            Some(Arc::clone(self.pseudo_styles.get(&(source.id(), pseudo))?))
        } else {
            None
        };
        Some(SourcePseudo {
            revalidation,
            style,
        })
    }
}
