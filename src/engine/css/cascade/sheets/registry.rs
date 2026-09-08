//! Bounded weak discovery of independently retained cascades for one document.
use super::*;
use std::cell::RefCell;
use std::rc::Weak;

const MAX_DOCUMENTS: usize = 64;
const MAX_CANDIDATES: usize = 4;

thread_local! {
    // Node IDs are never reused. Neither keys nor weak candidates retain DOM nodes or CSS.
    static COMPILED: RefCell<HashMap<NodeId, Vec<Weak<CompiledRules>>>> = RefCell::new(HashMap::new());
}

pub(super) fn candidates(document: NodeId) -> Vec<Rc<CompiledRules>> {
    COMPILED.with(|cache| {
        cache
            .borrow()
            .get(&document)
            .into_iter()
            .flat_map(|sets| sets.iter().rev())
            .filter_map(Weak::upgrade)
            .collect()
    })
}

pub(super) fn remember(document: NodeId, compiled: &Rc<CompiledRules>) {
    COMPILED.with(|cache| {
        let mut cache = cache.borrow_mut();
        cache.retain(|_, sets| {
            sets.retain(|set| set.strong_count() != 0);
            !sets.is_empty()
        });
        if !cache.contains_key(&document)
            && cache.len() >= MAX_DOCUMENTS
            && let Some(key) = cache.keys().next().copied()
        {
            cache.remove(&key);
        }
        let current = Rc::downgrade(compiled);
        let sets = cache.entry(document).or_default();
        // Keep recently reused candidates, not just the most recently constructed temporary.
        sets.retain(|set| !set.ptr_eq(&current));
        sets.push(current);
        if sets.len() > MAX_CANDIDATES {
            sets.remove(0);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_bounds_weak_bookkeeping_without_retaining_cascades() {
        let document = Node::create_document().id();
        let variants = (0..MAX_CANDIDATES + 2)
            .map(|_| Rc::new(CompiledRules::default()))
            .collect::<Vec<_>>();
        for variant in &variants {
            remember(document, variant);
        }
        assert_eq!(candidates(document).len(), MAX_CANDIDATES);
        assert!(Rc::ptr_eq(
            &candidates(document)[0],
            variants.last().unwrap()
        ));
        drop(variants);
        assert!(candidates(document).is_empty());

        let documents = (0..MAX_DOCUMENTS + 2)
            .map(|_| {
                (
                    Node::create_document().id(),
                    Rc::new(CompiledRules::default()),
                )
            })
            .collect::<Vec<_>>();
        for (document, compiled) in &documents {
            remember(*document, compiled);
        }
        COMPILED.with(|cache| assert_eq!(cache.borrow().len(), MAX_DOCUMENTS));
        let weak = Rc::downgrade(&documents.last().unwrap().1);
        drop(documents);
        assert!(weak.upgrade().is_none());
    }
}
