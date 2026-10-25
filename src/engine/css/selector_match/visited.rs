//! `:visited` matching with the privacy restrictions browsers apply.
//! https://drafts.csswg.org/selectors-4/#link
//!
//! The visited set is this renderer's own record of documents it loaded, including
//! subframe navigations, partitioned by top-level site; it is not the browser-wide history. Ordinary selector matching
//! (`matches()`, `querySelector()`, `getComputedStyle()`, invalidation) treats every link
//! as unvisited. Only the rendering cascade matches `:visited`, for the link being styled,
//! in a second pass whose color-type values alone reach paint. Layout never sees them.
//! https://developer.mozilla.org/docs/Web/CSS/Privacy_and_the_:visited_selector
use crate::engine::dom::{NodeId, NodeRef};
use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};

/// Bounds the per-renderer record; older entries are not evicted, new ones are dropped.
const MAX_VISITED_URLS: usize = 4096;
/// Bounds the resolved-href cache; it is simply cleared when full.
const MAX_RESOLVED_HREFS: usize = 1024;

/// Visited URLs recorded while one top-level site was displayed. Browsers partition
/// `:visited` by top-level site, so a page cannot probe history made under another site.
#[derive(Default)]
struct Partition {
    site: String,
    urls: HashSet<String>,
}

thread_local! {
    static VISITED: RefCell<Partition> = RefCell::default();
    static RESOLVED_HREFS: RefCell<HashMap<(String, String), Option<String>>> = RefCell::default();
    static VISITED_LINK: Cell<Option<NodeId>> = const { Cell::new(None) };
}

/// The partition key: scheme and host. This is stricter than a registrable-domain site,
/// which needs the public suffix list, and never shares history across hosts.
fn site_of(url: &str) -> String {
    url::Url::parse(url).map_or_else(
        |_| String::new(),
        |url| format!("{}://{}", url.scheme(), url.host_str().unwrap_or_default()),
    )
}

/// A top-level navigation records its URL and starts a new partition when the site changes.
pub(crate) fn record_top_level_navigation(url: &str) {
    let site = site_of(url);
    VISITED.with(|visited| {
        let mut visited = visited.borrow_mut();
        if visited.site != site {
            *visited = Partition {
                site,
                urls: HashSet::new(),
            };
        }
    });
    record_visited_url(url);
}

/// Records a document URL navigated to under the current top-level site, including a
/// subframe's final (possibly redirected, cross-origin) URL. The partition confines any such
/// entry to links shown under this same top-level site. The fragment is kept, as in history.
pub(crate) fn record_visited_url(url: &str) {
    if url.is_empty() || url.starts_with("about:") || url.starts_with("data:") {
        return;
    }
    VISITED.with(|visited| {
        let mut visited = visited.borrow_mut();
        if visited.urls.len() < MAX_VISITED_URLS {
            visited.urls.insert(url.to_owned());
        }
    });
}

fn has_visited_urls() -> bool {
    VISITED.with(|visited| !visited.borrow().urls.is_empty())
}

/// An HTML `a` element with an `href` is a hyperlink for `:any-link`.
/// https://html.spec.whatwg.org/multipage/semantics-other.html#selector-link
fn is_link(node: &NodeRef) -> bool {
    node.tag_name() == Some("a") && node.attr_ref("href").is_some()
}

/// `:any-link`, `:link` and `:visited`. Outside the rendering cascade's visited-style pass
/// every link is unvisited, and no link is both (`:link:visited` never matches).
pub(super) fn link_matches(
    selector: &crate::engine::css::CompoundSelector,
    node: &NodeRef,
) -> bool {
    is_link(node)
        && match (selector.requires_visited, selector.requires_unvisited) {
            (true, true) => false,
            (true, false) => matches_as_visited(node),
            (false, true) => !matches_as_visited(node),
            (false, false) => true,
        }
}

/// Whether `node` is an HTML `a` hyperlink whose URL is in the visited set. Resolution is
/// cached per (base, href), so restyles do not reparse each link's URL.
pub(crate) fn is_visited_link(node: &NodeRef, base_url: &str) -> bool {
    if !has_visited_urls() || !is_link(node) {
        return false;
    }
    let Some(href) = node.attr_ref("href") else {
        return false;
    };
    let key = (base_url.to_owned(), href.trim().to_owned());
    drop(href);
    let resolved = RESOLVED_HREFS.with(|cache| {
        if let Some(resolved) = cache.borrow().get(&key) {
            return resolved.clone();
        }
        let resolved = crate::navigation::resolve_url(&key.0, &key.1);
        let mut cache = cache.borrow_mut();
        if cache.len() >= MAX_RESOLVED_HREFS {
            cache.clear();
        }
        cache.insert(key, resolved.clone());
        resolved
    });
    resolved.is_some_and(|url| VISITED.with(|visited| visited.borrow().urls.contains(&url)))
}

/// Runs `cascade` with `link` matching `:visited` instead of `:link`.
pub(crate) fn with_visited_link<T>(link: &NodeRef, cascade: impl FnOnce() -> T) -> T {
    let previous = VISITED_LINK.with(|current| current.replace(Some(link.id())));
    let result = cascade();
    VISITED_LINK.with(|current| current.set(previous));
    result
}

/// Whether the current visited-style pass is styling `node` as a visited link.
pub(crate) fn matches_as_visited(node: &NodeRef) -> bool {
    VISITED_LINK.with(|current| current.get() == Some(node.id()))
}

/// Paints a visited link with its visited-style colors. Only color-type values cross over,
/// and each keeps the unvisited alpha, so `:visited` cannot change layout or transparency.
pub(crate) fn adopt_visited_colors(
    style: &mut crate::engine::css::ComputedStyle,
    visited: &crate::engine::css::ComputedStyle,
) {
    use crate::engine::css::Color;
    let blend = |unvisited: Color, visited: Color| Color {
        alpha: unvisited.alpha,
        ..visited
    };
    style.color = blend(style.color, visited.color);
    style.background_color = blend(style.background_color, visited.background_color);
    for (unvisited, visited) in style.border_colors.iter_mut().zip(visited.border_colors) {
        if let (Some(unvisited), Some(visited)) = (unvisited.as_mut(), visited) {
            *unvisited = blend(*unvisited, visited);
        }
    }
}

#[cfg(test)]
pub(crate) fn clear_visited_urls() {
    VISITED.with(|visited| *visited.borrow_mut() = Partition::default());
}
