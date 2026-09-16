//! Reuse geometry only when a refreshed mutation cannot affect any rendered box.
use super::*;
use crate::engine::css::Display;
use crate::engine::invalidation::RenderInvalidation;

impl Page {
    pub(crate) fn invalidation_is_nonrendered(
        &self,
        invalidation: &RenderInvalidation,
        stats: &StyleRefreshStats,
    ) -> bool {
        if stats.layout_changed
            || stats.full_rebuild
            || (stats.removed_styles != 0 && !invalidation.removals_are_local)
            || invalidation.roots.is_empty()
        {
            return false;
        }
        let Some((_, _, styles)) = self.cached_styles.as_ref() else {
            return false;
        };
        invalidation.roots.iter().all(|id| {
            let Some(root) = self.dom.find_node(*id) else {
                return false;
            };
            // Fullscreen can participate independently of the ordinary composed traversal.
            // Unknown roots and top-layer descendants always retain the regular layout path.
            if Node::shadow_including_descendants(&root).any(|node| {
                node.is_fullscreen()
                    || node.tag_name() == Some("base")
                    || node
                        .namespace_uri()
                        .is_some_and(|namespace| namespace != "http://www.w3.org/1999/xhtml")
            }) {
                return false;
            }
            // display:none omits an entire subtree from the box tree, unlike visibility:hidden
            // or opacity:0. Styles were refreshed first, so revealing it cannot use this path.
            // https://drafts.csswg.org/css-display/#valdef-display-none
            let mut nonrendered = false;
            for ancestor in std::iter::successors(Some(root), Node::composed_parent) {
                if ancestor
                    .namespace_uri()
                    .is_some_and(|namespace| namespace != "http://www.w3.org/1999/xhtml")
                {
                    return false;
                }
                nonrendered |= styles
                    .styles
                    .get(&ancestor.id())
                    .is_some_and(|style| style.display == Display::None);
            }
            nonrendered
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::invalidation::MutationKind;

    #[test]
    fn only_unchanged_nonrendered_subtrees_can_reuse_geometry() {
        let mut page = Page::parse_scripted(
            "<style>section{display:none}aside{visibility:hidden}nav{opacity:0}</style><section><b>x</b></section><aside>y</aside><nav>z</nav>",
            "https://example.test/",
        );
        page.refresh_resources(800.0);
        let root = page.dom.elements_named("section").next().unwrap();
        let hidden_child = page.dom.elements_named("b").next().unwrap();
        let mut invalidation = RenderInvalidation {
            roots: vec![hidden_child.id()],
            impact: MutationKind::CharacterData.impact(),
            ..RenderInvalidation::default()
        };
        assert!(page.invalidation_is_nonrendered(&invalidation, &StyleRefreshStats::default()));
        assert!(!page.invalidation_is_nonrendered(
            &invalidation,
            &StyleRefreshStats {
                layout_changed: true,
                ..StyleRefreshStats::default()
            }
        ));
        for stats in [
            StyleRefreshStats {
                full_rebuild: true,
                ..StyleRefreshStats::default()
            },
            StyleRefreshStats {
                removed_styles: 1,
                ..StyleRefreshStats::default()
            },
        ] {
            assert!(!page.invalidation_is_nonrendered(&invalidation, &stats));
        }
        hidden_child.set_fullscreen(true);
        assert!(!page.invalidation_is_nonrendered(&invalidation, &StyleRefreshStats::default()));
        hidden_child.set_fullscreen(false);
        for tag in ["aside", "nav"] {
            invalidation.roots = vec![page.dom.elements_named(tag).next().unwrap().id()];
            assert!(
                !page.invalidation_is_nonrendered(&invalidation, &StyleRefreshStats::default())
            );
        }
        root.set_attr("style", "display:block");
        invalidation.roots = vec![root.id()];
        invalidation.impact = MutationKind::Attribute("style").impact();
        let stats = page.refresh_resources_after_invalidation(800.0, &invalidation);
        assert!(stats.layout_changed);
        assert!(!page.invalidation_is_nonrendered(&invalidation, &stats));
        assert!(!page.invalidation_is_nonrendered(
            &RenderInvalidation::default(),
            &StyleRefreshStats::default()
        ));
        Node::remove_from_parent(&root);
        assert!(!page.invalidation_is_nonrendered(&invalidation, &StyleRefreshStats::default()));
    }

    #[test]
    fn hidden_resource_definitions_and_mixed_visible_roots_keep_the_layout_path() {
        for contents in [
            "<svg><path d='M0 0L1 1'/></svg>",
            "<base href='https://other.test/'>",
        ] {
            let mut page = Page::parse_scripted(
                "<section style='display:none'></section><aside>visible</aside>",
                "https://example.test/",
            );
            let root = page.dom.elements_named("section").next().unwrap();
            Node::replace_inner_html(&root, contents, true);
            page.refresh_resources(800.0);
            let invalidation = RenderInvalidation {
                roots: vec![root.id()],
                ..RenderInvalidation::default()
            };
            assert!(
                !page.invalidation_is_nonrendered(&invalidation, &StyleRefreshStats::default())
            );
        }
        let mut page = Page::parse_scripted(
            "<section style='display:none'></section><aside>visible</aside>",
            "https://example.test/",
        );
        page.refresh_resources(800.0);
        let invalidation = RenderInvalidation {
            roots: vec![
                page.dom.elements_named("section").next().unwrap().id(),
                page.dom.elements_named("aside").next().unwrap().id(),
            ],
            ..RenderInvalidation::default()
        };
        assert!(!page.invalidation_is_nonrendered(&invalidation, &StyleRefreshStats::default()));
    }

    #[test]
    fn inspected_hidden_removals_reuse_geometry_but_reveals_do_not() {
        let mut page = Page::parse_scripted(
            "<style>section:empty{display:block}</style><section style='display:none'><b>gone</b><i>remaining</i></section>",
            "https://example.test/",
        );
        page.refresh_resources(800.0);
        let root = page.dom.elements_named("section").next().unwrap();
        let child = page.dom.elements_named("b").next().unwrap();
        Node::remove_from_parent(&child);
        let mut invalidation = RenderInvalidation {
            roots: vec![root.id()],
            removed_nodes: vec![child.id()],
            removals_are_local: true,
            impact: MutationKind::ChildList.impact(),
            ..RenderInvalidation::default()
        };
        let stats = page.refresh_resources_after_invalidation(800.0, &invalidation);
        assert!(stats.removed_styles > 0);
        assert!(page.invalidation_is_nonrendered(&invalidation, &stats));
        invalidation.removals_are_local = false;
        assert!(!page.invalidation_is_nonrendered(&invalidation, &stats));
        invalidation.removals_are_local = true;
        root.set_attr("style", "display:block");
        let stats = page.refresh_resources_after_invalidation(800.0, &invalidation);
        assert!(!page.invalidation_is_nonrendered(&invalidation, &stats));
    }
}
